//! Antigravity 系统钥匙串（macOS Keychain / Windows Credential Manager / Linux Secret Service）与 `~/.gemini/` 文件级凭据同步。

use std::path::PathBuf;

#[cfg(unix)]
use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::{SecondsFormat, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::db::DiscoveredLocalToken;

#[cfg(unix)]
const GO_KEYRING_PREFIX: &str = "go-keyring-base64:";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct KeyringTokenInner {
    access_token: String,
    #[serde(default = "default_bearer")]
    token_type: String,
    refresh_token: String,
    #[serde(default)]
    expiry: String,
    #[serde(default)]
    expires_in: i64,
}

fn default_bearer() -> String {
    "Bearer".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct KeyringPayload {
    token: KeyringTokenInner,
    #[serde(default = "default_consumer")]
    auth_method: String,
    #[serde(default)]
    id_token_email: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    id_token: Option<String>,
}

fn default_consumer() -> String {
    "consumer".to_string()
}

fn format_expiry_rfc3339(expiry_unix_secs: i64) -> String {
    Utc.timestamp_opt(expiry_unix_secs, 0)
        .single()
        .unwrap_or_else(Utc::now)
        .to_rfc3339_opts(SecondsFormat::Micros, true)
}

fn build_keyring_payload_json(
    email: &str,
    access_token: &str,
    refresh_token: &str,
    expiry_unix_secs: i64,
    id_token: Option<&str>,
) -> anyhow::Result<String> {
    let now = Utc::now().timestamp();
    let expires_in = (expiry_unix_secs - now).max(60);
    let payload = KeyringPayload {
        token: KeyringTokenInner {
            access_token: access_token.to_string(),
            token_type: "Bearer".to_string(),
            refresh_token: refresh_token.to_string(),
            expiry: format_expiry_rfc3339(expiry_unix_secs),
            expires_in,
        },
        auth_method: "consumer".to_string(),
        id_token_email: email.to_string(),
        id_token: id_token
            .filter(|s| !s.trim().is_empty())
            .map(ToString::to_string),
    };
    Ok(serde_json::to_string(&payload)?)
}

fn parse_keyring_payload_json(raw_json: &str, source: &str) -> Option<DiscoveredLocalToken> {
    let parsed: KeyringPayload = serde_json::from_str(raw_json.trim()).ok()?;
    if parsed.token.refresh_token.trim().is_empty() {
        return None;
    }
    let email = (!parsed.id_token_email.trim().is_empty()).then_some(parsed.id_token_email);
    let access_token =
        (!parsed.token.access_token.trim().is_empty()).then_some(parsed.token.access_token);
    Some(DiscoveredLocalToken {
        email,
        access_token,
        refresh_token: parsed.token.refresh_token,
        id_token: parsed.id_token.filter(|s| !s.trim().is_empty()),
        project_id: None,
        is_gcp_tos: false,
        source: source.to_string(),
    })
}

fn gemini_dir() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|d| d.home_dir().join(".gemini"))
}

fn write_secure_file(path: &std::path::Path, contents: &str) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("swivox-tmp");
    {
        use std::io::Write as _;
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(contents.as_bytes())?;
        let _ = file.sync_all();
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// 读取系统钥匙串及 `~/.gemini/jetski-standalone-oauth-token` 中的当前桌面版登录凭据。
pub fn read_system_keyring_or_fallback_token() -> Option<DiscoveredLocalToken> {
    #[cfg(target_os = "macos")]
    {
        if let Ok(output) = std::process::Command::new("/usr/bin/security")
            .args([
                "find-generic-password",
                "-s",
                "gemini",
                "-a",
                "antigravity",
                "-w",
            ])
            .output()
            && output.status.success()
        {
            let secret = String::from_utf8_lossy(&output.stdout);
            let trimmed = secret.trim();
            let decoded_str = if let Some(b64) = trimmed.strip_prefix(GO_KEYRING_PREFIX) {
                STANDARD
                    .decode(b64.trim())
                    .ok()
                    .and_then(|bytes| String::from_utf8(bytes).ok())
            } else {
                Some(trimmed.to_string())
            };
            if let Some(json_str) = decoded_str
                && let Some(found) = parse_keyring_payload_json(&json_str, "macOS Keychain")
            {
                return Some(found);
            }
        }
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Ok(output) = std::process::Command::new("secret-tool")
            .args(["lookup", "service", "gemini", "username", "antigravity"])
            .output()
            && output.status.success()
        {
            let raw = String::from_utf8_lossy(&output.stdout);
            let trimmed = raw.trim();
            let decoded = trimmed
                .strip_prefix(GO_KEYRING_PREFIX)
                .and_then(|b64| STANDARD.decode(b64.trim()).ok())
                .and_then(|b| String::from_utf8(b).ok())
                .unwrap_or_else(|| trimmed.to_string());
            if let Some(found) = parse_keyring_payload_json(&decoded, "Secret Service") {
                return Some(found);
            }
        }
    }

    // 文件级回退：`~/.gemini/jetski-standalone-oauth-token` 与 `~/.gemini/oauth_creds.json`
    if let Some(dir) = gemini_dir() {
        let standalone = dir.join("jetski-standalone-oauth-token");
        if let Ok(raw) = std::fs::read_to_string(&standalone)
            && let Some(found) = parse_keyring_payload_json(&raw, "jetski-standalone-oauth-token")
        {
            return Some(found);
        }
        let oauth_creds = dir.join("oauth_creds.json");
        if let Ok(raw) = std::fs::read_to_string(&oauth_creds)
            && let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw)
            && let Some(rt) = val
                .get("refresh_token")
                .and_then(serde_json::Value::as_str)
                .filter(|s| !s.trim().is_empty())
        {
            let active_email = std::fs::read_to_string(dir.join("google_accounts.json"))
                .ok()
                .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
                .and_then(|v| {
                    v.get("active")
                        .and_then(serde_json::Value::as_str)
                        .map(ToString::to_string)
                });
            return Some(DiscoveredLocalToken {
                email: active_email,
                access_token: val
                    .get("access_token")
                    .and_then(serde_json::Value::as_str)
                    .map(ToString::to_string),
                refresh_token: rt.to_string(),
                id_token: val
                    .get("id_token")
                    .and_then(serde_json::Value::as_str)
                    .map(ToString::to_string),
                project_id: None,
                is_gcp_tos: false,
                source: "~/.gemini/oauth_creds.json".to_string(),
            });
        }
    }

    None
}

