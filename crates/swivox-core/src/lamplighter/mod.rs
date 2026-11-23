//! Lamplighter（点灯人）核心协调引擎：已支持客户端（Codex / Antigravity）额度监控、5h 窗口自动点火、趋势曲线、Bark/Webhook 提醒与本地 `/mcp` 服务。

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

pub use config::{
    DEFAULT_LAMPLIGHTER_MCP_LISTEN, ISOLATED_CLIENT_IDS, IgnitionConfig, LamplighterConfig,
    ProviderRuleConfig, WebhookConfig, WindowModes, canonical_provider_id,
    client_id_for_credential_source, generate_mcp_bearer_key, normalize_client_id,
    plan_display_and_default_weight, provider_display_title, provider_supports_ignition,
    proxy_client_key_for_lamplighter_client,
};
pub use store::{HistorySamplePoint, HistorySeries, LamplighterEventRecord};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitoredGroupView {
    pub client_id: String,
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
    pub effective_proxy: String,
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
    pub history_series: Vec<HistorySeries>,
    pub events: Vec<LamplighterEventRecord>,
    pub client_effective_proxies: BTreeMap<String, String>,
    pub last_poll_at: i64,
    pub next_poll_at: i64,
}

#[derive(Default)]
struct RuntimeCache {
    groups_by_client: BTreeMap<String, Vec<MonitoredGroupView>>,
    last_poll_by_client: BTreeMap<String, i64>,
    next_poll_by_client: BTreeMap<String, i64>,
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

fn describe_cred_effective_proxy(cred: &providers::UnifiedCredential) -> String {
    match providers::resolve_effective_ignition_proxy_spec(cred) {
        Some(s) => s,
        None => "system (direct)".to_string(),
    }
}

fn build_client_effective_proxies() -> BTreeMap<String, String> {
    let snap = crate::network_proxies::get_network_proxy_snapshot();
    let mut map = BTreeMap::new();
    map.insert("codex".to_string(), snap.effective_codex_ignition);
    map.insert(
        "antigravity".to_string(),
        snap.effective_antigravity_ignition,
    );
    map.insert(
        "antigravity_desktop".to_string(),
        snap.effective_antigravity_desktop,
    );
    map.insert(
        "antigravity_ide".to_string(),
        snap.effective_antigravity_ide,
    );
    map
}

/// 根据客户端隔离范围组装即时可读的 `LamplighterSnapshot`（`client_filter = Some("codex")` 或 `Some("antigravity")` 仅返回对应客户端物理隔离的数据）。
pub fn get_lamplighter_snapshot_for_client(
    client_filter: Option<&str>,
    history_hours: u64,
) -> LamplighterSnapshot {
    let cfg = store::load_config();
    let now = Utc::now().timestamp();
    let target_clients: Vec<&'static str> = match client_filter.map(str::trim).filter(|s| !s.is_empty()) {
        Some(raw) => vec![normalize_client_id(raw)],
        None => ISOLATED_CLIENT_IDS.to_vec(),
    };

    let all_creds = providers::collect_unified_credentials(&cfg);
    let mut groups: Vec<MonitoredGroupView> = Vec::new();
    let mut label_map: BTreeMap<String, (String, String, f64)> = BTreeMap::new();
    let mut max_last_poll = 0i64;
    let mut min_next_poll = i64::MAX;

