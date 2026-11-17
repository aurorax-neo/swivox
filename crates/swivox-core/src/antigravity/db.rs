//! Antigravity `state.vscdb` SQLite 读写与本地路径解析（严格隔离桌面版 `Antigravity` 与编辑器版 `Antigravity IDE`）。

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Context;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use super::device::AntigravityDeviceProfile;
use super::protobuf::{
    create_minimal_user_status_payload, create_oauth_info, create_string_value_payload,
    create_unified_state_entry, create_unified_topic_entry, decode_unified_state_entry_with_target,
    find_field, find_string_field, find_varint_field, remove_unified_topic_entry,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AntigravityClientKind {
    #[default]
    Auto,
    Desktop,
    Ide,
}

impl AntigravityClientKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Desktop => "desktop",
            Self::Ide => "ide",
        }
    }

    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "desktop" | "classic" | "hub" => Self::Desktop,
            "ide" | "editor" => Self::Ide,
            _ => Self::Auto,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredLocalToken {
    pub email: Option<String>,
    pub access_token: Option<String>,
    pub refresh_token: String,
    pub id_token: Option<String>,
    pub project_id: Option<String>,
    pub is_gcp_tos: bool,
    pub source: String,
}

fn platform_roaming_config_root() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        if let Some(base) = directories::BaseDirs::new() {
            return base.home_dir().join("Library").join("Application Support");
        }
    }
    #[cfg(windows)]
    {
        if let Ok(appdata) = std::env::var("APPDATA")
            && !appdata.trim().is_empty()
        {
            return PathBuf::from(appdata);
        }
        if let Some(base) = directories::BaseDirs::new() {
            return base.data_dir().to_path_buf();
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Some(base) = directories::BaseDirs::new() {
            return base.config_dir().to_path_buf();
        }
    }
    PathBuf::from(".")
}

/// 返回桌面版 `Antigravity` 的用户数据根目录。
pub fn desktop_user_data_dir() -> PathBuf {
    platform_roaming_config_root().join("Antigravity")
}

/// 返回编辑器版 `Antigravity IDE` 的用户数据根目录（按已存在的候选名优先匹配）。
pub fn ide_user_data_dir() -> PathBuf {
    let root = platform_roaming_config_root();
    for candidate in ["Antigravity IDE", "antigravity-ide", "antigravity_ide"] {
        let dir = root.join(candidate);
        if dir.exists() {
            return dir;
        }
    }
    root.join("Antigravity IDE")
}

pub fn user_data_dir_for(kind: AntigravityClientKind) -> PathBuf {
    match kind {
        AntigravityClientKind::Ide => ide_user_data_dir(),
        AntigravityClientKind::Desktop => desktop_user_data_dir(),
        AntigravityClientKind::Auto => {
            let desktop = desktop_user_data_dir();
            if desktop.exists() {
                desktop
            } else {
                let ide = ide_user_data_dir();
                if ide.exists() { ide } else { desktop }
            }
        }
    }
}

pub fn global_storage_dir(user_data_dir: &Path) -> PathBuf {
    user_data_dir.join("User").join("globalStorage")
}

pub fn state_vscdb_path(user_data_dir: &Path) -> PathBuf {
    global_storage_dir(user_data_dir).join("state.vscdb")
}

pub fn storage_json_path(user_data_dir: &Path) -> PathBuf {
    global_storage_dir(user_data_dir).join("storage.json")
}

fn open_state_db(db_path: &Path) -> anyhow::Result<Connection> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("创建数据库父目录失败: {}", parent.display()))?;
    }
    let conn = Connection::open(db_path)
        .with_context(|| format!("打开 state.vscdb 失败: {}", db_path.display()))?;
    conn.busy_timeout(Duration::from_millis(2000))?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS ItemTable (key TEXT PRIMARY KEY, value TEXT);",
    )?;
    Ok(conn)
}

