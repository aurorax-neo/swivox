//! Lamplighter（点灯人）配置模型、三档提醒阈值单调规整、点火时段与各服务监控参数。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub const DEFAULT_LAMPLIGHTER_MCP_LISTEN: &str = "127.0.0.1:58318";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct WindowModes {
    pub five_hour: String,
    pub seven_day: String,
}

impl Default for WindowModes {
    fn default() -> Self {
        Self {
            five_hour: "after_exhausted".to_string(),
            seven_day: "all".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct WebhookConfig {
    pub url: String,
    pub method: String,
    /// `"feishu"` | `"dingtalk"` | `"telegram"` | `"ntfy"` | `"custom"`
    pub preset: String,
    pub headers: BTreeMap<String, String>,
    pub body_template: String,
}

impl Default for WebhookConfig {
    fn default() -> Self {
        Self {
            url: String::new(),
            method: "POST".to_string(),
            preset: "feishu".to_string(),
            headers: BTreeMap::new(),
            body_template: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct IgnitionConfig {
    pub enabled: bool,
    pub start_hour: u8,
    pub end_hour: u8,
    pub end_grace_minutes: u16,
    pub grace_seconds: u64,
    pub failure_retry_seconds: u64,
    pub failure_backoff_multiplier: u32,
    pub max_transient_failures: u32,
    pub post_success_hold_seconds: u64,
}

impl Default for IgnitionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            start_hour: 7,
            end_hour: 22,
            end_grace_minutes: 30,
            grace_seconds: 3,
            failure_retry_seconds: 300,
            failure_backoff_multiplier: 3,
            max_transient_failures: 3,
            post_success_hold_seconds: 60,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct ProviderRuleConfig {
    pub monitor: bool,
    pub ignite: bool,
    pub model: String,
    pub groups: Vec<String>,
    pub models: BTreeMap<String, String>,
}

impl Default for ProviderRuleConfig {
    fn default() -> Self {
        Self {
            monitor: true,
            ignite: false,
            model: String::new(),
            groups: Vec::new(),
            models: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct CodexResetUpdatesConfig {
    pub enabled: bool,
    pub poll_seconds: u64,
    pub notify_current_pending: bool,
}

impl Default for CodexResetUpdatesConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            poll_seconds: 300,
            notify_current_pending: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct LamplighterConfig {
    pub enabled: bool,
    pub bark_url: String,
    pub bark_group: String,
    pub bark_icon: String,
    pub webhook: WebhookConfig,
    pub notice_threshold: f64,
    pub low_threshold: f64,
    pub critical_threshold: f64,
    pub recovery_notify: WindowModes,
    pub reset_reminder: WindowModes,
    pub timezone: String,
    pub timezone_offset_hours: Option<f64>,
    pub poll_interval_seconds: u64,
    pub request_timeout_seconds: u64,
    pub passive_skip_seconds: u64,
    pub passive_skip_max_minutes: u64,
    pub history_retention_days: u64,
    pub account_weights: BTreeMap<String, f64>,
    pub account_order: Vec<String>,
    pub ignition: IgnitionConfig,
    pub providers: BTreeMap<String, ProviderRuleConfig>,
    pub codex_reset_updates: CodexResetUpdatesConfig,
    // 可选外部 CLIProxyAPI (CPA) 接入
    pub cpa_enabled: bool,
    pub cpa_base_url: String,
    pub cpa_management_key: String,
    pub cpa_auth_dir: String,
    pub models_api_key: String,
    // 可选本地 MCP Streamable HTTP 额度查询接口
    pub mcp_enabled: bool,
    pub mcp_listen: String,
    pub mcp_key: String,
}

pub fn default_providers_map() -> BTreeMap<String, ProviderRuleConfig> {
    let mut m = BTreeMap::new();
    m.insert(
        "codex".to_string(),
        ProviderRuleConfig {
            monitor: true,
            ignite: true,
            model: String::new(),
            groups: Vec::new(),
            models: BTreeMap::new(),
        },
    );
    m.insert(
        "claude".to_string(),
        ProviderRuleConfig {
            monitor: true,
            ignite: true,
            model: String::new(),
            groups: Vec::new(),
            models: BTreeMap::new(),
        },
    );
    m.insert(
        "antigravity".to_string(),
        ProviderRuleConfig {
            monitor: true,
            ignite: false,
            model: String::new(),
            groups: vec!["Gemini".to_string()],
            models: BTreeMap::new(),
        },
    );
    m.insert(
        "kimi".to_string(),
        ProviderRuleConfig {
            monitor: true,
            ignite: false,
            model: String::new(),
            groups: Vec::new(),
            models: BTreeMap::new(),
        },
    );
    m.insert(
        "xai".to_string(),
        ProviderRuleConfig {
            monitor: true,
            ignite: false,
            model: String::new(),
            groups: Vec::new(),
            models: BTreeMap::new(),
        },
    );
    m
}

impl Default for LamplighterConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            bark_url: String::new(),
            bark_group: "Swivox".to_string(),
            bark_icon: String::new(),
            webhook: WebhookConfig::default(),
            notice_threshold: 50.0,
            low_threshold: 20.0,
            critical_threshold: 10.0,
            recovery_notify: WindowModes {
                five_hour: "after_exhausted".to_string(),
                seven_day: "all".to_string(),
            },
            reset_reminder: WindowModes {
                five_hour: "off".to_string(),
                seven_day: "has_remaining".to_string(),
            },
            timezone: String::new(),
            timezone_offset_hours: None,
            poll_interval_seconds: 300,
            request_timeout_seconds: 20,
            passive_skip_seconds: 60,
            passive_skip_max_minutes: 30,
            history_retention_days: 40,
            account_weights: BTreeMap::new(),
            account_order: Vec::new(),
            ignition: IgnitionConfig::default(),
            providers: default_providers_map(),
            codex_reset_updates: CodexResetUpdatesConfig::default(),
            cpa_enabled: false,
            cpa_base_url: "http://127.0.0.1:8317".to_string(),
            cpa_management_key: String::new(),
            cpa_auth_dir: String::new(),
            models_api_key: String::new(),
            mcp_enabled: false,
            mcp_listen: DEFAULT_LAMPLIGHTER_MCP_LISTEN.to_string(),
            mcp_key: String::new(),
        }
    }
}

fn sanitize_recovery_mode(raw: &str, fallback: &str) -> String {
    match raw.trim() {
        "off" | "all" | "after_exhausted" => raw.trim().to_string(),
        _ => fallback.to_string(),
    }
}

fn sanitize_reminder_mode(raw: &str, fallback: &str) -> String {
    match raw.trim() {
        "off" | "all" | "has_remaining" => raw.trim().to_string(),
        _ => fallback.to_string(),
    }
}

pub fn canonical_provider_id(raw: &str) -> &'static str {
    match raw.trim().to_ascii_lowercase().as_str() {
        "codex" | "chatgpt" | "openai" | "gpt" => "codex",
        "claude" | "anthropic" => "claude",
        "antigravity" | "google" | "gemini" => "antigravity",
        "kimi" | "kimi-ai" | "kimi.ai" | "kimi.com" | "moonshot" => "kimi",
        "xai" | "x-ai" | "x.ai" | "grok" => "xai",
        _ => "codex",
    }
}

pub fn provider_display_title(provider_id: &str) -> &'static str {
    match canonical_provider_id(provider_id) {
        "codex" => "GPT",
        "claude" => "Claude",
        "antigravity" => "Antigravity",
        "kimi" => "Kimi",
        "xai" => "Grok",
        _ => "GPT",
    }
}

pub fn provider_supports_ignition(provider_id: &str) -> bool {
    matches!(
        canonical_provider_id(provider_id),
        "codex" | "claude" | "antigravity"
    )
}

impl LamplighterConfig {
    /// 规整所有配置项，严格保证 `100 >= notice_threshold >= low_threshold >= critical_threshold >= 0`。
    pub fn normalize(&mut self) {
        self.bark_url = self.bark_url.trim().trim_end_matches('/').to_string();
        if self.bark_group.trim().is_empty() {
            self.bark_group = "Swivox".to_string();
        }
        self.webhook.url = self.webhook.url.trim().to_string();
        let method = self.webhook.method.trim().to_ascii_uppercase();
        self.webhook.method = match method.as_str() {
            "GET" | "POST" | "PUT" => method,
            _ => "POST".to_string(),
        };

        let notice = if self.notice_threshold.is_finite() {
            self.notice_threshold.clamp(0.0, 100.0)
        } else {
            50.0
        };
        let low = if self.low_threshold.is_finite() {
            self.low_threshold.clamp(0.0, notice)
        } else {
            20.0_f64.min(notice)
        };
        let crit = if self.critical_threshold.is_finite() {
            self.critical_threshold.clamp(0.0, low)
        } else {
            10.0_f64.min(low)
        };
        self.notice_threshold = notice;
        self.low_threshold = low;
        self.critical_threshold = crit;

        self.recovery_notify.five_hour =
            sanitize_recovery_mode(&self.recovery_notify.five_hour, "after_exhausted");
        self.recovery_notify.seven_day =
            sanitize_recovery_mode(&self.recovery_notify.seven_day, "all");
        self.reset_reminder.five_hour =
            sanitize_reminder_mode(&self.reset_reminder.five_hour, "off");
        self.reset_reminder.seven_day =
            sanitize_reminder_mode(&self.reset_reminder.seven_day, "has_remaining");

        if let Some(off) = self.timezone_offset_hours
            && (!off.is_finite() || !(-14.0..=14.0).contains(&off))
        {
            self.timezone_offset_hours = None;
        }
        self.poll_interval_seconds = self.poll_interval_seconds.max(60);
        self.request_timeout_seconds = self.request_timeout_seconds.clamp(5, 120);
        self.passive_skip_max_minutes = self.passive_skip_max_minutes.max(1);
        self.history_retention_days = self.history_retention_days.clamp(1, 365);

        self.account_weights
            .retain(|_, v| v.is_finite() && *v > 0.0);

        self.ignition.start_hour = self.ignition.start_hour.min(23);
        self.ignition.end_hour = self.ignition.end_hour.min(23);
        self.ignition.failure_retry_seconds = self.ignition.failure_retry_seconds.max(60);
        self.ignition.failure_backoff_multiplier =
            self.ignition.failure_backoff_multiplier.clamp(1, 10);
        self.ignition.max_transient_failures = self.ignition.max_transient_failures.clamp(1, 10);
        self.ignition.post_success_hold_seconds = self.ignition.post_success_hold_seconds.max(15);

        self.codex_reset_updates.poll_seconds = self.codex_reset_updates.poll_seconds.max(300);

        let defaults = default_providers_map();
        let mut normalized_providers = BTreeMap::new();
        for (k, v) in std::mem::take(&mut self.providers) {
            let cid = canonical_provider_id(&k).to_string();
            let mut rule = v;
            if !provider_supports_ignition(&cid) {
                rule.ignite = false;
            }
            normalized_providers.insert(cid, rule);
        }
        for (k, def) in defaults {
            normalized_providers.entry(k).or_insert(def);
        }
        self.providers = normalized_providers;

        if self.cpa_base_url.trim().is_empty() {
            self.cpa_base_url = "http://127.0.0.1:8317".to_string();
        } else {
            self.cpa_base_url = self.cpa_base_url.trim().trim_end_matches('/').to_string();
        }
        if self.mcp_listen.trim().is_empty() {
            self.mcp_listen = DEFAULT_LAMPLIGHTER_MCP_LISTEN.to_string();
        }
        if self.mcp_enabled && self.mcp_key.trim().is_empty() {
            self.mcp_key = generate_mcp_bearer_key();
        }
    }

    pub fn notifications_configured(&self) -> bool {
        !self.bark_url.trim().is_empty() || !self.webhook.url.trim().is_empty()
    }

    pub fn provider_rule(&self, provider_id: &str) -> ProviderRuleConfig {
        let cid = canonical_provider_id(provider_id);
        self.providers.get(cid).cloned().unwrap_or_default()
    }
}

pub fn generate_mcp_bearer_key() -> String {
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    let mut raw = [0u8; 24];
    raw[..16].copy_from_slice(uuid::Uuid::new_v4().as_bytes());
    raw[16..].copy_from_slice(&uuid::Uuid::new_v4().as_bytes()[..8]);
    format!("llmcp_{}", URL_SAFE_NO_PAD.encode(raw))
}

/// 根据识别到的原始套餐标识返回 `(显示套餐名, 默认权重)`。
pub fn plan_display_and_default_weight(provider_id: &str, raw_plan: Option<&str>) -> (String, f64) {
    let cid = canonical_provider_id(provider_id);
    let raw = raw_plan.unwrap_or("").trim();
    if raw.is_empty() {
        return (String::new(), 1.0);
    }
    let lower = raw.to_ascii_lowercase();
    match cid {
        "codex" => match lower.as_str() {
            "free" => ("Free".to_string(), 0.1),
            "go" => ("Go".to_string(), 0.1),
            "plus" => ("Plus".to_string(), 1.0),
            "prolite" => ("Pro $100".to_string(), 5.0),
            "pro" => ("Pro $200".to_string(), 10.0),
            "promax" => ("Pro $500".to_string(), 25.0),
            "self_serve_business_prolite" => ("Business Premium".to_string(), 5.0),
            "team" | "self_serve_business_usage_based" => ("Business".to_string(), 1.0),
            "business"
            | "enterprise"
            | "ent26"
            | "enterprise_cbp_automation"
            | "enterprise_cbp_usage_based" => ("Enterprise".to_string(), 1.0),
            "edu" => ("Edu".to_string(), 1.0),
            "edu_plus" => ("Edu Plus".to_string(), 1.0),
            "edu_pro" => ("Edu Pro".to_string(), 1.0),
            _ => (raw.to_string(), 1.0),
        },
        "claude" => {
            if lower.contains("20x") {
                ("Max 20x".to_string(), 20.0)
            } else if lower.contains("5x") || lower.contains("claude_max") || lower == "max" {
                ("Max 5x".to_string(), 5.0)
            } else if lower.contains("enterprise") {
                ("Enterprise".to_string(), 1.0)
            } else if lower.contains("team") {
                ("Team".to_string(), 1.0)
            } else if lower.contains("pro") {
                ("Pro".to_string(), 1.0)
            } else {
                (raw.to_string(), 1.0)
            }
        }
        "antigravity" => {
            if lower.contains("ultra") || lower.contains("helium") {
                ("Ultra".to_string(), 1.0)
            } else if lower.contains("pro") || lower.contains("premium") {
                ("Pro".to_string(), 1.0)
            } else if lower.contains("enterprise") || lower.contains("standard") {
                ("Enterprise".to_string(), 1.0)
            } else {
                ("Free".to_string(), 1.0)
            }
        }
        _ => (raw.to_string(), 1.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_monotonic_thresholds_and_weights() {
        let mut cfg = LamplighterConfig {
            notice_threshold: 25.0,
            low_threshold: 60.0,
            critical_threshold: 40.0,
            ..Default::default()
        };
        cfg.account_weights.insert("bad".to_string(), -2.0);
        cfg.account_weights.insert("good".to_string(), 5.0);
        cfg.normalize();

        assert_eq!(cfg.notice_threshold, 25.0);
        assert_eq!(cfg.low_threshold, 25.0);
        assert_eq!(cfg.critical_threshold, 25.0);
        assert!(!cfg.account_weights.contains_key("bad"));
        assert_eq!(cfg.account_weights.get("good"), Some(&5.0));
    }

    #[test]
    fn maps_codex_claude_and_antigravity_plan_weights() {
        assert_eq!(
            plan_display_and_default_weight("codex", Some("pro")),
            ("Pro $200".to_string(), 10.0)
        );
        assert_eq!(
            plan_display_and_default_weight("codex", Some("free")),
            ("Free".to_string(), 0.1)
        );
        assert_eq!(
            plan_display_and_default_weight("claude", Some("claude_max 20x")),
            ("Max 20x".to_string(), 20.0)
        );
        assert_eq!(
            plan_display_and_default_weight("antigravity", Some("g1-pro-tier")),
            ("Pro".to_string(), 1.0)
        );
    }
}
