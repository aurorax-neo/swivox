//! Phase 3 下沉纯业务规则：供应商预设目录、4 大分区判定、每模型上下文窗口/自动压缩表格序列化与 Profile 编辑同步。

use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::{SystemTime, UNIX_EPOCH};

use swivox_core::model_suffix::parse_model_suffix;
use swivox_core::relay_config::{
    normalize_relay_profile_for_storage, relay_profile_api_key, relay_profile_base_url,
    relay_profile_model,
};
use swivox_core::settings::{
    AggregateRelayMember, AggregateRelayProfile, AggregateRelayStrategy, BackendSettings,
    RelayMode, RelayProfile, RelayProtocol, RelaySessionProvider,
};
use serde_json::Value;

use crate::i18n::t;

pub const DEFAULT_AUTO_COMPACT_PERCENT: &str = "90";

// ============================================================================
// 1. 内置供应商预设目录（100% 对应 presets.ts）
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresetCategory {
    Official,
    CnOfficial,
    Aggregator,
    ThirdParty,
}

impl PresetCategory {
    pub fn label(self) -> String {
        match self {
            Self::Official => t("官方"),
            Self::CnOfficial => t("模型厂商"),
            Self::Aggregator => t("聚合/中转"),
            Self::ThirdParty => t("第三方平台"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProviderPreset {
    pub id: &'static str,
    pub name: &'static str,
    pub website_url: &'static str,
    pub category: PresetCategory,
    pub base_url: &'static str,
    pub protocol: RelayProtocol,
    pub model: &'static str,
    pub model_list: &'static [&'static str],
}

pub const PROVIDER_PRESETS: &[ProviderPreset] = &[
    ProviderPreset {
        id: "deepseek",
        name: "DeepSeek",
        website_url: "https://platform.deepseek.com",
        category: PresetCategory::CnOfficial,
        base_url: "https://api.deepseek.com/",
        protocol: RelayProtocol::Responses,
        model: "deepseek-v4-flash",
        model_list: &["deepseek-v4-flash", "deepseek-v4-pro"],
    },
    ProviderPreset {
        id: "zhipu-glm",
        name: "Zhipu GLM",
        website_url: "https://open.bigmodel.cn",
        category: PresetCategory::CnOfficial,
        base_url: "https://open.bigmodel.cn/api/coding/paas/v4",
        protocol: RelayProtocol::ChatCompletions,
        model: "glm-5.1",
        model_list: &["glm-5.1"],
    },
    ProviderPreset {
        id: "kimi",
        name: "Kimi",
        website_url: "https://platform.moonshot.cn",
        category: PresetCategory::CnOfficial,
        base_url: "https://api.moonshot.cn/v1",
        protocol: RelayProtocol::ChatCompletions,
        model: "kimi-k2.6",
        model_list: &["kimi-k2.6"],
    },
    ProviderPreset {
        id: "bailian",
        name: "Bailian (Qwen)",
        website_url: "https://bailian.console.aliyun.com",
        category: PresetCategory::CnOfficial,
        base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1",
        protocol: RelayProtocol::ChatCompletions,
        model: "qwen3-coder-plus",
        model_list: &["qwen3-coder-plus", "qwen3-max"],
    },
    ProviderPreset {
        id: "stepfun",
        name: "StepFun",
        website_url: "https://platform.stepfun.com/step-plan",
        category: PresetCategory::CnOfficial,
        base_url: "https://api.stepfun.com/step_plan/v1",
        protocol: RelayProtocol::ChatCompletions,
        model: "step-3.5-flash-2603",
        model_list: &["step-3.5-flash-2603", "step-3.5-flash"],
    },
    ProviderPreset {
        id: "minimax",
        name: "MiniMax (China)",
        website_url: "https://platform.minimaxi.com",
        category: PresetCategory::CnOfficial,
        base_url: "https://api.minimaxi.com/v1",
        protocol: RelayProtocol::ChatCompletions,
        model: "MiniMax-M3",
        model_list: &["MiniMax-M3", "MiniMax-M2.7"],
    },
    ProviderPreset {
        id: "volcano-ark",
        name: "火山引擎 Ark",
        website_url: "https://www.volcengine.com/product/ark",
        category: PresetCategory::CnOfficial,
        base_url: "https://ark.cn-beijing.volces.com/api/coding/v3",
        protocol: RelayProtocol::ChatCompletions,
        model: "ark-code-latest",
        model_list: &["ark-code-latest"],
    },
    ProviderPreset {
        id: "baidu-qianfan",
        name: "百度千帆 Coding Plan",
        website_url: "https://cloud.baidu.com/product/qianfan_modelbuilder",
        category: PresetCategory::CnOfficial,
        base_url: "https://qianfan.baidubce.com/v2/coding",
        protocol: RelayProtocol::ChatCompletions,
        model: "qianfan-code-latest",
        model_list: &["qianfan-code-latest"],
    },
    ProviderPreset {
        id: "xiaomi-mimo",
        name: "小米 MiMo",
        website_url: "https://platform.xiaomimimo.com",
        category: PresetCategory::CnOfficial,
        base_url: "https://api.xiaomimimo.com/v1",
        protocol: RelayProtocol::ChatCompletions,
        model: "mimo-v2.5-pro",
        model_list: &["mimo-v2.5-pro"],
    },
    ProviderPreset {
        id: "modelscope",
        name: "ModelScope",
        website_url: "https://modelscope.cn",
        category: PresetCategory::CnOfficial,
        base_url: "https://api-inference.modelscope.cn/v1",
        protocol: RelayProtocol::ChatCompletions,
        model: "ZhipuAI/GLM-5.1",
        model_list: &["ZhipuAI/GLM-5.1"],
    },
    ProviderPreset {
        id: "longcat",
        name: "Longcat",
        website_url: "https://longcat.chat/platform",
        category: PresetCategory::CnOfficial,
        base_url: "https://api.longcat.chat/openai/v1",
        protocol: RelayProtocol::ChatCompletions,
        model: "LongCat-Flash-Chat",
        model_list: &["LongCat-Flash-Chat"],
    },
    ProviderPreset {
        id: "jojocode",
        name: "JOJO Code",
        website_url: "https://jojocode.com/",
        category: PresetCategory::Aggregator,
        base_url: "https://jojocode.com/v1",
        protocol: RelayProtocol::Responses,
        model: "gpt-5.5",
        model_list: &["gpt-5.5"],
    },
    ProviderPreset {
        id: "jojocode-max",
        name: "JOJO Code 包月",
        website_url: "https://max.jojocode.com/",
        category: PresetCategory::Aggregator,
        base_url: "https://max.jojocode.com/v1",
        protocol: RelayProtocol::Responses,
        model: "gpt-5.5",
        model_list: &["gpt-5.5"],
    },
    ProviderPreset {
        id: "runapi",
        name: "RunAPI",
        website_url: "https://runapi.host",
        category: PresetCategory::Aggregator,
        base_url: "https://runapi.host/v1",
        protocol: RelayProtocol::Responses,
        model: "gpt-5.5",
        model_list: &["gpt-5.5"],
    },
    ProviderPreset {
        id: "grooroute",
        name: "GrooRoute",
        website_url: "https://grooroute.com/",
        category: PresetCategory::Aggregator,
        base_url: "https://grooroute.com",
        protocol: RelayProtocol::Responses,
        model: "gpt-5.5",
        model_list: &["gpt-5.5"],
    },
    ProviderPreset {
        id: "siliconflow",
        name: "SiliconFlow",
        website_url: "https://siliconflow.cn",
        category: PresetCategory::Aggregator,
        base_url: "https://api.siliconflow.cn/v1",
        protocol: RelayProtocol::ChatCompletions,
        model: "Pro/MiniMaxAI/MiniMax-M2.7",
        model_list: &["Pro/MiniMaxAI/MiniMax-M2.7"],
    },
    ProviderPreset {
        id: "openrouter",
        name: "OpenRouter",
        website_url: "https://openrouter.ai",
        category: PresetCategory::Aggregator,
        base_url: "https://openrouter.ai/api/v1",
        protocol: RelayProtocol::ChatCompletions,
        model: "gpt-5.5",
        model_list: &["gpt-5.5"],
    },
    ProviderPreset {
        id: "aihubmix",
        name: "AiHubMix",
        website_url: "https://aihubmix.com",
        category: PresetCategory::Aggregator,
        base_url: "https://aihubmix.com/v1",
        protocol: RelayProtocol::Responses,
        model: "gpt-5.5",
        model_list: &["gpt-5.5"],
    },
    ProviderPreset {
        id: "apikeyfun",
        name: "APIKEY.FUN",
        website_url: "https://apikey.fun",
        category: PresetCategory::Aggregator,
        base_url: "https://api.apikey.fun/v1",
        protocol: RelayProtocol::Responses,
        model: "gpt-5.5",
        model_list: &["gpt-5.5"],
    },
    ProviderPreset {
        id: "pateway",
        name: "PatewayAI",
        website_url: "https://pateway.ai",
        category: PresetCategory::Aggregator,
        base_url: "https://api.pateway.ai/v1",
        protocol: RelayProtocol::Responses,
        model: "gpt-5.5",
        model_list: &["gpt-5.5"],
    },
    ProviderPreset {
        id: "therouter",
        name: "TheRouter",
        website_url: "https://therouter.ai",
        category: PresetCategory::Aggregator,
        base_url: "https://api.therouter.ai/v1",
        protocol: RelayProtocol::ChatCompletions,
        model: "openai/gpt-5.3-codex",
        model_list: &["openai/gpt-5.3-codex"],
    },
    ProviderPreset {
        id: "novita",
        name: "Novita AI",
        website_url: "https://novita.ai",
        category: PresetCategory::Aggregator,
        base_url: "https://api.novita.ai/openai/v1",
        protocol: RelayProtocol::ChatCompletions,
        model: "zai-org/glm-5.1",
        model_list: &["zai-org/glm-5.1"],
    },
    ProviderPreset {
        id: "shengsuanyun",
        name: "Shengsuanyun",
        website_url: "https://www.shengsuanyun.com",
        category: PresetCategory::Aggregator,
        base_url: "https://router.shengsuanyun.com/api/v1",
        protocol: RelayProtocol::ChatCompletions,
        model: "openai/gpt-5.5",
        model_list: &["openai/gpt-5.5"],
    },
    ProviderPreset {
        id: "ccsub",
        name: "CCSub",
        website_url: "https://www.ccsub.net",
        category: PresetCategory::Aggregator,
        base_url: "https://www.ccsub.net/v1",
        protocol: RelayProtocol::Responses,
        model: "gpt-5.5",
        model_list: &["gpt-5.5"],
    },
    ProviderPreset {
        id: "azure",
        name: "Azure OpenAI",
        website_url: "https://learn.microsoft.com/en-us/azure/ai-foundry/openai/how-to/codex",
        category: PresetCategory::ThirdParty,
        base_url: "https://YOUR_RESOURCE_NAME.openai.azure.com/openai",
        protocol: RelayProtocol::Responses,
        model: "gpt-5.5",
        model_list: &["gpt-5.5"],
    },
];

pub const CONTEXT_WINDOW_PRESETS: &[(&str, u64)] = &[
    ("128k", 128_000),
    ("200k", 200_000),
    ("256k", 256_000),
    ("312k", 312_000),
    ("500k", 500_000),
    ("1M", 1_000_000),
    ("2M", 2_000_000),
];

// ============================================================================
// 2. 供应商分区视图判定（直连 / 模型聚合 / 需协议转换 / 故障转移）
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProviderViewKind {
    #[default]
    Direct,
    ModelAggregation,
    Routing,
    Failover,
}

pub fn relay_provider_view(
    profile: &RelayProfile,
    aggregates: &HashMap<String, AggregateRelayProfile>,
) -> ProviderViewKind {
    if profile.relay_mode == RelayMode::Aggregate {
        let model_agg = aggregates
            .get(&profile.id)
            .is_some_and(|a| a.model_aggregation_enabled);
        return if model_agg {
            ProviderViewKind::ModelAggregation
        } else {
            ProviderViewKind::Failover
        };
    }

    let has_routes = profile
        .model_routes
        .iter()
        .any(|r| !r.model.trim().is_empty() && !r.target_relay_id.trim().is_empty());

    if profile.protocol == RelayProtocol::ChatCompletions || has_routes {
        ProviderViewKind::Routing
    } else {
        ProviderViewKind::Direct
    }
}

// ============================================================================
// 3. 每模型上下文窗口与自动压缩表格规则（对应 model-windows.ts & auto-compact.ts）
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelWindowRow {
    pub model: String,
    pub window: String,
    pub auto_compact: String,
    pub image_handling: String,
}

pub fn model_slug_from_row_name(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let (slug, _) = parse_model_suffix(trimmed);
    slug
}

pub fn normalize_auto_compact_percent(value: &str) -> String {
    let trimmed = value.trim().trim_end_matches('%').trim();
    if trimmed.is_empty() {
        return DEFAULT_AUTO_COMPACT_PERCENT.to_string();
    }
    if let Ok(num) = trimmed.parse::<u32>()
        && (10..=100).contains(&num)
    {
        return num.to_string();
    }
    trimmed.to_string()
}

pub fn is_valid_auto_compact_percent(value: &str) -> bool {
    let trimmed = value.trim().trim_end_matches('%').trim();
    if trimmed.is_empty() {
        return true;
    }
    trimmed
        .parse::<u32>()
        .is_ok_and(|num| (10..=100).contains(&num))
}

pub fn is_valid_model_window(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return true;
    }
    let (digits, mult): (&str, u128) = if let Some(base) = trimmed.strip_suffix(['m', 'M']) {
        (base.trim(), 1_000_000)
    } else if let Some(base) = trimmed.strip_suffix(['k', 'K']) {
        (base.trim(), 1_000)
    } else {
        (trimmed, 1)
    };
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    let Ok(n) = digits.parse::<u128>() else {
        return false;
    };
    let tokens = n.saturating_mul(mult);
    tokens > 0 && tokens <= u64::MAX as u128
}

fn parse_json_str_map(json_text: &str) -> BTreeMap<String, String> {
    let Ok(Value::Object(obj)) = serde_json::from_str::<Value>(json_text) else {
        return BTreeMap::new();
    };
    let mut out = BTreeMap::new();
    for (k, v) in obj {
        if let Some(s) = v.as_str() {
            out.insert(k, s.to_string());
        }
    }
    out
}

fn lookup_model_map<'a>(map: &'a BTreeMap<String, String>, model_name: &str) -> Option<&'a str> {
    let raw = model_name.trim();
    if raw.is_empty() {
        return None;
    }
    let slug = model_slug_from_row_name(raw);
    if let Some(v) = map.get(&slug).or_else(|| map.get(raw)) {
        return Some(v.as_str());
    }
    for (k, v) in map {
        if k.eq_ignore_ascii_case(&slug) {
            return Some(v.as_str());
        }
    }
    None
}

