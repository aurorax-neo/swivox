//! Lamplighter 状态持久化（`state.json`）、额外凭据库（`accounts.json`）与额度历史曲线（`history/YYYY-MM-DD.jsonl`）。

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

use anyhow::Context;
use chrono::{TimeZone, Utc};
use serde::{Deserialize, Serialize};

use super::config::LamplighterConfig;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ExternalLamplighterAccount {
    pub id: String,
    /// `"claude"` | `"kimi"` | `"xai"` | `"codex"` | `"antigravity"`
    pub provider: String,
    pub email: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub access_token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy_url: Option<String>,
    #[serde(default)]
    pub created_at: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct WindowAlertState {
    pub remaining: f64,
    pub reset: i64,
    pub ended_reset: i64,
    pub pre_reset_remaining: Option<f64>,
    pub pre_reset_at: i64,
    pub read_at: i64,
    /// `0` = normal, `1` = <=notice, `2` = <=low, `3` = <=critical, `4` = exhausted
    pub notified_severity: u8,
    pub exhausted: bool,
    pub rebase: bool,
    pub reset_notice_sent_for: i64,
    pub bounce_since: i64,
    pub bounce_readings: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct GroupAlertState {
    pub label: String,
    pub active_windows: Option<Vec<String>>,
    pub last_seen: i64,
    pub windows: BTreeMap<String, WindowAlertState>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct IgnitionTargetState {
    pub last_reset_seen: i64,
    pub last_seen_at: i64,
    pub rolling_reset: bool,
    pub last_success_at: i64,
    pub last_model: Option<String>,
    pub consecutive_failures: u32,
    pub retry_at: i64,
    pub paused_until: i64,
    pub paused_reason: Option<String>,
    pub circuit_notified_for: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct PlanTrackState {
    pub plan: String,
    pub checked_at: i64,
    pub windows_sig: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct CredentialTrackState {
    /// `"auth"` | `"suspended"` | `""`
    pub problem_kind: String,
    pub summary: String,
    pub appeal_url: Option<String>,
    pub fingerprint: String,
    pub from_ignition: bool,
    pub consecutive_query_failures: u32,
    pub first_failure_at: i64,
    pub notified: bool,
    pub fingerprint_changed: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct CodexResetTrackState {
    pub seen_announcements: BTreeMap<String, String>,
    pub initialized: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct LamplighterState {
    pub version: u32,
    pub groups: BTreeMap<String, GroupAlertState>,
    pub scheduler: BTreeMap<String, IgnitionTargetState>,
    pub plans: BTreeMap<String, PlanTrackState>,
    pub credentials: BTreeMap<String, CredentialTrackState>,
    pub codex_reset: CodexResetTrackState,
}

impl Default for LamplighterState {
    fn default() -> Self {
        Self {
            version: 1,
            groups: BTreeMap::new(),
            scheduler: BTreeMap::new(),
            plans: BTreeMap::new(),
            credentials: BTreeMap::new(),
            codex_reset: CodexResetTrackState::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistorySamplePoint {
    pub timestamp: i64,
    pub remaining: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reset_at: Option<i64>,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistorySeries {
    pub group_key: String,
    pub provider: String,
    pub label: String,
    pub window_id: String,
    pub weight: f64,
    pub points: Vec<HistorySamplePoint>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LamplighterEventRecord {
    pub timestamp: i64,
    pub group_key: String,
    pub kind: String,
    pub level: String,
    pub label: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct RawJsonlEntry {
    k: String,
    t: i64,
    #[serde(default)]
    g: String,
    #[serde(default)]
    w: String,
    #[serde(default)]
    r: Option<f64>,
    #[serde(default)]
    x: Option<i64>,
    #[serde(default)]
    s: Option<String>,
    #[serde(default)]
    e: Option<String>,
    #[serde(default)]
    v: Option<String>,
    #[serde(default)]
    l: Option<String>,
    #[serde(default)]
    m: Option<String>,
    #[serde(default)]
    d: Option<String>,
}

fn atomic_write_json<T: Serialize>(path: &Path, value: &T) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("创建目录失败: {}", parent.display()))?;
    }
    let tmp = path.with_extension("json.swivox-tmp");
    let text = serde_json::to_string_pretty(value)?;
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(text.as_bytes())?;
        let _ = f.sync_all();
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

pub fn load_config() -> LamplighterConfig {
    let path = crate::paths::default_lamplighter_config_path();
    let mut cfg: LamplighterConfig = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    cfg.normalize();
    cfg
}

pub fn save_config(cfg: &mut LamplighterConfig) -> anyhow::Result<()> {
    cfg.normalize();
    atomic_write_json(&crate::paths::default_lamplighter_config_path(), cfg)
}

pub fn load_state() -> LamplighterState {
    let path = crate::paths::default_lamplighter_state_path();
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_state(state: &LamplighterState) -> anyhow::Result<()> {
    atomic_write_json(&crate::paths::default_lamplighter_state_path(), state)
}

pub fn load_external_accounts() -> Vec<ExternalLamplighterAccount> {
    let path = crate::paths::default_lamplighter_accounts_path();
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_external_accounts(accounts: &[ExternalLamplighterAccount]) -> anyhow::Result<()> {
    atomic_write_json(&crate::paths::default_lamplighter_accounts_path(), &accounts)
}

fn history_file_for_timestamp(ts: i64) -> std::path::PathBuf {
    let dt = Utc
        .timestamp_opt(ts, 0)
        .single()
        .unwrap_or_else(Utc::now);
    let date_name = format!("{}.jsonl", dt.format("%Y-%m-%d"));
    crate::paths::default_lamplighter_history_dir().join(date_name)
}

pub fn append_history_sample(
    timestamp: i64,
    group_key: &str,
    window_id: &str,
    remaining: f64,
    reset_at: Option<i64>,
    source: &str,
) {
    let path = history_file_for_timestamp(timestamp);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let rounded = (remaining.clamp(0.0, 100.0) * 1_000_000.0).round() / 1_000_000.0;
    let entry = RawJsonlEntry {
        k: "s".to_string(),
        t: timestamp,
        g: group_key.to_string(),
        w: window_id.to_string(),
        r: Some(rounded),
        x: reset_at.filter(|v| *v > 0),
        s: Some(source.to_string()),
        e: None,
        v: None,
        l: None,
        m: None,
        d: None,
    };
    if let Ok(mut line) = serde_json::to_string(&entry) {
        line.push('\n');
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
        {
            let _ = f.write_all(line.as_bytes());
        }
    }
}

pub fn append_event_record(ev: &LamplighterEventRecord) {
    let path = history_file_for_timestamp(ev.timestamp);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let entry = RawJsonlEntry {
        k: "e".to_string(),
        t: ev.timestamp,
        g: ev.group_key.clone(),
        w: String::new(),
        r: None,
        x: None,
        s: None,
        e: Some(ev.kind.clone()),
        v: Some(ev.level.clone()),
        l: Some(ev.label.clone()),
        m: Some(ev.message.clone()),
        d: ev.detail.clone(),
    };
    if let Ok(mut line) = serde_json::to_string(&entry) {
        line.push('\n');
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
        {
            let _ = f.write_all(line.as_bytes());
        }
    }
}

pub fn history_bucket_seconds(span_seconds: i64) -> i64 {
    if span_seconds <= 2 * 86_400 {
        0
    } else if span_seconds <= 4 * 86_400 {
        600
    } else if span_seconds <= 8 * 86_400 {
        900
    } else if span_seconds <= 15 * 86_400 {
        1800
    } else {
        3600
    }
}

/// 读取最近事件与指定时间范围（`hours` 小时）的降采样曲线数据。
pub fn query_history_and_events(
    hours: u64,
    window_filter: Option<&str>,
    group_labels: &BTreeMap<String, (String, String, f64)>,
) -> (Vec<HistorySeries>, Vec<LamplighterEventRecord>) {
    let now = Utc::now().timestamp();
    let span = (hours.clamp(1, 24 * 90) as i64) * 3600;
    let from = now - span;
    let lead_from = from - 86_400;
    let bucket = history_bucket_seconds(span);

    let dir = crate::paths::default_lamplighter_history_dir();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return (Vec::new(), Vec::new());
    };

    let min_date = Utc
        .timestamp_opt(lead_from.min(now - 7 * 86_400), 0)
        .single()
        .unwrap_or_else(Utc::now)
        .format("%Y-%m-%d")
        .to_string();

    let mut files: Vec<(String, std::path::PathBuf)> = entries
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            let stem = p.file_stem()?.to_str()?.to_string();
            (p.extension().is_some_and(|ext| ext == "jsonl") && stem >= min_date)
                .then_some((stem, p))
        })
        .collect();
    files.sort_by(|a, b| a.0.cmp(&b.0));

    let mut lead_points: BTreeMap<(String, String), HistorySamplePoint> = BTreeMap::new();
    let mut series_points: BTreeMap<(String, String), Vec<HistorySamplePoint>> = BTreeMap::new();
    let mut events: Vec<LamplighterEventRecord> = Vec::new();

    for (_, path) in files {
        let Ok(file) = std::fs::File::open(&path) else {
            continue;
        };
        let reader = BufReader::new(file);
        for line in reader.lines().map_while(Result::ok) {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let Ok(rec) = serde_json::from_str::<RawJsonlEntry>(trimmed) else {
                continue;
            };
            if rec.k == "e" {
                events.push(LamplighterEventRecord {
                    timestamp: rec.t,
                    group_key: rec.g,
                    kind: rec.e.unwrap_or_else(|| "info".to_string()),
                    level: rec.v.unwrap_or_else(|| "info".to_string()),
                    label: rec.l.unwrap_or_default(),
                    message: rec.m.unwrap_or_default(),
                    detail: rec.d,
                });
            } else if rec.k == "s"
                && let Some(rem) = rec.r
            {
                if let Some(wf) = window_filter.filter(|s| !s.is_empty())
                    && rec.w != wf
                {
                    continue;
                }
                let pt = HistorySamplePoint {
                    timestamp: rec.t,
                    remaining: rem,
                    reset_at: rec.x,
                    source: rec.s.unwrap_or_else(|| "active".to_string()),
                };
                let key = (rec.g, rec.w);
                if rec.t < from {
                    if rec.t >= lead_from {
                        lead_points.insert(key, pt);
                    }
                } else {
                    let vec = series_points.entry(key).or_default();
                    if bucket > 0
                        && let Some(last) = vec.last_mut()
                        && last.timestamp / bucket == pt.timestamp / bucket
                    {
                        *last = pt;
                    } else {
                        vec.push(pt);
                    }
                }
            }
        }
    }

    let mut out_series = Vec::new();
    for ((gkey, wid), mut pts) in series_points {
        if let Some(lead) = lead_points.remove(&(gkey.clone(), wid.clone())) {
            pts.insert(0, lead);
        }
        let (provider, label, weight) = group_labels
            .get(&gkey)
            .cloned()
            .unwrap_or_else(|| {
                let prov = gkey.split(':').next().unwrap_or("codex").to_string();
                (prov, gkey.clone(), 1.0)
            });
        out_series.push(HistorySeries {
            group_key: gkey,
            provider,
            label,
            window_id: wid,
            weight,
            points: pts,
        });
    }

    events.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    events.truncate(100);
    (out_series, events)
}
