//! Phase 2 下沉纯业务规则：官方账号模式判定、配额窗口解析、Codex 多开状态与供应商摘要。

use std::time::{SystemTime, UNIX_EPOCH};

use swivox_core::official_accounts::{OfficialAccountView, OfficialAccountsSnapshot, OfficialQuota};
use swivox_core::settings::{
    AggregateRelayProfile, AggregateRelayStrategy, RelayMode, RelayProfile, RelayProtocol,
};
use chrono::{Local, TimeZone};
use serde_json::Value;

use crate::commands::CodexMultiInstance;
use crate::i18n::{t, tf};

// ============================================================================
// 1. 官方账号生效模式判定（对应 official-accounts-mode.ts）
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OfficialAccountMode {
    Mixed {
        live_email: String,
        profile_id: String,
        profile_name: String,
    },
    Official {
        live_email: String,
    },
    PureApi {
        profile_id: String,
        profile_name: String,
    },
    None,
}

impl OfficialAccountMode {
    pub fn live_email(&self) -> Option<&str> {
        match self {
            Self::Mixed { live_email, .. } | Self::Official { live_email } => Some(live_email),
            _ => None,
        }
    }

    pub fn summary_text(&self) -> String {
        match self {
            Self::Mixed { profile_name, .. } => format!("{} + {}", t("官方账号"), profile_name),
            Self::Official { live_email } => format!("{} ({})", t("纯官方登录"), live_email),
            Self::PureApi { profile_name, .. } => format!("{} ({})", t("纯 API 模式"), profile_name),
            Self::None => t("未选择"),
        }
    }
}

pub fn resolve_official_account_mode(
    snapshot: Option<&OfficialAccountsSnapshot>,
    profiles: &[RelayProfile],
    active_relay_id: &str,
) -> OfficialAccountMode {
    let live_email = snapshot
        .and_then(|s| s.live_account_email.as_deref())
        .map(str::trim)
        .unwrap_or("");

    if !live_email.is_empty() {
        let matched_account: Option<&OfficialAccountView> = snapshot.and_then(|s| {
            s.accounts
                .iter()
                .find(|acc| acc.email.trim().eq_ignore_ascii_case(live_email))
        });
        let bound_id = matched_account
            .and_then(|acc| acc.bound_relay_profile_id.as_deref())
            .map(str::trim)
            .unwrap_or("");
        if !bound_id.is_empty()
            && let Some(profile) = profiles.iter().find(|p| p.id == bound_id)
        {
            return OfficialAccountMode::Mixed {
                live_email: live_email.to_string(),
                profile_id: profile.id.clone(),
                profile_name: profile_display_name(profile),
            };
        }
        return OfficialAccountMode::Official {
            live_email: live_email.to_string(),
        };
    }

    if let Some(profile) = profiles.iter().find(|p| p.id == active_relay_id) {
        return OfficialAccountMode::PureApi {
            profile_id: profile.id.clone(),
            profile_name: profile_display_name(profile),
        };
    }

    OfficialAccountMode::None
}

fn profile_display_name(profile: &RelayProfile) -> String {
    let trimmed = profile.name.trim();
    if trimmed.is_empty() {
        profile.id.clone()
    } else {
        trimmed.to_string()
    }
}

pub fn account_initial(value: &str) -> String {
    value
        .trim()
        .chars()
        .next()
        .map(|ch| ch.to_uppercase().collect::<String>())
        .unwrap_or_else(|| "O".to_string())
}

pub fn provider_initial(value: &str) -> String {
    value
        .trim()
        .chars()
        .next()
        .map(|ch| ch.to_uppercase().collect::<String>())
        .unwrap_or_else(|| "P".to_string())
}

pub fn format_plan_display_name(plan_type: Option<&str>) -> String {
    let Some(raw) = plan_type.map(str::trim).filter(|s| !s.is_empty()) else {
        return String::new();
    };
    let upper = raw.to_uppercase();
    if upper.contains("TEAM") {
        "TEAM".to_string()
    } else if upper.contains("ENTERPRISE") {
        "ENTERPRISE".to_string()
    } else if upper.contains("PRO") {
        "PRO".to_string()
    } else if upper.contains("PLUS") {
        "PLUS".to_string()
    } else if upper.contains("EDU") {
        "EDU".to_string()
    } else if upper.contains("FREE") {
        "FREE".to_string()
    } else {
        upper
    }
}

