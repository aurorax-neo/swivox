//! Antigravity 系统钥匙串（macOS Keychain / Windows Credential Manager / Linux Secret Service）与 `~/.gemini/` 文件级凭据同步。

use std::path::PathBuf;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::{SecondsFormat, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::db::DiscoveredLocalToken;

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

/// 对齐 Antigravity `language_server` (`codeassistclient.StoredToken`) 的序列化结构。
/// 注意：新版 `language_server` 已将 `id_token` 标记为 `LegacyIDToken` 并从钥匙串载荷中省略，
/// 仅保留 `id_token_email`，以规避 Windows Credential Manager `CRED_MAX_CREDENTIAL_BLOB_SIZE` (2560B) 限制。
#[derive(Debug, Clone, Serialize, Deserialize)]
struct KeyringPayload {
    token: KeyringTokenInner,
    #[serde(default = "default_consumer")]
    auth_method: String,
    #[serde(default)]
    id_token_email: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    project_id: Option<String>,
    #[serde(default, skip_serializing)]
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
    project_id: Option<&str>,
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
        project_id: project_id
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToString::to_string),
        id_token: None,
    };
    Ok(serde_json::to_string(&payload)?)
}

fn encode_go_keyring_secret(payload_json: &str) -> String {
    format!("{GO_KEYRING_PREFIX}{}", STANDARD.encode(payload_json.as_bytes()))
}

