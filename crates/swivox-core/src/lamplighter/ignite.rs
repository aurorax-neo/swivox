//! Lamplighter 5 小时额度窗口自动点火引擎（`07:00..22:30` 排期、滑动占位检测、24h 被拒模型记忆、首包 2 秒流截断与窗口确认）。

use std::collections::{BTreeMap, HashMap};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use anyhow::bail;
use chrono::{Datelike, Local, TimeZone, Utc};
use futures_util::StreamExt;
use serde_json::{Value, json};
use uuid::Uuid;

use super::config::{IgnitionConfig, LamplighterConfig, normalize_client_id};
use super::providers::{
    RawQuotaGroup, UnifiedCredential, build_account_http_client, parse_passive_response_headers,
    query_credential_quotas, summarize_upstream_error_body,
};
use super::store::IgnitionTargetState;

pub const IGNITION_TRIGGER_PROMPT: &str =
    "Reply with only OK. Do not think, do not use tools, do not do anything else.";

static REFUSED_MODELS: OnceLock<Mutex<HashMap<(String, String, String), i64>>> = OnceLock::new();

fn refused_models_map() -> &'static Mutex<HashMap<(String, String, String), i64>> {
    REFUSED_MODELS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn is_model_refused(client_id: &str, group_key: &str, model: &str, now: i64) -> bool {
    let Ok(mut map) = refused_models_map().lock() else {
        return false;
    };
    map.retain(|_, exp| *exp > now);
    map.get(&(
        normalize_client_id(client_id).to_string(),
        group_key.to_string(),
        model.to_string(),
    ))
    .is_some_and(|exp| *exp > now)
}

fn mark_model_refused_24h(client_id: &str, group_key: &str, model: &str, now: i64) {
    if let Ok(mut map) = refused_models_map().lock() {
        map.insert(
            (
                normalize_client_id(client_id).to_string(),
                group_key.to_string(),
                model.to_string(),
            ),
            now + 86_400,
        );
    }
}

/// 观测 5h 窗口的最新重置时间，识别「每次查询都返回 `now + 5h`」的滑动占位状态。
pub fn observe_five_hour_reset(
    ts: &mut IgnitionTargetState,
    reset_at: Option<i64>,
    now: i64,
    passive: bool,
) {
    let Some(rst) = reset_at.filter(|v| *v > 0) else {
        return;
    };
    if !passive && ts.last_seen_at > 0 && ts.last_reset_seen > 0 {
        let elapsed = now - ts.last_seen_at;
        let near_5h = ((rst - now) - 18_000).abs() <= 120;
        let tol = ((elapsed as f64) * 0.02).clamp(3.0, 10.0) as i64;
        let drifted_with_clock = elapsed >= 2 && ((rst - ts.last_reset_seen) - elapsed).abs() <= tol;
        ts.rolling_reset = near_5h && drifted_with_clock;
    }
    ts.last_reset_seen = rst;
    ts.last_seen_at = now;
    if rst > now && !ts.rolling_reset {
        // 观测到固定的未来重置时间，说明窗口已在计时，清空失败退避与熔断
        ts.consecutive_failures = 0;
        ts.retry_at = 0;
        ts.paused_until = 0;
        ts.paused_reason = None;
    }
}

pub fn next_morning_start_epoch_for(ig_cfg: &IgnitionConfig, after_epoch: i64) -> i64 {
    let dt = Local
        .timestamp_opt(after_epoch, 0)
        .single()
        .unwrap_or_else(Local::now);
    let today_start = Local
        .with_ymd_and_hms(
            dt.year(),
            dt.month(),
            dt.day(),
            u32::from(ig_cfg.start_hour),
            0,
            0,
        )
        .single()
        .unwrap_or(dt);
    if dt < today_start {
        today_start.timestamp()
    } else {
        (today_start + chrono::Duration::days(1)).timestamp()
    }
}

pub fn next_morning_start_epoch(cfg: &LamplighterConfig, after_epoch: i64) -> i64 {
    next_morning_start_epoch_for(&cfg.ignition, after_epoch)
}

