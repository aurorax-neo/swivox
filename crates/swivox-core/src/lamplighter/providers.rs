//! Lamplighter 已支持客户端账号汇聚（Swivox 原生 Codex + Swivox 原生 Antigravity）与配额查询。

use std::collections::{BTreeMap, HashMap};
use std::time::Duration;

use anyhow::{Context, bail};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::config::{
    LamplighterConfig, plan_display_and_default_weight, provider_display_title,
    proxy_client_key_for_lamplighter_client,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaWindowReading {
    /// `"five-hour"` | `"seven-day"` | 自定义窗口 ID
    pub id: String,
    /// `"5h"` | `"7d"` | 其他简写
    pub short_label: String,
    pub display_label: String,
    pub is_five_hour: bool,
    pub remaining_percent: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reset_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawQuotaGroup {
    pub raw_group_key: String,
    pub source_label: String,
    pub windows: Vec<QuotaWindowReading>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detected_plan: Option<String>,
}

#[derive(Debug, Clone)]
pub struct UnifiedCredential {
    /// 物理隔离客户端域：`"codex"` | `"antigravity"`
    pub client_id: String,
    /// 稳定全局键：`swivox_codex:<id>` | `swivox_ag:<id>`
    pub account_key: String,
    pub provider: String,
    pub email: String,
    pub note_name: Option<String>,
    pub source_kind: &'static str,
    pub raw_account_id: String,
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub chatgpt_account_id: Option<String>,
    pub google_project_id: Option<String>,
    pub proxy_spec: Option<String>,
    pub cached_plan: Option<String>,
}

impl UnifiedCredential {
    pub fn credential_fingerprint(&self) -> String {
        let (tag, secret) = if let Some(ref rt) = self.refresh_token
            && !rt.trim().is_empty()
        {
            ("rt", rt.trim())
        } else {
            ("at", self.access_token.trim())
        };
        let mut hasher = Sha256::new();
        hasher.update(tag.as_bytes());
        hasher.update(b"\0");
        hasher.update(secret.as_bytes());
        let digest = hasher.finalize();
        digest[..8]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    }
}

/// 汇聚全部可监控账号并按配置顺序排列、计算唯一短标签（如 `GPT#主力`、`GPT#rk`、`Antigravity`）。
pub fn collect_unified_credentials(cfg: &LamplighterConfig) -> Vec<(UnifiedCredential, String)> {
    let mut creds: Vec<UnifiedCredential> = Vec::new();

    // 1. Swivox 原生 Codex 官方账号
    let official_state = crate::official_accounts::load_official_accounts_state();
    for acc in &official_state.accounts {
        if acc.tokens.access_token.trim().is_empty()
            && acc
                .tokens
                .refresh_token
                .as_deref()
                .unwrap_or("")
                .trim()
                .is_empty()
        {
            continue;
        }
        let proxy_spec = crate::official_accounts::effective_proxy_for_account(Some(acc));
        creds.push(UnifiedCredential {
            client_id: "codex".to_string(),
            account_key: format!("swivox_codex:{}", acc.id),
            provider: "codex".to_string(),
            email: acc.email.clone(),
            note_name: acc.label.clone().or_else(|| acc.account_name.clone()),
            source_kind: "swivox_codex",
            raw_account_id: acc.id.clone(),
            access_token: acc.tokens.access_token.clone(),
            refresh_token: acc.tokens.refresh_token.clone(),
            chatgpt_account_id: acc.account_id.clone(),
            google_project_id: None,
            proxy_spec,
            cached_plan: acc
                .plan_type
                .clone()
                .or_else(|| acc.quota.as_ref().and_then(|q| q.plan_type.clone())),
        });
    }

    // 2. Swivox 原生 Antigravity 账号
    let ag_state = crate::antigravity::load_antigravity_state();
    for acc in &ag_state.accounts {
        if acc.disabled {
            continue;
        }
        let proxy_spec =
            crate::antigravity::effective_proxy_for_ag_account(Some(acc), &ag_state)
                .ok()
                .flatten();
        creds.push(UnifiedCredential {
            client_id: "antigravity".to_string(),
            account_key: format!("swivox_ag:{}", acc.id),
            provider: "antigravity".to_string(),
            email: acc.email.clone(),
            note_name: acc.label.clone().or_else(|| acc.name.clone()),
            source_kind: "swivox_ag",
            raw_account_id: acc.id.clone(),
            access_token: acc.token.access_token.clone(),
            refresh_token: Some(acc.token.refresh_token.clone()),
            chatgpt_account_id: None,
            google_project_id: acc.token.project_id.clone(),
            proxy_spec,
            cached_plan: acc.quota.as_ref().and_then(|q| q.raw_tier_id.clone()),
        });
    }

    // 过滤未启用监控的已支持客户端服务
    creds.retain(|c| cfg.provider_rule(&c.provider).monitor);

    // 按 account_order 排序
    if !cfg.account_order.is_empty() {
        let order_map: HashMap<&str, usize> = cfg
            .account_order
            .iter()
            .enumerate()
            .map(|(i, k)| (k.as_str(), i))
            .collect();
        creds.sort_by_key(|c| order_map.get(c.account_key.as_str()).copied().unwrap_or(usize::MAX));
    }

    // 计算同服务账号数，构造 Lamplighter 短标签（如 GPT#主力、GPT#rk）
    let mut provider_counts: HashMap<String, usize> = HashMap::new();
    for c in &creds {
        *provider_counts.entry(c.provider.clone()).or_insert(0) += 1;
    }

    let mut used_labels: HashMap<String, usize> = HashMap::new();
    let mut out = Vec::with_capacity(creds.len());
    for c in creds {
        let title = provider_display_title(&c.provider);
        let count = provider_counts.get(&c.provider).copied().unwrap_or(1);
        let email_tail = email_last_two_chars(&c.email);
        let base_label = if let Some(ref name) = c
            .note_name
            .as_ref()
            .map(|s| s.lines().next().unwrap_or("").trim())
            .filter(|s| !s.is_empty())
        {
            let truncated: String = name.chars().take(20).collect();
            format!("{title}#{truncated}")
        } else if count > 1 {
            format!("{title}#{email_tail}")
        } else {
            title.to_string()
        };

        let n = used_labels.entry(base_label.clone()).or_insert(0);
        *n += 1;
        let final_label = if *n > 1 {
            format!("{base_label}#{email_tail}")
        } else {
            base_label
        };
        out.push((c, final_label));
    }
    out
}

pub fn email_last_two_chars(email: &str) -> String {
    let local = email.split('@').next().unwrap_or(email).trim();
    let chars: Vec<char> = local.chars().collect();
    if chars.is_empty() {
        "01".to_string()
    } else if chars.len() <= 2 {
        chars.into_iter().collect()
    } else {
        chars[chars.len() - 2..].iter().collect()
    }
}

/// 按账号与所属客户端隔离代理解析 HTTP 客户端（点火与额度轮询严守客户端代理隔离，绝不混用本程序或其他客户端代理）。
pub fn resolve_effective_ignition_proxy_spec(cred: &UnifiedCredential) -> Option<String> {
    if let Some(spec) = cred.proxy_spec.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        return Some(spec.to_string());
    }
    crate::network_proxies::resolve_client_proxy_spec(proxy_client_key_for_lamplighter_client(
        &cred.client_id,
    ))
}

pub fn build_account_http_client(
    cred: &UnifiedCredential,
    timeout_secs: u64,
    user_agent: &str,
) -> anyhow::Result<reqwest::Client> {
    let dur = Duration::from_secs(timeout_secs.clamp(5, 120));
    match resolve_effective_ignition_proxy_spec(cred).as_deref() {
        Some(spec) if spec.eq_ignore_ascii_case("direct") => Ok(reqwest::Client::builder()
            .no_proxy()
            .user_agent(user_agent)
            .timeout(dur)
            .build()?),
        Some(spec) => {
            let proxy = reqwest::Proxy::all(spec)
                .with_context(|| format!("账号或客户端专属代理无效: {spec}"))?
                .no_proxy(reqwest::NoProxy::from_string("localhost,127.0.0.1,::1"));
            Ok(reqwest::Client::builder()
                .proxy(proxy)
                .user_agent(user_agent)
                .timeout(dur)
                .build()?)
        }
        None => Ok(reqwest::Client::builder()
            .no_proxy()
            .user_agent(user_agent)
            .timeout(dur)
            .build()?),
    }
}

/// 提取上游多行 JSON 或文本错误响应中的核心单行可读原因，避免被 `.lines().next()` 截断为 `HTTP 401: {`。
pub fn summarize_upstream_error_body(raw_body: &str, max_chars: usize) -> String {
    let trimmed = raw_body.trim();
    if trimmed.is_empty() {
        return "无详细错误信息".to_string();
    }
    if let Ok(val) = serde_json::from_str::<Value>(trimmed) {
        for ptr in [
            "/error/message",
            "/message",
            "/error_description",
            "/detail",
            "/error",
        ] {
            if let Some(msg) = val.pointer(ptr).and_then(Value::as_str).map(str::trim)
                && !msg.is_empty()
            {
                let collapsed = msg.split_whitespace().collect::<Vec<_>>().join(" ");
                return collapsed.chars().take(max_chars).collect();
            }
        }
    }
    let collapsed = trimmed.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed.chars().take(max_chars).collect()
}

fn parse_epoch_or_rfc3339(val: Option<&Value>) -> Option<i64> {
    let v = val?;
    if let Some(i) = v.as_i64() {
        return Some(if i > 10_000_000_000 { i / 1000 } else { i });
    }
    if let Some(f) = v.as_f64() {
        let i = f as i64;
        return Some(if i > 10_000_000_000 { i / 1000 } else { i });
    }
    let s = v.as_str()?.trim();
    if let Ok(i) = s.parse::<i64>() {
        return Some(if i > 10_000_000_000 { i / 1000 } else { i });
    }
    DateTime::parse_from_rfc3339(s).ok().map(|d| d.timestamp())
}

/// 主动查询单个已支持客户端账号的最新额度组列表（Codex 与 Antigravity 支持自动刷新过期 Token）。
pub async fn query_credential_quotas(
    cred: &mut UnifiedCredential,
    timeout_secs: u64,
) -> anyhow::Result<Vec<RawQuotaGroup>> {
    match cred.provider.as_str() {
        "codex" => query_codex_quota(cred, timeout_secs).await,
        "antigravity" => query_antigravity_quota(cred).await,
        other => bail!("不支持的额度监控服务: {other}"),
    }
}

async fn query_codex_quota(
    cred: &mut UnifiedCredential,
    timeout_secs: u64,
) -> anyhow::Result<Vec<RawQuotaGroup>> {
    let ua = "codex-tui/0.154.0 (Mac OS 26.5.2; arm64) Swivox-Lamplighter";
    let client = build_account_http_client(cred, timeout_secs, ua)?;

    let mut attempt_token = cred.access_token.clone();
    for attempt in 0..2 {
        let mut req = client
            .get(crate::official_accounts::OFFICIAL_USAGE_URL)
            .bearer_auth(attempt_token.trim())
            .header("Content-Type", "application/json");
        if let Some(ref acc_id) = cred.chatgpt_account_id
            && !acc_id.trim().is_empty()
        {
            req = req.header("Chatgpt-Account-Id", acc_id.trim());
        }
        let resp = req.send().await.context("请求 ChatGPT 额度接口失败")?;
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();

        if status.as_u16() == 401
            && attempt == 0
            && cred.source_kind == "swivox_codex"
        {
            // 调用 Swivox 官方账号刷新并重试一次
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
                    cred.access_token = updated.tokens.access_token.clone();
                    cred.refresh_token = updated.tokens.refresh_token.clone();
                    attempt_token = cred.access_token.clone();
                    continue;
                }
            }
        }

        if !status.is_success() {
            bail!(
                "HTTP {}: {}",
                status.as_u16(),
                summarize_upstream_error_body(&text, 220)
            );
        }

        let payload: Value = serde_json::from_str(&text).context("解析 ChatGPT 额度 JSON 失败")?;
        let now = Utc::now().timestamp();
        let plan = payload
            .get("plan_type")
            .and_then(Value::as_str)
            .map(ToString::to_string);

        let mut windows = Vec::new();
        for (field, default_is_5h) in [("primary_window", true), ("secondary_window", false)] {
            let Some(win) = payload.pointer(&format!("/rate_limit/{field}")) else {
                continue;
            };
            if win.is_null() {
                continue;
            }
            let win_secs = win
                .get("limit_window_seconds")
                .and_then(Value::as_i64)
                .unwrap_or(if default_is_5h { 18_000 } else { 604_800 });
            let is_5h = (win_secs - 18_000).abs() <= 900;
            let (id, short_lbl, disp_lbl) = if is_5h {
                ("five-hour".to_string(), "5h".to_string(), "5 小时".to_string())
            } else if (win_secs - 604_800).abs() <= 30_240 {
                ("seven-day".to_string(), "7d".to_string(), "7 天".to_string())
            } else {
                let hrs = (win_secs / 3600).max(1);
                (
                    format!("window-{hrs}h"),
                    format!("{hrs}h"),
                    format!("{hrs} 小时"),
                )
            };
            let used = win
                .get("used_percent")
                .and_then(Value::as_f64)
                .unwrap_or_else(|| {
                    if win
                        .get("limit_reached")
                        .and_then(Value::as_bool)
                        .unwrap_or(false)
                    {
                        100.0
                    } else {
                        0.0
                    }
                });
            let remaining = (100.0 - used).clamp(0.0, 100.0);
            let reset_at = parse_epoch_or_rfc3339(win.get("reset_at")).or_else(|| {
                win.get("reset_after_seconds")
                    .and_then(Value::as_i64)
                    .map(|d| now + d)
            });
            windows.push(QuotaWindowReading {
                id,
                short_label: short_lbl,
                display_label: disp_lbl,
                is_five_hour: is_5h,
                remaining_percent: remaining,
                reset_at,
            });
        }

        return Ok(vec![RawQuotaGroup {
            raw_group_key: "codex:main".to_string(),
            source_label: "GPT".to_string(),
            windows,
            detected_plan: plan,
        }]);
    }

    bail!("ChatGPT 额度查询重试失败")
}