pub fn model_window_rows_from_profile(profile: &RelayProfile) -> Vec<ModelWindowRow> {
    let win_map = parse_json_str_map(&profile.model_windows);
    let compact_map = parse_json_str_map(&profile.model_auto_compact);
    let raw_vlm_map = parse_json_str_map(&profile.model_vlm);
    let mut vlm_map = BTreeMap::new();
    for (m, mode) in raw_vlm_map {
        let key = model_slug_from_row_name(&m);
        if !key.is_empty() && matches!(mode.as_str(), "vlm" | "strip") {
            vlm_map.insert(key, mode);
        }
    }

    let mut rows: Vec<ModelWindowRow> = profile
        .model_list
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|line| {
            let mut win = lookup_model_map(&win_map, line)
                .unwrap_or("")
                .trim()
                .to_string();
            if win.is_empty()
                && let (_, Some(tokens)) = parse_model_suffix(line)
            {
                win = format_tokens_compact(tokens);
            }
            let compact = lookup_model_map(&compact_map, line)
                .map(normalize_auto_compact_percent)
                .unwrap_or_else(|| DEFAULT_AUTO_COMPACT_PERCENT.to_string());
            let img = lookup_model_map(&vlm_map, line)
                .unwrap_or("send-as-is")
                .to_string();
            ModelWindowRow {
                model: line.to_string(),
                window: win,
                auto_compact: compact,
                image_handling: img,
            }
        })
        .collect();

    let configured = relay_profile_model(profile);
    ensure_configured_model_row(&mut rows, &configured);
    rows
}