fn day_window_bounds_for(ig_cfg: &IgnitionConfig, epoch: i64) -> (i64, i64) {
    let dt = Local
        .timestamp_opt(epoch, 0)
        .single()
        .unwrap_or_else(Local::now);
    let start = Local
        .with_ymd_and_hms(
            dt.year(),
            dt.month(),
            dt.day(),
            u32::from(ig_cfg.start_hour),
            0,
            0,
        )
        .single()
        .unwrap_or(dt);
    let end_base = Local
        .with_ymd_and_hms(
            dt.year(),
            dt.month(),
            dt.day(),
            u32::from(ig_cfg.end_hour),
            0,
            0,
        )
        .single()
        .unwrap_or(dt);
    let end = end_base + chrono::Duration::minutes(i64::from(ig_cfg.end_grace_minutes));
    (start.timestamp(), end.timestamp())
}

/// 按指定客户端独立隔离的点火配置计算额度组下一次计划自动点火时间。
pub fn compute_next_ignition_due_for_client(
    cfg: &LamplighterConfig,
    client_id: &str,
    ts: &IgnitionTargetState,
    current_5h_reset: Option<i64>,
    now: i64,
) -> Option<i64> {
    let ig_cfg = cfg.ignition_for_client(client_id);
    if !ig_cfg.enabled {
        return None;
    }
    if ts.paused_until > now {
        return Some(ts.paused_until);
    }

    let (today_start, today_end) = day_window_bounds_for(ig_cfg, now);
    if now < today_start {
        return Some(today_start);
    }
    if now > today_end {
        return Some(next_morning_start_epoch_for(ig_cfg, now));
    }

    let hold = (ts.last_success_at + ig_cfg.post_success_hold_seconds as i64)
        .max(ts.retry_at)
        .max(now);

    if ts.rolling_reset {
        return Some(hold);
    }

    if let Some(rst) = current_5h_reset.filter(|v| *v > now) {
        let target = rst + ig_cfg.grace_seconds as i64;
        let (target_day_start, target_day_end) = day_window_bounds_for(ig_cfg, target);
        if target >= target_day_start && target <= target_day_end {
            return Some(target.max(hold));
        }
        return Some(next_morning_start_epoch_for(ig_cfg, target));
    }

    Some(hold)
}

/// 计算某个支持点火的额度组下一次计划自动点火的 Unix 秒时间戳（兼容默认客户端）。
pub fn compute_next_ignition_due(
    cfg: &LamplighterConfig,
    ts: &IgnitionTargetState,
    current_5h_reset: Option<i64>,
    now: i64,
) -> Option<i64> {
    compute_next_ignition_due_for_client(cfg, "codex", ts, current_5h_reset, now)
}

fn numeric_segments(name: &str) -> Vec<u64> {
    let mut segs = Vec::new();
    let mut cur = String::new();
    for ch in name.chars() {
        if ch.is_ascii_digit() {
            cur.push(ch);
        } else if !cur.is_empty() {
            if let Ok(v) = cur.parse::<u64>() {
                segs.push(v);
            }
            cur.clear();
        }
    }
    if !cur.is_empty()
        && let Ok(v) = cur.parse::<u64>()
    {
        segs.push(v);
    }
    segs
}

pub fn sort_models_newest_first(models: &mut [String]) {
    models.sort_by(|a, b| {
        let sa = numeric_segments(a);
        let sb = numeric_segments(b);
        sb.cmp(&sa)
            .then_with(|| b.to_ascii_lowercase().cmp(&a.to_ascii_lowercase()))
    });
}

fn default_candidate_models(provider: &str, group_label: &str) -> Vec<String> {
    match provider {
        "codex" => vec![
            "gpt-5-codex-mini".to_string(),
            "codex-mini-latest".to_string(),
            "gpt-5.2-codex".to_string(),
            "gpt-5-codex".to_string(),
            "gpt-5".to_string(),
        ],
        "antigravity" => {
            let lower = group_label.to_ascii_lowercase();
            if lower.contains("argon") {
                vec![
                    "gemini-4-argon-low".to_string(),
                    "gemini-4-argon-medium".to_string(),
                ]
            } else if lower.contains("claude") || lower.contains("gpt") {
                vec![
                    "claude-sonnet-5-5-low".to_string(),
                    "claude-sonnet-4-6".to_string(),
                    "claude-sonnet-4-5".to_string(),
                    "gpt-oss-120b-medium".to_string(),
                ]
            } else {
                vec![
                    "gemini-3.5-flash-lite".to_string(),
                    "gemini-2.5-flash".to_string(),
                    "gemini-3-flash".to_string(),
                    "gemini-2.5-pro".to_string(),
                ]
            }
        }
        _ => Vec::new(),
    }
}

