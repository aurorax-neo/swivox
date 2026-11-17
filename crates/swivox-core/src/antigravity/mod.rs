//! Antigravity 原生支持核心模块（多账号 OAuth、三端点配额、三层凭据热切号、设备指纹、免 TUN 代理、可逆汉化与上下文圆环）。

pub mod cdp_enhance;
pub mod db;
pub mod device;
pub mod keyring;
pub mod oauth;
pub mod process;
pub mod protobuf;
pub mod quota;

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use anyhow::{Context, bail};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub use cdp_enhance::{AntigravityEnhanceStatus, apply_cdp_enhancements_once, ensure_background_enhance_supervisor};
pub use db::{AntigravityClientKind, desktop_user_data_dir, ide_user_data_dir, state_vscdb_path, storage_json_path, user_data_dir_for};
pub use device::{AntigravityDeviceHistoryEntry, AntigravityDeviceProfile, ensure_device_baseline_saved, generate_device_profile, load_device_baseline, read_device_profile_from_storage_json, write_device_profile_to_storage_json};
pub use oauth::{AntigravityOAuthStartResponse, TOKEN_REFRESH_SKEW_SECONDS, cancel_pending_oauth_session, complete_manual_oauth_callback, refresh_google_access_token, start_oauth_login, wait_for_oauth_callback};
pub use process::{
    AntigravityInstallation, AntigravityLaunchOutcome, DEFAULT_ANTIGRAVITY_CDP_PORT,
    discover_installation, launch_antigravity_instance, resolve_effective_target,
    resolve_launch_proxy, resolve_launch_proxy_for_target, stop_antigravity_target,
};
pub use quota::{ANTIGRAVITY_HUB_USER_AGENT, AntigravityGroupQuota, AntigravityQuotaSnapshot, fetch_antigravity_quota, normalize_subscription_tier};

static AG_STATE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn state_lock() -> &'static Mutex<()> {
    AG_STATE_LOCK.get_or_init(|| Mutex::new(()))
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AntigravityTokenData {
    #[serde(default)]
    pub access_token: String,
    pub refresh_token: String,
    #[serde(default)]
    pub expiry_timestamp: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(default = "default_client_key")]
    pub oauth_client_key: String,
    #[serde(default)]
    pub is_gcp_tos: bool,
}

fn default_client_key() -> String {
    "personal".to_string()
}

fn default_proxy_mode() -> String {
    "inherit".to_string()
}