    for cid in &target_clients {
        let client_state = store::load_client_state(cid);
        let cached_for_client = runtime_cache()
            .lock()
            .ok()
            .map(|c| {
                (
                    c.groups_by_client.get(*cid).cloned().unwrap_or_default(),
                    c.last_poll_by_client.get(*cid).copied().unwrap_or(0),
                    c.next_poll_by_client.get(*cid).copied().unwrap_or(0),
                )
            })
            .unwrap_or_default();

        max_last_poll = max_last_poll.max(cached_for_client.1);
        if cached_for_client.2 > now {
            min_next_poll = min_next_poll.min(cached_for_client.2);
        }

        if !cached_for_client.0.is_empty() {
            for mut g in cached_for_client.0 {
                // 实时刷新每个账号当前解析出的点火代理描述，便于修改代理后立即反映
                if let Some((cred, _)) = all_creds.iter().find(|(c, _)| c.account_key == g.account_key) {
                    g.effective_proxy = describe_cred_effective_proxy(cred);
                }
                label_map.insert(
                    g.group_key.clone(),
                    (g.provider.clone(), g.display_label.clone(), g.weight),
                );
                groups.push(g);
            }
            continue;
        }

        // 若该客户端尚未完成首轮网络轮询，利用其专属 state.json 与 Swivox 本地缓存即时渲染卡片
        for (cred, disp_label) in all_creds.iter().filter(|(c, _)| c.client_id == *cid) {
            let cached_plan = client_state
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
            let effective_proxy = describe_cred_effective_proxy(cred);
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
                        let ignite_allowed =
                            cfg.is_antigravity_group_ignite_enabled(&grp.display_name);
                        let ts = client_state
                            .scheduler
                            .get(&sub_gkey)
                            .cloned()
                            .unwrap_or_default();
                        let reset_5h = gwins.iter().find(|w| w.is_five_hour).and_then(|w| w.reset_at);
                        let next_ig = if ignite_allowed {
                            ignite::compute_next_ignition_due_for_client(&cfg, cid, &ts, reset_5h, now)
                        } else {
                            None
                        };
                        label_map.insert(
                            sub_gkey.clone(),
                            (cred.provider.clone(), sub_lbl.clone(), weight),
                        );
                        groups.push(MonitoredGroupView {
                            client_id: (*cid).to_string(),
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
                            effective_proxy: effective_proxy.clone(),
                            windows: gwins,
                            ignite_supported: true,
                            ignite_enabled: ignite_allowed,
                            seven_day_exhausted: grp.weekly_exhausted,
                            next_ignition_at: next_ig,
                            last_ignition_at: (ts.last_success_at > 0).then_some(ts.last_success_at),
                            last_ignition_model: ts.last_model,
                            paused_reason: (ts.paused_until > now)
                                .then_some(ts.paused_reason)
                                .flatten(),
                            last_updated_at: q.updated_at,
                        });
                    }
                    continue;
                }
            }

            let ignite_sup = provider_supports_ignition(&cred.provider)
                && wins.iter().any(|w| w.is_five_hour);
            let ignite_en = ignite_sup && rule.ignite;
            let ts = client_state.scheduler.get(&gkey).cloned().unwrap_or_default();
            let reset_5h = wins.iter().find(|w| w.is_five_hour).and_then(|w| w.reset_at);
            let seven_exh = wins
                .iter()
                .any(|w| !w.is_five_hour && w.remaining_percent <= 0.01);
            let next_ig = if ignite_en && !seven_exh {
                ignite::compute_next_ignition_due_for_client(&cfg, cid, &ts, reset_5h, now)
            } else {
                None
            };
            label_map.insert(
                gkey.clone(),
                (cred.provider.clone(), disp_label.clone(), weight),
            );
            let last_updated_at = client_state
                .groups
                .get(&gkey)
                .map(|g| g.last_seen)
                .unwrap_or(0);
            groups.push(MonitoredGroupView {
                client_id: (*cid).to_string(),
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
                effective_proxy,
                windows: wins,
                ignite_supported: ignite_sup,
                ignite_enabled: ignite_en,
                seven_day_exhausted: seven_exh,
                next_ignition_at: next_ig,
                last_ignition_at: (ts.last_success_at > 0).then_some(ts.last_success_at),
                last_ignition_model: ts.last_model,
                paused_reason: (ts.paused_until > now)
                    .then_some(ts.paused_reason)
                    .flatten(),
                last_updated_at,
            });
        }
    }

    let (history_series, events) = if target_clients.len() == 1 {
        store::query_client_history_and_events(target_clients[0], history_hours, None, &label_map)
    } else {
        store::query_history_and_events(history_hours, None, &label_map)
    };

    let poll_int = cfg.poll_interval_seconds.max(60) as i64;
    let next_poll = if min_next_poll < i64::MAX && min_next_poll > now {
        min_next_poll
    } else {
        ((now / poll_int) + 1) * poll_int
    };

    LamplighterSnapshot {
        config: cfg,
        groups,
        history_series,
        events,
        client_effective_proxies: build_client_effective_proxies(),
        last_poll_at: max_last_poll,
        next_poll_at: next_poll,
    }
}