/// 按低首包延迟优先级对 Antigravity 可用模型列表分组排序（优先 `flash-lite` / `-low`，规避慢速 `tiered`/`high`/`thinking` 模型）。
pub fn rank_antigravity_ignition_models(
    available_model_ids: &[String],
    group_label: &str,
) -> Vec<String> {
    let lower_label = group_label.to_ascii_lowercase();
    let is_argon = lower_label.contains("argon");
    let is_3p = lower_label.contains("claude") || lower_label.contains("gpt");

    let mut buckets: [Vec<String>; 5] = Default::default();
    for raw in available_model_ids {
        let l = raw.to_ascii_lowercase();
        if l.contains("image")
            || l.contains("tab_")
            || l.contains("embedding")
            || l.contains("tts")
            || l.contains("audio")
        {
            continue;
        }
        if is_argon {
            if !l.contains("argon") || l.contains("thinking") {
                continue;
            }
            let tier = if l.contains("low") {
                0
            } else if l.contains("medium") {
                1
            } else {
                2
            };
            buckets[tier].push(raw.clone());
        } else if is_3p {
            if !(l.contains("claude") || l.contains("gpt")) || l.contains("thinking") {
                continue;
            }
            let tier = if l.contains("haiku") || (l.contains("claude-sonnet") && l.ends_with("-low"))
            {
                0
            } else if l.contains("claude-sonnet") && !l.contains("high") {
                1
            } else if l.contains("gpt-oss") {
                2
            } else if l.contains("claude-opus") && l.ends_with("-low") {
                3
            } else {
                4
            };
            buckets[tier].push(raw.clone());
        } else {
            if !l.contains("gemini") || l.contains("argon") || l.contains("thinking") {
                continue;
            }
            let has_heavy_tier =
                l.contains("tiered") || l.contains("high") || l.contains("medium");
            let tier = if l.contains("flash-lite") {
                0
            } else if l.contains("flash-low") || (l.contains("flash") && !has_heavy_tier) {
                1
            } else if l.contains("flash") {
                2
            } else if l.contains("-low") {
                3
            } else {
                4
            };
            buckets[tier].push(raw.clone());
        }
    }

    let mut ordered = Vec::new();
    for (idx, bucket) in buckets.iter_mut().enumerate() {
        if is_3p && idx == 2 && bucket.len() > 1 {
            // gpt-oss 同档内低延迟后缀 (-low) 优先于 -medium/-high
            let mut low = Vec::new();
            let mut mid = Vec::new();
            let mut rest = Vec::new();
            for m in bucket.drain(..) {
                let lm = m.to_ascii_lowercase();
                if lm.ends_with("-low") {
                    low.push(m);
                } else if lm.ends_with("-medium") {
                    mid.push(m);
                } else {
                    rest.push(m);
                }
            }
            sort_models_newest_first(&mut low);
            sort_models_newest_first(&mut mid);
            sort_models_newest_first(&mut rest);
            ordered.extend(low);
            ordered.extend(mid);
            ordered.extend(rest);
            continue;
        }
        sort_models_newest_first(bucket);
        for m in bucket.iter() {
            if !ordered.contains(m) {
                ordered.push(m.clone());
            }
        }
    }
    ordered
}

async fn resolve_ignition_candidates(
    cfg: &LamplighterConfig,
    cred: &UnifiedCredential,
    group_key: &str,
    group_label: &str,
) -> Vec<String> {
    let rule = cfg.provider_rule(&cred.provider);
    if let Some(group_override) = rule.models.get(group_label).filter(|s| !s.trim().is_empty()) {
        return vec![group_override.trim().to_string()];
    }
    if !rule.model.trim().is_empty() {
        return vec![rule.model.trim().to_string()];
    }

    let now = Utc::now().timestamp();
    let mut candidates = Vec::new();

    if cred.provider == "antigravity" && cred.source_kind == "swivox_ag" {
        let ag_state = crate::antigravity::load_antigravity_state();
        if let Some(acc) = ag_state.accounts.iter().find(|a| a.id == cred.raw_account_id)
            && let Some(ref q) = acc.quota
        {
            candidates = rank_antigravity_ignition_models(&q.available_model_ids, group_label);
        }
    }

    if candidates.is_empty() {
        candidates = default_candidate_models(&cred.provider, group_label);
    }

    candidates.retain(|m| !is_model_refused(&cred.client_id, group_key, m, now));
    candidates.truncate(5);
    candidates
}

