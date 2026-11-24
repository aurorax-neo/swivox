//! Antigravity Cloud Code Private 配额、模型分组（Gemini / Claude & GPT / Gemini 4 Argon）与权威订阅等级查询。

use std::collections::HashMap;

use anyhow::{Context, bail};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::oauth::build_http_client_for_proxy;

/// 实测校准的官方 Hub User-Agent（版本、CL、平台同源，避免分档模型 404 或 403 门禁）。
pub const ANTIGRAVITY_HUB_USER_AGENT: &str =
    "antigravity/hub/2.28.0 (aidev_client; os_type=windows; arch=amd64; cl=1008300195)";

pub const LOAD_CODE_ASSIST_ENDPOINTS: [&str; 3] = [
    "https://daily-cloudcode-pa.googleapis.com/v1internal:loadCodeAssist",
    "https://cloudcode-pa.googleapis.com/v1internal:loadCodeAssist",
    "https://daily-cloudcode-pa.sandbox.googleapis.com/v1internal:loadCodeAssist",
];

pub const QUOTA_SUMMARY_ENDPOINTS: [&str; 3] = [
    "https://daily-cloudcode-pa.googleapis.com/v1internal:retrieveUserQuotaSummary",
    "https://cloudcode-pa.googleapis.com/v1internal:retrieveUserQuotaSummary",
    "https://daily-cloudcode-pa.sandbox.googleapis.com/v1internal:retrieveUserQuotaSummary",
];

pub const FETCH_MODELS_ENDPOINTS: [&str; 3] = [
    "https://daily-cloudcode-pa.googleapis.com/v1internal:fetchAvailableModels",
    "https://cloudcode-pa.googleapis.com/v1internal:fetchAvailableModels",
    "https://daily-cloudcode-pa.sandbox.googleapis.com/v1internal:fetchAvailableModels",
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AntigravityGroupQuota {
    /// `"gemini"` | `"claude_gpt"` | `"argon"`
    pub group_id: String,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub five_hour_percentage: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub five_hour_reset_time: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weekly_percentage: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weekly_reset_time: Option<i64>,
    #[serde(default)]
    pub weekly_exhausted: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sample_models: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AntigravityQuotaSnapshot {
    /// 归一化等级：`"FREE"` | `"PRO"` | `"ULTRA"` | `"ENTERPRISE"`
    pub subscription_tier: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_tier_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(default)]
    pub groups: Vec<AntigravityGroupQuota>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub available_model_ids: Vec<String>,
    #[serde(default)]
    pub is_forbidden: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub forbidden_reason: Option<String>,
    #[serde(default)]
    pub validation_blocked: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation_url: Option<String>,
    #[serde(default)]
    pub updated_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LoadProjectResponse {
    #[serde(rename = "cloudaicompanionProject")]
    project_id: Option<Value>,
    #[serde(rename = "currentTier")]
    current_tier: Option<TierEntry>,
    #[serde(rename = "paidTier")]
    paid_tier: Option<TierEntry>,
    #[serde(rename = "allowedTiers")]
    allowed_tiers: Option<Vec<TierEntry>>,
}

#[derive(Debug, Deserialize)]
struct TierEntry {
    #[serde(rename = "isDefault")]
    is_default: Option<bool>,
    id: Option<String>,
    #[serde(rename = "quotaTier")]
    quota_tier: Option<String>,
    name: Option<String>,
    slug: Option<String>,
}

impl TierEntry {
    fn identifier(&self) -> Option<String> {
        self.id
            .as_deref()
            .or(self.quota_tier.as_deref())
            .or(self.slug.as_deref())
            .or(self.name.as_deref())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToString::to_string)
    }
}

/// 将上游 `paidTier` / `currentTier` 原始标识归一化为 `FREE` / `PRO` / `ULTRA` / `ENTERPRISE`。
pub fn normalize_subscription_tier(raw: Option<&str>) -> String {
    let Some(s) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
        return "FREE".to_string();
    };
    let lower = s.to_ascii_lowercase();
    if lower.contains("ultra") || lower.contains("helium") {
        "ULTRA".to_string()
    } else if lower.contains("free") || lower.contains("starter") {
        "FREE".to_string()
    } else if lower.contains("pro") || lower.contains("premium") || lower.contains("advanced") {
        "PRO".to_string()
    } else if lower.contains("enterprise")
        || lower.contains("standard")
        || lower.contains("business")
        || lower.starts_with("gcp_")
        || lower.contains("vertex")
    {
        "ENTERPRISE".to_string()
    } else {
        "FREE".to_string()
    }
}