/// 根据磁盘缓存与内存快照组装全局即时可读的 `LamplighterSnapshot`（不发起网络请求，毫秒级返回）。
pub fn get_lamplighter_snapshot_with_hours(history_hours: u64) -> LamplighterSnapshot {
    get_lamplighter_snapshot_for_client(None, history_hours)
}

pub fn get_lamplighter_snapshot() -> LamplighterSnapshot {
    get_lamplighter_snapshot_for_client(None, 12)
}

/// 按指定客户端隔离范围立即主动刷新配额与自动点火计划。
pub async fn refresh_client_now(client_filter: Option<&str>) -> anyhow::Result<LamplighterSnapshot> {
    run_poll_and_ignition_cycle_for_client(client_filter, true).await?;
    Ok(get_lamplighter_snapshot_for_client(client_filter, 12))
}

/// 立即主动刷新全部监控账号的配额、评估阈值/恢复/套餐变化提醒并刷新自动点火计划。
pub async fn refresh_all_now() -> anyhow::Result<LamplighterSnapshot> {
    refresh_client_now(None).await
}

/// 手动点击「立即点火」：对指定 `group_key` 使用其所属客户端与账号隔离代理立即发送最小点火请求。
pub async fn manual_ignite_group(group_key: &str) -> anyhow::Result<String> {
    let cfg = store::load_config();
    let creds = providers::collect_unified_credentials(&cfg);

    let (mut target_cred, disp_label) = creds
        .into_iter()
        .find(|(c, _)| group_key.contains(&format!(":{}:", c.account_key)))
        .context("未找到该额度组对应的账号凭据")?;

    let client_id = target_cred.client_id.clone();
    let mut state = store::load_client_state(&client_id);

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
            let _ = store::save_client_state(&client_id, &state);

            let clock = notify::format_local_mmdd_hhmm(res.confirmed_reset_at);
            store::append_client_event_record(
                &client_id,
                &LamplighterEventRecord {
                    timestamp: now,
                    group_key: group_key.to_string(),
                    kind: "ignite".to_string(),
                    level: "info".to_string(),
                    label: disp_label.clone(),
                    message: format!(
                        "{disp_label} 手动点火成功（{}），下次重置 {clock}",
                        res.model_used
                    ),
                    detail: Some(format!("model={} · reset={clock}", res.model_used)),
                },
            );

            let _ = run_poll_and_ignition_cycle_for_client(Some(&client_id), true).await;
            Ok(format!(
                "{disp_label} 点火成功（模型：{}），新 5 小时窗口将于 {clock} 重置。",
                res.model_used
            ))
        }
        Err(err) => {
            store::append_client_event_record(
                &client_id,
                &LamplighterEventRecord {
                    timestamp: now,
                    group_key: group_key.to_string(),
                    kind: "ignite_failed".to_string(),
                    level: "error".to_string(),
                    label: disp_label.clone(),
                    message: format!("{disp_label} 手动点火失败：{}", err),
                    detail: None,
                },
            );
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

/// 供 `protocol_proxy.rs` 按实际发起请求的账号凭据异步记录被动响应头额度（严格按客户端与账号精确匹配，绝不跨账号串扰）。
pub fn observe_passive_headers_for_account(
    headers: &BTreeMap<String, String>,
    chatgpt_account_id: Option<&str>,
    access_token: Option<&str>,
    requested_at: i64,
) {
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
    let target_cred = if provider == "codex" {
        let aid = chatgpt_account_id.map(str::trim).filter(|s| !s.is_empty());
        let at = access_token.map(str::trim).filter(|s| !s.is_empty());
        if let Some(target_aid) = aid {
            creds
                .iter()
                .find(|(c, _)| {
                    c.client_id == "codex"
                        && c.chatgpt_account_id.as_deref().map(str::trim) == Some(target_aid)
                        && (at.is_none() || Some(c.access_token.trim()) == at)
                })
                .or_else(|| {
                    creds.iter().find(|(c, _)| {
                        c.client_id == "codex"
                            && c.chatgpt_account_id.as_deref().map(str::trim) == Some(target_aid)
                    })
                })
                .map(|(c, _)| c.clone())
        } else if let Some(target_at) = at {
            creds
                .into_iter()
                .find(|(c, _)| c.client_id == "codex" && c.access_token.trim() == target_at)
                .map(|(c, _)| c)
        } else {
            let active_id = crate::official_accounts::load_official_accounts_state().active_account_id;
            active_id.and_then(|act| {
                creds
                    .into_iter()
                    .find(|(c, _)| c.source_kind == "swivox_codex" && c.raw_account_id == act)
                    .map(|(c, _)| c)
            })
        }
    } else {
        let at = access_token.map(str::trim).filter(|s| !s.is_empty());
        if let Some(target_at) = at {
            creds
                .into_iter()
                .find(|(c, _)| c.provider == provider && c.access_token.trim() == target_at)
                .map(|(c, _)| c)
        } else {
            let matches: Vec<_> = creds
                .into_iter()
                .filter(|(c, _)| c.provider == provider)
                .collect();
            (matches.len() == 1).then(|| matches.into_iter().next().map(|(c, _)| c)).flatten()
        }
    };

    let Some(cred) = target_cred else {
        return;
    };

    let gkey = format!("{provider}:{}:{provider}:main", cred.account_key);
    let now = Utc::now().timestamp();
    for w in &windows {
        store::append_client_history_sample(
            &cred.client_id,
            now,
            &gkey,
            &w.id,
            w.remaining_percent,
            w.reset_at,
            "passive",
        );
    }
    if let Ok(mut cache) = runtime_cache().lock() {
        cache.last_passive_at.insert(cred.account_key, now);
    }
}

pub fn observe_passive_headers(headers: &BTreeMap<String, String>, requested_at: i64) {
    observe_passive_headers_for_account(headers, None, None, requested_at);
}

async fn run_poll_and_ignition_cycle_for_client(
    client_filter: Option<&str>,
    force_active: bool,
) -> anyhow::Result<()> {
    let cfg = store::load_config();
    if !cfg.enabled && !force_active {
        return Ok(());
    }

    let target_clients: Vec<&'static str> = match client_filter.map(str::trim).filter(|s| !s.is_empty()) {
        Some(raw) => vec![normalize_client_id(raw)],
        None => ISOLATED_CLIENT_IDS.to_vec(),
    };

    let all_creds = providers::collect_unified_credentials(&cfg);
    let now = Utc::now().timestamp();
    let poll_int = cfg.poll_interval_seconds.max(60) as i64;
    let next_poll = ((now / poll_int) + 1) * poll_int;
    let mut pending_notifications: Vec<notify::OutboundNotification> = Vec::new();

    for cid in target_clients {
        let mut state = store::load_client_state(cid);
        let client_creds: Vec<_> = all_creds
            .iter()
            .filter(|(c, _)| c.client_id == cid)
            .cloned()
            .collect();
        let valid_account_keys: std::collections::HashSet<String> = client_creds
            .iter()
            .map(|(c, _)| c.account_key.clone())
            .collect();
        let mut refreshed_account_keys: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        let mut updated_views: Vec<MonitoredGroupView> = Vec::new();

        for (mut cred, base_label) in client_creds {
            if !force_active
                && cred.provider == "codex"
                && cfg.passive_skip_seconds > 0
            {
                let skip = runtime_cache().lock().ok().is_some_and(|c| {
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
            let effective_proxy = describe_cred_effective_proxy(&cred);

            match providers::query_credential_quotas(&mut cred, cfg.request_timeout_seconds).await {
                Ok(raw_groups) => {
                    refreshed_account_keys.insert(cred.account_key.clone());
                    if let Ok(mut cache) = runtime_cache().lock() {
                        cache.last_active_at.insert(cred.account_key.clone(), now);
                    }

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

                    let detected_raw_plan = raw_groups
                        .iter()
                        .find_map(|g| g.detected_plan.clone())
                        .or_else(|| cred.cached_plan.clone());
                    let (plan_name, weight, custom_weight) =
                        providers::resolve_account_plan_and_weight(
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
                            store::append_client_event_record(
                                cid,
                                &LamplighterEventRecord {
                                    timestamp: now,
                                    group_key: cred.account_key.clone(),
                                    kind: "plan_change".to_string(),
                                    level: "info".to_string(),
                                    label: base_label.clone(),
                                    message: format!("{base_label} 套餐变更：{old_plan} → {plan_name}"),
                                    detail: None,
                                },
                            );
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
                            store::append_client_history_sample(
                                cid,
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

                            let prev_rst = ws.reset;
                            let new_rst = w.reset_at.unwrap_or(0);
                            let cycle_reset = (prev_rst > 0 && new_rst > prev_rst + 60)
                                || (ws.remaining + 20.0 < w.remaining_percent
                                    && w.remaining_percent >= 95.0);

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
                                    .map(|r| {
                                        format!(" | {}", notify::compact_duration_label(r - now, false))
                                    })
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

                        let w5 = rg.windows.iter().find(|w| w.is_five_hour);
                        let seven_exhausted = rg
                            .windows
                            .iter()
                            .any(|w| !w.is_five_hour && w.remaining_percent <= 0.01);
                        let ignite_supported =
                            provider_supports_ignition(&cred.provider) && w5.is_some();
                        let ignite_enabled = ignite_supported
                            && if cred.provider == "antigravity" {
                                cfg.is_antigravity_group_ignite_enabled(&rg.source_label)
                            } else {
                                rule.ignite
                            };

                        let ts = state.scheduler.entry(gkey.clone()).or_default();
                        // 熔断到期自动清理；且若此前因过期 access_token (HTTP 401) 被误熔断，而本轮配额查询已携新 Token 成功，立即自愈恢复
                        if (ts.paused_until > 0 && ts.paused_until <= now)
                            || ts
                                .paused_reason
                                .as_deref()
                                .is_some_and(|r| r.to_ascii_lowercase().contains("http 401"))
                        {
                            ts.paused_until = 0;
                            ts.paused_reason = None;
                            ts.consecutive_failures = 0;
                            ts.retry_at = 0;
                        }
                        if let Some(win5) = w5 {
                            ignite::observe_five_hour_reset(ts, win5.reset_at, now, false);
                        }

                        let mut next_due = if ignite_enabled && !seven_exhausted {
                            ignite::compute_next_ignition_due_for_client(
                                &cfg,
                                cid,
                                ts,
                                w5.and_then(|w| w.reset_at),
                                now,
                            )
                        } else {
                            None
                        };

                        if let Some(due) = next_due
                            && due <= now
                            && cfg.ignition_for_client(cid).enabled
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
                                    next_due = ignite::compute_next_ignition_due_for_client(
                                        &cfg,
                                        cid,
                                        ts,
                                        Some(res.confirmed_reset_at),
                                        now,
                                    );
                                    let clock = notify::format_local_mmdd_hhmm(res.confirmed_reset_at);
                                    store::append_client_event_record(
                                        cid,
                                        &LamplighterEventRecord {
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
                                        },
                                    );
                                }
                                Err(err) => {
                                    let err_s = err.to_string();
                                    store::append_client_event_record(
                                        cid,
                                        &LamplighterEventRecord {
                                            timestamp: now,
                                            group_key: gkey.clone(),
                                            kind: "ignite_failed".to_string(),
                                            level: "warn".to_string(),
                                            label: full_label.clone(),
                                            message: format!("{full_label} 自动点火失败：{err_s}"),
                                            detail: None,
                                        },
                                    );
                                    if let Some((summary, pause_to)) =
                                        ignite::record_auto_ignition_failure_for_client(
                                            &cfg, cid, ts, &err_s, now,
                                        )
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
                                    next_due = ignite::compute_next_ignition_due_for_client(
                                        &cfg,
                                        cid,
                                        ts,
                                        w5.and_then(|w| w.reset_at),
                                        now,
                                    );
                                }
                            }
                        }

                        updated_views.push(MonitoredGroupView {
                            client_id: cid.to_string(),
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
                            effective_proxy: effective_proxy.clone(),
                            windows: rg.windows,
                            ignite_supported,
                            ignite_enabled,
                            seven_day_exhausted: seven_exhausted,
                            next_ignition_at: next_due,
                            last_ignition_at: (ts.last_success_at > 0).then_some(ts.last_success_at),
                            last_ignition_model: ts.last_model.clone(),
                            paused_reason: (ts.paused_until > now)
                                .then(|| ts.paused_reason.clone())
                                .flatten(),
                            last_updated_at: now,
                        });
                    }
                }
                Err(err) => {
                    let msg = err.to_string();
                    let lower = msg.to_ascii_lowercase();
                    if lower.contains("401")
                        || lower.contains("invalid_grant")
                        || lower.contains("unauthorized")
                    {
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
                                body: format!(
                                    "{}\n来源：额度查询\n重新登录或刷新凭证后将自动恢复。",
                                    ct.summary
                                ),
                                jump_url: None,
                            });
                        }
                    }
                }
            }
        }

        if cid == "codex" {
            let reset_msgs = notify::poll_did_codex_reset(&cfg, &mut state.codex_reset).await;
            pending_notifications.extend(reset_msgs);
        }

        let _ = store::save_client_state(cid, &state);
        if let Ok(mut cache) = runtime_cache().lock() {
            // 保留本次被动跳过或查询失败但仍存在的账号缓存项，避免单账号刷新覆盖掉同客户端其它账号。
            let mut merged: Vec<MonitoredGroupView> = cache
                .groups_by_client
                .get(cid)
                .map(|prev| {
                    prev.iter()
                        .filter(|view| {
                            valid_account_keys.contains(&view.account_key)
                                && !refreshed_account_keys.contains(&view.account_key)
                        })
                        .cloned()
                        .collect()
                })
                .unwrap_or_default();
            merged.extend(updated_views);
            if !merged.is_empty() || valid_account_keys.is_empty() {
                cache.groups_by_client.insert(cid.to_string(), merged);
            }
            cache.last_poll_by_client.insert(cid.to_string(), now);
            cache.next_poll_by_client.insert(cid.to_string(), next_poll);
        }
    }

    if cfg.notifications_configured() {
        for note in &pending_notifications {
            let _ = notify::send_notification(&cfg, note).await;
        }
    }

    Ok(())
}

async fn run_poll_and_ignition_cycle(force_active: bool) -> anyhow::Result<()> {
    run_poll_and_ignition_cycle_for_client(None, force_active).await
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
                for g in cache.groups_by_client.values().flatten() {
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