#[derive(Debug, Clone)]
pub struct IgnitionExecutionResult {
    pub model_used: String,
    pub confirmed_reset_at: i64,
    pub refreshed_groups: Vec<RawQuotaGroup>,
}

async fn ensure_credential_fresh_before_ignition(cred: &mut UnifiedCredential) -> anyhow::Result<()> {
    match cred.source_kind {
        "swivox_ag" => {
            let acc = crate::antigravity::ensure_fresh_account_token(&cred.raw_account_id).await?;
            cred.access_token = acc.token.access_token;
            cred.refresh_token = Some(acc.token.refresh_token);
            if let Some(pid) = acc
                .token
                .project_id
                .or_else(|| acc.quota.and_then(|q| q.project_id))
                .filter(|s| !s.trim().is_empty())
            {
                cred.google_project_id = Some(pid);
            }
            if cred
                .google_project_id
                .as_deref()
                .map(str::trim)
                .unwrap_or("")
                .is_empty()
                && let Ok(snap) = crate::antigravity::refresh_account_quota(&cred.raw_account_id).await
                && let Some(pid) = snap.project_id.filter(|s| !s.trim().is_empty())
            {
                cred.google_project_id = Some(pid);
            }
        }
        "swivox_codex" => {
            if crate::official_accounts::is_access_token_expired_or_near_expiry(&cred.access_token)
                && crate::official_accounts::refresh_official_account_quota(&cred.raw_account_id)
                    .await
                    .is_ok()
            {
                let reloaded = crate::official_accounts::load_official_accounts_state();
                if let Some(updated) = reloaded
                    .accounts
                    .into_iter()
                    .find(|a| a.id == cred.raw_account_id)
                {
                    cred.access_token = updated.tokens.access_token;
                    cred.refresh_token = updated.tokens.refresh_token;
                }
            }
        }
        _ => {}
    }
    Ok(())
}

async fn force_refresh_credential_on_401(cred: &mut UnifiedCredential) -> bool {
    match cred.source_kind {
        "swivox_ag" => {
            if let Ok(acc) =
                crate::antigravity::force_refresh_account_token(&cred.raw_account_id).await
            {
                cred.access_token = acc.token.access_token;
                cred.refresh_token = Some(acc.token.refresh_token);
                if acc.token.project_id.is_some() {
                    cred.google_project_id = acc.token.project_id;
                }
                return true;
            }
            false
        }
        "swivox_codex" => {
            if crate::official_accounts::refresh_official_account_quota(&cred.raw_account_id)
                .await
                .is_ok()
            {
                let reloaded = crate::official_accounts::load_official_accounts_state();
                if let Some(updated) = reloaded
                    .accounts
                    .into_iter()
                    .find(|a| a.id == cred.raw_account_id)
                {
                    cred.access_token = updated.tokens.access_token;
                    cred.refresh_token = updated.tokens.refresh_token;
                    return true;
                }
            }
            false
        }
        _ => {
            if cred.provider == "antigravity"
                && let Some(ref rt) = cred.refresh_token.clone()
                && !rt.trim().is_empty()
                && let Ok(refreshed) = crate::antigravity::refresh_google_access_token(
                    rt,
                    None,
                    cred.proxy_spec.as_deref(),
                )
                .await
            {
                cred.access_token = refreshed.access_token;
                cred.refresh_token = Some(refreshed.refresh_token);
                return true;
            }
            false
        }
    }
}