// ============================================================================
// 2. 官方账号配额窗口解析与展示规则
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuotaTone {
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuotaDisplayWindow {
    pub id: String,
    pub label: String,
    pub percentage: i32,
    pub reset_text: String,
    pub tone: QuotaTone,
}

pub fn clamp_percentage(value: i32) -> i32 {
    value.clamp(0, 100)
}

pub fn quota_tone_for(pct: i32) -> QuotaTone {
    if pct >= 50 {
        QuotaTone::High
    } else if pct >= 20 {
        QuotaTone::Medium
    } else {
        QuotaTone::Low
    }
}

fn is_quota_window_present(quota: &OfficialQuota, is_hourly: bool) -> bool {
    let has_flags = quota.hourly_window_present.is_some() || quota.weekly_window_present.is_some();
    if !has_flags {
        return true;
    }
    if quota.hourly_window_present == Some(false) && quota.weekly_window_present == Some(false) {
        return is_hourly;
    }
    if is_hourly {
        quota.hourly_window_present == Some(true)
    } else {
        quota.weekly_window_present == Some(true)
    }
}

pub fn get_codex_quota_window_label(window_minutes: Option<i64>, is_weekly_fallback: bool) -> String {
    const HOUR_MINUTES: i64 = 60;
    const DAY_MINUTES: i64 = 24 * HOUR_MINUTES;
    const WEEK_MINUTES: i64 = 7 * DAY_MINUTES;

    let Some(safe_minutes) = window_minutes.filter(|&m| m > 0) else {
        return if is_weekly_fallback {
            t("周配额")
        } else {
            t("5小时配额")
        };
    };

    if (27 * DAY_MINUTES..=32 * DAY_MINUTES).contains(&safe_minutes) {
        return t("月配额");
    }
    if (WEEK_MINUTES - 60..=WEEK_MINUTES + 60).contains(&safe_minutes) {
        return t("周配额");
    }
    if safe_minutes > WEEK_MINUTES && (safe_minutes % WEEK_MINUTES).abs() <= 60 {
        let weeks = (safe_minutes as f64 / WEEK_MINUTES as f64).round() as i64;
        return format!("{weeks}{}", t("周配额"));
    }
    if safe_minutes >= DAY_MINUTES - 1 {
        let days = (safe_minutes as f64 / DAY_MINUTES as f64).round() as i64;
        return format!("{days}{}", t("天配额"));
    }
    if safe_minutes >= HOUR_MINUTES {
        let hours = (safe_minutes as f64 / HOUR_MINUTES as f64).round() as i64;
        return format!("{hours}{}", t("小时配额"));
    }
    format!("{safe_minutes}{}", t("分钟配额"))
}

pub fn format_codex_reset_time(reset_time: Option<i64>) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    format_codex_reset_time_at(reset_time, now)
}

pub fn format_codex_reset_time_at(reset_time: Option<i64>, now_secs: i64) -> String {
    let Some(reset_secs) = reset_time.filter(|&ts| ts > 0) else {
        return String::new();
    };
    let diff = reset_secs - now_secs;
    if diff <= 0 {
        return t("即将重置");
    }

    let total_minutes = diff / 60;
    let days = total_minutes / (60 * 24);
    let hours = (total_minutes % (60 * 24)) / 60;
    let minutes = total_minutes % 60;

    let is_en = crate::i18n::get_language() == crate::i18n::Language::En;
    let mut parts = Vec::new();
    if days > 0 {
        parts.push(if is_en {
            format!("{days}d")
        } else {
            format!("{days}天")
        });
    }
    if hours > 0 {
        parts.push(if is_en {
            format!("{hours}h")
        } else {
            format!("{hours}小时")
        });
    }
    if minutes > 0 && days == 0 {
        parts.push(if is_en {
            format!("{minutes}m")
        } else {
            format!("{minutes}分")
        });
    }

    let relative = if parts.is_empty() {
        if is_en {
            "Resets in <1m".to_string()
        } else {
            "1分钟内重置".to_string()
        }
    } else if is_en {
        format!("Resets in {}", parts.join(" "))
    } else {
        format!("{}后重置", parts.join(""))
    };

    let absolute = Local
        .timestamp_opt(reset_secs, 0)
        .single()
        .map(|dt| dt.format("%m/%d %H:%M").to_string())
        .unwrap_or_default();

    if absolute.is_empty() {
        relative
    } else {
        format!("{relative} ({absolute})")
    }
}

fn normalize_unix_seconds(val: &Value) -> Option<i64> {
    let n = val.as_f64()?;
    if !n.is_finite() || n <= 0.0 {
        return None;
    }
    let ts = if n > 10_000_000_000.0 { n / 1000.0 } else { n };
    Some(ts.floor() as i64)
}