fn default_cdp_port() -> u16 {
    DEFAULT_ANTIGRAVITY_CDP_PORT
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AntigravityAccount {
    pub id: String,
    pub email: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub token: AntigravityTokenData,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quota: Option<AntigravityQuotaSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_profile: Option<AntigravityDeviceProfile>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub device_history: Vec<AntigravityDeviceHistoryEntry>,
    #[serde(default = "default_proxy_mode")]
    pub proxy_mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub egress_proxy_url: Option<String>,
    #[serde(default)]
    pub disabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled_reason: Option<String>,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub last_used: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AntigravityState {
    #[serde(default)]
    pub active_account_id: Option<String>,
    #[serde(default)]
    pub accounts: Vec<AntigravityAccount>,
    /// `"auto"` | `"desktop"` | `"ide"`
    #[serde(default)]
    pub preferred_target: AntigravityClientKind,
    #[serde(default)]
    pub custom_desktop_path: String,
    #[serde(default)]
    pub custom_ide_path: String,
    /// 免 TUN 启动代理模式：`"inherit"`（跟随全局/系统）| `"custom"`（指定代理）| `"direct"`（强制直连）
    #[serde(default = "default_proxy_mode")]
    pub launch_proxy_mode: String,
    #[serde(default)]
    pub launch_proxy_url: String,
    /// 可逆 CDP 界面汉化（仅桌面版生效，关闭瞬间恢复英文原文）
    #[serde(default)]
    pub enable_ui_localization: bool,
    /// 上下文监控圆环与生成速度浮层（桌面版 + IDE 均生效）
    #[serde(default = "default_true")]
    pub enable_context_ring: bool,
    #[serde(default = "default_cdp_port")]
    pub cdp_port: u16,
}

fn default_true() -> bool {
    true
}

impl Default for AntigravityState {
    fn default() -> Self {
        Self {
            active_account_id: None,
            accounts: Vec::new(),
            preferred_target: AntigravityClientKind::Auto,
            custom_desktop_path: String::new(),
            custom_ide_path: String::new(),
            launch_proxy_mode: "inherit".to_string(),
            launch_proxy_url: String::new(),
            enable_ui_localization: false,
            enable_context_ring: true,
            cdp_port: DEFAULT_ANTIGRAVITY_CDP_PORT,
        }
    }
}

/// 暴露给前端 GPUI 的脱敏账号视图（隐藏明文 `access_token` 与 `refresh_token`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AntigravityAccountView {
    pub id: String,
    pub email: String,
    pub name: Option<String>,
    pub label: Option<String>,
    pub is_active: bool,
    pub is_live_desktop: bool,
    pub is_live_ide: bool,
    pub subscription_tier: String,
    pub project_id: Option<String>,
    pub oauth_client_key: String,
    pub quota: Option<AntigravityQuotaSnapshot>,
    pub device_profile: Option<AntigravityDeviceProfile>,
    pub proxy_mode: String,
    pub egress_proxy_url: Option<String>,
    pub disabled: bool,
    pub disabled_reason: Option<String>,
    pub created_at: i64,
    pub last_used: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AntigravitySnapshot {
    pub active_account_id: Option<String>,
    pub live_desktop_email: Option<String>,
    pub live_ide_email: Option<String>,
    pub accounts: Vec<AntigravityAccountView>,
    pub preferred_target: String,
    pub custom_desktop_path: String,
    pub custom_ide_path: String,
    pub launch_proxy_mode: String,
    pub launch_proxy_url: String,
    pub effective_launch_proxy: Option<String>,
    pub effective_desktop_proxy: Option<String>,
    pub effective_ide_proxy: Option<String>,
    pub enable_ui_localization: bool,
    pub enable_context_ring: bool,
    pub cdp_port: u16,
    pub desktop_install: AntigravityInstallation,
    pub ide_install: AntigravityInstallation,
    pub current_device_profile: Option<AntigravityDeviceProfile>,
    pub has_device_baseline: bool,
}

pub fn default_antigravity_home_dir() -> PathBuf {
    let state = load_antigravity_state();
    user_data_dir_for(state.preferred_target)
}

pub fn load_antigravity_state() -> AntigravityState {
    let _guard = state_lock().lock().ok();
    let path = crate::paths::default_antigravity_state_path();
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return AntigravityState::default();
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

pub fn save_antigravity_state(state: &AntigravityState) -> anyhow::Result<()> {
    let _guard = state_lock().lock().ok();
    let path = crate::paths::default_antigravity_state_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("创建状态目录失败: {}", parent.display()))?;
    }
    let tmp = path.with_extension("json.swivox-tmp");
    let serialized = serde_json::to_string_pretty(state)?;
    {
        use std::io::Write as _;
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(serialized.as_bytes())?;
        let _ = f.sync_all();
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
    }
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

pub fn effective_proxy_for_ag_account(
    account: Option<&AntigravityAccount>,
    state: &AntigravityState,
) -> anyhow::Result<Option<String>> {
    if let Some(acc) = account {
        match acc.proxy_mode.trim() {
            "direct" => return Ok(Some("direct".to_string())),
            m if m.starts_with("node:") => {
                let cfg = crate::network_proxies::load_network_proxy_config();
                if let Some(node) = cfg.find_node(m) {
                    return Ok(Some(node.url.clone()));
                }
            }
            "custom" => {
                let url = acc
                    .egress_proxy_url
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .context("账号配置了自定义出口代理但地址为空，已中止请求以防直连泄露")?;
                return Ok(Some(url.to_string()));
            }
            _ => {}
        }
    }
    Ok(resolve_launch_proxy_for_target(
        AntigravityClientKind::Desktop,
        &state.launch_proxy_mode,
        &state.launch_proxy_url,
    ))
}

fn detect_live_desktop_token() -> Option<db::DiscoveredLocalToken> {
    keyring::read_system_keyring_or_fallback_token().or_else(|| {
        let db_path = state_vscdb_path(&desktop_user_data_dir());
        db::read_token_from_state_vscdb(&db_path, "Desktop state.vscdb")
    })
}

fn detect_live_ide_token() -> Option<db::DiscoveredLocalToken> {
    let db_path = state_vscdb_path(&ide_user_data_dir());
    db::read_token_from_state_vscdb(&db_path, "IDE state.vscdb")
}

/// 构建面向前端展示与概览页的完整脱敏快照。
pub fn build_antigravity_snapshot() -> AntigravitySnapshot {
    let state = load_antigravity_state();
    let cdp_port = if state.cdp_port == 0 {
        DEFAULT_ANTIGRAVITY_CDP_PORT
    } else {
        state.cdp_port
    };
    let desktop_install = discover_installation(
        AntigravityClientKind::Desktop,
        Some(&state.custom_desktop_path),
        cdp_port,
    );
    let ide_install = discover_installation(
        AntigravityClientKind::Ide,
        Some(&state.custom_ide_path),
        cdp_port,
    );

    let live_desktop = detect_live_desktop_token();
    let live_ide = detect_live_ide_token();

    let match_email = |tok: &Option<db::DiscoveredLocalToken>| -> Option<String> {
        let t = tok.as_ref()?;
        if let Some(found) = state
            .accounts
            .iter()
            .find(|a| a.token.refresh_token.trim() == t.refresh_token.trim())
        {
            return Some(found.email.clone());
        }
        t.email.clone()
    };

    let live_desktop_email = match_email(&live_desktop);
    let live_ide_email = match_email(&live_ide);

    let target_dir = user_data_dir_for(state.preferred_target);
    let storage_path = storage_json_path(&target_dir);
    let _ = ensure_device_baseline_saved(&storage_path);
    let current_device_profile = read_device_profile_from_storage_json(&storage_path);
    let has_device_baseline = load_device_baseline().is_some();

    let accounts = state
        .accounts
        .iter()
        .map(|acc| {
            let is_live_desktop = live_desktop
                .as_ref()
                .is_some_and(|t| t.refresh_token.trim() == acc.token.refresh_token.trim())
                || live_desktop_email
                    .as_deref()
                    .is_some_and(|e| e.eq_ignore_ascii_case(&acc.email));
            let is_live_ide = live_ide
                .as_ref()
                .is_some_and(|t| t.refresh_token.trim() == acc.token.refresh_token.trim())
                || live_ide_email
                    .as_deref()
                    .is_some_and(|e| e.eq_ignore_ascii_case(&acc.email));
            let is_active = state.active_account_id.as_deref() == Some(acc.id.as_str())
                || is_live_desktop
                || is_live_ide;
            let tier = acc
                .quota
                .as_ref()
                .map(|q| q.subscription_tier.clone())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "FREE".to_string());
            let pid = acc
                .token
                .project_id
                .clone()
                .or_else(|| acc.quota.as_ref().and_then(|q| q.project_id.clone()));

            AntigravityAccountView {
                id: acc.id.clone(),
                email: acc.email.clone(),
                name: acc.name.clone(),
                label: acc.label.clone(),
                is_active,
                is_live_desktop,
                is_live_ide,
                subscription_tier: tier,
                project_id: pid,
                oauth_client_key: acc.token.oauth_client_key.clone(),
                quota: acc.quota.clone(),
                device_profile: acc.device_profile.clone(),
                proxy_mode: acc.proxy_mode.clone(),
                egress_proxy_url: acc.egress_proxy_url.clone(),
                disabled: acc.disabled,
                disabled_reason: acc.disabled_reason.clone(),
                created_at: acc.created_at,
                last_used: acc.last_used,
            }
        })
        .collect();

    AntigravitySnapshot {
        active_account_id: state.active_account_id.clone(),
        live_desktop_email,
        live_ide_email,
        accounts,
        preferred_target: state.preferred_target.as_str().to_string(),
        custom_desktop_path: state.custom_desktop_path.clone(),
        custom_ide_path: state.custom_ide_path.clone(),
        launch_proxy_mode: state.launch_proxy_mode.clone(),
        launch_proxy_url: state.launch_proxy_url.clone(),
        effective_launch_proxy: resolve_launch_proxy_for_target(
            state.preferred_target,
            &state.launch_proxy_mode,
            &state.launch_proxy_url,
        ),
        effective_desktop_proxy: resolve_launch_proxy_for_target(
            AntigravityClientKind::Desktop,
            &state.launch_proxy_mode,
            &state.launch_proxy_url,
        ),
        effective_ide_proxy: resolve_launch_proxy_for_target(
            AntigravityClientKind::Ide,
            &state.launch_proxy_mode,
            &state.launch_proxy_url,
        ),
        enable_ui_localization: state.enable_ui_localization,
        enable_context_ring: state.enable_context_ring,
        cdp_port,
        desktop_install,
        ide_install,
        current_device_profile,
        has_device_baseline,
    }
}

/// 按忽略大小写邮箱幂等新增或更新 Antigravity 账号，并自动分配独立设备指纹与拉取最新配额。
pub async fn upsert_account_from_tokens(
    email: String,
    name: Option<String>,
    access_token: String,
    refresh_token: String,
    expiry_timestamp: i64,
    id_token: Option<String>,
    client_key: String,
    project_id_hint: Option<String>,
    is_gcp_tos: bool,
) -> anyhow::Result<AntigravityAccount> {
    let now = Utc::now().timestamp();
    let mut state = load_antigravity_state();
    let proxy = effective_proxy_for_ag_account(None, &state)?;

    let quota = fetch_antigravity_quota(
        &access_token,
        project_id_hint.as_deref(),
        proxy.as_deref(),
    )
    .await
    .ok();

    let resolved_pid = quota
        .as_ref()
        .and_then(|q| q.project_id.clone())
        .or(project_id_hint);

    if let Some(existing) = state
        .accounts
        .iter_mut()
        .find(|a| a.email.eq_ignore_ascii_case(&email))
    {
        existing.name = name.or_else(|| existing.name.clone());
        existing.token.access_token = access_token;
        existing.token.refresh_token = refresh_token;
        existing.token.expiry_timestamp = expiry_timestamp;
        if id_token.is_some() {
            existing.token.id_token = id_token;
        }
        if resolved_pid.is_some() {
            existing.token.project_id = resolved_pid;
        }
        existing.token.oauth_client_key = client_key;
        existing.token.is_gcp_tos = is_gcp_tos;
        existing.disabled = false;
        existing.disabled_reason = None;
        if quota.is_some() {
            existing.quota = quota;
        }
        if existing.device_profile.is_none() {
            existing.device_profile = Some(generate_device_profile());
        }
        let saved = existing.clone();
        if state.active_account_id.is_none() {
            state.active_account_id = Some(saved.id.clone());
        }
        save_antigravity_state(&state)?;
        return Ok(saved);
    }

    let dev = generate_device_profile();
    let id = Uuid::new_v4().to_string();
    let account = AntigravityAccount {
        id: id.clone(),
        email,
        name,
        label: None,
        token: AntigravityTokenData {
            access_token,
            refresh_token,
            expiry_timestamp,
            id_token,
            project_id: resolved_pid,
            oauth_client_key: client_key,
            is_gcp_tos,
        },
        quota,
        device_profile: Some(dev.clone()),
        device_history: vec![AntigravityDeviceHistoryEntry {
            label: "初始隔离指纹".to_string(),
            created_at: now,
            profile: dev,
        }],
        proxy_mode: "inherit".to_string(),
        egress_proxy_url: None,
        disabled: false,
        disabled_reason: None,
        created_at: now,
        last_used: 0,
    };
    if state.active_account_id.is_none() {
        state.active_account_id = Some(id);
    }
    state.accounts.push(account.clone());
    save_antigravity_state(&state)?;
    Ok(account)
}

/// 扫描并导入本机已登录的 Antigravity 账号（覆盖系统钥匙串、桌面版 `state.vscdb`、IDE 版 `state.vscdb` 与 `~/.antigravity_tools`）。
pub async fn scan_and_import_local_accounts() -> anyhow::Result<usize> {
    let state = load_antigravity_state();
    let proxy = effective_proxy_for_ag_account(None, &state)?;

    let mut discovered: Vec<db::DiscoveredLocalToken> = Vec::new();
    if let Some(tok) = keyring::read_system_keyring_or_fallback_token() {
        discovered.push(tok);
    }
    if let Some(tok) = db::read_token_from_state_vscdb(
        &state_vscdb_path(&desktop_user_data_dir()),
        "Desktop state.vscdb",
    ) {
        discovered.push(tok);
    }
    if let Some(tok) = db::read_token_from_state_vscdb(
        &state_vscdb_path(&ide_user_data_dir()),
        "IDE state.vscdb",
    ) {
        discovered.push(tok);
    }

    // 同时支持从 Antigravity-Manager 的 `~/.antigravity_tools/accounts/` 无损导入
    if let Some(base) = directories::BaseDirs::new() {
        let ag_tools_dir = base.home_dir().join(".antigravity_tools").join("accounts");
        if let Ok(entries) = std::fs::read_dir(ag_tools_dir) {
            for entry in entries.flatten() {
                if entry.path().extension().is_some_and(|e| e == "json")
                    && let Ok(raw) = std::fs::read_to_string(entry.path())
                    && let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw)
                    && let Some(rt) = val
                        .pointer("/token/refresh_token")
                        .or_else(|| val.get("refresh_token"))
                        .and_then(serde_json::Value::as_str)
                        .filter(|s| !s.trim().is_empty())
                {
                    discovered.push(db::DiscoveredLocalToken {
                        email: val
                            .get("email")
                            .and_then(serde_json::Value::as_str)
                            .map(ToString::to_string),
                        access_token: val
                            .pointer("/token/access_token")
                            .and_then(serde_json::Value::as_str)
                            .map(ToString::to_string),
                        refresh_token: rt.to_string(),
                        id_token: val
                            .pointer("/token/id_token")
                            .and_then(serde_json::Value::as_str)
                            .map(ToString::to_string),
                        project_id: val
                            .pointer("/token/project_id")
                            .and_then(serde_json::Value::as_str)
                            .map(ToString::to_string),
                        is_gcp_tos: val
                            .pointer("/token/is_gcp_tos")
                            .and_then(serde_json::Value::as_bool)
                            .unwrap_or(false),
                        source: "antigravity_tools".to_string(),
                    });
                }
            }
        }
    }

    // 按 refresh_token 去重
    let mut seen_rt = std::collections::HashSet::new();
    discovered.retain(|d| seen_rt.insert(d.refresh_token.trim().to_string()));

    if discovered.is_empty() {
        bail!("未在本机检测到已登录的 Antigravity 账号凭据。");
    }

    let mut imported_count = 0usize;
    for item in discovered {
        if let Ok(exchanged) =
            refresh_google_access_token(&item.refresh_token, None, proxy.as_deref()).await
        {
            let user_res = oauth::fetch_google_userinfo(&exchanged.access_token, proxy.as_deref()).await;
            let (email, name) = match user_res {
                Ok(u) => {
                    let n = u.display_name();
                    (u.email, n)
                }
                Err(_) => {
                    let Some(em) = item.email.clone() else { continue };
                    (em, None)
                }
            };
            let _ = upsert_account_from_tokens(
                email,
                name,
                exchanged.access_token,
                exchanged.refresh_token,
                exchanged.expiry_timestamp,
                exchanged.id_token.or(item.id_token),
                exchanged.client_key,
                item.project_id,
                item.is_gcp_tos,
            )
            .await?;
            imported_count += 1;
        }
    }

    if imported_count == 0 {
        bail!("检测到本地凭据但令牌已失效，无法刷新导入。");
    }
    Ok(imported_count)
}