/// 对指定账号与额度组执行一次真实最小流式点火（命中首个活跃生成事件即刻截断流，随后快速轮询确认新 5h 重置时间）。
pub async fn execute_ignition_once(
    cfg: &LamplighterConfig,
    cred: &mut UnifiedCredential,
    group_key: &str,
    group_label: &str,
) -> anyhow::Result<IgnitionExecutionResult> {
    ensure_credential_fresh_before_ignition(cred).await?;

    let candidates = resolve_ignition_candidates(cfg, cred, group_key, group_label).await;
    if candidates.is_empty() {
        bail!("未找到可用的点火候选模型。");
    }

    let start_ts = Utc::now().timestamp();
    let mut chosen_model = None;
    let mut passive_groups: Option<Vec<RawQuotaGroup>> = None;
    let mut last_send_err = None;

    for (idx, model) in candidates.iter().take(2).enumerate() {
        match send_minimal_streaming_prompt(cred, model).await {
            Ok(passive) => {
                chosen_model = Some(model.clone());
                passive_groups = passive;
                break;
            }
            Err((status_code, msg)) => {
                if matches!(status_code, Some(400) | Some(404)) && idx == 0 && candidates.len() > 1 {
                    mark_model_refused_24h(&cred.client_id, group_key, model, start_ts);
                    last_send_err = Some(msg);
                    continue;
                }
                bail!("{msg}");
            }
        }
    }

    let Some(model_used) = chosen_model else {
        bail!(
            "{}",
            last_send_err.unwrap_or_else(|| "点火模型均被上游拒绝".to_string())
        );
    };

    // Codex 若在响应头已返回已计时的 5h 窗口，可直接零等待确认
    let check_now = Utc::now().timestamp();
    if cred.provider == "codex"
        && let Some(groups) = passive_groups
        && let Some(rst) = groups
            .iter()
            .flat_map(|g| g.windows.iter())
            .find(|w| w.is_five_hour && w.remaining_percent < 100.0)
            .and_then(|w| w.reset_at.filter(|r| *r > check_now))
    {
        return Ok(IgnitionExecutionResult {
            model_used,
            confirmed_reset_at: rst,
            refreshed_groups: groups,
        });
    }

    // 快速渐进轮询确认该组 5h 窗口产生了固定的未来 reset_at（[350ms, 1000ms, 2000ms, 3500ms]）
    for wait_ms in [350u64, 1000, 2000, 3500] {
        tokio::time::sleep(Duration::from_millis(wait_ms)).await;
        if let Ok(groups) = query_credential_quotas(cred, cfg.request_timeout_seconds).await {
            let now = Utc::now().timestamp();
            let confirmed_rst = groups
                .iter()
                .filter(|grp| cred.provider != "antigravity" || grp.source_label == group_label)
                .flat_map(|grp| grp.windows.iter())
                .find(|w| w.is_five_hour)
                .and_then(|w| w.reset_at)
                .filter(|rst| *rst > now);
            if let Some(rst) = confirmed_rst {
                return Ok(IgnitionExecutionResult {
                    model_used,
                    confirmed_reset_at: rst,
                    refreshed_groups: groups,
                });
            }
        }
    }

    bail!("点火请求已发出（{model_used}），但尚未确认到新的 5 小时窗口重置时间。")
}