fn normalize_additional_limit_label(limit_name: &str, metered_feature: &str) -> String {
    let raw = if !limit_name.trim().is_empty() {
        limit_name.trim()
    } else {
        metered_feature.trim()
    };
    if raw.is_empty() {
        return String::new();
    }
    let replaced = raw.replace(['-', '_'], " ");
    let mut words = Vec::new();
    for part in replaced.split_whitespace() {
        let lower = part.to_ascii_lowercase();
        if lower == "gpt" {
            words.push("GPT".to_string());
        } else if let Some(rest) = lower.strip_prefix("gpt")
            && !rest.is_empty()
        {
            words.push(format!("GPT {rest}"));
        } else if lower == "codex" {
            words.push("Codex".to_string());
        } else if lower == "spark" {
            words.push("Spark".to_string());
        } else {
            words.push(part.to_string());
        }
    }
    words.join(" ")
}

pub fn build_account_quota_windows(quota: Option<&OfficialQuota>) -> Vec<QuotaDisplayWindow> {
    let Some(quota) = quota else {
        return Vec::new();
    };

    let hourly_present = is_quota_window_present(quota, true);
    let weekly_present = is_quota_window_present(quota, false);

    let raw_hourly = hourly_present.then(|| clamp_percentage(quota.hourly_percentage));
    let weekly = weekly_present.then(|| clamp_percentage(quota.weekly_percentage));
    let hourly = match (raw_hourly, weekly) {
        (Some(_), Some(0)) => Some(0),
        (h, _) => h,
    };

    let mut windows = Vec::new();

    if let Some(pct) = hourly {
        windows.push(QuotaDisplayWindow {
            id: "primary".to_string(),
            label: get_codex_quota_window_label(quota.hourly_window_minutes, false),
            percentage: pct,
            reset_text: format_codex_reset_time(quota.hourly_reset_time),
            tone: quota_tone_for(pct),
        });
    }

    if let Some(pct) = weekly {
        windows.push(QuotaDisplayWindow {
            id: "secondary".to_string(),
            label: get_codex_quota_window_label(quota.weekly_window_minutes, true),
            percentage: pct,
            reset_text: format_codex_reset_time(quota.weekly_reset_time),
            tone: quota_tone_for(pct),
        });
    }

    if windows.is_empty() {
        let pct = clamp_percentage(quota.hourly_percentage);
        windows.push(QuotaDisplayWindow {
            id: "primary".to_string(),
            label: get_codex_quota_window_label(quota.hourly_window_minutes, false),
            percentage: pct,
            reset_text: format_codex_reset_time(quota.hourly_reset_time),
            tone: quota_tone_for(pct),
        });
    }

    if let Some(Value::Object(raw)) = &quota.raw_data {
        if let Some(Value::Array(additional)) = raw.get("additional_rate_limits") {
            for (idx, entry) in additional.iter().enumerate() {
                let Some(record) = entry.as_object() else {
                    continue;
                };
                let Some(rate_limit) = record.get("rate_limit").and_then(Value::as_object) else {
                    continue;
                };
                let limit_name = record
                    .get("limit_name")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let metered = record
                    .get("metered_feature")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let base_label = {
                    let normalized = normalize_additional_limit_label(limit_name, metered);
                    if normalized.is_empty() {
                        t("额外额度")
                    } else {
                        normalized
                    }
                };

                for (win_key, win_kind, weekly_fb) in [
                    ("primary_window", "primary", false),
                    ("secondary_window", "secondary", true),
                ] {
                    let Some(win) = rate_limit.get(win_key).and_then(Value::as_object) else {
                        continue;
                    };
                    let Some(used_pct) = win.get("used_percent").and_then(Value::as_f64) else {
                        continue;
                    };
                    if !used_pct.is_finite() {
                        continue;
                    }
                    let pct = clamp_percentage(100 - used_pct.round() as i32);
                    let win_mins = win
                        .get("limit_window_seconds")
                        .and_then(Value::as_f64)
                        .filter(|&s| s.is_finite() && s > 0.0)
                        .map(|s| (s / 60.0).ceil() as i64);
                    let reset_sec = win.get("reset_at").and_then(normalize_unix_seconds);

                    windows.push(QuotaDisplayWindow {
                        id: format!("additional:{idx}:{win_kind}"),
                        label: format!(
                            "{base_label} {}",
                            get_codex_quota_window_label(win_mins, weekly_fb)
                        ),
                        percentage: pct,
                        reset_text: format_codex_reset_time(reset_sec),
                        tone: quota_tone_for(pct),
                    });
                }
            }
        }

        let mut cr_pct = quota.code_review_percentage.map(clamp_percentage);
        let mut cr_reset = quota.code_review_reset_time;
        if cr_pct.is_none()
            && let Some(cr_obj) = raw.get("code_review_rate_limit").and_then(Value::as_object)
            && let Some(win) = cr_obj
                .get("primary_window")
                .and_then(Value::as_object)
                .or_else(|| cr_obj.get("secondary_window").and_then(Value::as_object))
            && let Some(used) = win.get("used_percent").and_then(Value::as_f64)
            && used.is_finite()
        {
            cr_pct = Some(clamp_percentage(100 - used.round() as i32));
            cr_reset = win.get("reset_at").and_then(normalize_unix_seconds);
        }
        if let Some(pct) = cr_pct {
            windows.push(QuotaDisplayWindow {
                id: "code_review".to_string(),
                label: "Code Review".to_string(),
                percentage: pct,
                reset_text: format_codex_reset_time(cr_reset),
                tone: quota_tone_for(pct),
            });
        }
    } else if let Some(pct) = quota.code_review_percentage.map(clamp_percentage) {
        windows.push(QuotaDisplayWindow {
            id: "code_review".to_string(),
            label: "Code Review".to_string(),
            percentage: pct,
            reset_text: format_codex_reset_time(quota.code_review_reset_time),
            tone: quota_tone_for(pct),
        });
    }

    windows
}