/// 从 JSON 数组、对象或每行一条 `refresh_token` 批量导入 Antigravity 账号。
pub async fn import_accounts_from_text(raw_text: &str) -> anyhow::Result<usize> {
    let trimmed = raw_text.trim();
    if trimmed.is_empty() {
        bail!("导入内容不能为空。");
    }
    let state = load_antigravity_state();
    let proxy = effective_proxy_for_ag_account(None, &state)?;

    let mut tokens_to_import: Vec<(String, Option<String>)> = Vec::new();
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed) {
        let items = if let Some(arr) = val.as_array() {
            arr.clone()
        } else {
            vec![val]
        };
        for item in items {
            if let Some(s) = item.as_str() {
                if !s.trim().is_empty() {
                    tokens_to_import.push((s.trim().to_string(), None));
                }
            } else if let Some(obj) = item.as_object() {
                let rt = obj
                    .get("refreshToken")
                    .or_else(|| obj.get("refresh_token"))
                    .and_then(serde_json::Value::as_str)
                    .or_else(|| {
                        obj.get("token")
                            .and_then(|t| t.get("refreshToken").or_else(|| t.get("refresh_token")))
                            .and_then(serde_json::Value::as_str)
                    });
                let label = obj
                    .get("label")
                    .or_else(|| obj.get("custom_label"))
                    .and_then(serde_json::Value::as_str)
                    .map(ToString::to_string);
                if let Some(r) = rt.map(str::trim).filter(|s| !s.is_empty()) {
                    tokens_to_import.push((r.to_string(), label));
                }
            }
        }
    } else {
        for line in trimmed.lines() {
            let l = line.trim();
            if !l.is_empty() && !l.starts_with('#') {
                tokens_to_import.push((l.to_string(), None));
            }
        }
    }

    if tokens_to_import.is_empty() {
        bail!("未能从输入内容中识别出有效的 refresh_token。");
    }

    let mut count = 0usize;
    for (rt, label) in tokens_to_import {
        let exchanged = refresh_google_access_token(&rt, None, proxy.as_deref()).await?;
        let user = oauth::fetch_google_userinfo(&exchanged.access_token, proxy.as_deref()).await?;
        let acc = upsert_account_from_tokens(
            user.email.clone(),
            user.display_name(),
            exchanged.access_token,
            exchanged.refresh_token,
            exchanged.expiry_timestamp,
            exchanged.id_token,
            exchanged.client_key,
            None,
            false,
        )
        .await?;
        if let Some(lbl) = label.filter(|s| !s.trim().is_empty()) {
            let _ = update_account_metadata(&acc.id, Some(lbl), None, None);
        }
        count += 1;
    }

    Ok(count)
}