pub fn format_tokens_compact(tokens: u64) -> String {
    if tokens >= 1_000_000 && tokens.is_multiple_of(1_000_000) {
        format!("{}M", tokens / 1_000_000)
    } else if tokens == 1_048_576 {
        "1M".to_string()
    } else if tokens == 2_097_152 {
        "2M".to_string()
    } else if tokens >= 1_000 && tokens.is_multiple_of(1_000) {
        format!("{}k", tokens / 1_000)
    } else {
        tokens.to_string()
    }
}

pub fn ensure_configured_model_row(rows: &mut Vec<ModelWindowRow>, configured_model: &str) {
    let trimmed = configured_model.trim();
    if trimmed.is_empty() {
        return;
    }
    let target_slug = model_slug_from_row_name(trimmed);
    let exists = rows.iter().any(|r| {
        let m = r.model.trim();
        m == trimmed || model_slug_from_row_name(m) == target_slug
    });
    if !exists {
        rows.push(ModelWindowRow {
            model: trimmed.to_string(),
            window: String::new(),
            auto_compact: DEFAULT_AUTO_COMPACT_PERCENT.to_string(),
            image_handling: "send-as-is".to_string(),
        });
    }
}

pub fn validate_model_window_rows(rows: &[ModelWindowRow]) -> Result<(), String> {
    let mut seen = HashSet::new();
    for row in rows {
        let model = row.model.trim();
        if model.is_empty() {
            continue;
        }
        let slug = model_slug_from_row_name(model);
        let key = if slug.is_empty() {
            model.to_string()
        } else {
            slug
        };
        if !seen.insert(key) {
            return Err(format!("{}：{model}", t("模型列表中存在重复模型")));
        }
        if !is_valid_model_window(&row.window) {
            return Err(format!(
                "{}：{model} ({})",
                t("上下文窗口格式无效（示例：128k、1M 或正整数）"),
                row.window
            ));
        }
        if !is_valid_auto_compact_percent(&row.auto_compact) {
            return Err(format!(
                "{}：{model} ({})",
                t("自动压缩百分比需在 10~100 之间"),
                row.auto_compact
            ));
        }
    }
    Ok(())
}