async fn query_antigravity_quota(cred: &mut UnifiedCredential) -> anyhow::Result<Vec<RawQuotaGroup>> {
    let snap = if cred.source_kind == "swivox_ag" {
        let s = crate::antigravity::refresh_account_quota(&cred.raw_account_id).await?;
        // 关键修复：refresh_account_quota 内部可能已使用 refresh_token 刷新了过期 access_token 并落盘，
        // 必须同步回内存中的 `cred`，否则紧接着在同一轮循环触发的自动点火仍会携带旧的过期 Token 导致 HTTP 401。
        let reloaded = crate::antigravity::load_antigravity_state();
        if let Some(updated) = reloaded
            .accounts
            .into_iter()
            .find(|a| a.id == cred.raw_account_id)
        {
            cred.access_token = updated.token.access_token;
            cred.refresh_token = Some(updated.token.refresh_token);
            if updated.token.project_id.is_some() {
                cred.google_project_id = updated.token.project_id;
            }
        }
        s
    } else {
        if let Some(ref rt) = cred.refresh_token
            && (cred.access_token.trim().is_empty() || rt.len() > 8)
            && let Ok(refreshed) =
                crate::antigravity::refresh_google_access_token(rt, None, cred.proxy_spec.as_deref())
                    .await
        {
            cred.access_token = refreshed.access_token;
        }
        crate::antigravity::fetch_antigravity_quota(
            &cred.access_token,
            cred.google_project_id.as_deref(),
            cred.proxy_spec.as_deref(),
        )
        .await?
    };

    if snap.is_forbidden || snap.validation_blocked {
        bail!(
            "{}",
            snap.error
                .unwrap_or_else(|| "Antigravity 账号访问受限 (403)".to_string())
        );
    }

    if let Some(ref pid) = snap.project_id {
        cred.google_project_id = Some(pid.clone());
    }

    let mut groups = Vec::new();
    for g in snap.groups {
        let mut windows = Vec::new();
        if let Some(pct_5h) = g.five_hour_percentage {
            windows.push(QuotaWindowReading {
                id: "five-hour".to_string(),
                short_label: "5h".to_string(),
                display_label: "5 小时".to_string(),
                is_five_hour: true,
                remaining_percent: pct_5h,
                reset_at: g.five_hour_reset_time,
            });
        }
        if let Some(pct_7d) = g.weekly_percentage {
            windows.push(QuotaWindowReading {
                id: "seven-day".to_string(),
                short_label: "7d".to_string(),
                display_label: "7 天".to_string(),
                is_five_hour: false,
                remaining_percent: pct_7d,
                reset_at: g.weekly_reset_time,
            });
        }
        groups.push(RawQuotaGroup {
            raw_group_key: format!("antigravity:{}", g.group_id),
            source_label: g.display_name,
            windows,
            detected_plan: snap.raw_tier_id.clone().or(Some(snap.subscription_tier.clone())),
        });
    }
    Ok(groups)
}