pub fn export_accounts_json() -> anyhow::Result<String> {
    let state = load_antigravity_state();
    let exported: Vec<serde_json::Value> = state
        .accounts
        .iter()
        .map(|a| {
            serde_json::json!({
                "email": a.email,
                "name": a.name,
                "label": a.label,
                "refresh_token": a.token.refresh_token,
                "project_id": a.token.project_id,
                "oauth_client_key": a.token.oauth_client_key,
            })
        })
        .collect();
    Ok(serde_json::to_string_pretty(&exported)?)
}

/// 确保目标账号的 `access_token` 处于有效期内（距离过期 `<= 15 分钟` 时自动刷新并落盘）。
pub async fn ensure_fresh_account_token(account_id: &str) -> anyhow::Result<AntigravityAccount> {
    let mut state = load_antigravity_state();
    let idx = state
        .accounts
        .iter()
        .position(|a| a.id == account_id)
        .context("未找到指定的 Antigravity 账号")?;
    let now = Utc::now().timestamp();
    let needs_refresh = state.accounts[idx].token.access_token.trim().is_empty()
        || state.accounts[idx].token.expiry_timestamp - now <= TOKEN_REFRESH_SKEW_SECONDS;

    if !needs_refresh {
        return Ok(state.accounts[idx].clone());
    }

    let proxy = effective_proxy_for_ag_account(Some(&state.accounts[idx]), &state)?;
    let rt = state.accounts[idx].token.refresh_token.clone();
    let ckey = state.accounts[idx].token.oauth_client_key.clone();

    match refresh_google_access_token(&rt, Some(&ckey), proxy.as_deref()).await {
        Ok(refreshed) => {
            let acc = &mut state.accounts[idx];
            acc.token.access_token = refreshed.access_token;
            acc.token.refresh_token = refreshed.refresh_token;
            acc.token.expiry_timestamp = refreshed.expiry_timestamp;
            if refreshed.id_token.is_some() {
                acc.token.id_token = refreshed.id_token;
            }
            acc.token.oauth_client_key = refreshed.client_key;
            acc.disabled = false;
            acc.disabled_reason = None;
            let out = acc.clone();
            save_antigravity_state(&state)?;
            Ok(out)
        }
        Err(err) => {
            let msg = err.to_string();
            if msg.to_ascii_lowercase().contains("invalid_grant") {
                state.accounts[idx].disabled = true;
                state.accounts[idx].disabled_reason = Some(msg.clone());
                let _ = save_antigravity_state(&state);
            }
            Err(err)
        }
    }
}