fn query_item_value(conn: &Connection, key: &str) -> Option<String> {
    conn.query_row(
        "SELECT value FROM ItemTable WHERE key = ?1",
        params![key],
        |row| row.get::<_, String>(0),
    )
    .optional()
    .ok()
    .flatten()
}

/// 从指定 `state.vscdb` 中解析当前登录账号的 `refresh_token` 与元信息。
pub fn read_token_from_state_vscdb(
    db_path: &Path,
    source_label: &str,
) -> Option<DiscoveredLocalToken> {
    if !db_path.is_file() {
        return None;
    }
    let conn = Connection::open(db_path).ok()?;
    let _ = conn.busy_timeout(Duration::from_millis(1500));

    let email_from_status = query_item_value(&conn, "antigravityUnifiedStateSync.userStatus")
        .and_then(|raw| {
            decode_unified_state_entry_with_target(&raw, Some("userStatusSentinelKey")).ok()
        })
        .and_then(|(_, payload)| {
            find_string_field(&payload, 7).or_else(|| find_string_field(&payload, 3))
        })
        .filter(|e| e.contains('@'));

    let project_id = query_item_value(&conn, "antigravityUnifiedStateSync.enterprisePreferences")
        .and_then(|raw| {
            decode_unified_state_entry_with_target(&raw, Some("enterpriseGcpProjectId")).ok()
        })
        .and_then(|(_, payload)| find_string_field(&payload, 3))
        .filter(|s| !s.trim().is_empty());

    // 1. 新版 >= 1.16.5: antigravityUnifiedStateSync.oauthToken
    if let Some(raw_oauth) = query_item_value(&conn, "antigravityUnifiedStateSync.oauthToken")
        && let Ok((_, oauth_blob)) =
            decode_unified_state_entry_with_target(&raw_oauth, Some("oauthTokenInfoSentinelKey"))
        && let Some(refresh_token) = find_string_field(&oauth_blob, 3)
        && !refresh_token.trim().is_empty()
    {
        let access_token = find_string_field(&oauth_blob, 1).filter(|s| !s.trim().is_empty());
        let id_token = find_string_field(&oauth_blob, 5).filter(|s| !s.trim().is_empty());
        let is_gcp_tos = find_varint_field(&oauth_blob, 6)
            .ok()
            .flatten()
            .is_some_and(|v| v != 0);
        return Some(DiscoveredLocalToken {
            email: email_from_status,
            access_token,
            refresh_token,
            id_token,
            project_id,
            is_gcp_tos,
            source: source_label.to_string(),
        });
    }

    // 2. 旧版 < 1.16.5 回退: jetskiStateSync.agentManagerInitState
    if let Some(legacy_b64) = query_item_value(&conn, "jetskiStateSync.agentManagerInitState")
        && let Ok(outer) = STANDARD.decode(legacy_b64.trim())
        && let Ok(Some(oauth_blob)) = find_field(&outer, 6)
        && let Some(refresh_token) = find_string_field(&oauth_blob, 3)
        && !refresh_token.trim().is_empty()
    {
        let access_token = find_string_field(&oauth_blob, 1).filter(|s| !s.trim().is_empty());
        let email = email_from_status.or_else(|| find_string_field(&outer, 2));
        return Some(DiscoveredLocalToken {
            email,
            access_token,
            refresh_token,
            id_token: None,
            project_id,
            is_gcp_tos: false,
            source: format!("{source_label} (legacy)"),
        });
    }

    None
}

