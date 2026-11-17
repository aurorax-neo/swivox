//! Lamplighter 5 小时额度窗口自动点火引擎（`07:00..22:30` 排期、滑动占位检测、24h 被拒模型记忆、首包 2 秒流截断与窗口确认）。

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use anyhow::bail;
use chrono::{Datelike, Local, TimeZone, Utc};
use futures_util::StreamExt;
use serde_json::{Value, json};
use uuid::Uuid;

use super::config::{IgnitionConfig, LamplighterConfig, normalize_client_id};
use super::providers::{UnifiedCredential, build_account_http_client, query_credential_quotas};
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
        "claude" => vec![
            "claude-haiku-4-5-20251001".to_string(),
            "claude-3-5-haiku-latest".to_string(),
            "claude-sonnet-4-5-20250929".to_string(),
        ],
        "antigravity" => {
            if group_label.to_ascii_lowercase().contains("claude") {
                vec![
                    "claude-sonnet-4-6".to_string(),
                    "claude-sonnet-4-5".to_string(),
                    "gpt-oss-120b-medium".to_string(),
                ]
            } else {
                vec![
                    "gemini-2.5-flash".to_string(),
                    "gemini-3-flash".to_string(),
                    "gemini-2.5-pro".to_string(),
                ]
            }
        }
        _ => Vec::new(),
    }
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
            let is_3p = group_label.to_ascii_lowercase().contains("claude");
            let mut flash = Vec::new();
            let mut other = Vec::new();
            for m in &q.available_model_ids {
                let l = m.to_ascii_lowercase();
                if l.contains("image") || l.contains("argon") {
                    continue;
                }
                if is_3p {
                    if (l.contains("claude") || l.contains("gpt")) && !l.contains("thinking") {
                        other.push(m.clone());
                    }
                } else if l.contains("gemini") {
                    if l.contains("flash") {
                        flash.push(m.clone());
                    } else {
                        other.push(m.clone());
                    }
                }
            }
            sort_models_newest_first(&mut flash);
            sort_models_newest_first(&mut other);
            candidates.extend(flash);
            candidates.extend(other);
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
}