/// 刷新单个 Antigravity 账号配额。
pub async fn refresh_account_quota(account_id: &str) -> anyhow::Result<AntigravityQuotaSnapshot> {
    let acc = ensure_fresh_account_token(account_id).await?;
    let state = load_antigravity_state();
    let proxy = effective_proxy_for_ag_account(Some(&acc), &state)?;
    let snapshot = fetch_antigravity_quota(
        &acc.token.access_token,
        acc.token.project_id.as_deref(),
        proxy.as_deref(),
    )
    .await?;

    let mut latest = load_antigravity_state();
    if let Some(target) = latest.accounts.iter_mut().find(|a| a.id == account_id) {
        if let Some(ref pid) = snapshot.project_id {
            target.token.project_id = Some(pid.clone());
        }
        target.quota = Some(snapshot.clone());
        save_antigravity_state(&latest)?;
    }
    Ok(snapshot)
}

pub async fn refresh_all_account_quotas() -> (usize, usize) {
    let ids: Vec<String> = load_antigravity_state()
        .accounts
        .into_iter()
        .map(|a| a.id)
        .collect();
    let mut ok_count = 0usize;
    let mut fail_count = 0usize;
    for id in ids {
        if refresh_account_quota(&id).await.is_ok() {
            ok_count += 1;
        } else {
            fail_count += 1;
        }
    }
    (ok_count, fail_count)
}