fn extract_project_id_string(val: Option<&Value>) -> Option<String> {
    match val? {
        Value::String(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
        Value::Object(map) => map
            .get("id")
            .or_else(|| map.get("projectId"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToString::to_string),
        _ => None,
    }
}

pub fn parse_rfc3339_or_unix(raw: Option<&str>) -> Option<i64> {
    let text = raw?.trim();
    if text.is_empty() {
        return None;
    }
    if let Ok(sec) = text.parse::<i64>() {
        return Some(if sec > 10_000_000_000 { sec / 1000 } else { sec });
    }
    DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|dt| dt.timestamp())
}

fn extract_validation_url_from_error_text(body: &str) -> Option<String> {
    let unescaped = body.replace("\\u0026", "&").replace("\\/", "/");
    if let Ok(val) = serde_json::from_str::<Value>(&unescaped)
        && let Some(details) = val
            .get("error")
            .and_then(|e| e.get("details"))
            .and_then(Value::as_array)
    {
        for item in details {
            if let Some(meta) = item.get("metadata").and_then(Value::as_object) {
                for key in ["validation_url", "appeal_url", "verify_url"] {
                    if let Some(u) = meta.get(key).and_then(Value::as_str)
                        && u.starts_with("https://")
                    {
                        return Some(u.to_string());
                    }
                }
            }
            if let Some(links) = item.get("links").and_then(Value::as_array) {
                for link in links {
                    if let Some(u) = link.get("url").and_then(Value::as_str)
                        && u.starts_with("https://")
                    {
                        return Some(u.to_string());
                    }
                }
            }
        }
    }
    for token in unescaped.split(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | '<' | '>')) {
        if token.starts_with("https://") {
            return Some(token.trim_end_matches([',', '.', ')', ']']).to_string());
        }
    }
    None
}