// ============================================================================
// 3. Codex 多开状态与供应商摘要（对应 multi-launch-model.ts 及 App.tsx）
// ============================================================================

pub fn multi_launch_instance_label(instance: &CodexMultiInstance) -> String {
    if instance.is_main {
        return t("主 Codex");
    }
    let name = instance.profile_name.trim();
    if !name.is_empty() {
        return name.to_string();
    }
    let id = instance.instance_id.trim();
    if !id.is_empty() {
        return id.to_string();
    }
    t("Codex 实例")
}

pub fn multi_launch_instance_status(instance: &CodexMultiInstance) -> String {
    let s = instance.status.trim().to_ascii_lowercase();
    if s.is_empty() {
        "unknown".to_string()
    } else {
        s
    }
}

pub fn multi_launch_instance_running(instance: &CodexMultiInstance) -> bool {
    instance.running
        || matches!(
            multi_launch_instance_status(instance).as_str(),
            "running" | "running_degraded" | "starting" | "ready" | "connected" | "ok"
        )
}

pub fn multi_launch_instance_failure(instance: &CodexMultiInstance) -> Option<String> {
    if multi_launch_instance_running(instance) {
        return None;
    }
    let res = instance.launch_result.as_ref()?;
    if res.status != "failed" {
        return None;
    }
    let msg = res.message.trim();
    if msg.is_empty() {
        Some(t("Codex 实例启动失败，原因未记录。"))
    } else {
        Some(msg.to_string())
    }
}

pub fn should_display_multi_instance(instance: &CodexMultiInstance) -> bool {
    multi_launch_instance_running(instance) || multi_launch_instance_failure(instance).is_some()
}

pub fn multi_launch_supported(profile: &RelayProfile) -> bool {
    profile.relay_mode != RelayMode::Aggregate
        && matches!(
            profile.protocol,
            RelayProtocol::Responses | RelayProtocol::ChatCompletions
        )
        && !profile
            .model_routes
            .iter()
            .any(|r| !r.model.trim().is_empty() || !r.target_relay_id.trim().is_empty())
}

pub fn relay_protocol_label(protocol: RelayProtocol) -> String {
    match protocol {
        RelayProtocol::ChatCompletions => t("Chat Completions 转 Responses"),
        RelayProtocol::Responses => "Responses API".to_string(),
    }
}

pub fn aggregate_strategy_label(strategy: AggregateRelayStrategy) -> String {
    match strategy {
        AggregateRelayStrategy::Failover => t("失败切换"),
        AggregateRelayStrategy::ConversationRoundRobin => t("按对话轮询"),
        AggregateRelayStrategy::RequestRoundRobin => t("按请求轮询"),
        AggregateRelayStrategy::WeightedRoundRobin => t("按权重轮询"),
    }
}