/// 将指定账号的 OAuth 凭据与设备指纹写入目标 `state.vscdb`（写前自动备份 `.vscdb.backup`）。
pub fn write_credentials_to_state_vscdb(
    db_path: &Path,
    email: &str,
    access_token: &str,
    refresh_token: &str,
    expiry_unix_secs: i64,
    id_token: Option<&str>,
    project_id: Option<&str>,
    is_gcp_tos: bool,
    device_profile: Option<&AntigravityDeviceProfile>,
) -> anyhow::Result<()> {
    if db_path.is_file() {
        let backup = db_path.with_extension("vscdb.backup");
        let _ = std::fs::copy(db_path, backup);
    }

    let mut conn = open_state_db(db_path)?;

    let oauth_info = create_oauth_info(
        access_token,
        refresh_token,
        expiry_unix_secs,
        is_gcp_tos,
        id_token,
        Some(email),
    );

    let merged_oauth_b64 = if let Some(existing_b64) =
        query_item_value(&conn, "antigravityUnifiedStateSync.oauthToken")
        && let Ok(existing_blob) = STANDARD.decode(existing_b64.trim())
        && let Ok(mut stripped) =
            remove_unified_topic_entry(&existing_blob, "oauthTokenInfoSentinelKey")
    {
        stripped.extend(create_unified_topic_entry(
            "oauthTokenInfoSentinelKey",
            &oauth_info,
        ));
        STANDARD.encode(stripped)
    } else {
        create_unified_state_entry("oauthTokenInfoSentinelKey", &oauth_info)
    };

    let user_status_b64 = create_unified_state_entry(
        "userStatusSentinelKey",
        &create_minimal_user_status_payload(email),
    );

    let tx = conn.transaction()?;
    tx.execute(
        "INSERT OR REPLACE INTO ItemTable (key, value) VALUES (?1, ?2)",
        params!["antigravityUnifiedStateSync.oauthToken", merged_oauth_b64],
    )?;
    tx.execute(
        "INSERT OR REPLACE INTO ItemTable (key, value) VALUES (?1, ?2)",
        params!["antigravityUnifiedStateSync.userStatus", user_status_b64],
    )?;
    tx.execute(
        "INSERT OR REPLACE INTO ItemTable (key, value) VALUES ('antigravityOnboarding', 'true')",
        [],
    )?;
    // 清理旧版残留键，防止旧 UserID 干扰会话列表加载
    tx.execute(
        "DELETE FROM ItemTable WHERE key = 'jetskiStateSync.agentManagerInitState'",
        [],
    )?;

    if let Some(pid) = project_id.map(str::trim).filter(|s| !s.is_empty()) {
        let ent_b64 = create_unified_state_entry(
            "enterpriseGcpProjectId",
            &create_string_value_payload(pid),
        );
        tx.execute(
            "INSERT OR REPLACE INTO ItemTable (key, value) VALUES (?1, ?2)",
            params!["antigravityUnifiedStateSync.enterprisePreferences", ent_b64],
        )?;
    } else {
        tx.execute(
            "DELETE FROM ItemTable WHERE key = 'antigravityUnifiedStateSync.enterprisePreferences'",
            [],
        )?;
    }

    if let Some(dev) = device_profile {
        tx.execute(
            "INSERT OR REPLACE INTO ItemTable (key, value) VALUES ('storage.serviceMachineId', ?1)",
            params![dev.dev_device_id],
        )?;
        tx.execute(
            "INSERT OR REPLACE INTO ItemTable (key, value) VALUES ('telemetry.serviceMachineId', ?1)",
            params![dev.mac_machine_id],
        )?;
    }

    tx.commit()?;
    Ok(())
}

/// 仅同步机器码双键到目标 `state.vscdb`。
pub fn sync_device_profile_to_state_vscdb(
    db_path: &Path,
    profile: &AntigravityDeviceProfile,
) -> anyhow::Result<()> {
    if !db_path.is_file() {
        return Ok(());
    }
    let conn = open_state_db(db_path)?;
    conn.execute(
        "INSERT OR REPLACE INTO ItemTable (key, value) VALUES ('storage.serviceMachineId', ?1)",
        params![profile.dev_device_id],
    )?;
    conn.execute(
        "INSERT OR REPLACE INTO ItemTable (key, value) VALUES ('telemetry.serviceMachineId', ?1)",
        params![profile.mac_machine_id],
    )?;
    Ok(())
}