async fn send_minimal_streaming_prompt(
    cred: &mut UnifiedCredential,
    model: &str,
) -> Result<Option<Vec<RawQuotaGroup>>, (Option<u16>, String)> {
    let mut resp_opt = None;
    for attempt in 0..2 {
        let resp = match cred.provider.as_str() {
            "codex" => {
                let ua = "codex-tui/0.154.0 (Mac OS 26.5.2; arm64) Swivox-Lamplighter";
                let client = build_account_http_client(cred, 20, ua)
                    .map_err(|e| (None, e.to_string()))?;
                let mut req = client
                    .post("https://chatgpt.com/backend-api/codex/responses")
                    .bearer_auth(cred.access_token.trim())
                    .header("Content-Type", "application/json")
                    .header("Accept", "text/event-stream")
                    .json(&json!({
                        "model": model,
                        "instructions": "Reply with only OK.",
                        "input": [{
                            "role": "user",
                            "content": [{ "type": "input_text", "text": IGNITION_TRIGGER_PROMPT }]
                        }],
                        "tools": [],
                        "stream": true,
                        "store": false
                    }));
                if let Some(ref aid) = cred.chatgpt_account_id
                    && !aid.trim().is_empty()
                {
                    req = req.header("Chatgpt-Account-Id", aid.trim());
                }
                req.send().await.map_err(|e| (None, e.to_string()))?
            }
            "antigravity" => {
                let client = build_account_http_client(
                    cred,
                    20,
                    crate::antigravity::ANTIGRAVITY_HUB_USER_AGENT,
                )
                .map_err(|e| (None, e.to_string()))?;
                let body = json!({
                    "project": cred.google_project_id.clone().unwrap_or_default(),
                    "requestId": format!("agent-{}", Uuid::new_v4()),
                    "model": model,
                    "userAgent": "antigravity",
                    "request": {
                        "contents": [{
                            "role": "user",
                            "parts": [{ "text": IGNITION_TRIGGER_PROMPT }]
                        }],
                        "generationConfig": { "temperature": 0.0 }
                    }
                });
                let endpoints = [
                    "https://daily-cloudcode-pa.googleapis.com/v1internal:streamGenerateContent?alt=sse",
                    "https://cloudcode-pa.googleapis.com/v1internal:streamGenerateContent?alt=sse",
                ];
                let mut last_net_err = None;
                let mut got_resp = None;
                for (ep_idx, ep) in endpoints.iter().enumerate() {
                    match client
                        .post(*ep)
                        .bearer_auth(cred.access_token.trim())
                        .header("Content-Type", "application/json")
                        .json(&body)
                        .send()
                        .await
                    {
                        Ok(r) => {
                            if r.status().is_server_error() && ep_idx + 1 < endpoints.len() {
                                continue;
                            }
                            got_resp = Some(r);
                            break;
                        }
                        Err(e) => {
                            last_net_err = Some(e.to_string());
                        }
                    }
                }
                match got_resp {
                    Some(r) => r,
                    None => {
                        return Err((
                            None,
                            last_net_err.unwrap_or_else(|| "请求 Cloud Code 流式接口失败".to_string()),
                        ));
                    }
                }
            }
            other => {
                return Err((None, format!("该服务不支持自动点火: {other}")));
            }
        };

        if resp.status().as_u16() == 401
            && attempt == 0
            && force_refresh_credential_on_401(cred).await
        {
            continue;
        }
        resp_opt = Some(resp);
        break;
    }

    let resp = resp_opt.ok_or_else(|| (None, "点火请求未能完成发送".to_string()))?;
    let status = resp.status();
    if !status.is_success() {
        let code = status.as_u16();
        let err_text = resp.text().await.unwrap_or_default();
        return Err((
            Some(code),
            format!("HTTP {code}: {}", summarize_upstream_error_body(&err_text, 220)),
        ));
    }

    let passive_groups = if cred.provider == "codex" {
        let hdrs: BTreeMap<String, String> = resp
            .headers()
            .iter()
            .filter_map(|(k, v)| {
                Some((k.as_str().to_ascii_lowercase(), v.to_str().ok()?.to_string()))
            })
            .collect();
        parse_passive_response_headers(&hdrs, Utc::now().timestamp()).map(|(_, wins)| {
            vec![RawQuotaGroup {
                raw_group_key: "codex:main".to_string(),
                source_label: "GPT".to_string(),
                windows: wins,
                detected_plan: None,
            }]
        })
    } else {
        None
    };

    // 按跨包行缓冲监听 SSE：命中首个有效生成/结束帧立即关闭连接，不再硬等 2 秒
    let mut stream = resp.bytes_stream();
    let hard_deadline = tokio::time::Instant::now() + Duration::from_secs(12);
    let mut line_buf = String::new();

    loop {
        match tokio::time::timeout_at(hard_deadline, stream.next()).await {
            Ok(Some(Ok(chunk))) => {
                line_buf.push_str(&String::from_utf8_lossy(&chunk));
                let mut should_stop = false;
                while let Some(pos) = line_buf.find('\n') {
                    let line = line_buf[..pos].trim();
                    if let Some(data) = line.strip_prefix("data:").map(str::trim)
                        && sse_data_indicates_stream_active(data)
                    {
                        should_stop = true;
                        break;
                    }
                    line_buf.drain(..=pos);
                }
                if should_stop {
                    break;
                }
            }
            Ok(Some(Err(_))) | Ok(None) | Err(_) => {
                break;
            }
        }
    }

    Ok(passive_groups)
}

