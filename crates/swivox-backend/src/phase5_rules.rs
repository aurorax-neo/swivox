//! Phase 5 下沉纯业务规则：语音听写预设/校验、插件市场过滤分页、用户脚本与脚本市场解析、Agent 缓存分组统计与字节格式化。

use std::collections::HashSet;

use swivox_core::settings::{DictationProvider, DictationSettings};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::phase4_rules::matches_management_query;

// ============================================================================
// 1. 语音听写预设与校验（100% 对应 dictation-settings.ts）
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DictationPreset {
    Groq,
    OpenAi,
    Local,
    SenseVoice,
    Custom,
}

impl DictationPreset {
    pub fn label(self) -> &'static str {
        match self {
            Self::Groq => "Groq Whisper",
            Self::OpenAi => "OpenAI Whisper",
            Self::Local => "Local HTTP (127.0.0.1:8000)",
            Self::SenseVoice => "离线 SenseVoice (本机 ONNX)",
            Self::Custom => "自定义端点",
        }
    }
}

pub fn detect_dictation_preset(settings: &DictationSettings) -> DictationPreset {
    if settings.provider == DictationProvider::SenseVoice
        || settings.base_url.eq_ignore_ascii_case("local://sensevoice")
    {
        return DictationPreset::SenseVoice;
    }
    match settings.base_url.trim().trim_end_matches('/') {
        "https://api.groq.com/openai/v1" => DictationPreset::Groq,
        "https://api.openai.com/v1" => DictationPreset::OpenAi,
        "http://127.0.0.1:8000/v1" => DictationPreset::Local,
        _ => DictationPreset::Custom,
    }
}

pub fn apply_dictation_preset(settings: &mut DictationSettings, preset: DictationPreset) {
    match preset {
        DictationPreset::Groq => {
            settings.provider = DictationProvider::Api;
            settings.base_url = "https://api.groq.com/openai/v1".to_string();
            settings.model = "whisper-large-v3-turbo".to_string();
        }
        DictationPreset::OpenAi => {
            settings.provider = DictationProvider::Api;
            settings.base_url = "https://api.openai.com/v1".to_string();
            settings.model = "whisper-1".to_string();
        }
        DictationPreset::Local => {
            settings.provider = DictationProvider::Api;
            settings.base_url = "http://127.0.0.1:8000/v1".to_string();
            settings.model = "whisper-1".to_string();
        }
        DictationPreset::SenseVoice => {
            settings.provider = DictationProvider::SenseVoice;
            settings.base_url = "local://sensevoice".to_string();
            settings.model = "SenseVoiceSmall (INT8)".to_string();
        }
        DictationPreset::Custom => {
            settings.provider = DictationProvider::Api;
        }
    }
}

pub fn validate_dictation_settings(settings: &DictationSettings) -> Option<&'static str> {
    if !settings.enabled || settings.provider == DictationProvider::SenseVoice {
        return None;
    }
    let url = settings.base_url.trim();
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Some("请填写以 http:// 或 https:// 开头的转写服务 Base URL");
    }
    if settings.model.trim().is_empty() {
        return Some("请填写语音转写模型名称");
    }
    let env_name = settings.api_key_env.trim();
    if !env_name.is_empty() {
        let mut chars = env_name.chars();
        let valid_head = chars
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_');
        let valid_tail = chars.all(|c| c.is_ascii_alphanumeric() || c == '_');
        if !valid_head || !valid_tail {
            return Some("API Key 环境变量名只能包含字母、数字和下划线，且不能以数字开头");
        }
    }
    None
}

// ============================================================================
// 2. Codex 插件市场模型（100% 对应 plugin-market-model.ts）
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MarketSource {
    #[default]
    Public,
    Full,
}