/// 按显式生效模式切换本机当前激活的 Antigravity 账号：
/// - `"write_only"`：仅将凭据与专属设备指纹写入本地（不重启客户端，下次启动生效）
/// - `"desktop_hot"`：桌面版运行中时执行无感热重载 `language_server`，未运行时免 TUN 拉起桌面版
/// - `"desktop_restart"`：切换并完整重启 Antigravity 桌面版
/// - `"ide_restart"`：同步 IDE `state.vscdb` 并重启/拉起 Antigravity IDE
pub async fn switch_active_account_with_mode(
    account_id: &str,
    switch_mode: &str,
) -> anyhow::Result<String> {
    let mut acc = ensure_fresh_account_token(account_id).await?;
    let mut state = load_antigravity_state();

    let now = Utc::now().timestamp();
    if acc.device_profile.is_none() {
        let dev = generate_device_profile();
        acc.device_profile = Some(dev.clone());
        acc.device_history.push(AntigravityDeviceHistoryEntry {
            label: "切号自动生成".to_string(),
            created_at: now,
            profile: dev,
        });
    }

    if let Some(stored) = state.accounts.iter_mut().find(|a| a.id == account_id) {
        stored.device_profile = acc.device_profile.clone();
        stored.device_history = acc.device_history.clone();
        stored.last_used = now;
    }
    state.active_account_id = Some(account_id.to_string());
    save_antigravity_state(&state)?;

    let cdp_port = if state.cdp_port == 0 {
        DEFAULT_ANTIGRAVITY_CDP_PORT
    } else {
        state.cdp_port
    };

    let apply_for_target = |kind: AntigravityClientKind| -> anyhow::Result<()> {
        let data_dir = user_data_dir_for(kind);
        if let Some(ref dev) = acc.device_profile {
            let _ = write_device_profile_to_storage_json(&storage_json_path(&data_dir), dev);
        }
        if kind != AntigravityClientKind::Ide {
            let _ = keyring::write_system_keyring_and_gemini_files(
                &acc.email,
                &acc.token.access_token,
                &acc.token.refresh_token,
                acc.token.expiry_timestamp,
                acc.token.id_token.as_deref(),
            );
        }
        db::write_credentials_to_state_vscdb(
            &state_vscdb_path(&data_dir),
            &acc.email,
            &acc.token.access_token,
            &acc.token.refresh_token,
            acc.token.expiry_timestamp,
            acc.token.id_token.as_deref(),
            acc.token.project_id.as_deref(),
            acc.token.is_gcp_tos,
            acc.device_profile.as_ref(),
        )?;
        Ok(())
    };

    let mode = switch_mode.trim().to_ascii_lowercase();
    match mode.as_str() {
        "write_only" | "only" => {
            apply_for_target(AntigravityClientKind::Desktop)?;
            if user_data_dir_for(AntigravityClientKind::Ide).exists() {
                let _ = apply_for_target(AntigravityClientKind::Ide);
            }
            Ok(format!(
                "已将 {} 写入本机 Antigravity 凭据与设备指纹（重启客户端后生效）。",
                acc.email
            ))
        }
        "ide_restart" | "ide" => {
            apply_for_target(AntigravityClientKind::Ide)?;
            let _ = stop_antigravity_target(true);
            apply_for_target(AntigravityClientKind::Ide)?;
            launch_antigravity_instance(
                AntigravityClientKind::Ide,
                Some(&state.custom_desktop_path),
                Some(&state.custom_ide_path),
                &state.launch_proxy_mode,
                &state.launch_proxy_url,
                cdp_port,
                true,
            )?;
            Ok(format!(
                "已切换至 {} 并启动/重启 Antigravity IDE 生效。",
                acc.email
            ))
        }
        "desktop_restart" => {
            apply_for_target(AntigravityClientKind::Desktop)?;
            let _ = stop_antigravity_target(false);
            apply_for_target(AntigravityClientKind::Desktop)?;
            launch_antigravity_instance(
                AntigravityClientKind::Desktop,
                Some(&state.custom_desktop_path),
                Some(&state.custom_ide_path),
                &state.launch_proxy_mode,
                &state.launch_proxy_url,
                cdp_port,
                true,
            )?;
            Ok(format!(
                "已切换至 {} 并重启 Antigravity 桌面版生效。",
                acc.email
            ))
        }
        _ => {
            apply_for_target(AntigravityClientKind::Desktop)?;
            if user_data_dir_for(AntigravityClientKind::Ide).exists() {
                let _ = apply_for_target(AntigravityClientKind::Ide);
            }
            let desktop_inst = discover_installation(
                AntigravityClientKind::Desktop,
                Some(&state.custom_desktop_path),
                cdp_port,
            );
            if desktop_inst.running {
                let hot_ok = process::hot_reload_desktop_language_server(|| {
                    let _ = apply_for_target(AntigravityClientKind::Desktop);
                });
                if hot_ok {
                    return Ok(format!(
                        "已无感热切换至 {}（主窗口保持运行，语言服务已重载新凭据）。",
                        acc.email
                    ));
                }
                let _ = stop_antigravity_target(false);
                apply_for_target(AntigravityClientKind::Desktop)?;
            }
            if desktop_inst.found {
                let _ = launch_antigravity_instance(
                    AntigravityClientKind::Desktop,
                    Some(&state.custom_desktop_path),
                    Some(&state.custom_ide_path),
                    &state.launch_proxy_mode,
                    &state.launch_proxy_url,
                    cdp_port,
                    true,
                );
                return Ok(format!(
                    "已切换至 {} 并拉起 Antigravity 桌面版生效。",
                    acc.email
                ));
            }
            Ok(format!(
                "已将 {} 写入本机 Antigravity 凭据与设备指纹，下次启动立即生效。",
                acc.email
            ))
        }
    }
}