/// 判定单个 SSE `data:` 负载是否已表明上游开始生成或完成响应（覆盖 OpenAI / Anthropic / Google Cloud Code 格式）。
pub fn sse_data_indicates_stream_active(data: &str) -> bool {
    let trimmed = data.trim();
    if trimmed.is_empty() {
        return false;
    }
    if trimmed == "[DONE]" {
        return true;
    }
    let Ok(val) = serde_json::from_str::<Value>(trimmed) else {
        return false;
    };
    if val
        .get("delta")
        .and_then(Value::as_str)
        .is_some_and(|s| !s.is_empty())
        || val
            .pointer("/delta/text")
            .and_then(Value::as_str)
            .is_some_and(|s| !s.is_empty())
    {
        return true;
    }
    if val
        .get("type")
        .and_then(Value::as_str)
        .is_some_and(|t| {
            matches!(
                t,
                "response.output_text.delta"
                    | "response.output_text.done"
                    | "response.completed"
                    | "content_block_delta"
                    | "message_delta"
                    | "message_stop"
            )
        })
    {
        return true;
    }
    let has_parts = ["/response/candidates/0/content/parts", "/candidates/0/content/parts"]
        .iter()
        .any(|ptr| {
            val.pointer(ptr)
                .and_then(Value::as_array)
                .is_some_and(|arr| !arr.is_empty())
        });
    if has_parts {
        return true;
    }
    val.pointer("/response/candidates/0/finishReason").is_some()
        || val.pointer("/candidates/0/finishReason").is_some()
        || val.pointer("/response/usageMetadata").is_some()
        || val.get("usageMetadata").is_some()
}

/// 记录指定客户端的自动点火失败并应用 `5m -> 15m -> 次日 07:00` 熔断保护规则（返回是否需要发送熔断告警）。
pub fn record_auto_ignition_failure_for_client(
    cfg: &LamplighterConfig,
    client_id: &str,
    ts: &mut IgnitionTargetState,
    err_msg: &str,
    now: i64,
) -> Option<(String, i64)> {
    let ig_cfg = cfg.ignition_for_client(client_id);
    let lower = err_msg.to_ascii_lowercase();
    // 仅对不可自动恢复的永久权限/凭据吊销错误立即熔断至次日；偶发 401/429/窗口延迟走 5m -> 15m -> 次日阶梯退避
    let is_permanent_hard = [
        "http 403",
        "invalid_grant",
        "account_deactivated",
        "token_revoked",
    ]
    .iter()
    .any(|pat| lower.contains(pat));

    ts.consecutive_failures = ts.consecutive_failures.saturating_add(1);
    let collapsed = err_msg.split_whitespace().collect::<Vec<_>>().join(" ");
    let summary: String = collapsed.chars().take(180).collect();

    if is_permanent_hard || ts.consecutive_failures >= ig_cfg.max_transient_failures {
        let pause_to = next_morning_start_epoch_for(ig_cfg, now);
        ts.paused_until = pause_to;
        ts.paused_reason = Some(summary.clone());
        ts.retry_at = 0;
        if ts.circuit_notified_for != pause_to {
            ts.circuit_notified_for = pause_to;
            return Some((summary, pause_to));
        }
        return None;
    }

    let mult = ig_cfg
        .failure_backoff_multiplier
        .saturating_pow(ts.consecutive_failures.saturating_sub(1));
    let delay = ig_cfg.failure_retry_seconds.saturating_mul(u64::from(mult));
    ts.retry_at = now + delay as i64;
    None
}