pub fn relay_profile_mode_label(
    profile: &RelayProfile,
    aggregate: Option<&AggregateRelayProfile>,
) -> String {
    match profile.relay_mode {
        RelayMode::Aggregate => {
            if aggregate.is_some_and(|a| a.model_aggregation_enabled) {
                t("模型聚合")
            } else {
                t("故障转移")
            }
        }
        RelayMode::PureApi => t("纯 API"),
        RelayMode::MixedApi => t("官方登录 + API"),
        RelayMode::Official => {
            if profile.official_mix_api_key {
                t("官方登录 + API")
            } else {
                t("官方登录")
            }
        }
    }
}

pub fn relay_profile_config_brief(
    profile: &RelayProfile,
    aggregate: Option<&AggregateRelayProfile>,
) -> String {
    if profile.relay_mode == RelayMode::Aggregate {
        let count = aggregate.map(|a| a.members.len()).unwrap_or(0);
        if aggregate.is_some_and(|a| a.model_aggregation_enabled) {
            return tf("模型固定绑定 · {0} 个成员", &[&count.to_string()]);
        }
        let strat = aggregate
            .map(|a| aggregate_strategy_label(a.strategy))
            .unwrap_or_else(|| t("失败切换"));
        return tf("{0} · {1} 个成员", &[&strat, &count.to_string()]);
    }
    if profile.relay_mode == RelayMode::Official {
        return if profile.official_mix_api_key {
            t("混入 API Key")
        } else {
            t("不写 API 文件")
        };
    }
    let url = swivox_core::relay_config::relay_profile_base_url(profile);
    if url.trim().is_empty() {
        t("未填写 URL")
    } else {
        url
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn official_mode_resolution_prefers_live_account_and_detects_bound_relay() {
        let profile = RelayProfile {
            id: "deepseek".to_string(),
            name: "DeepSeek".to_string(),
            ..RelayProfile::default()
        };
        let snap = OfficialAccountsSnapshot {
            active_account_id: Some("a1".to_string()),
            live_account_email: Some("User@Example.com".to_string()),
            live_account_id: Some("a1".to_string()),
            accounts: vec![OfficialAccountView {
                id: "a1".to_string(),
                email: "user@example.com".to_string(),
                label: None,
                user_id: None,
                plan_type: Some("chatgpt_plus_plan".to_string()),
                account_id: None,
                organization_id: None,
                account_name: None,
                has_refresh_token: true,
                quota: None,
                bound_relay_profile_id: Some("deepseek".to_string()),
                proxy_mode: "inherit".to_string(),
                egress_proxy_url: None,
                created_at: 0,
                last_used: 0,
                tags: Vec::new(),
                is_active: true,
            }],
            global_proxy_mode: "inherit".to_string(),
            global_proxy_url: String::new(),
            effective_launch_proxy: String::new(),
            proxy_resources: Vec::new(),
            relay_profile_proxies: HashMap::new(),
            relay_profile_bound_accounts: HashMap::new(),
        };

        let mode = resolve_official_account_mode(Some(&snap), &[profile.clone()], "deepseek");
        assert_eq!(
            mode,
            OfficialAccountMode::Mixed {
                live_email: "User@Example.com".to_string(),
                profile_id: "deepseek".to_string(),
                profile_name: "DeepSeek".to_string(),
            }
        );
        assert_eq!(format_plan_display_name(Some("chatgpt_plus_plan")), "PLUS");

        let pure_mode = resolve_official_account_mode(None, &[profile], "deepseek");
        assert_eq!(
            pure_mode,
            OfficialAccountMode::PureApi {
                profile_id: "deepseek".to_string(),
                profile_name: "DeepSeek".to_string(),
            }
        );
    }

    #[test]
    fn quota_windows_respect_weekly_exhaustion_and_additional_limits() {
        let quota = OfficialQuota {
            hourly_percentage: 80,
            weekly_percentage: 0,
            hourly_window_present: Some(true),
            weekly_window_present: Some(true),
            code_review_percentage: Some(65),
            raw_data: Some(serde_json::json!({
                "additional_rate_limits": [{
                    "limit_name": "gpt-codex-spark",
                    "rate_limit": {
                        "primary_window": {
                            "used_percent": 25.0,
                            "limit_window_seconds": 300 * 60
                        }
                    }
                }]
            })),
            ..OfficialQuota::default()
        };
        let wins = build_account_quota_windows(Some(&quota));
        assert_eq!(wins.len(), 4);
        assert_eq!(wins[0].percentage, 0);
        assert_eq!(wins[1].percentage, 0);
        assert_eq!(wins[2].percentage, 75);
        assert!(wins[2].label.contains("GPT Codex Spark"));
        assert_eq!(wins[3].label, "Code Review");
        assert_eq!(wins[3].percentage, 65);
    }
}