impl MarketSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Full => "full",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MarketFilter {
    #[default]
    All,
    Available,
    Installed,
    Updates,
    Auth,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct MarketPluginItem {
    pub id: String,
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub version: String,
    pub author: String,
    pub tags: Vec<String>,
    pub license: String,
    pub path: String,
    pub bytes: u64,
    pub skills: usize,
    pub requires_auth: bool,
    pub installed: bool,
    pub installed_version: String,
    pub update_available: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct MarketCatalogView {
    pub status: String,
    pub message: String,
    pub source: String,
    pub repository: String,
    pub total: usize,
    pub plugins: Vec<MarketPluginItem>,
}

pub fn parse_plugin_market_catalog(raw: &Value) -> MarketCatalogView {
    serde_json::from_value::<MarketCatalogView>(raw.clone()).unwrap_or_default()
}

pub fn filter_market_plugins(
    plugins: &[MarketPluginItem],
    query: &str,
    filter: MarketFilter,
) -> Vec<MarketPluginItem> {
    plugins
        .iter()
        .filter(|p| match filter {
            MarketFilter::All => true,
            MarketFilter::Available => !p.installed,
            MarketFilter::Installed => p.installed,
            MarketFilter::Updates => p.update_available,
            MarketFilter::Auth => p.requires_auth,
        })
        .filter(|p| {
            let tag_joined = p.tags.join(" ");
            matches_management_query(
                &[
                    &p.id,
                    &p.name,
                    &p.display_name,
                    &p.description,
                    &p.author,
                    &tag_joined,
                ],
                query,
            )
        })
        .cloned()
        .collect()
}

// ============================================================================
// 3. 用户脚本清单与脚本市场解析
// ============================================================================

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct LocalUserScriptView {
    pub key: String,
    pub name: String,
    pub source: String,
    pub enabled: bool,
    pub status: String,
    pub error: String,
    pub market_id: String,
    pub version: String,
    pub homepage: String,
}

#[derive(Debug, Clone, Default)]
pub struct UserScriptInventoryView {
    pub enabled: bool,
    pub user_dir: String,
    pub scripts: Vec<LocalUserScriptView>,
}

pub fn parse_user_script_inventory(raw: &Value) -> UserScriptInventoryView {
    let enabled = raw.get("enabled").and_then(Value::as_bool).unwrap_or(true);
    let user_dir = raw
        .get("user_dir")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let scripts = raw
        .get("scripts")
        .and_then(|v| serde_json::from_value::<Vec<LocalUserScriptView>>(v.clone()).ok())
        .unwrap_or_default();
    UserScriptInventoryView {
        enabled,
        user_dir,
        scripts,
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct MarketScriptView {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub author: String,
    pub tags: Vec<String>,
    pub homepage: String,
    pub installed: bool,
    pub installed_version: String,
    pub update_available: bool,
}

pub fn parse_script_market_list(raw: &Value) -> Vec<MarketScriptView> {
    raw.get("scripts")
        .and_then(|v| serde_json::from_value::<Vec<MarketScriptView>>(v.clone()).ok())
        .unwrap_or_default()
}

// ============================================================================
// 4. AI Agent 缓存报告解析与推荐选择（100% 对应 agent-cache-model.ts）
// ============================================================================

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AgentCacheGroupView {
    pub id: String,
    pub app: String,
    pub kind: String,
    pub path: String,
    pub total_bytes: u64,
    pub eligible_bytes: u64,
    pub eligible_files: usize,
    pub skipped_files: usize,
    pub cleanable: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AgentCacheReportView {
    pub scan_id: String,
    pub groups: Vec<AgentCacheGroupView>,
    pub warnings: Vec<String>,
}

pub fn parse_agent_cache_scan_payload(payload: &Value) -> Option<AgentCacheReportView> {
    let report = payload.get("report").unwrap_or(payload);
    serde_json::from_value::<AgentCacheReportView>(report.clone()).ok()
}

pub fn recommended_cache_selection(groups: &[AgentCacheGroupView], include_logs: bool) -> HashSet<String> {
    groups
        .iter()
        .filter(|g| g.cleanable && g.eligible_files > 0)
        .filter(|g| include_logs || g.kind != "logs")
        .map(|g| g.id.clone())
        .collect()
}

pub fn format_bytes_human(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = 1024.0 * 1024.0;
    const GB: f64 = 1024.0 * 1024.0 * 1024.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.2} GB", b / GB)
    } else if b >= MB {
        format!("{:.1} MB", b / MB)
    } else if b >= KB {
        format!("{:.1} KB", b / KB)
    } else {
        format!("{bytes} B")
    }
}

// ============================================================================
// 5. 在线更新检查结果解析
// ============================================================================

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct UpdateCheckView {
    pub current_version: String,
    pub latest_version: Option<String>,
    pub release_summary: String,
    pub asset_name: Option<String>,
    pub asset_url: Option<String>,
    pub update_available: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dictation_preset_roundtrips_and_validates_env_identifier() {
        let mut s = DictationSettings::default();
        assert_eq!(detect_dictation_preset(&s), DictationPreset::Groq);

        apply_dictation_preset(&mut s, DictationPreset::SenseVoice);
        assert_eq!(detect_dictation_preset(&s), DictationPreset::SenseVoice);
        s.enabled = true;
        assert!(validate_dictation_settings(&s).is_none());

        apply_dictation_preset(&mut s, DictationPreset::OpenAi);
        s.api_key_env = "9INVALID".to_string();
        assert!(validate_dictation_settings(&s).is_some());
        s.api_key_env = "OPENAI_API_KEY".to_string();
        assert!(validate_dictation_settings(&s).is_none());
    }

    #[test]
    fn recommended_cache_selection_excludes_logs_by_default() {
        let groups = vec![
            AgentCacheGroupView {
                id: "codex-gpu".to_string(),
                app: "Codex".to_string(),
                kind: "gpu".to_string(),
                path: "/tmp/gpu".to_string(),
                total_bytes: 2048,
                eligible_bytes: 2048,
                eligible_files: 3,
                skipped_files: 0,
                cleanable: true,
            },
            AgentCacheGroupView {
                id: "codex-logs".to_string(),
                app: "Codex".to_string(),
                kind: "logs".to_string(),
                path: "/tmp/logs".to_string(),
                total_bytes: 1024,
                eligible_bytes: 1024,
                eligible_files: 2,
                skipped_files: 0,
                cleanable: true,
            },
        ];
        let rec = recommended_cache_selection(&groups, false);
        assert!(rec.contains("codex-gpu"));
        assert!(!rec.contains("codex-logs"));
        let all = recommended_cache_selection(&groups, true);
        assert!(all.contains("codex-logs"));
    }

    #[test]
    fn format_bytes_human_scales_cleanly() {
        assert_eq!(format_bytes_human(512), "512 B");
        assert_eq!(format_bytes_human(2048), "2.0 KB");
        assert_eq!(format_bytes_human(5 * 1024 * 1024), "5.0 MB");
    }
}
