//! Lamplighter（点灯人）核心协调引擎：多服务额度监控、5h 窗口自动点火、趋势曲线、Bark/Webhook 提醒与本地 `/mcp` 服务。

pub mod config;
pub mod ignite;
pub mod mcp;
pub mod notify;
pub mod providers;
pub mod store;

use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use anyhow::{Context, bail};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub use config::{
    DEFAULT_LAMPLIGHTER_MCP_LISTEN, IgnitionConfig, LamplighterConfig, ProviderRuleConfig,
    WebhookConfig, WindowModes, canonical_provider_id, generate_mcp_bearer_key,
    plan_display_and_default_weight, provider_display_title, provider_supports_ignition,
};
pub use store::{
    ExternalLamplighterAccount, HistorySamplePoint, HistorySeries, LamplighterEventRecord,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitoredGroupView {
    pub group_key: String,
    pub account_key: String,
    pub provider: String,
    pub account_email: String,
    pub source_kind: String,
    pub display_label: String,
    pub group_title: String,
    pub plan_name: String,
    pub weight: f64,
    pub custom_weight: bool,
    pub windows: Vec<providers::QuotaWindowReading>,
    pub ignite_supported: bool,
    pub ignite_enabled: bool,
    pub seven_day_exhausted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_ignition_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_ignition_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_ignition_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paused_reason: Option<String>,
    pub last_updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LamplighterSnapshot {
    pub config: LamplighterConfig,
    pub groups: Vec<MonitoredGroupView>,
    pub external_accounts: Vec<RedactedExternalAccount>,
    pub history_series: Vec<HistorySeries>,
    pub events: Vec<LamplighterEventRecord>,
    pub last_poll_at: i64,
    pub next_poll_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RedactedExternalAccount {
    pub id: String,
    pub provider: String,
    pub email: String,
    pub label: Option<String>,
    pub domain: Option<String>,
    pub proxy_url: Option<String>,
}

#[derive(Default)]
struct RuntimeCache {
    groups: Vec<MonitoredGroupView>,
    last_poll_at: i64,
    next_poll_at: i64,
    last_passive_at: BTreeMap<String, i64>,
    last_active_at: BTreeMap<String, i64>,
}

static RUNTIME_CACHE: OnceLock<Mutex<RuntimeCache>> = OnceLock::new();
static ENGINE_STARTED: OnceLock<Mutex<bool>> = OnceLock::new();

fn runtime_cache() -> &'static Mutex<RuntimeCache> {
    RUNTIME_CACHE.get_or_init(|| Mutex::new(RuntimeCache::default()))
}

fn severity_for_remaining(cfg: &LamplighterConfig, remaining: f64) -> u8 {
    if remaining <= 0.01 {
        4
    } else if remaining <= cfg.critical_threshold {
        3
    } else if remaining <= cfg.low_threshold {
        2
    } else if remaining <= cfg.notice_threshold {
        1
    } else {
        0
    }
}

/// 根据磁盘缓存与内存快照组装即时可读的 `LamplighterSnapshot`（不发起网络请求，毫秒级返回）。
pub fn get_lamplighter_snapshot_with_hours(history_hours: u64) -> LamplighterSnapshot {
    let cfg = store::load_config();
    let state = store::load_state();
    let now = Utc::now().timestamp();

    let creds = providers::collect_unified_credentials(&cfg);
    let cached_groups = runtime_cache()
        .lock()
        .ok()
        .map(|c| (c.groups.clone(), c.last_poll_at, c.next_poll_at))
        .unwrap_or_default();

    let mut groups = cached_groups.0;
    let mut label_map: BTreeMap<String, (String, String, f64)> = BTreeMap::new();

    // 若引擎尚未完成首轮网络轮询，直接利用 state.json 或 Swivox 已有缓存快照即时渲染账号卡片
    if groups.is_empty() {
        for (cred, disp_label) in &creds {
            let cached_plan = state
                .plans
                .get(&cred.account_key)
                .map(|p| p.plan.clone())
                .or_else(|| cred.cached_plan.clone());
            let (plan_name, weight, custom_weight) = providers::resolve_account_plan_and_weight(
                &cfg,
                &cred.account_key,
                &cred.provider,
                cached_plan.as_deref(),
            );
            let rule = cfg.provider_rule(&cred.provider);
            let gkey = format!("{}:{}:{}:main", cred.provider, cred.account_key, cred.provider);
            let mut wins = Vec::new();
            if cred.source_kind == "swivox_codex" {
                let official = crate::official_accounts::load_official_accounts_state();
                if let Some(acc) = official.accounts.iter().find(|a| a.id == cred.raw_account_id)
                    && let Some(ref q) = acc.quota
                {
                    if q.hourly_window_present != Some(false) {
                        wins.push(providers::QuotaWindowReading {
                            id: "five-hour".to_string(),
                            short_label: "5h".to_string(),
                            display_label: "5 小时".to_string(),
                            is_five_hour: true,
                            remaining_percent: f64::from(q.hourly_percentage).clamp(0.0, 100.0),
                            reset_at: q.hourly_reset_time,
                        });
                    }
                    if q.weekly_window_present != Some(false) {
                        wins.push(providers::QuotaWindowReading {
                            id: "seven-day".to_string(),
                            short_label: "7d".to_string(),
                            display_label: "7 天".to_string(),
                            is_five_hour: false,
                            remaining_percent: f64::from(q.weekly_percentage).clamp(0.0, 100.0),
                            reset_at: q.weekly_reset_time,
                        });
                    }
                }
            } else if cred.source_kind == "swivox_ag" {
                let ag = crate::antigravity::load_antigravity_state();
                if let Some(acc) = ag.accounts.iter().find(|a| a.id == cred.raw_account_id)
                    && let Some(ref q) = acc.quota
                {
                    for grp in &q.groups {
                        let mut gwins = Vec::new();
                        if let Some(p5) = grp.five_hour_percentage {
                            gwins.push(providers::QuotaWindowReading {
                                id: "five-hour".to_string(),
                                short_label: "5h".to_string(),
                                display_label: "5 小时".to_string(),
                                is_five_hour: true,
                                remaining_percent: p5,
                                reset_at: grp.five_hour_reset_time,
                            });
                        }
                        if let Some(p7) = grp.weekly_percentage {
                            gwins.push(providers::QuotaWindowReading {
                                id: "seven-day".to_string(),
                                short_label: "7d".to_string(),
                                display_label: "7 天".to_string(),
                                is_five_hour: false,
                                remaining_percent: p7,
                                reset_at: grp.weekly_reset_time,
                            });
                        }
                        let sub_gkey = format!(
                            "antigravity:{}:antigravity:{}",
                            cred.account_key, grp.group_id
                        );
                        let sub_lbl = format!("{disp_label} · {}", grp.display_name);
                        let ignite_allowed = rule.ignite
                            && (rule.groups.is_empty()
                                || rule.groups.iter().any(|g| g.eq_ignore_ascii_case(&grp.display_name)));
                        let ts = state.scheduler.get(&sub_gkey).cloned().unwrap_or_default();
                        let reset_5h = gwins.iter().find(|w| w.is_five_hour).and_then(|w| w.reset_at);
                        let next_ig = if ignite_allowed {
                            ignite::compute_next_ignition_due(&cfg, &ts, reset_5h, now)
                        } else {
                            None
                        };
                        label_map.insert(
                            sub_gkey.clone(),
                            (cred.provider.clone(), sub_lbl.clone(), weight),
                        );
                        groups.push(MonitoredGroupView {
                            group_key: sub_gkey,
                            account_key: cred.account_key.clone(),
                            provider: cred.provider.clone(),
                            account_email: cred.email.clone(),
                            source_kind: cred.source_kind.to_string(),
                            display_label: sub_lbl,
                            group_title: grp.display_name.clone(),
                            plan_name: plan_name.clone(),
                            weight,
                            custom_weight,
                            windows: gwins,
                            ignite_supported: true,
                            ignite_enabled: ignite_allowed,
                            seven_day_exhausted: grp.weekly_exhausted,
                            next_ignition_at: next_ig,
                            last_ignition_at: (ts.last_success_at > 0).then_some(ts.last_success_at),
                            last_ignition_model: ts.last_model,
                            paused_reason: ts.paused_reason,
                            last_updated_at: q.updated_at,
                        });
                    }
                    continue;
                }
            }

            let ignite_sup = provider_supports_ignition(&cred.provider)
                && wins.iter().any(|w| w.is_five_hour);
            let ignite_en = ignite_sup && rule.ignite;
            let ts = state.scheduler.get(&gkey).cloned().unwrap_or_default();
            let reset_5h = wins.iter().find(|w| w.is_five_hour).and_then(|w| w.reset_at);
            let seven_exh = wins
                .iter()
                .any(|w| !w.is_five_hour && w.remaining_percent <= 0.01);
            let next_ig = if ignite_en && !seven_exh {
                ignite::compute_next_ignition_due(&cfg, &ts, reset_5h, now)
            } else {
                None
            };
            label_map.insert(
                gkey.clone(),
                (cred.provider.clone(), disp_label.clone(), weight),
            );
            groups.push(MonitoredGroupView {
                group_key: gkey,
                account_key: cred.account_key.clone(),
                provider: cred.provider.clone(),
                account_email: cred.email.clone(),
                source_kind: cred.source_kind.to_string(),
                display_label: disp_label.clone(),
                group_title: provider_display_title(&cred.provider).to_string(),
                plan_name,
                weight,
                custom_weight,
                windows: wins,
                ignite_supported: ignite_sup,
                ignite_enabled: ignite_en,
                seven_day_exhausted: seven_exh,
                next_ignition_at: next_ig,
                last_ignition_at: (ts.last_success_at > 0).then_some(ts.last_success_at),
                last_ignition_model: ts.last_model,
                paused_reason: ts.paused_reason,
                last_updated_at: state.groups.get(&cred.account_key).map(|g| g.last_seen).unwrap_or(0),
            });
        }
    } else {
        for g in &groups {
            label_map.insert(
                g.group_key.clone(),
                (g.provider.clone(), g.display_label.clone(), g.weight),
            );
        }
    }

    let (history_series, events) = store::query_history_and_events(history_hours, None, &label_map);
    let external_accounts = store::load_external_accounts()
        .into_iter()
        .map(|a| RedactedExternalAccount {
            id: a.id,
            provider: a.provider,
            email: a.email,
            label: a.label,
            domain: a.domain,
            proxy_url: a.proxy_url,
        })
        .collect();

    let poll_int = cfg.poll_interval_seconds.max(60) as i64;
    let next_poll = if cached_groups.2 > now {
        cached_groups.2
    } else {
        ((now / poll_int) + 1) * poll_int
    };

    LamplighterSnapshot {
        config: cfg,
        groups,
        external_accounts,
        history_series,
        events,
        last_poll_at: cached_groups.1,
        next_poll_at: next_poll,
    }
}

pub fn get_lamplighter_snapshot() -> LamplighterSnapshot {
    get_lamplighter_snapshot_with_hours(12)
}

/// 立即主动刷新全部监控账号的配额、评估阈值/恢复/套餐变化提醒并刷新自动点火计划。
pub async fn refresh_all_now() -> anyhow::Result<LamplighterSnapshot> {
    run_poll_and_ignition_cycle(true).await?;
    Ok(get_lamplighter_snapshot())
}

/// 手动点击「立即点火」：对指定 `group_key` 立即发送最小点火请求（失败不计入自动熔断计数，成功则恢复熔断）。
pub async fn manual_ignite_group(group_key: &str) -> anyhow::Result<String> {
    let cfg = store::load_config();
    let creds = providers::collect_unified_credentials(&cfg);
    let mut state = store::load_state();

    let (mut target_cred, disp_label) = creds
        .into_iter()
        .find(|(c, _)| group_key.contains(&format!(":{}:", c.account_key)))
        .context("未找到该额度组对应的账号凭据")?;

    let group_label = if group_key.ends_with("claude_gpt") {
        "Claude / GPT"
    } else if target_cred.provider == "antigravity" {
        "Gemini"
    } else {
        provider_display_title(&target_cred.provider)
    };

    let now = Utc::now().timestamp();
    match ignite::execute_ignition_once(&cfg, &mut target_cred, group_key, group_label).await {
        Ok(res) => {
            let ts = state.scheduler.entry(group_key.to_string()).or_default();
            ts.last_success_at = now;
            ts.last_model = Some(res.model_used.clone());
            ts.consecutive_failures = 0;
            ts.retry_at = 0;
            ts.paused_until = 0;
            ts.paused_reason = None;
            ts.rolling_reset = false;
            ts.last_reset_seen = res.confirmed_reset_at;
            ts.last_seen_at = now;
            let _ = store::save_state(&state);

            let clock = notify::format_local_mmdd_hhmm(res.confirmed_reset_at);
            store::append_event_record(&LamplighterEventRecord {
                timestamp: now,
                group_key: group_key.to_string(),
                kind: "ignite".to_string(),
                level: "info".to_string(),
                label: disp_label.clone(),
                message: format!("{disp_label} 手动点火成功（{}），下次重置 {clock}", res.model_used),
                detail: Some(format!("model={} · reset={clock}", res.model_used)),
            });

            let _ = run_poll_and_ignition_cycle(true).await;
            Ok(format!(
                "{disp_label} 点火成功（模型：{}），新 5 小时窗口将于 {clock} 重置。",
                res.model_used
            ))
        }
        Err(err) => {
            store::append_event_record(&LamplighterEventRecord {
                timestamp: now,
                group_key: group_key.to_string(),
                kind: "ignite_failed".to_string(),
                level: "error".to_string(),
                label: disp_label.clone(),
                message: format!("{disp_label} 手动点火失败：{}", err),
                detail: None,
            });
            Err(err)
        }
    }
}

/// 发送一条测试通知以验证 Bark 或通用 Webhook 配置是否可用。
pub async fn send_test_notification(cfg_override: Option<LamplighterConfig>) -> anyhow::Result<String> {
    let mut cfg = cfg_override.unwrap_or_else(store::load_config);
    cfg.normalize();
    if !cfg.notifications_configured() {
        bail!("请先填写 Bark 推送地址或通用 Webhook 地址。");
    }
    let msg = notify::OutboundNotification {
        kind: "test".to_string(),
        priority: "normal",
        title: "🕯️ Swivox Lamplighter 测试通知".to_string(),
        body: "点灯人额度监控与自动点火推送通道正常。".to_string(),
        jump_url: None,
    };
    notify::send_notification(&cfg, &msg).await?;
    Ok("测试推送已成功送达。".to_string())
}

/// 添加或更新独立监控凭据（Claude / Kimi / Grok 等）。
pub fn upsert_external_account(
    provider: String,
    email: String,
    label: Option<String>,
    access_token: String,
    refresh_token: Option<String>,
    domain: Option<String>,
    proxy_url: Option<String>,
) -> anyhow::Result<()> {
    if email.trim().is_empty() || access_token.trim().is_empty() {
        bail!("账号标识与 Access Token 不能为空。");
    }
    let prov = canonical_provider_id(&provider).to_string();
    let mut list = store::load_external_accounts();
    if let Some(existing) = list
        .iter_mut()
        .find(|a| a.provider == prov && a.email.eq_ignore_ascii_case(email.trim()))
    {
        existing.label = label.filter(|s| !s.trim().is_empty());
        existing.access_token = access_token.trim().to_string();
        existing.refresh_token = refresh_token.filter(|s| !s.trim().is_empty());
        existing.domain = domain.filter(|s| !s.trim().is_empty());
        existing.proxy_url = proxy_url.filter(|s| !s.trim().is_empty());
    } else {
        list.push(ExternalLamplighterAccount {
            id: Uuid::new_v4().to_string(),
            provider: prov,
            email: email.trim().to_string(),
            label: label.filter(|s| !s.trim().is_empty()),
            access_token: access_token.trim().to_string(),
            refresh_token: refresh_token.filter(|s| !s.trim().is_empty()),
            domain: domain.filter(|s| !s.trim().is_empty()),
            proxy_url: proxy_url.filter(|s| !s.trim().is_empty()),
            created_at: Utc::now().timestamp(),
        });
    }
    store::save_external_accounts(&list)
}

pub fn delete_external_account(id: &str) -> anyhow::Result<()> {
    let mut list = store::load_external_accounts();
    list.retain(|a| a.id != id);
    store::save_external_accounts(&list)
}

/// 供 `protocol_proxy.rs` 异步非阻塞调用的被动响应头钩子（队列绝不阻塞主请求流）。
pub fn observe_passive_headers(headers: &BTreeMap<String, String>, requested_at: i64) {
    let Some((provider, windows)) =
        providers::parse_passive_response_headers(headers, requested_at)
    else {
        return;
    };
    let cfg = store::load_config();
    if !cfg.enabled {
        return;
    }
    let creds = providers::collect_unified_credentials(&cfg);
    let Some((cred, _)) = creds.into_iter().find(|(c, _)| c.provider == provider) else {
        return;
    };
    let gkey = format!("{provider}:{}:{provider}:main", cred.account_key);
    let now = Utc::now().timestamp();
    for w in &windows {
        store::append_history_sample(now, &gkey, &w.id, w.remaining_percent, w.reset_at, "passive");
    }
    if let Ok(mut cache) = runtime_cache().lock() {
        cache.last_passive_at.insert(cred.account_key, now);
    }
}

async fn run_poll_and_ignition_cycle(force_active: bool) -> anyhow::Result<()> {
    let cfg = store::load_config();
    if !cfg.enabled && !force_active {
        return Ok(());
    }

    let mut state = store::load_state();
    let creds = providers::collect_unified_credentials(&cfg);
    let now = Utc::now().timestamp();
    let mut updated_views: Vec<MonitoredGroupView> = Vec::new();
    let mut pending_notifications: Vec<notify::OutboundNotification> = Vec::new();

    for (mut cred, base_label) in creds {
        // 被动跳过判断
        if !force_active && matches!(cred.provider.as_str(), "codex" | "claude") && cfg.passive_skip_seconds > 0 {
            let skip = runtime_cache()
                .lock()
                .ok()
                .is_some_and(|c| {
                    let lp = c.last_passive_at.get(&cred.account_key).copied().unwrap_or(0);
                    let la = c.last_active_at.get(&cred.account_key).copied().unwrap_or(0);
                    now - lp <= cfg.passive_skip_seconds as i64
                        && now - la < (cfg.passive_skip_max_minutes as i64 * 60)
                });
            if skip {
                continue;
            }
        }

        let rule = cfg.provider_rule(&cred.provider);
        match providers::query_credential_quotas(&mut cred, cfg.request_timeout_seconds).await {
            Ok(raw_groups) => {
                if let Ok(mut cache) = runtime_cache().lock() {
                    cache.last_active_at.insert(cred.account_key.clone(), now);
                }

                // 恢复非点火确认的查询故障态
                if let Some(ct) = state.credentials.get_mut(&cred.account_key) {
                    ct.consecutive_query_failures = 0;
                    if !ct.problem_kind.is_empty() && !ct.from_ignition {
                        if ct.notified {
                            pending_notifications.push(notify::OutboundNotification {
                                kind: "credential_ok".to_string(),
                                priority: "normal",
                                title: format!("✅ {base_label} 凭证已恢复"),
                                body: "额度查询已恢复正常。".to_string(),
                                jump_url: None,
                            });
                        }
                        *ct = store::CredentialTrackState::default();
                    }
                }

                // 套餐变化检测
                let detected_raw_plan = raw_groups
                    .iter()
                    .find_map(|g| g.detected_plan.clone())
                    .or_else(|| cred.cached_plan.clone());
                let (plan_name, weight, custom_weight) = providers::resolve_account_plan_and_weight(
                    &cfg,
                    &cred.account_key,
                    &cred.provider,
                    detected_raw_plan.as_deref(),
                );
                let mut plan_rebased = false;
                if !plan_name.is_empty() {
                    let pt = state.plans.entry(cred.account_key.clone()).or_default();
                    if !pt.plan.is_empty() && pt.plan != plan_name {
                        let old_plan = pt.plan.clone();
                        plan_rebased = true;
                        store::append_event_record(&LamplighterEventRecord {
                            timestamp: now,
                            group_key: cred.account_key.clone(),
                            kind: "plan_change".to_string(),
                            level: "info".to_string(),
                            label: base_label.clone(),
                            message: format!("{base_label} 套餐变更：{old_plan} → {plan_name}"),
                            detail: None,
                        });
                        pending_notifications.push(notify::OutboundNotification {
                            kind: "plan".to_string(),
                            priority: "high",
                            title: format!("🎫 {base_label} · 套餐 {old_plan} → {plan_name}"),
                            body: "该账号的额度提醒基线已按新套餐重新开始。".to_string(),
                            jump_url: None,
                        });
                    }
                    pt.plan = plan_name.clone();
                    pt.checked_at = now;
                }

                for rg in raw_groups {
                    let gkey = format!("{}:{}:{}", cred.provider, cred.account_key, rg.raw_group_key);
                    let full_label = if cred.provider == "antigravity" {
                        format!("{base_label} · {}", rg.source_label)
                    } else {
                        base_label.clone()
                    };

                    // 写入历史曲线样本并执行告警状态机
                    let gs = state.groups.entry(gkey.clone()).or_default();
                    gs.label = full_label.clone();
                    gs.last_seen = now;
                    gs.active_windows = Some(rg.windows.iter().map(|w| w.id.clone()).collect());

                    let all_lines: Vec<String> = rg
                        .windows
                        .iter()
                        .map(|w| {
                            notify::format_window_summary_line(
                                &w.short_label,
                                w.remaining_percent,
                                w.reset_at,
                                now,
                                false,
                            )
                        })
                        .collect();

                    for w in &rg.windows {
                        store::append_history_sample(
                            now,
                            &gkey,
                            &w.id,
                            w.remaining_percent,
                            w.reset_at,
                            "active",
                        );
                        let sev = severity_for_remaining(&cfg, w.remaining_percent);
                        let is_first = !gs.windows.contains_key(&w.id);
                        let ws = gs.windows.entry(w.id.clone()).or_default();

                        if is_first || ws.rebase || plan_rebased {
                            ws.remaining = w.remaining_percent;
                            ws.reset = w.reset_at.unwrap_or(0);
                            ws.read_at = now;
                            ws.notified_severity = sev;
                            ws.exhausted = sev >= 4;
                            ws.rebase = false;
                            continue;
                        }

                        // 防重置后短暂回跳（Bounced）
                        if let Some(rst) = w.reset_at
                            && ws.ended_reset > 0
                            && (rst - ws.ended_reset).abs() <= 10
                            && rst > now
                        {
                            ws.bounce_readings += 1;
                            if ws.bounce_since == 0 {
                                ws.bounce_since = now;
                            }
                            if ws.bounce_readings >= 3 && now - ws.bounce_since >= 1800 {
                                ws.ended_reset = 0;
                                ws.notified_severity = sev;
                            }
                            continue;
                        }
                        ws.bounce_readings = 0;
                        ws.bounce_since = 0;

                        // 检测窗口周期重置恢复
                        let prev_rst = ws.reset;
                        let new_rst = w.reset_at.unwrap_or(0);
                        let cycle_reset = (prev_rst > 0 && new_rst > prev_rst + 60)
                            || (ws.remaining + 20.0 < w.remaining_percent && w.remaining_percent >= 95.0);

                        if cycle_reset {
                            let prev_rem = ws.remaining;
                            let was_exhausted = ws.exhausted;
                            ws.ended_reset = prev_rst;
                            ws.notified_severity = sev;
                            ws.exhausted = sev >= 4;

                            let rec_mode = if w.is_five_hour {
                                cfg.recovery_notify.five_hour.as_str()
                            } else {
                                cfg.recovery_notify.seven_day.as_str()
                            };
                            let should_notify = match rec_mode {
                                "all" => prev_rem < 99.5,
                                "after_exhausted" => was_exhausted,
                                _ => false,
                            };
                            if should_notify {
                                let mut body_lines = vec![format!(
                                    "上周期剩余：{} {:.0}%",
                                    w.short_label, prev_rem
                                )];
                                body_lines.extend(all_lines.clone());
                                pending_notifications.push(notify::OutboundNotification {
                                    kind: "recovery".to_string(),
                                    priority: "normal",
                                    title: format!("✅ {full_label} · {} 已恢复", w.short_label),
                                    body: body_lines.join("\n"),
                                    jump_url: None,
                                });
                            }
                        } else if sev > ws.notified_severity {
                            ws.notified_severity = sev;
                            if sev >= 4 {
                                ws.exhausted = true;
                            }
                            let emoji = if sev == 1 { "🟡" } else { "🔴" };
                            let prio = if sev >= 3 { "high" } else { "normal" };
                            let dur = w
                                .reset_at
                                .map(|r| format!(" | {}", notify::compact_duration_label(r - now, false)))
                                .unwrap_or_default();
                            pending_notifications.push(notify::OutboundNotification {
                                kind: "quota".to_string(),
                                priority: prio,
                                title: format!(
                                    "{emoji} {full_label} · {} {:.0}%{dur}",
                                    w.short_label, w.remaining_percent
                                ),
                                body: all_lines.join("\n"),
                                jump_url: None,
                            });
                        }

                        // 重置前提醒（5h 提前 1 小时，7d 提前 24 小时）
                        if let Some(rst) = w.reset_at.filter(|r| *r > now) {
                            let lead = if w.is_five_hour { 3600 } else { 86_400 };
                            let rem_mode = if w.is_five_hour {
                                cfg.reset_reminder.five_hour.as_str()
                            } else {
                                cfg.reset_reminder.seven_day.as_str()
                            };
                            let remind_ok = match rem_mode {
                                "all" => true,
                                "has_remaining" => w.remaining_percent > cfg.critical_threshold,
                                _ => false,
                            };
                            if remind_ok
                                && rst - now <= lead
                                && (ws.reset_notice_sent_for - rst).abs() > 60
                            {
                                ws.reset_notice_sent_for = rst;
                                pending_notifications.push(notify::OutboundNotification {
                                    kind: "reminder".to_string(),
                                    priority: "normal",
                                    title: format!("⏰ {full_label} · {} 重置提醒", w.short_label),
                                    body: all_lines.join("\n"),
                                    jump_url: None,
                                });
                            }
                        }

                        ws.remaining = w.remaining_percent;
                        ws.reset = new_rst;
                        ws.read_at = now;
                    }

                    // 更新并评估 5h 自动点火计划
                    let w5 = rg.windows.iter().find(|w| w.is_five_hour);
                    let seven_exhausted = rg
                        .windows
                        .iter()
                        .any(|w| !w.is_five_hour && w.remaining_percent <= 0.01);
                    let ignite_supported = provider_supports_ignition(&cred.provider) && w5.is_some();
                    let ignite_enabled = ignite_supported
                        && rule.ignite
                        && (cred.provider != "antigravity"
                            || rule.groups.is_empty()
                            || rule
                                .groups
                                .iter()
                                .any(|g| g.eq_ignore_ascii_case(&rg.source_label)));

                    let ts = state.scheduler.entry(gkey.clone()).or_default();
                    if let Some(win5) = w5 {
                        ignite::observe_five_hour_reset(ts, win5.reset_at, now, false);
                    }

                    let mut next_due = if ignite_enabled && !seven_exhausted {
                        ignite::compute_next_ignition_due(
                            &cfg,
                            ts,
                            w5.and_then(|w| w.reset_at),
                            now,
                        )
                    } else {
                        None
                    };

                    // 若当前已到自动点火时刻，立即执行自动点火！
                    if let Some(due) = next_due
                        && due <= now
                        && cfg.ignition.enabled
                    {
                        match ignite::execute_ignition_once(
                            &cfg,
                            &mut cred,
                            &gkey,
                            &rg.source_label,
                        )
                        .await
                        {
                            Ok(res) => {
                                ts.last_success_at = now;
                                ts.last_model = Some(res.model_used.clone());
                                ts.consecutive_failures = 0;
                                ts.retry_at = 0;
                                ts.paused_until = 0;
                                ts.paused_reason = None;
                                ts.rolling_reset = false;
                                ts.last_reset_seen = res.confirmed_reset_at;
                                ts.last_seen_at = now;
                                next_due = ignite::compute_next_ignition_due(
                                    &cfg,
                                    ts,
                                    Some(res.confirmed_reset_at),
                                    now,
                                );
                                let clock = notify::format_local_mmdd_hhmm(res.confirmed_reset_at);
                                store::append_event_record(&LamplighterEventRecord {
                                    timestamp: now,
                                    group_key: gkey.clone(),
                                    kind: "ignite".to_string(),
                                    level: "info".to_string(),
                                    label: full_label.clone(),
                                    message: format!(
                                        "{full_label} 自动点火成功（{}），下次重置 {clock}",
                                        res.model_used
                                    ),
                                    detail: None,
                                });
                            }
                            Err(err) => {
                                let err_s = err.to_string();
                                store::append_event_record(&LamplighterEventRecord {
                                    timestamp: now,
                                    group_key: gkey.clone(),
                                    kind: "ignite_failed".to_string(),
                                    level: "warn".to_string(),
                                    label: full_label.clone(),
                                    message: format!("{full_label} 自动点火失败：{err_s}"),
                                    detail: None,
                                });
                                if let Some((summary, pause_to)) =
                                    ignite::record_auto_ignition_failure(&cfg, ts, &err_s, now)
                                {
                                    let clock = notify::format_local_mmdd_hhmm(pause_to);
                                    pending_notifications.push(notify::OutboundNotification {
                                        kind: "circuit".to_string(),
                                        priority: "high",
                                        title: format!("⚠️ {full_label} 自动点火已暂停"),
                                        body: format!(
                                            "连续或高风险错误触发保护，暂停自动重试至 {clock}。\n{summary}"
                                        ),
                                        jump_url: None,
                                    });
                                }
                                next_due = ignite::compute_next_ignition_due(
                                    &cfg,
                                    ts,
                                    w5.and_then(|w| w.reset_at),
                                    now,
                                );
                            }
                        }
                    }

                    updated_views.push(MonitoredGroupView {
                        group_key: gkey,
                        account_key: cred.account_key.clone(),
                        provider: cred.provider.clone(),
                        account_email: cred.email.clone(),
                        source_kind: cred.source_kind.to_string(),
                        display_label: full_label,
                        group_title: rg.source_label,
                        plan_name: plan_name.clone(),
                        weight,
                        custom_weight,
                        windows: rg.windows,
                        ignite_supported,
                        ignite_enabled,
                        seven_day_exhausted: seven_exhausted,
                        next_ignition_at: next_due,
                        last_ignition_at: (ts.last_success_at > 0).then_some(ts.last_success_at),
                        last_ignition_model: ts.last_model.clone(),
                        paused_reason: ts.paused_reason.clone(),
                        last_updated_at: now,
                    });
                }
            }
            Err(err) => {
                let msg = err.to_string();
                let lower = msg.to_ascii_lowercase();
                if lower.contains("401") || lower.contains("invalid_grant") || lower.contains("unauthorized") {
                    let ct = state.credentials.entry(cred.account_key.clone()).or_default();
                    ct.consecutive_query_failures += 1;
                    if ct.first_failure_at == 0 {
                        ct.first_failure_at = now;
                    }
                    if ct.consecutive_query_failures >= 3
                        && now - ct.first_failure_at >= 1800
                        && !ct.notified
                    {
                        ct.problem_kind = "auth".to_string();
                        ct.summary = msg.chars().take(160).collect();
                        ct.fingerprint = cred.credential_fingerprint();
                        ct.notified = true;
                        pending_notifications.push(notify::OutboundNotification {
                            kind: "credential".to_string(),
                            priority: "high",
                            title: format!("🔑 {base_label} 凭证失效"),
                            body: format!("{}\n来源：额度查询\n重新登录或刷新凭证后将自动恢复。", ct.summary),
                            jump_url: None,
                        });
                    }
                }
            }
        }
    }

    // 轮询 Did Codex Reset 公开信号
    let reset_msgs = notify::poll_did_codex_reset(&cfg, &mut state.codex_reset).await;
    pending_notifications.extend(reset_msgs);

    let _ = store::save_state(&state);

    let poll_int = cfg.poll_interval_seconds.max(60) as i64;
    let next_poll = ((now / poll_int) + 1) * poll_int;
    if let Ok(mut cache) = runtime_cache().lock() {
        if !updated_views.is_empty() {
            cache.groups = updated_views;
        }
        cache.last_poll_at = now;
        cache.next_poll_at = next_poll;
    }

    if cfg.notifications_configured() {
        for note in &pending_notifications {
            let _ = notify::send_notification(&cfg, note).await;
        }
    }

    Ok(())
}

/// 启动全局单例 Lamplighter 后台调度守护（整点对齐轮询 + 重置前后 30 秒探针 + 自动点火唤醒 + MCP 服务同步）。
pub fn ensure_lamplighter_engine_started(handle: tokio::runtime::Handle) {
    let slot = ENGINE_STARTED.get_or_init(|| Mutex::new(false));
    if let Ok(mut guard) = slot.lock() {
        if *guard {
            return;
        }
        *guard = true;
    }

    let handle_for_mcp = handle.clone();
    let cfg = store::load_config();
    mcp::sync_mcp_server(&handle_for_mcp, cfg.mcp_enabled, &cfg.mcp_listen, &cfg.mcp_key);

    handle.spawn(async move {
        // 启动后延迟 8 秒执行首次静默同步，不抢占 UI 冷启动资源
        tokio::time::sleep(Duration::from_secs(8)).await;
        let _ = run_poll_and_ignition_cycle(false).await;

        loop {
            let cfg = store::load_config();
            mcp::sync_mcp_server(&handle_for_mcp, cfg.mcp_enabled, &cfg.mcp_listen, &cfg.mcp_key);

            let now = Utc::now().timestamp();
            let poll_int = cfg.poll_interval_seconds.max(60) as i64;
            let mut next_wake = ((now / poll_int) + 1) * poll_int;

            // 检查是否有更早的自动点火时间或 reset ± 30s 探针时间
            if let Ok(cache) = runtime_cache().lock() {
                for g in &cache.groups {
                    if let Some(ig) = g.next_ignition_at
                        && ig > now
                        && ig < next_wake
                    {
                        next_wake = ig;
                    }
                    for w in &g.windows {
                        if let Some(rst) = w.reset_at {
                            for probe in [rst - 30, rst + 30] {
                                if probe > now && probe < next_wake {
                                    next_wake = probe;
                                }
                            }
                        }
                    }
                }
            }

            let sleep_secs = (next_wake - now).clamp(5, 300) as u64;
            tokio::time::sleep(Duration::from_secs(sleep_secs)).await;
            if store::load_config().enabled {
                let _ = run_poll_and_ignition_cycle(false).await;
            }
        }
    });
}