fn decode_go_keyring_secret(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(b64) = trimmed.strip_prefix(GO_KEYRING_PREFIX) {
        STANDARD
            .decode(b64.trim())
            .ok()
            .and_then(|bytes| String::from_utf8(bytes).ok())
    } else {
        Some(trimmed.to_string())
    }
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
        project_id: parsed.project_id.filter(|s| !s.trim().is_empty()),
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
        if let Some(secret_bytes) =
            crate::macos_keychain::find_generic_password_silent("gemini", "antigravity")
        {
            let secret = String::from_utf8_lossy(&secret_bytes);
            if let Some(json_str) = decode_go_keyring_secret(&secret)
                && let Some(found) = parse_keyring_payload_json(&json_str, "macOS Keychain")
            {
                return Some(found);
            }
        }
    }

    #[cfg(windows)]
    {
        if let Some(secret_bytes) = read_windows_generic_credential("gemini:antigravity")
            && let Ok(secret_str) = String::from_utf8(secret_bytes)
            && let Some(json_str) = decode_go_keyring_secret(&secret_str)
            && let Some(found) =
                parse_keyring_payload_json(&json_str, "Windows Credential Manager")
        {
            return Some(found);
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
            if let Some(decoded) = decode_go_keyring_secret(&raw)
                && let Some(found) = parse_keyring_payload_json(&decoded, "Secret Service")
            {
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

fn write_os_keyring_secret(encoded_secret: &str) -> anyhow::Result<()> {
    #[cfg(target_os = "macos")]
    {
        use anyhow::Context as _;
        crate::macos_keychain::add_generic_password(
            "gemini",
            "antigravity",
            encoded_secret.as_bytes(),
            true,
            true,
        )
        .context("写入 macOS Keychain (gemini/antigravity) 失败")?;
        return Ok(());
    }

    #[cfg(windows)]
    {
        use anyhow::Context as _;
        write_windows_generic_credential(
            "gemini:antigravity",
            "antigravity",
            encoded_secret.as_bytes(),
        )
        .context("写入 Windows Credential Manager (gemini:antigravity) 失败")?;
        return Ok(());
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        use std::io::Write as _;
        let mut child = std::process::Command::new("secret-tool")
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
            .stderr(std::process::Stdio::piped())
            .spawn()?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(encoded_secret.as_bytes())?;
        }
        let output = child.wait_with_output()?;
        anyhow::ensure!(
            output.status.success(),
            "secret-tool 写入失败: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
        return Ok(());
    }

    #[allow(unreachable_code)]
    Ok(())
}

fn clear_stale_os_keyring_entry() {
    #[cfg(target_os = "macos")]
    {
        let _ = crate::macos_keychain::delete_generic_password("gemini", "antigravity");
    }
    #[cfg(windows)]
    {
        delete_windows_generic_credential("gemini:antigravity");
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = std::process::Command::new("secret-tool")
            .args(["clear", "service", "gemini", "username", "antigravity"])
            .output();
    }
}

/// 将账号凭据同步写入 `~/.gemini/` 文件级回退凭据与系统钥匙串（适用于桌面版 2.0+ 与 `agy` 语言服务）。
/// 先落盘 `~/.gemini/` 确保文件回退始终最新；若系统钥匙串写入失败则主动清理旧钥匙串条目，使 `language_server`
/// 平滑降级读取新写入的文件凭据而绝不误读旧账号。
pub fn write_system_keyring_and_gemini_files(
    email: &str,
    access_token: &str,
    refresh_token: &str,
    expiry_unix_secs: i64,
    id_token: Option<&str>,
    project_id: Option<&str>,
) -> anyhow::Result<()> {
    let payload_json = build_keyring_payload_json(
        email,
        access_token,
        refresh_token,
        expiry_unix_secs,
        project_id,
    )?;
    let encoded_secret = encode_go_keyring_secret(&payload_json);

    let mut file_fallback_ok = false;
    if let Some(dir) = gemini_dir() {
        let pretty_payload = serde_json::to_string_pretty(
            &serde_json::from_str::<serde_json::Value>(&payload_json).unwrap_or_default(),
        )
        .unwrap_or_else(|_| payload_json.clone());
        if write_secure_file(&dir.join("jetski-standalone-oauth-token"), &pretty_payload).is_ok() {
            file_fallback_ok = true;
        }

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

    match write_os_keyring_secret(&encoded_secret) {
        Ok(()) => {
            if let Some(dir) = gemini_dir() {
                let marker = dir.join("jetski-standalone-oauth-token-keyring-unavailable");
                if marker.is_file() {
                    let _ = std::fs::remove_file(marker);
                }
            }
            Ok(())
        }
        Err(keyring_err) => {
            clear_stale_os_keyring_entry();
            let _ = crate::diagnostic_log::append_diagnostic_log(
                "antigravity.keyring_write_failed",
                json!({
                    "error": keyring_err.to_string(),
                    "file_fallback_ok": file_fallback_ok,
                }),
            );
            if file_fallback_ok {
                Ok(())
            } else {
                Err(keyring_err)
            }
        }
    }
}

#[cfg(windows)]
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

#[cfg(windows)]
#[link(name = "advapi32")]
unsafe extern "system" {
    fn CredWriteW(credential: *const CredentialW, flags: u32) -> i32;
    fn CredReadW(target_name: *const u16, cred_type: u32, flags: u32, credential: *mut *mut CredentialW) -> i32;
    fn CredDeleteW(target_name: *const u16, cred_type: u32, flags: u32) -> i32;
    fn CredFree(buffer: *mut core::ffi::c_void);
}

#[cfg(windows)]
fn write_windows_generic_credential(target: &str, username: &str, blob: &[u8]) -> anyhow::Result<()> {
    use std::os::windows::ffi::OsStrExt as _;
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
    let ok = unsafe { CredWriteW(&cred, 0) };
    if ok == 0 {
        let err = std::io::Error::last_os_error();
        anyhow::bail!("CredWriteW 失败 (len={}): {err}", blob_buf.len());
    }
    Ok(())
}

#[cfg(windows)]
fn read_windows_generic_credential(target: &str) -> Option<Vec<u8>> {
    use std::os::windows::ffi::OsStrExt as _;
    let target_w: Vec<u16> = std::ffi::OsStr::new(target)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut cred_ptr: *mut CredentialW = std::ptr::null_mut();
    // SAFETY: target_w 以 NUL 结尾；成功后通过 CredFree 释放系统分配的缓冲区。
    let ok = unsafe { CredReadW(target_w.as_ptr(), 1, 0, &mut cred_ptr) };
    if ok == 0 || cred_ptr.is_null() {
        return None;
    }
    let bytes = unsafe {
        let cred = &*cred_ptr;
        let out = if cred.credential_blob.is_null() || cred.credential_blob_size == 0 {
            Vec::new()
        } else {
            std::slice::from_raw_parts(cred.credential_blob, cred.credential_blob_size as usize)
                .to_vec()
        };
        CredFree(cred_ptr.cast());
        out
    };
    (!bytes.is_empty()).then_some(bytes)
}

#[cfg(windows)]
fn delete_windows_generic_credential(target: &str) {
    use std::os::windows::ffi::OsStrExt as _;
    let target_w: Vec<u16> = std::ffi::OsStr::new(target)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        let _ = CredDeleteW(target_w.as_ptr(), 1, 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyring_payload_stays_compact_for_wincred_and_roundtrips_go_keyring_prefix() {
        let json_str = build_keyring_payload_json(
            "alice@example.com",
            &"a".repeat(260),
            &"r".repeat(128),
            1_893_456_000,
            Some("my-gcp-project"),
        )
        .unwrap();
        assert!(!json_str.contains("\"id_token\""));

        let encoded = encode_go_keyring_secret(&json_str);
        assert!(encoded.starts_with("go-keyring-base64:"));
        // 必须远小于 Windows CRED_MAX_CREDENTIAL_BLOB_SIZE (2560 字节)
        assert!(
            encoded.len() < 1500,
            "encoded secret len={} must stay well under 2560 bytes",
            encoded.len()
        );

        let decoded = decode_go_keyring_secret(&encoded).unwrap();
        let token = parse_keyring_payload_json(&decoded, "unit-test").unwrap();
        assert_eq!(token.email.as_deref(), Some("alice@example.com"));
        assert_eq!(token.project_id.as_deref(), Some("my-gcp-project"));
        assert_eq!(token.refresh_token.len(), 128);
    }
}