async fn post_cloudcode_with_fallback(
    client: &reqwest::Client,
    access_token: &str,
    endpoints: &[&str],
    payload: &Value,
) -> Result<(u16, String), anyhow::Error> {
    let mut last_err = None;
    for (idx, url) in endpoints.iter().enumerate() {
        match client
            .post(*url)
            .bearer_auth(access_token.trim())
            .header("User-Agent", ANTIGRAVITY_HUB_USER_AGENT)
            .header("Content-Type", "application/json")
            .json(payload)
            .send()
            .await
        {
            Ok(resp) => {
                let status = resp.status();
                let code = status.as_u16();
                let text = resp.text().await.unwrap_or_default();
                if status.is_success() || code == 401 || code == 403 || idx + 1 == endpoints.len() {
                    return Ok((code, text));
                }
                last_err = Some(anyhow::anyhow!("HTTP {code}: {text}"));
            }
            Err(err) => {
                last_err = Some(err.into());
            }
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("请求 Cloud Code 接口失败")))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QuotaSummaryResponse {
    #[serde(default)]
    groups: Vec<QuotaSummaryGroup>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QuotaSummaryGroup {
    display_name: Option<String>,
    #[serde(default)]
    buckets: Vec<QuotaSummaryBucket>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QuotaSummaryBucket {
    bucket_id: Option<String>,
    window: Option<String>,
    remaining_fraction: Option<Value>,
    reset_time: Option<String>,
}

fn parse_remaining_percent(val: Option<&Value>) -> Option<f64> {
    let v = val?;
    let raw = match v {
        Value::Number(n) => n.as_f64()?,
        Value::String(s) => {
            let trimmed = s.trim();
            if let Some(pct) = trimmed.strip_suffix('%') {
                return pct.trim().parse::<f64>().ok().map(|p| p.clamp(0.0, 100.0));
            }
            trimmed.parse::<f64>().ok()?
        }
        _ => return None,
    };
    if !raw.is_finite() {
        return None;
    }
    let pct = if raw <= 1.000_001 { raw * 100.0 } else { raw };
    Some(((pct.clamp(0.0, 100.0)) * 100.0).round() / 100.0)
}

/// 查询单个 Antigravity 账号的完整配额快照（`loadCodeAssist` + `retrieveUserQuotaSummary` + `fetchAvailableModels`）。
pub async fn fetch_antigravity_quota(
    access_token: &str,
    existing_project_id: Option<&str>,
    proxy_spec: Option<&str>,
) -> anyhow::Result<AntigravityQuotaSnapshot> {
    let client = build_http_client_for_proxy(proxy_spec, 15)?;
    let now = Utc::now().timestamp();

    let preset_project_id = existing_project_id
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string);

    let assist_body = json!({ "metadata": { "ideType": "ANTIGRAVITY" } });

    let (assist_code, assist_text, summary_res, models_res) =
        if let Some(ref pid) = preset_project_id {
            let project_body = json!({ "project": pid });
            // 已知 project_id 时 3 个只读端点并发发起，将 3 轮跨洋 RTT 压缩为 1 轮
            let (assist_out, sum_out, mod_out) = tokio::join!(
                post_cloudcode_with_fallback(
                    &client,
                    access_token,
                    &LOAD_CODE_ASSIST_ENDPOINTS,
                    &assist_body,
                ),
                post_cloudcode_with_fallback(
                    &client,
                    access_token,
                    &QUOTA_SUMMARY_ENDPOINTS,
                    &project_body,
                ),
                post_cloudcode_with_fallback(
                    &client,
                    access_token,
                    &FETCH_MODELS_ENDPOINTS,
                    &project_body,
                ),
            );
            let (ac, at) = assist_out.context("请求 loadCodeAssist 失败")?;
            (ac, at, sum_out, mod_out)
        } else {
            // 首次无 project_id 时先取 loadCodeAssist，再并发查询后两项
            let (ac, at) = post_cloudcode_with_fallback(
                &client,
                access_token,
                &LOAD_CODE_ASSIST_ENDPOINTS,
                &assist_body,
            )
            .await
            .context("请求 loadCodeAssist 失败")?;

            if ac == 401 {
                bail!("HTTP 401: Antigravity access_token 已失效: {at}");
            }

            let parsed_pid = (200..300)
                .contains(&ac)
                .then(|| serde_json::from_str::<LoadProjectResponse>(&at).ok())
                .flatten()
                .and_then(|p| extract_project_id_string(p.project_id.as_ref()));
            let body = match parsed_pid.as_deref() {
                Some(pid) => json!({ "project": pid }),
                None => json!({}),
            };
            let (sum_out, mod_out) = tokio::join!(
                post_cloudcode_with_fallback(&client, access_token, &QUOTA_SUMMARY_ENDPOINTS, &body),
                post_cloudcode_with_fallback(&client, access_token, &FETCH_MODELS_ENDPOINTS, &body),
            );
            (ac, at, sum_out, mod_out)
        };

    if assist_code == 401 {
        bail!("HTTP 401: Antigravity access_token 已失效: {assist_text}");
    }

    let mut project_id = preset_project_id;
    let mut raw_tier_id: Option<String> = None;

    if (200..300).contains(&assist_code)
        && let Ok(parsed) = serde_json::from_str::<LoadProjectResponse>(&assist_text)
    {
        if let Some(pid) = extract_project_id_string(parsed.project_id.as_ref()) {
            project_id = Some(pid);
        }
        // paidTier 严格优先于 currentTier（Google AI Pro 账号的 currentTier 常仍为 free-tier）
        raw_tier_id = parsed
            .paid_tier
            .as_ref()
            .and_then(TierEntry::identifier)
            .or_else(|| parsed.current_tier.as_ref().and_then(TierEntry::identifier))
            .or_else(|| {
                parsed
                    .allowed_tiers
                    .as_ref()?
                    .iter()
                    .find(|t| t.is_default == Some(true) || t.id.as_deref() == Some("free-tier"))
                    .and_then(TierEntry::identifier)
            });
    }

    let (summary_code, summary_text) = summary_res?;
    if summary_code == 401 {
        bail!("HTTP 401: Antigravity access_token 已失效: {summary_text}");
    }

    let subscription_tier = normalize_subscription_tier(raw_tier_id.as_deref());

    if summary_code == 403 || assist_code == 403 {
        let err_body = if summary_code == 403 {
            &summary_text
        } else {
            &assist_text
        };
        let lower = err_body.to_ascii_lowercase();
        let validation_needed = [
            "validation_required",
            "verify your account",
            "further action is required",
            "validation_url",
        ]
        .iter()
        .any(|pat| lower.contains(pat));
        let validation_url = extract_validation_url_from_error_text(err_body);
        return Ok(AntigravityQuotaSnapshot {
            subscription_tier,
            raw_tier_id,
            project_id,
            groups: Vec::new(),
            available_model_ids: Vec::new(),
            is_forbidden: !validation_needed,
            forbidden_reason: (!validation_needed).then(|| err_body.chars().take(280).collect()),
            validation_blocked: validation_needed,
            validation_url,
            updated_at: now,
            error: Some(format!("HTTP 403: {}", err_body.chars().take(200).collect::<String>())),
        });
    }

    // 3. 解析 fetchAvailableModels 获取可用模型列表与补充配额信息
    let mut available_model_ids = Vec::new();
    let mut fallback_model_quotas: HashMap<String, (Option<f64>, Option<i64>)> = HashMap::new();
    if let Ok((models_code, models_text)) = models_res
        && (200..300).contains(&models_code)
        && let Ok(models_json) = serde_json::from_str::<Value>(&models_text)
        && let Some(models_obj) = models_json.get("models").and_then(Value::as_object)
    {
        for (model_id, info) in models_obj {
            available_model_ids.push(model_id.clone());
            if let Some(qinfo) = info.get("quotaInfo") {
                let pct = parse_remaining_percent(qinfo.get("remainingFraction"));
                let rst = parse_rfc3339_or_unix(
                    qinfo.get("resetTime").and_then(Value::as_str),
                );
                fallback_model_quotas.insert(model_id.clone(), (pct, rst));
            }
        }
        available_model_ids.sort();
    }

    let mut group_map: HashMap<String, AntigravityGroupQuota> = HashMap::new();
    if (200..300).contains(&summary_code) {
        let parsed_summary: Option<QuotaSummaryResponse> = serde_json::from_str(&summary_text)
            .ok()
            .or_else(|| {
                let outer: Value = serde_json::from_str(&summary_text).ok()?;
                let body = outer.get("body")?;
                if let Some(s) = body.as_str() {
                    serde_json::from_str(s).ok()
                } else {
                    serde_json::from_value(body.clone()).ok()
                }
            });

        if let Some(summary) = parsed_summary {
            for grp in summary.groups {
                let raw_name = grp.display_name.unwrap_or_default();
                let lower_name = raw_name.to_ascii_lowercase();
                for bucket in grp.buckets {
                    let bid = bucket.bucket_id.as_deref().unwrap_or("").to_ascii_lowercase();
                    let (gid, title) = if lower_name.contains("argon") || bid.contains("argon") {
                        ("argon", "Gemini 4 Argon")
                    } else if lower_name.contains("claude")
                        || lower_name.contains("gpt")
                        || bid.contains("3p")
                    {
                        ("claude_gpt", "Claude / GPT")
                    } else {
                        ("gemini", "Gemini")
                    };

                    let entry = group_map
                        .entry(gid.to_string())
                        .or_insert_with(|| AntigravityGroupQuota {
                            group_id: gid.to_string(),
                            display_name: title.to_string(),
                            five_hour_percentage: None,
                            five_hour_reset_time: None,
                            weekly_percentage: None,
                            weekly_reset_time: None,
                            weekly_exhausted: false,
                            sample_models: Vec::new(),
                        });

                    let pct = parse_remaining_percent(bucket.remaining_fraction.as_ref());
                    let rst = parse_rfc3339_or_unix(bucket.reset_time.as_deref());
                    let win = bucket.window.as_deref().unwrap_or("").to_ascii_lowercase();
                    if win.contains("5h") || win.contains("five") || bid.contains("5h") {
                        entry.five_hour_percentage = pct;
                        entry.five_hour_reset_time = rst;
                    } else if win.contains("week") || bid.contains("week") {
                        entry.weekly_percentage = pct;
                        entry.weekly_reset_time = rst;
                    } else if entry.five_hour_percentage.is_none() {
                        entry.five_hour_percentage = pct;
                        entry.five_hour_reset_time = rst;
                    }
                }
            }
        }
    }

    // 挂载代表模型并计算周配额木桶约束
    let has_3p_models = available_model_ids.iter().any(|m| {
        let l = m.to_ascii_lowercase();
        l.contains("claude") || l.contains("gpt")
    });
    if !has_3p_models && available_model_ids.len() > 2 {
        group_map.remove("claude_gpt");
    }
    if subscription_tier == "FREE" {
        group_map.remove("argon");
    }

    // 若 summary 未返回任何组但 fetchAvailableModels 有 quotaInfo，做兜底组装
    if group_map.is_empty() && !fallback_model_quotas.is_empty() {
        for (mid, (pct, rst)) in &fallback_model_quotas {
            let l = mid.to_ascii_lowercase();
            let (gid, title) = if l.contains("claude") || l.contains("gpt") {
                ("claude_gpt", "Claude / GPT")
            } else {
                ("gemini", "Gemini")
            };
            let entry = group_map
                .entry(gid.to_string())
                .or_insert_with(|| AntigravityGroupQuota {
                    group_id: gid.to_string(),
                    display_name: title.to_string(),
                    five_hour_percentage: *pct,
                    five_hour_reset_time: *rst,
                    weekly_percentage: None,
                    weekly_reset_time: None,
                    weekly_exhausted: false,
                    sample_models: Vec::new(),
                });
            if let (Some(cur), Some(next)) = (entry.five_hour_percentage, *pct)
                && next < cur
            {
                entry.five_hour_percentage = Some(next);
                entry.five_hour_reset_time = *rst;
            }
        }
    }

    let mut groups: Vec<AntigravityGroupQuota> = Vec::new();
    for key in ["gemini", "claude_gpt", "argon"] {
        if let Some(mut grp) = group_map.remove(key) {
            if subscription_tier == "FREE" {
                // Free 账号仅展示周配额窗口
                if grp.weekly_percentage.is_none() {
                    grp.weekly_percentage = grp.five_hour_percentage.take();
                    grp.weekly_reset_time = grp.five_hour_reset_time.take();
                } else {
                    grp.five_hour_percentage = None;
                    grp.five_hour_reset_time = None;
                }
            }
            if let Some(weekly) = grp.weekly_percentage
                && weekly <= 1.5
            {
                grp.weekly_exhausted = true;
                if grp.five_hour_percentage.is_some() {
                    grp.five_hour_percentage = Some(0.0);
                }
            }
            grp.sample_models = available_model_ids
                .iter()
                .filter(|m| {
                    let l = m.to_ascii_lowercase();
                    match key {
                        "argon" => l.contains("argon"),
                        "claude_gpt" => l.contains("claude") || l.contains("gpt"),
                        _ => l.contains("gemini") && !l.contains("argon"),
                    }
                })
                .take(4)
                .cloned()
                .collect();
            groups.push(grp);
        }
    }

    Ok(AntigravityQuotaSnapshot {
        subscription_tier,
        raw_tier_id,
        project_id,
        groups,
        available_model_ids,
        is_forbidden: false,
        forbidden_reason: None,
        validation_blocked: false,
        validation_url: None,
        updated_at: now,
        error: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_google_subscription_tiers_accurately() {
        assert_eq!(normalize_subscription_tier(Some("free-tier")), "FREE");
        assert_eq!(
            normalize_subscription_tier(Some("Antigravity Starter Quota")),
            "FREE"
        );
        assert_eq!(normalize_subscription_tier(Some("g1-pro-tier")), "PRO");
        assert_eq!(normalize_subscription_tier(Some("Google AI Pro")), "PRO");
        assert_eq!(normalize_subscription_tier(Some("g1-ultra-tier")), "ULTRA");
        assert_eq!(
            normalize_subscription_tier(Some("GCP_ENTERPRISE_TIER")),
            "ENTERPRISE"
        );
        assert_eq!(normalize_subscription_tier(None), "FREE");
    }
}