/// 将账号凭据同步写入系统钥匙串及 `~/.gemini/` 文件级回退凭据（适用于桌面版 2.0+ 与 `agy` 语言服务）。
pub fn write_system_keyring_and_gemini_files(
    email: &str,
    access_token: &str,
    refresh_token: &str,
    expiry_unix_secs: i64,
    id_token: Option<&str>,
) -> anyhow::Result<()> {
    let payload_json = build_keyring_payload_json(
        email,
        access_token,
        refresh_token,
        expiry_unix_secs,
        id_token,
    )?;

    #[cfg(target_os = "macos")]
    {
        use anyhow::Context as _;
        let encoded = format!("{GO_KEYRING_PREFIX}{}", STANDARD.encode(payload_json.as_bytes()));
        let _ = std::process::Command::new("/usr/bin/security")
            .args(["delete-generic-password", "-s", "gemini", "-a", "antigravity"])
            .output();
        // 密钥经 stdin 传给 `security -i`，不再出现在进程参数里。
        // 保留 -A：该条目由 Antigravity 进程读取，去掉会导致其每次启动弹出钥匙串授权。
        crate::macos_keychain::add_generic_password(
            "gemini",
            "antigravity",
            encoded.as_bytes(),
            false,
            true,
        )
        .context("写入 Antigravity 钥匙串凭据失败")?;
    }

    #[cfg(windows)]
    {
        write_windows_generic_credential("gemini:antigravity", "antigravity", payload_json.as_bytes());
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        use std::io::Write as _;
        if let Ok(mut child) = std::process::Command::new("secret-tool")
            .args([
                "store",
                "--label=Password for 'antigravity' on 'gemini'",
                "service",
                "gemini",
                "username",
                "antigravity",
            ])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
        {
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(payload_json.as_bytes());
            }
            let _ = child.wait();
        }
    }

    if let Some(dir) = gemini_dir() {
        let pretty_payload = serde_json::to_string_pretty(
            &serde_json::from_str::<serde_json::Value>(&payload_json).unwrap_or_default(),
        )
        .unwrap_or(payload_json);
        let _ = write_secure_file(&dir.join("jetski-standalone-oauth-token"), &pretty_payload);

        let oauth_creds = json!({
            "access_token": access_token,
            "refresh_token": refresh_token,
            "token_type": "Bearer",
            "expiry_date": expiry_unix_secs.saturating_mul(1000),
            "id_token": id_token.unwrap_or_default(),
            "scope": "https://www.googleapis.com/auth/userinfo.email openid https://www.googleapis.com/auth/cloud-platform https://www.googleapis.com/auth/userinfo.profile"
        });
        let _ = write_secure_file(
            &dir.join("oauth_creds.json"),
            &serde_json::to_string_pretty(&oauth_creds)?,
        );

        let accounts_json = json!({
            "active": email,
            "old": []
        });
        let _ = write_secure_file(
            &dir.join("google_accounts.json"),
            &serde_json::to_string_pretty(&accounts_json)?,
        );
    }

    Ok(())
}

#[cfg(windows)]
fn write_windows_generic_credential(target: &str, username: &str, blob: &[u8]) {
    use std::os::windows::ffi::OsStrExt as _;
    #[repr(C)]
    struct CredentialW {
        flags: u32,
        cred_type: u32,
        target_name: *mut u16,
        comment: *mut u16,
        last_written: [u32; 2],
        credential_blob_size: u32,
        credential_blob: *mut u8,
        persist: u32,
        attribute_count: u32,
        attributes: *mut core::ffi::c_void,
        target_alias: *mut u16,
        user_name: *mut u16,
    }
    #[link(name = "advapi32")]
    unsafe extern "system" {
        fn CredWriteW(credential: *const CredentialW, flags: u32) -> i32;
    }
    let mut target_w: Vec<u16> = std::ffi::OsStr::new(target)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut user_w: Vec<u16> = std::ffi::OsStr::new(username)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut blob_buf = blob.to_vec();
    let cred = CredentialW {
        flags: 0,
        cred_type: 1, // CRED_TYPE_GENERIC
        target_name: target_w.as_mut_ptr(),
        comment: std::ptr::null_mut(),
        last_written: [0, 0],
        credential_blob_size: blob_buf.len() as u32,
        credential_blob: blob_buf.as_mut_ptr(),
        persist: 2, // CRED_PERSIST_LOCAL_MACHINE
        attribute_count: 0,
        attributes: std::ptr::null_mut(),
        target_alias: std::ptr::null_mut(),
        user_name: user_w.as_mut_ptr(),
    };
    // SAFETY: target_w, user_w, blob_buf 在 CredWriteW 调用期间保持有效且以 NUL 结尾。
    unsafe {
        let _ = CredWriteW(&cred, 0);
    }
}