pub async fn switch_active_account(account_id: &str) -> anyhow::Result<String> {
    switch_active_account_with_mode(account_id, "desktop_hot").await
}

pub fn update_account_metadata(
    account_id: &str,
    label: Option<String>,
    proxy_mode: Option<String>,
    egress_proxy_url: Option<String>,
) -> anyhow::Result<()> {
    let mut state = load_antigravity_state();
    let acc = state
        .accounts
        .iter_mut()
        .find(|a| a.id == account_id)
        .context("未找到指定的 Antigravity 账号")?;
    if let Some(l) = label {
        let trimmed = l.trim();
        acc.label = (!trimmed.is_empty()).then(|| trimmed.to_string());
    }
    if let Some(mode) = proxy_mode {
        let m = mode.trim();
        acc.proxy_mode = if m.starts_with("node:") && m.len() > 5 {
            m.to_string()
        } else {
            match m {
                "direct" => "direct".to_string(),
                "custom" => "custom".to_string(),
                _ => "inherit".to_string(),
            }
        };
    }
    if let Some(url) = egress_proxy_url {
        let trimmed = url.trim();
        acc.egress_proxy_url = (!trimmed.is_empty()).then(|| trimmed.to_string());
    }
    save_antigravity_state(&state)
}

pub fn delete_account(account_id: &str) -> anyhow::Result<()> {
    let mut state = load_antigravity_state();
    let before = state.accounts.len();
    state.accounts.retain(|a| a.id != account_id);
    if state.accounts.len() == before {
        bail!("未找到要删除的 Antigravity 账号。");
    }
    if state.active_account_id.as_deref() == Some(account_id) {
        state.active_account_id = state.accounts.first().map(|a| a.id.clone());
    }
    save_antigravity_state(&state)
}