pub fn record_auto_ignition_failure(
    cfg: &LamplighterConfig,
    ts: &mut IgnitionTargetState,
    err_msg: &str,
    now: i64,
) -> Option<(String, i64)> {
    record_auto_ignition_failure_for_client(cfg, "codex", ts, err_msg, now)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summarizes_multiline_google_json_401_without_truncating_at_brace() {
        let raw = "{\n  \"error\": {\n    \"code\": 401,\n    \"message\": \"Request had invalid authentication credentials.\"\n  }\n}";
        let summary = summarize_upstream_error_body(raw, 200);
        assert_eq!(summary, "Request had invalid authentication credentials.");
    }

    #[test]
    fn transient_401_uses_step_backoff_before_morning_circuit_break() {
        let cfg = LamplighterConfig::default();
        let mut ts = IgnitionTargetState::default();
        let now = 1_795_396_500;

        // 第 1 次偶发 401：退避重试而非直接暂停至次日
        let first = record_auto_ignition_failure_for_client(
            &cfg,
            "antigravity",
            &mut ts,
            "HTTP 401: Request had invalid authentication credentials.",
            now,
        );
        assert!(first.is_none());
        assert_eq!(ts.paused_until, 0);
        assert!(ts.retry_at > now);

        // 永久 403 / invalid_grant：立即熔断保护
        let hard = record_auto_ignition_failure_for_client(
            &cfg,
            "antigravity",
            &mut ts,
            "HTTP 403: Permission denied",
            now + 60,
        );
        assert!(hard.is_some());
        assert!(ts.paused_until > now + 60);
    }

    #[test]
    fn ranks_fast_lite_and_low_models_first_for_antigravity_groups() {
        let models = vec![
            "claude-opus-5-5-high".to_string(),
            "claude-sonnet-5-5-high".to_string(),
            "claude-sonnet-5-5-low".to_string(),
            "claude-sonnet-5-5-medium".to_string(),
            "gemini-2.5-flash".to_string(),
            "gemini-3-flash".to_string(),
            "gemini-3.5-flash-lite".to_string(),
            "gemini-3.8-flash-high".to_string(),
            "gemini-3.8-flash-low".to_string(),
            "gemini-3.8-flash-tiered".to_string(),
            "gemini-4-argon-high".to_string(),
            "gemini-4-argon-low".to_string(),
            "gemini-4-argon-medium".to_string(),
            "gpt-oss-120b-medium".to_string(),
            "tab_flash_lite_preview".to_string(),
        ];

        let gemini_ranked = rank_antigravity_ignition_models(&models, "Gemini");
        assert_eq!(gemini_ranked.first().map(String::as_str), Some("gemini-3.5-flash-lite"));
        assert!(!gemini_ranked.contains(&"tab_flash_lite_preview".to_string()));
        let lite_pos = gemini_ranked.iter().position(|m| m == "gemini-3.5-flash-lite").unwrap();
        let tiered_pos = gemini_ranked.iter().position(|m| m == "gemini-3.8-flash-tiered").unwrap();
        assert!(lite_pos < tiered_pos);

        let cp_ranked = rank_antigravity_ignition_models(&models, "Claude / GPT");
        assert_eq!(cp_ranked.first().map(String::as_str), Some("claude-sonnet-5-5-low"));
        let sonnet_low_pos = cp_ranked.iter().position(|m| m == "claude-sonnet-5-5-low").unwrap();
        let oss_pos = cp_ranked.iter().position(|m| m == "gpt-oss-120b-medium").unwrap();
        let opus_high_pos = cp_ranked.iter().position(|m| m == "claude-opus-5-5-high").unwrap();
        assert!(sonnet_low_pos < oss_pos && oss_pos < opus_high_pos);

        let argon_ranked = rank_antigravity_ignition_models(&models, "Gemini 4 Argon");
        assert_eq!(argon_ranked.first().map(String::as_str), Some("gemini-4-argon-low"));
    }

    #[test]
    fn sse_stream_activity_detector_handles_openai_anthropic_and_cloudcode_chunks() {
        assert!(!sse_data_indicates_stream_active(""));
        assert!(sse_data_indicates_stream_active("[DONE]"));
        assert!(sse_data_indicates_stream_active(r#"{"delta":"OK"}"#));
        assert!(sse_data_indicates_stream_active(
            r#"{"type":"content_block_delta","delta":{"text":"OK"}}"#
        ));
        assert!(sse_data_indicates_stream_active(
            r#"{"response":{"candidates":[{"content":{"parts":[{"thought":true,"text":""}]}}]}}"#
        ));
        assert!(sse_data_indicates_stream_active(
            r#"{"response":{"candidates":[{"finishReason":"STOP"}]}}"#
        ));
    }
}