pub struct SerializedModelRows {
    pub model_list: String,
    pub model_windows: String,
    pub model_auto_compact: String,
    pub model_vlm: String,
}

pub fn serialize_model_window_rows(rows: &[ModelWindowRow]) -> SerializedModelRows {
    let mut seen = HashSet::new();
    let mut list = Vec::new();
    let mut windows = BTreeMap::new();
    let mut auto_compact = BTreeMap::new();
    let mut vlm = BTreeMap::new();

    for row in rows {
        let model = row.model.trim();
        if model.is_empty() {
            continue;
        }
        let slug = model_slug_from_row_name(model);
        let key = if slug.is_empty() {
            model.to_string()
        } else {
            slug
        };
        if !seen.insert(key.clone()) {
            continue;
        }
        list.push(model.to_string());
        let w = row.window.trim();
        if !w.is_empty() {
            windows.insert(key.clone(), w.to_string());
        }
        if matches!(row.image_handling.as_str(), "vlm" | "strip") {
            vlm.insert(key.clone(), row.image_handling.clone());
        }
        auto_compact.insert(key, normalize_auto_compact_percent(&row.auto_compact));
    }

    SerializedModelRows {
        model_list: list.join("\n"),
        model_windows: serde_json::to_string(&windows).unwrap_or_else(|_| "{}".to_string()),
        model_auto_compact: serde_json::to_string(&auto_compact)
            .unwrap_or_else(|_| "{}".to_string()),
        model_vlm: serde_json::to_string(&vlm).unwrap_or_else(|_| "{}".to_string()),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelWindowPreview {
    pub matched: bool,
    pub window_compact: String,
    pub source: String,
}

/// 在不改动上游原始模型名的前提下，预览该模型未手填窗口时的实际生效窗口：
/// - 若带 `[1M]` 行内后缀，显示该后缀值；
/// - 若命中内置 / 运行时 / `models.dev` 元数据，返回匹配窗口与来源名；
/// - 若均未命中，返回默认保底 `1M`（`UNMATCHED_FALLBACK_CONTEXT_WINDOW`）。
pub fn resolve_model_window_preview(model_raw: &str) -> ModelWindowPreview {
    let trimmed = model_raw.trim();
    if trimmed.is_empty() {
        return ModelWindowPreview {
            matched: false,
            window_compact: format_tokens_compact(
                swivox_core::model_suffix::UNMATCHED_FALLBACK_CONTEXT_WINDOW,
            ),
            source: String::new(),
        };
    }
    let (slug, inline_win) = parse_model_suffix(trimmed);
    if let Some(tokens) = inline_win {
        return ModelWindowPreview {
            matched: true,
            window_compact: format_tokens_compact(tokens),
            source: "suffix".to_string(),
        };
    }
    if let Some(meta) = swivox_core::model_suffix::builtin_model_metadata(&slug)
        && let Some(tokens) = meta
            .entry
            .get("context_window")
            .or_else(|| meta.entry.get("max_context_window"))
            .and_then(Value::as_u64)
            .filter(|v| *v > 0)
    {
        return ModelWindowPreview {
            matched: true,
            window_compact: format_tokens_compact(tokens),
            source: meta.source,
        };
    }
    ModelWindowPreview {
        matched: false,
        window_compact: format_tokens_compact(
            swivox_core::model_suffix::UNMATCHED_FALLBACK_CONTEXT_WINDOW,
        ),
        source: String::new(),
    }
}

pub fn merge_fetched_models_with_windows_into_rows(
    rows: &mut Vec<ModelWindowRow>,
    fetched: &[String],
    upstream_windows: &HashMap<String, u64>,
) -> usize {
    let mut existing: HashSet<String> = rows
        .iter()
        .map(|r| model_slug_from_row_name(&r.model))
        .filter(|s| !s.is_empty())
        .collect();
    let mut added = 0usize;
    for item in fetched {
        let trimmed = item.trim();
        if trimmed.is_empty() {
            continue;
        }
        let slug = model_slug_from_row_name(trimmed);
        if slug.is_empty() || !existing.insert(slug) {
            continue;
        }
        // 严格保留上游原始模型名 trimmed；仅当上游 /v1/models 明确声明了窗口时填入 window
        let win = upstream_windows
            .get(trimmed)
            .copied()
            .filter(|v| *v > 0)
            .map(format_tokens_compact)
            .unwrap_or_default();
        rows.push(ModelWindowRow {
            model: trimmed.to_string(),
            window: win,
            auto_compact: DEFAULT_AUTO_COMPACT_PERCENT.to_string(),
            image_handling: "send-as-is".to_string(),
        });
        added += 1;
    }
    added
}

// ============================================================================
// 4. Profile 创建、编辑同步与列表增删改
// ============================================================================

fn unique_profile_id(settings: &BackendSettings, prefix: &str) -> String {
    static NEXT_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let seq = NEXT_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let base_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(1);
    let mut candidate = format!("{prefix}-{base_ms}-{seq}");
    let mut suffix = 1u32;
    while settings.relay_profiles.iter().any(|p| p.id == candidate) {
        candidate = format!("{prefix}-{base_ms}-{seq}-{suffix}");
        suffix += 1;
    }
    candidate
}

pub fn create_custom_relay_profile(settings: &BackendSettings) -> RelayProfile {
    let mut p = RelayProfile {
        id: unique_profile_id(settings, "provider"),
        name: t("自定义供应商"),
        relay_mode: RelayMode::PureApi,
        protocol: RelayProtocol::Responses,
        official_mix_api_key: false,
        use_common_config: true,
        ..RelayProfile::default()
    };
    let _ = normalize_relay_profile_for_storage(&mut p);
    p
}

pub fn create_preset_relay_profile(
    settings: &BackendSettings,
    preset: &ProviderPreset,
) -> RelayProfile {
    let mut p = RelayProfile {
        id: unique_profile_id(settings, preset.id),
        name: preset.name.to_string(),
        model: preset.model.to_string(),
        base_url: preset.base_url.to_string(),
        upstream_base_url: preset.base_url.to_string(),
        protocol: preset.protocol,
        relay_mode: RelayMode::PureApi,
        official_mix_api_key: false,
        use_common_config: true,
        model_list: preset.model_list.join("\n"),
        ..RelayProfile::default()
    };
    let initial_rows = model_window_rows_from_profile(&p);
    let _ = apply_editable_fields_to_profile(
        &mut p,
        preset.name,
        preset.model,
        preset.base_url,
        "",
        preset.protocol,
        RelayMode::PureApi,
        false,
        &initial_rows,
    );
    p
}

pub fn create_aggregate_draft(
    settings: &BackendSettings,
    model_aggregation: bool,
) -> (RelayProfile, AggregateRelayProfile) {
    let id = unique_profile_id(settings, "aggregate");
    let name = if model_aggregation {
        t("新建模型聚合")
    } else {
        t("新建故障转移")
    };
    let profile = RelayProfile {
        id: id.clone(),
        name: name.clone(),
        relay_mode: RelayMode::Aggregate,
        protocol: RelayProtocol::Responses,
        use_common_config: true,
        ..RelayProfile::default()
    };
    let agg = AggregateRelayProfile {
        id,
        name,
        session_provider: RelaySessionProvider::Custom,
        model_aggregation_enabled: model_aggregation,
        strategy: AggregateRelayStrategy::Failover,
        members: Vec::new(),
        routes: Vec::new(),
        default_relay_id: String::new(),
        sync_default_member_login: false,
    };
    (profile, agg)
}

#[derive(Debug, Clone)]
pub struct HydratedProfileFields {
    pub name: String,
    pub model: String,
    pub base_url: String,
    pub api_key: String,
    pub protocol: RelayProtocol,
    pub relay_mode: RelayMode,
    pub official_mix_api_key: bool,
    pub rows: Vec<ModelWindowRow>,
}

pub fn hydrate_profile_editable_fields(profile: &RelayProfile) -> HydratedProfileFields {
    // 供应商配置页仅承载纯 API Key 与聚合模式；官方登录与官方绑定第三方 API 由「官方账号」模块统一管理。
    let relay_mode = if profile.relay_mode == RelayMode::Aggregate {
        RelayMode::Aggregate
    } else {
        RelayMode::PureApi
    };
    HydratedProfileFields {
        name: profile.name.clone(),
        model: relay_profile_model(profile),
        base_url: relay_profile_base_url(profile),
        api_key: relay_profile_api_key(profile),
        protocol: profile.protocol,
        relay_mode,
        official_mix_api_key: false,
        rows: model_window_rows_from_profile(profile),
    }
}

/// 将编辑表单中的基础字段与模型窗口行写回 `RelayProfile`，并同步重新生成 `config_contents` / `auth_contents`。
#[allow(clippy::too_many_arguments)]
pub fn apply_editable_fields_to_profile(
    profile: &mut RelayProfile,
    name: &str,
    model: &str,
    base_url: &str,
    api_key: &str,
    protocol: RelayProtocol,
    relay_mode: RelayMode,
    _official_mix_api_key: bool,
    rows: &[ModelWindowRow],
) -> Result<(), String> {
    validate_model_window_rows(rows)?;

    let effective_mode = if relay_mode == RelayMode::Aggregate {
        RelayMode::Aggregate
    } else {
        RelayMode::PureApi
    };

    let mut merged_rows = rows.to_vec();
    ensure_configured_model_row(&mut merged_rows, model);
    let serialized = serialize_model_window_rows(&merged_rows);

    profile.name = name.trim().to_string();
    profile.protocol = protocol;
    profile.relay_mode = effective_mode;
    profile.official_mix_api_key = false;
    profile.model = model.trim().to_string();
    profile.base_url = base_url.trim().to_string();
    profile.upstream_base_url = base_url.trim().to_string();
    profile.api_key = api_key.trim().to_string();
    profile.model_list = serialized.model_list;
    profile.model_windows = serialized.model_windows;
    profile.model_auto_compact = serialized.model_auto_compact;
    profile.model_vlm = serialized.model_vlm;

    // 清除旧快照中的 model / base_url / auth key，让 normalize_relay_profile_for_storage 用最新输入重建
    if let Ok(mut doc) = profile.config_contents.parse::<toml_edit::DocumentMut>() {
        doc.as_table_mut().remove("model");
        doc.as_table_mut()
            .remove("swivox_chat_completions_base_url");
        if let Some(toml_edit::Item::Table(providers)) = doc.get_mut("model_providers") {
            for (_, v) in providers.iter_mut() {
                if let Some(tbl) = v.as_table_mut() {
                    tbl.remove("base_url");
                    tbl.remove("experimental_bearer_token");
                }
            }
        }
        profile.config_contents = doc.to_string();
    } else {
        profile.config_contents.clear();
    }

    if effective_mode == RelayMode::PureApi {
        if api_key.trim().is_empty() {
            profile.auth_contents.clear();
        } else {
            profile.auth_contents = serde_json::json!({
                "OPENAI_API_KEY": api_key.trim()
            })
            .to_string();
        }
    }

    normalize_relay_profile_for_storage(profile).map_err(|e| e.to_string())
}

pub fn duplicate_relay_profile(settings: &mut BackendSettings, profile_id: &str) -> Option<String> {
    let idx = settings
        .relay_profiles
        .iter()
        .position(|p| p.id == profile_id)?;
    let orig = settings.relay_profiles[idx].clone();
    let new_id = unique_profile_id(settings, "copy");
    let copy_name = format!(
        "{} ({})",
        if orig.name.trim().is_empty() {
            t("未命名供应商")
        } else {
            orig.name.clone()
        },
        t("副本")
    );
    let mut cloned = orig;
    cloned.id = new_id.clone();
    cloned.name = copy_name.clone();
    settings.relay_profiles.insert(idx + 1, cloned);

    if let Some(agg) = settings
        .aggregate_relay_profiles
        .iter()
        .find(|a| a.id == profile_id)
        .cloned()
    {
        let mut agg_copy = agg;
        agg_copy.id = new_id.clone();
        agg_copy.name = copy_name;
        settings.aggregate_relay_profiles.push(agg_copy);
    }
    Some(new_id)
}

pub fn remove_relay_profile(settings: &mut BackendSettings, profile_id: &str) -> bool {
    if settings.relay_profiles.len() <= 1 {
        return false;
    }
    let before = settings.relay_profiles.len();
    settings.relay_profiles.retain(|p| p.id != profile_id);
    if settings.relay_profiles.len() == before {
        return false;
    }
    settings
        .aggregate_relay_profiles
        .retain(|a| a.id != profile_id);
    for agg in &mut settings.aggregate_relay_profiles {
        agg.members.retain(|m| m.relay_id != profile_id);
        agg.routes.retain(|r| r.relay_id != profile_id);
        if agg.default_relay_id == profile_id {
            agg.default_relay_id = agg
                .members
                .first()
                .map(|m| m.relay_id.clone())
                .unwrap_or_default();
        }
    }
    if settings.active_relay_id == profile_id
        && let Some(first) = settings.relay_profiles.first()
    {
        settings.active_relay_id = first.id.clone();
    }
    true
}

pub fn move_relay_profile_in_view(
    settings: &mut BackendSettings,
    profile_id: &str,
    move_up: bool,
    view: ProviderViewKind,
) -> bool {
    let agg_map: HashMap<String, AggregateRelayProfile> = settings
        .aggregate_relay_profiles
        .iter()
        .map(|a| (a.id.clone(), a.clone()))
        .collect();
    let visible_indices: Vec<usize> = settings
        .relay_profiles
        .iter()
        .enumerate()
        .filter(|(_, p)| relay_provider_view(p, &agg_map) == view)
        .map(|(i, _)| i)
        .collect();

    let Some(pos) = visible_indices
        .iter()
        .position(|&idx| settings.relay_profiles[idx].id == profile_id)
    else {
        return false;
    };
    let target_pos = if move_up {
        if pos == 0 {
            return false;
        }
        pos - 1
    } else {
        if pos + 1 >= visible_indices.len() {
            return false;
        }
        pos + 1
    };
    settings
        .relay_profiles
        .swap(visible_indices[pos], visible_indices[target_pos]);
    true
}

pub fn upsert_relay_profile_into_settings(
    settings: &mut BackendSettings,
    profile: RelayProfile,
    aggregate: Option<AggregateRelayProfile>,
) {
    let id = profile.id.clone();
    if let Some(existing) = settings.relay_profiles.iter_mut().find(|p| p.id == id) {
        *existing = profile;
    } else {
        settings.relay_profiles.push(profile);
    }

    if let Some(mut agg) = aggregate {
        agg.id = id.clone();
        if agg.model_aggregation_enabled
            && (agg.default_relay_id.is_empty()
                || !agg.members.iter().any(|m| m.relay_id == agg.default_relay_id))
        {
            agg.default_relay_id = agg
                .members
                .first()
                .map(|m| m.relay_id.clone())
                .unwrap_or_default();
        }
        if let Some(existing_agg) = settings
            .aggregate_relay_profiles
            .iter_mut()
            .find(|a| a.id == id)
        {
            *existing_agg = agg;
        } else {
            settings.aggregate_relay_profiles.push(agg);
        }
    } else {
        settings.aggregate_relay_profiles.retain(|a| a.id != id);
    }
}

pub fn toggle_aggregate_member(agg: &mut AggregateRelayProfile, member_relay_id: &str) {
    if let Some(pos) = agg.members.iter().position(|m| m.relay_id == member_relay_id) {
        agg.members.remove(pos);
        if agg.default_relay_id == member_relay_id {
            agg.default_relay_id = agg
                .members
                .first()
                .map(|m| m.relay_id.clone())
                .unwrap_or_default();
        }
    } else {
        agg.members.push(AggregateRelayMember {
            relay_id: member_relay_id.to_string(),
            weight: 1,
        });
        if agg.model_aggregation_enabled && agg.default_relay_id.is_empty() {
            agg.default_relay_id = member_relay_id.to_string();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_profile_generates_valid_config_and_auth() {
        let settings = BackendSettings::default();
        let deepseek = &PROVIDER_PRESETS[0];
        let mut p = create_preset_relay_profile(&settings, deepseek);
        let rows = vec![
            ModelWindowRow {
                model: "deepseek-v4-flash".to_string(),
                window: "128k".to_string(),
                auto_compact: "90".to_string(),
                image_handling: "send-as-is".to_string(),
            },
            ModelWindowRow {
                model: "deepseek-v4-pro[1M]".to_string(),
                window: "1M".to_string(),
                auto_compact: "85".to_string(),
                image_handling: "vlm".to_string(),
            },
        ];
        apply_editable_fields_to_profile(
            &mut p,
            "My DeepSeek",
            "deepseek-v4-pro",
            "https://api.deepseek.com/v1",
            "sk-test-123",
            RelayProtocol::Responses,
            RelayMode::PureApi,
            false,
            &rows,
        )
        .unwrap();

        assert_eq!(relay_profile_model(&p), "deepseek-v4-pro");
        assert_eq!(relay_profile_base_url(&p), "https://api.deepseek.com/v1");
        assert_eq!(relay_profile_api_key(&p), "sk-test-123");

        let reloaded = model_window_rows_from_profile(&p);
        assert_eq!(reloaded.len(), 2);
        assert_eq!(reloaded[1].window, "1M");
        assert_eq!(reloaded[1].auto_compact, "85");
        assert_eq!(reloaded[1].image_handling, "vlm");
    }

    #[test]
    fn duplicate_model_slugs_with_and_without_suffix_are_rejected() {
        let rows = vec![
            ModelWindowRow {
                model: "gpt-5.5".to_string(),
                window: "128k".to_string(),
                auto_compact: "90".to_string(),
                image_handling: "send-as-is".to_string(),
            },
            ModelWindowRow {
                model: "gpt-5.5[1M]".to_string(),
                window: "1M".to_string(),
                auto_compact: "90".to_string(),
                image_handling: "send-as-is".to_string(),
            },
        ];
        assert!(validate_model_window_rows(&rows).is_err());
    }

    #[test]
    fn provider_view_classifies_direct_routing_failover_and_model_aggregation() {
        let mut settings = BackendSettings::default();
        let direct = create_preset_relay_profile(&settings, &PROVIDER_PRESETS[0]); // DeepSeek Responses
        let routing = create_preset_relay_profile(&settings, &PROVIDER_PRESETS[1]); // GLM ChatCompletions
        let (failover_p, failover_agg) = create_aggregate_draft(&settings, false);
        let (model_agg_p, model_agg) = create_aggregate_draft(&settings, true);

        upsert_relay_profile_into_settings(&mut settings, direct.clone(), None);
        upsert_relay_profile_into_settings(&mut settings, routing.clone(), None);
        upsert_relay_profile_into_settings(&mut settings, failover_p.clone(), Some(failover_agg));
        upsert_relay_profile_into_settings(&mut settings, model_agg_p.clone(), Some(model_agg));

        let agg_map: HashMap<String, AggregateRelayProfile> = settings
            .aggregate_relay_profiles
            .iter()
            .map(|a| (a.id.clone(), a.clone()))
            .collect();

        assert_eq!(
            relay_provider_view(&direct, &agg_map),
            ProviderViewKind::Direct
        );
        assert_eq!(
            relay_provider_view(&routing, &agg_map),
            ProviderViewKind::Routing
        );
        assert_eq!(
            relay_provider_view(&failover_p, &agg_map),
            ProviderViewKind::Failover
        );
        assert_eq!(
            relay_provider_view(&model_agg_p, &agg_map),
            ProviderViewKind::ModelAggregation
        );
    }

    #[test]
    fn duplicate_move_and_remove_preserve_active_and_aggregate_integrity() {
        let mut settings = BackendSettings::default();
        settings.relay_profiles.clear();
        let p1 = create_preset_relay_profile(&settings, &PROVIDER_PRESETS[0]);
        upsert_relay_profile_into_settings(&mut settings, p1.clone(), None);
        settings.active_relay_id = p1.id.clone();

        let dup_id = duplicate_relay_profile(&mut settings, &p1.id).unwrap();
        assert_eq!(settings.relay_profiles.len(), 2);
        assert!(move_relay_profile_in_view(
            &mut settings,
            &dup_id,
            true,
            ProviderViewKind::Direct
        ));
        assert_eq!(settings.relay_profiles[0].id, dup_id);

        assert!(remove_relay_profile(&mut settings, &p1.id));
        assert_eq!(settings.active_relay_id, dup_id);
        // 最后一个不允许删除
        assert!(!remove_relay_profile(&mut settings, &dup_id));
    }

    #[test]
    fn legacy_official_relay_profile_hydrates_and_saves_as_pure_api() {
        let mut legacy = RelayProfile {
            relay_mode: RelayMode::Official,
            official_mix_api_key: true,
            ..RelayProfile::default()
        };
        let hydrated = hydrate_profile_editable_fields(&legacy);
        assert_eq!(hydrated.relay_mode, RelayMode::PureApi);
        assert!(!hydrated.official_mix_api_key);

        apply_editable_fields_to_profile(
            &mut legacy,
            "默认中转",
            "gpt-5.5",
            "https://api.example.com/v1",
            "sk-pure",
            RelayProtocol::Responses,
            RelayMode::Official,
            true,
            &[],
        )
        .unwrap();
        assert_eq!(legacy.relay_mode, RelayMode::PureApi);
        assert!(!legacy.official_mix_api_key);
    }

    #[test]
    fn fetched_models_preserve_upstream_names_and_preview_defaults_unmatched_to_1m() {
        let mut rows = Vec::new();
        let fetched = vec![
            "[官逆]claude-3-5-sonnet-thinking".to_string(),
            "my-org/totally-unknown-model-v3".to_string(),
            "openrouter/custom-256k".to_string(),
        ];
        let mut upstream_wins = HashMap::new();
        upstream_wins.insert("openrouter/custom-256k".to_string(), 256_000);

        let added =
            merge_fetched_models_with_windows_into_rows(&mut rows, &fetched, &upstream_wins);
        assert_eq!(added, 3);
        assert_eq!(rows[0].model, "[官逆]claude-3-5-sonnet-thinking");
        assert_eq!(rows[0].window, "");
        assert_eq!(rows[1].model, "my-org/totally-unknown-model-v3");
        assert_eq!(rows[1].window, "");
        assert_eq!(rows[2].model, "openrouter/custom-256k");
        assert_eq!(rows[2].window, "256k");

        let p0 = resolve_model_window_preview(&rows[0].model);
        assert!(p0.matched);
        assert_eq!(p0.window_compact, "200k");

        let p1 = resolve_model_window_preview(&rows[1].model);
        assert!(!p1.matched);
        assert_eq!(p1.window_compact, "1M");
    }
}