/// 重新生成一套全新的随机设备指纹并应用到目标 `storage.json` 与 `state.vscdb`。
pub fn regenerate_and_apply_device_profile(account_id: Option<&str>) -> anyhow::Result<AntigravityDeviceProfile> {
    let mut state = load_antigravity_state();
    let profile = generate_device_profile();
    let now = Utc::now().timestamp();

    let target_id = account_id
        .map(ToString::to_string)
        .or_else(|| state.active_account_id.clone());
    if let Some(id) = target_id
        && let Some(acc) = state.accounts.iter_mut().find(|a| a.id == id)
    {
        acc.device_profile = Some(profile.clone());
        acc.device_history.push(AntigravityDeviceHistoryEntry {
            label: "手动重置指纹".to_string(),
            created_at: now,
            profile: profile.clone(),
        });
        save_antigravity_state(&state)?;
    }

    let data_dir = user_data_dir_for(state.preferred_target);
    write_device_profile_to_storage_json(&storage_json_path(&data_dir), &profile)?;
    let _ = db::sync_device_profile_to_state_vscdb(&state_vscdb_path(&data_dir), &profile);
    Ok(profile)
}

/// 将本机原始设备指纹基线（`~/.swivox/antigravity-device-original.json`）恢复回 `storage.json` 与 `state.vscdb`。
pub fn restore_original_device_baseline() -> anyhow::Result<AntigravityDeviceProfile> {
    let baseline = load_device_baseline()
        .context("尚未保存本机原始设备指纹基线（可能本机尚未运行过 Antigravity）。")?;
    let state = load_antigravity_state();
    let data_dir = user_data_dir_for(state.preferred_target);
    write_device_profile_to_storage_json(&storage_json_path(&data_dir), &baseline)?;
    let _ = db::sync_device_profile_to_state_vscdb(&state_vscdb_path(&data_dir), &baseline);
    Ok(baseline)
}