/// 对指定账号与额度组执行一次真实最小流式点火（收到首包非空文本后最多读 2 秒即主动关闭流，随后确认新 5h 重置时间）。
pub async fn execute_ignition_once(
    cfg: &LamplighterConfig,
    cred: &mut UnifiedCredential,
    group_key: &str,
    group_label: &str,
) -> anyhow::Result<IgnitionExecutionResult> {
    let candidates = resolve_ignition_candidates(cfg, cred, group_key, group_label).await;
    if candidates.is_empty() {
        bail!("未找到可用的点火候选模型。");
    }

    let start_ts = Utc::now().timestamp();
    let mut chosen_model = None;
    let mut last_send_err = None;

    for (idx, model) in candidates.iter().take(2).enumerate() {
        match send_minimal_streaming_prompt(cred, model).await {
            Ok(()) => {
                chosen_model = Some(model.clone());
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

    // 按 [4s, 8s, 12s] 轮询确认该组 5h 窗口产生了固定的未来 reset_at
    for wait_secs in [4u64, 8, 12] {
        tokio::time::sleep(Duration::from_secs(wait_secs)).await;
        if let Ok(groups) = query_credential_quotas(cred, cfg.request_timeout_seconds).await {
            let now = Utc::now().timestamp();
            for grp in groups {
                if grp.source_label == group_label || candidates.len() == 1 || cred.provider != "antigravity" {
                    if let Some(w5) = grp.windows.iter().find(|w| w.is_five_hour)
                        && let Some(rst) = w5.reset_at
                        && rst > now
                    {
                        return Ok(IgnitionExecutionResult {
                            model_used,
                            confirmed_reset_at: rst,
                        });
                    }
                }
            }
        }
    }

    bail!("点火请求已发出（{model_used}），但尚未确认到新的 5 小时窗口重置时间。")
}

async fn send_minimal_streaming_prompt(
    cred: &UnifiedCredential,
    model: &str,
) -> Result<(), (Option<u16>, String)> {
    let resp = match cred.provider.as_str() {
        "codex" => {
            let ua = "codex-tui/0.154.0 (Mac OS 26.5.2; arm64) Swivox-Lamplighter";
            let client = build_account_http_client(cred, 45, ua)
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
                45,
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
            client
                .post("https://daily-cloudcode-pa.googleapis.com/v1internal:streamGenerateContent?alt=sse")
                .bearer_auth(cred.access_token.trim())
                .header("Content-Type", "application/json")
                .json(&body)
                .send()
                .await
                .map_err(|e| (None, e.to_string()))?
        }
        "claude" => {
            let ua = "claude-cli/2.1.280 (external, cli)";
            let client = build_account_http_client(cred, 45, ua)
                .map_err(|e| (None, e.to_string()))?;
            client
                .post("https://api.anthropic.com/v1/messages")
                .bearer_auth(cred.access_token.trim())
                .header("anthropic-beta", "oauth-2025-04-20")
                .header("anthropic-version", "2023-06-01")
                .header("Content-Type", "application/json")
                .json(&json!({
                    "model": model,
                    "max_tokens": 16,
                    "stream": true,
                    "messages": [{ "role": "user", "content": IGNITION_TRIGGER_PROMPT }]
                }))
                .send()
                .await
                .map_err(|e| (None, e.to_string()))?
        }
        other => {
            return Err((None, format!("该服务不支持自动点火: {other}")));
        }
    };

    let status = resp.status();
    if !status.is_success() {
        let code = status.as_u16();
        let err_text = resp.text().await.unwrap_or_default();
        return Err((
            Some(code),
            format!("HTTP {code}: {}", err_text.chars().take(240).collect::<String>()),
        ));
    }

    // 流式读取：捕获首个非空输出后计时 2 秒立即主动关闭连接（不耗费多余额度）
    let mut stream = resp.bytes_stream();
    let mut first_token_deadline: Option<tokio::time::Instant> = None;
    let hard_deadline = tokio::time::Instant::now() + Duration::from_secs(30);

    loop {
        let wait_until = first_token_deadline.unwrap_or(hard_deadline).min(hard_deadline);
        match tokio::time::timeout_at(wait_until, stream.next()).await {
            Ok(Some(Ok(chunk))) => {
                let s = String::from_utf8_lossy(&chunk);
                if first_token_deadline.is_none() && sse_chunk_has_visible_output(&s) {
                    first_token_deadline =
                        Some(tokio::time::Instant::now() + Duration::from_secs(2));
                }
                if s.contains("[DONE]") || s.contains("response.completed") || s.contains("message_stop") {
                    break;
                }
            }
            Ok(Some(Err(_))) | Ok(None) | Err(_) => {
                // 2 秒截断或流自然结束均视为正常收尾
                break;
            }
        }
    }

    Ok(())
}

fn sse_chunk_has_visible_output(chunk: &str) -> bool {
    for line in chunk.lines() {
        let Some(data) = line.strip_prefix("data:").map(str::trim) else {
            continue;
        };
        if data.is_empty() || data == "[DONE]" {
            continue;
        }
        if let Ok(val) = serde_json::from_str::<Value>(data) {
            if val.get("delta").and_then(Value::as_str).is_some_and(|s| !s.is_empty())
                || val
                    .pointer("/delta/text")
                    .and_then(Value::as_str)
                    .is_some_and(|s| !s.is_empty())
                || val
                    .pointer("/response/candidates/0/content/parts/0/text")
                    .and_then(Value::as_str)
                    .is_some_and(|s| !s.is_empty())
            {
                return true;
            }
        }
    }
    false
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
    let is_hard = ["http 400", "http 401", "http 403", "http 404", "http 429", "尚未确认"]
        .iter()
        .any(|pat| lower.contains(pat));

    ts.consecutive_failures = ts.consecutive_failures.saturating_add(1);
    let summary: String = err_msg.lines().next().unwrap_or(err_msg).chars().take(160).collect();

    if is_hard || ts.consecutive_failures >= ig_cfg.max_transient_failures {
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