/// 从 `protocol_proxy` 捕获的 Codex 上游响应头解析被动额度（零额外请求开销）。
pub fn parse_passive_response_headers(
    headers: &BTreeMap<String, String>,
    requested_at: i64,
) -> Option<(&'static str, Vec<QuotaWindowReading>)> {
    let lower: HashMap<String, &str> = headers
        .iter()
        .map(|(k, v)| (k.to_ascii_lowercase(), v.as_str()))
        .collect();

    // Codex 响应头: x-codex-primary-used-percent / x-codex-secondary-used-percent
    if lower.contains_key("x-codex-primary-used-percent")
        || lower.contains_key("x-codex-secondary-used-percent")
    {
        let mut wins = Vec::new();
        for (prefix, def_5h) in [("x-codex-primary", true), ("x-codex-secondary", false)] {
            let Some(used_str) = lower.get(&format!("{prefix}-used-percent")) else {
                continue;
            };
            let Ok(used) = used_str.trim().parse::<f64>() else {
                continue;
            };
            let mins = lower
                .get(&format!("{prefix}-window-minutes"))
                .and_then(|s| s.trim().parse::<i64>().ok())
                .unwrap_or(if def_5h { 300 } else { 10_080 });
            let is_5h = (mins - 300).abs() <= 15;
            let reset_at = lower
                .get(&format!("{prefix}-reset-at"))
                .and_then(|s| s.trim().parse::<i64>().ok())
                .or_else(|| {
                    lower
                        .get(&format!("{prefix}-reset-after-seconds"))
                        .and_then(|s| s.trim().parse::<i64>().ok())
                        .map(|d| requested_at + d)
                });
            wins.push(QuotaWindowReading {
                id: if is_5h {
                    "five-hour".to_string()
                } else {
                    "seven-day".to_string()
                },
                short_label: if is_5h {
                    "5h".to_string()
                } else {
                    "7d".to_string()
                },
                display_label: if is_5h {
                    "5 小时".to_string()
                } else {
                    "7 天".to_string()
                },
                is_five_hour: is_5h,
                remaining_percent: (100.0 - used).clamp(0.0, 100.0),
                reset_at,
            });
        }
        if !wins.is_empty() {
            return Some(("codex", wins));
        }
    }

    None
}

pub fn resolve_account_plan_and_weight(
    cfg: &LamplighterConfig,
    account_key: &str,
    provider: &str,
    raw_plan: Option<&str>,
) -> (String, f64, bool) {
    let (plan_name, default_weight) = plan_display_and_default_weight(provider, raw_plan);
    if let Some(&custom) = cfg.account_weights.get(account_key)
        && custom.is_finite()
        && custom > 0.0
    {
        (plan_name, custom, true)
    } else {
        (plan_name, default_weight, false)
    }
}
