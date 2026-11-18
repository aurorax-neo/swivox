//! Antigravity Google OAuth 登录（自动双栈回环监听 + 手动 URL/Code 回调解析）与 Token 自动刷新。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, UNIX_EPOCH};

use anyhow::{Context, bail};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use url::Url;
use uuid::Uuid;

pub const GOOGLE_AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
pub const GOOGLE_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
pub const GOOGLE_USERINFO_URL: &str = "https://www.googleapis.com/oauth2/v2/userinfo";
pub const TOKEN_REFRESH_SKEW_SECONDS: i64 = 900;
pub const OAUTH_USER_AGENT: &str = "vscode/1.X.X (Antigravity/4.3.0)";

/// 与 Antigravity 官方客户端（Antigravity 2.32.x `SIGN_IN_CLIENTS` / `language_server`）逐项对齐：
/// 不请求 `openid`、不带 `include_granted_scopes`，个人账号客户端额外带 `aicode`。
pub const ANTIGRAVITY_OAUTH_SCOPES: &[&str] = &[
    "https://www.googleapis.com/auth/cloud-platform",
    "https://www.googleapis.com/auth/userinfo.email",
    "https://www.googleapis.com/auth/userinfo.profile",
    "https://www.googleapis.com/auth/cclog",
    "https://www.googleapis.com/auth/experimentsandconfigs",
];
/// 个人账号客户端在通用 scope 之外额外请求的 scope。
pub const ANTIGRAVITY_PERSONAL_EXTRA_SCOPE: &str = "https://www.googleapis.com/auth/aicode";
/// 官方客户端的本地回环回调路径（`SIGN_IN_CALLBACK_PATH`）。
pub const ANTIGRAVITY_OAUTH_CALLBACK_PATH: &str = "/auth/callback";

/// 按客户端返回官方完全一致的 scope 列表（顺序与官方一致，单空格拼接）。
pub fn oauth_scopes_for_client(client_key: &str) -> Vec<&'static str> {
    let mut scopes = ANTIGRAVITY_OAUTH_SCOPES.to_vec();
    if client_key != "gcp" {
        scopes.push(ANTIGRAVITY_PERSONAL_EXTRA_SCOPE);
    }
    scopes
}

/// 兼容旧版 Swivox 发出的 `/oauth-callback` 回调（手动粘贴时可能仍是旧地址）。
fn is_oauth_callback_path(path: &str) -> bool {
    path.starts_with(ANTIGRAVITY_OAUTH_CALLBACK_PATH) || path.starts_with("/oauth-callback")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AntigravityOAuthStartResponse {
    pub login_id: String,
    pub verification_uri: String,
    pub callback_url: String,
    pub client_key: String,
}

#[derive(Debug, Clone)]
pub struct ExchangedGoogleTokens {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: i64,
    pub expiry_timestamp: i64,
    pub id_token: Option<String>,
    pub client_key: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GoogleUserInfo {
    pub email: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub given_name: Option<String>,
    #[serde(default)]
    pub family_name: Option<String>,
    #[serde(default)]
    pub picture: Option<String>,
}

impl GoogleUserInfo {
    pub fn display_name(&self) -> Option<String> {
        if let Some(n) = self.name.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            return Some(n.to_string());
        }
        let joined = [
            self.given_name.as_deref().unwrap_or("").trim(),
            self.family_name.as_deref().unwrap_or("").trim(),
        ]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
        (!joined.is_empty()).then_some(joined)
    }
}

struct PendingOAuthSession {
    login_id: String,
    state: String,
    redirect_uri: String,
    client_key: String,
    code_verifier: String,
    code_rx: Option<oneshot::Receiver<Result<String, String>>>,
    cancel_tx: Option<oneshot::Sender<()>>,
}

static PENDING_AG_OAUTH: OnceLock<Mutex<Option<PendingOAuthSession>>> = OnceLock::new();

fn pending_slot() -> &'static Mutex<Option<PendingOAuthSession>> {
    PENDING_AG_OAUTH.get_or_init(|| Mutex::new(None))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AntigravityOAuthClientPair {
    pub personal_client_id: String,
    pub personal_client_secret: String,
    pub gcp_client_id: String,
    pub gcp_client_secret: String,
    #[serde(default)]
    pub source_fingerprint: String,
}

static OAUTH_CLIENTS_CACHE: OnceLock<Mutex<Option<AntigravityOAuthClientPair>>> = OnceLock::new();

fn oauth_clients_slot() -> &'static Mutex<Option<AntigravityOAuthClientPair>> {
    OAUTH_CLIENTS_CACHE.get_or_init(|| Mutex::new(None))
}

fn non_empty_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn locate_antigravity_resources_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let mut push_from_install_path = |raw: &str| {
        let p = Path::new(raw.trim());
        if p.as_os_str().is_empty() {
            return;
        }
        // macOS: /Applications/Antigravity.app -> Contents/Resources
        if p.extension().and_then(|e| e.to_str()) == Some("app") {
            let res = p.join("Contents").join("Resources");
            if res.is_dir() && !dirs.contains(&res) {
                dirs.push(res);
            }
            return;
        }
        // macOS binary inside .app/Contents/MacOS/Electron -> ../Resources
        if let Some(macos_dir) = p.parent()
            && macos_dir.file_name().and_then(|n| n.to_str()) == Some("MacOS")
            && let Some(contents_dir) = macos_dir.parent()
        {
            let res = contents_dir.join("Resources");
            if res.is_dir() && !dirs.contains(&res) {
                dirs.push(res);
            }
        }
        // Windows / Linux: <install_dir>/Antigravity.exe -> <install_dir>/resources
        let base = if p.is_dir() { p } else { p.parent().unwrap_or(p) };
        for sub in ["resources", "Resources"] {
            let res = base.join(sub);
            if res.is_dir() && !dirs.contains(&res) {
                dirs.push(res);
            }
        }
    };

    let (custom_desk, custom_ide) = std::fs::read_to_string(crate::paths::default_antigravity_state_path())
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .map(|v| {
            (
                v.get("customDesktopPath")
                    .and_then(|x| x.as_str())
                    .map(str::to_string),
                v.get("customIdePath")
                    .and_then(|x| x.as_str())
                    .map(str::to_string),
            )
        })
        .unwrap_or((None, None));

    for kind in [
        super::db::AntigravityClientKind::Desktop,
        super::db::AntigravityClientKind::Ide,
    ] {
        let custom = match kind {
            super::db::AntigravityClientKind::Ide => custom_ide.as_deref(),
            _ => custom_desk.as_deref(),
        };
        let inst = super::process::discover_installation(kind, custom, 9333);
        if let Some(ref p) = inst.app_or_exe_path {
            push_from_install_path(p);
        }
        if let Some(ref p) = inst.executable_path {
            push_from_install_path(p);
        }
    }
    dirs
}

fn find_language_server_binary(resources_dir: &Path) -> Option<PathBuf> {
    let bin_dir = resources_dir.join("bin");
    if bin_dir.is_dir() {
        for exact in ["language_server", "language_server.exe"] {
            let p = bin_dir.join(exact);
            if p.is_file() {
                return Some(p);
            }
        }
        if let Ok(rd) = std::fs::read_dir(&bin_dir) {
            for entry in rd.flatten() {
                let p = entry.path();
                if p.is_file()
                    && p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.starts_with("language_server"))
                {
                    return Some(p);
                }
            }
        }
    }
    None
}

fn file_fingerprint(path: &Path) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    Some(format!("{}:{}:{}", path.display(), meta.len(), mtime))
}

/// 扫描形如 `<digits>-<alnum>.apps.googleusercontent.com` 的客户端 ID。
/// 返回 `(digits_start, hyphen_pos, end_pos)`，兼容 Go `.rodata` 无 `\0` 紧邻拼接字符串。
fn scan_google_client_id_spans(bytes: &[u8]) -> Vec<(usize, usize, usize)> {
    const SUFFIX: &[u8] = b".apps.googleusercontent.com";
    let mut results = Vec::new();
    let mut search_from = 0usize;
    while search_from + SUFFIX.len() <= bytes.len() {
        let Some(rel) = bytes[search_from..]
            .windows(SUFFIX.len())
            .position(|w| w == SUFFIX)
        else {
            break;
        };
        let suffix_pos = search_from + rel;
        let end_pos = suffix_pos + SUFFIX.len();
        search_from = end_pos;

        // 逆向扫描 `-` 右侧的 `[a-z0-9]+`（支持内部连字符，如个别自定义后缀，但先定位靠左的主连字符）
        let mut cur = suffix_pos;
        let mut hyphen_pos = None;
        while cur > 0 {
            let c = bytes[cur - 1];
            if c.is_ascii_lowercase() || c.is_ascii_digit() {
                cur -= 1;
            } else if c == b'-' {
                // 检查左侧是否有至少 10 位纯数字项目编号
                let h = cur - 1;
                let mut d_start = h;
                while d_start > 0 && bytes[d_start - 1].is_ascii_digit() {
                    d_start -= 1;
                }
                if h - d_start >= 10 {
                    hyphen_pos = Some((d_start, h));
                    break;
                }
                cur -= 1;
            } else {
                break;
            }
        }

        if let Some((digits_start, h_pos)) = hyphen_pos {
            let alnum_len = suffix_pos.saturating_sub(h_pos + 1);
            if (8..=64).contains(&alnum_len) && std::str::from_utf8(&bytes[digits_start..end_pos]).is_ok() {
                results.push((digits_start, h_pos, end_pos));
            }
        }
    }
    results
}

fn scan_google_client_ids(bytes: &[u8]) -> Vec<(usize, String)> {
    scan_google_client_id_spans(bytes)
        .into_iter()
        .filter_map(|(d_start, h_pos, end_pos)| {
            // 默认取最多 16 位数字项目号（常见为 11~13 位），避免前序紧邻数字串污染
            let start = d_start.max(h_pos.saturating_sub(16));
            std::str::from_utf8(&bytes[start..end_pos])
                .ok()
                .map(|s| (start, s.to_string()))
        })
        .collect()
}

fn scan_client_secrets_with_prefix(bytes: &[u8], prefix: &[u8], body_len: usize) -> Vec<(usize, String)> {
    let total_len = prefix.len() + body_len;
    let mut results = Vec::new();
    let mut pos = 0usize;
    while pos + total_len <= bytes.len() {
        let Some(rel) = bytes[pos..].windows(prefix.len()).position(|w| w == prefix) else {
            break;
        };
        let start = pos + rel;
        pos = start + prefix.len();
        if start + total_len > bytes.len() {
            break;
        }
        let body = &bytes[start + prefix.len()..start + total_len];
        if body
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || *b == b'-' || *b == b'_')
            && let Ok(sec) = std::str::from_utf8(&bytes[start..start + total_len])
        {
            results.push((start, sec.to_string()));
        }
    }
    results
}

fn slice_primary_macho_arch(bytes: &[u8]) -> &[u8] {
    // Universal Mach-O fat header: 0xCAFEBABE (big-endian)
    if bytes.len() >= 28 && &bytes[0..4] == &[0xCA, 0xFE, 0xBA, 0xBE] {
        let nfat = u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as usize;
        if nfat >= 1 {
            // 优先匹配当前架构切片（ARM64 = 0x0100000C，x86_64 = 0x01000007），否则取首个切片
            let preferred_cpu: u32 = if cfg!(target_arch = "aarch64") {
                0x0100_000c
            } else {
                0x0100_0007
            };
            let mut chosen: Option<(usize, usize)> = None;
            for idx in 0..nfat.min(8) {
                let base = 8 + idx * 20;
                if base + 20 > bytes.len() {
                    break;
                }
                let cpu = u32::from_be_bytes([bytes[base], bytes[base + 1], bytes[base + 2], bytes[base + 3]]);
                let offset =
                    u32::from_be_bytes([bytes[base + 8], bytes[base + 9], bytes[base + 10], bytes[base + 11]]) as usize;
                let size =
                    u32::from_be_bytes([bytes[base + 12], bytes[base + 13], bytes[base + 14], bytes[base + 15]]) as usize;
                if let Some(end) = offset.checked_add(size)
                    && end <= bytes.len()
                {
                    if cpu == preferred_cpu {
                        return &bytes[offset..end];
                    }
                    if chosen.is_none() {
                        chosen = Some((offset, end));
                    }
                }
            }
            if let Some((offset, end)) = chosen {
                return &bytes[offset..end];
            }
        }
    }
    bytes
}

struct BinaryLayout {
    /// `(file_off, file_size, vaddr)`
    segments: Vec<(u64, u64, u64)>,
    /// 可执行指令区间 `[start_off, end_off)`
    exec_ranges: Vec<(usize, usize)>,
}

impl BinaryLayout {
    fn parse(bytes: &[u8]) -> Self {
        // 1. Mach-O 64-bit little-endian (`0xfeedfacf`)
        if bytes.len() >= 32 && &bytes[0..4] == &[0xCF, 0xFA, 0xED, 0xFE] {
            let ncmds = u32::from_le_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]) as usize;
            let mut segments = Vec::new();
            let mut exec_ranges = Vec::new();
            let mut cmd_off = 32usize;
            for _ in 0..ncmds.min(256) {
                if cmd_off + 8 > bytes.len() {
                    break;
                }
                let cmd = u32::from_le_bytes([bytes[cmd_off], bytes[cmd_off + 1], bytes[cmd_off + 2], bytes[cmd_off + 3]]);
                let cmdsize =
                    u32::from_le_bytes([bytes[cmd_off + 4], bytes[cmd_off + 5], bytes[cmd_off + 6], bytes[cmd_off + 7]])
                        as usize;
                if cmdsize < 8 || cmd_off + cmdsize > bytes.len() {
                    break;
                }
                // LC_SEGMENT_64 == 0x19
                if cmd == 0x19 && cmdsize >= 72 {
                    let vmaddr = u64::from_le_bytes(bytes[cmd_off + 24..cmd_off + 32].try_into().unwrap());
                    let fileoff = u64::from_le_bytes(bytes[cmd_off + 40..cmd_off + 48].try_into().unwrap());
                    let filesize = u64::from_le_bytes(bytes[cmd_off + 48..cmd_off + 56].try_into().unwrap());
                    let initprot = u32::from_le_bytes(bytes[cmd_off + 60..cmd_off + 64].try_into().unwrap());
                    let nsects = u32::from_le_bytes(bytes[cmd_off + 64..cmd_off + 68].try_into().unwrap()) as usize;
                    if filesize > 0 {
                        segments.push((fileoff, filesize, vmaddr));
                    }
                    let mut found_exec_sect = false;
                    for s_idx in 0..nsects.min(128) {
                        let s = cmd_off + 72 + s_idx * 80;
                        if s + 80 > cmd_off + cmdsize {
                            break;
                        }
                        let s_size = u64::from_le_bytes(bytes[s + 40..s + 48].try_into().unwrap()) as usize;
                        let s_off = u32::from_le_bytes(bytes[s + 48..s + 52].try_into().unwrap()) as usize;
                        let s_flags = u32::from_le_bytes(bytes[s + 64..s + 68].try_into().unwrap());
                        // S_ATTR_PURE_INSTRUCTIONS (0x80000000) | S_ATTR_SOME_INSTRUCTIONS (0x00000400)
                        if (s_flags & 0x8000_0400) != 0 && s_size > 0 && s_off < bytes.len() {
                            exec_ranges.push((s_off, s_off.saturating_add(s_size).min(bytes.len())));
                            found_exec_sect = true;
                        }
                    }
                    if !found_exec_sect && (initprot & 0x04) != 0 && filesize > 0 && (fileoff as usize) < bytes.len() {
                        let start = fileoff as usize;
                        exec_ranges.push((start, start.saturating_add(filesize as usize).min(bytes.len())));
                    }
                }
                cmd_off += cmdsize;
            }
            if !segments.is_empty() {
                if exec_ranges.is_empty() {
                    exec_ranges.push((0, bytes.len()));
                }
                return Self { segments, exec_ranges };
            }
        }

        // 2. Windows PE32 / PE32+ (`MZ` + `PE\0\0`)
        if bytes.len() >= 0x40 && &bytes[0..2] == b"MZ" {
            let pe_off = u32::from_le_bytes([bytes[0x3c], bytes[0x3d], bytes[0x3e], bytes[0x3f]]) as usize;
            if pe_off + 24 <= bytes.len() && &bytes[pe_off..pe_off + 4] == b"PE\0\0" {
                let num_sections = u16::from_le_bytes([bytes[pe_off + 6], bytes[pe_off + 7]]) as usize;
                let opt_hdr_size = u16::from_le_bytes([bytes[pe_off + 20], bytes[pe_off + 21]]) as usize;
                let sec_table = pe_off + 24 + opt_hdr_size;
                if sec_table + num_sections * 40 <= bytes.len() {
                    let mut segments = Vec::with_capacity(num_sections);
                    let mut exec_ranges = Vec::new();
                    for i in 0..num_sections {
                        let s = sec_table + i * 40;
                        let virt_addr = u32::from_le_bytes(bytes[s + 12..s + 16].try_into().unwrap()) as u64;
                        let raw_size = u32::from_le_bytes(bytes[s + 16..s + 20].try_into().unwrap()) as u64;
                        let raw_ptr = u32::from_le_bytes(bytes[s + 20..s + 24].try_into().unwrap()) as u64;
                        let chars = u32::from_le_bytes(bytes[s + 36..s + 40].try_into().unwrap());
                        if raw_size > 0 && (raw_ptr > 0 || virt_addr == 0) {
                            segments.push((raw_ptr, raw_size, virt_addr));
                            // IMAGE_SCN_MEM_EXECUTE (0x20000000) | IMAGE_SCN_CNT_CODE (0x00000020)
                            if (chars & 0x2000_0020) != 0 && (raw_ptr as usize) < bytes.len() {
                                let start = raw_ptr as usize;
                                exec_ranges.push((start, start.saturating_add(raw_size as usize).min(bytes.len())));
                            }
                        }
                    }
                    if exec_ranges.is_empty() {
                        exec_ranges.push((0, bytes.len()));
                    }
                    return Self { segments, exec_ranges };
                }
            }
        }

        // 3. ELF64 little-endian (`\x7fELF`, class=2, data=1)
        if bytes.len() >= 64 && &bytes[0..4] == b"\x7fELF" && bytes[4] == 2 && bytes[5] == 1 {
            let phoff = u64::from_le_bytes(bytes[32..40].try_into().unwrap()) as usize;
            let phentsize = u16::from_le_bytes(bytes[54..56].try_into().unwrap()) as usize;
            let phnum = u16::from_le_bytes(bytes[56..58].try_into().unwrap()) as usize;
            if phentsize >= 56 && phoff.saturating_add(phentsize.saturating_mul(phnum)) <= bytes.len() {
                let mut segments = Vec::new();
                let mut exec_ranges = Vec::new();
                for i in 0..phnum.min(64) {
                    let p = phoff + i * phentsize;
                    let p_type = u32::from_le_bytes(bytes[p..p + 4].try_into().unwrap());
                    // PT_LOAD == 1
                    if p_type == 1 {
                        let p_flags = u32::from_le_bytes(bytes[p + 4..p + 8].try_into().unwrap());
                        let p_offset = u64::from_le_bytes(bytes[p + 8..p + 16].try_into().unwrap());
                        let p_vaddr = u64::from_le_bytes(bytes[p + 16..p + 24].try_into().unwrap());
                        let p_filesz = u64::from_le_bytes(bytes[p + 32..p + 40].try_into().unwrap());
                        if p_filesz > 0 {
                            segments.push((p_offset, p_filesz, p_vaddr));
                            // PF_X == 0x1
                            if (p_flags & 0x1) != 0 && (p_offset as usize) < bytes.len() {
                                let start = p_offset as usize;
                                exec_ranges.push((start, start.saturating_add(p_filesz as usize).min(bytes.len())));
                            }
                        }
                    }
                }
                if !segments.is_empty() {
                    if exec_ranges.is_empty() {
                        exec_ranges.push((0, bytes.len()));
                    }
                    return Self { segments, exec_ranges };
                }
            }
        }

        Self {
            segments: Vec::new(),
            exec_ranges: vec![(0, bytes.len())],
        }
    }

    #[inline]
    fn file_off_to_vaddr(&self, file_off: usize) -> u64 {
        let fo = file_off as u64;
        for &(raw_ptr, raw_size, virt_addr) in &self.segments {
            if fo >= raw_ptr && fo < raw_ptr.saturating_add(raw_size) {
                return virt_addr.wrapping_add(fo - raw_ptr);
            }
        }
        fo
    }
}

#[derive(Clone)]
enum TargetEntry {
    ClientId(String),
    ClientSecret(usize),
}

fn pair_client_ids_and_secrets_in_binary(
    raw_bytes: &[u8],
    secret_prefix: &[u8],
    secret_body_len: usize,
) -> HashMap<String, String> {
    let bytes = slice_primary_macho_arch(raw_bytes);
    let cid_spans = scan_google_client_id_spans(bytes);
    let secs = scan_client_secrets_with_prefix(bytes, secret_prefix, secret_body_len);
    let mut paired = HashMap::new();
    if cid_spans.is_empty() || secs.is_empty() {
        return paired;
    }

    let layout = BinaryLayout::parse(bytes);
    let mut target_vaddrs: HashMap<u64, TargetEntry> = HashMap::new();
    let mut target_pages: std::collections::HashSet<u64> = std::collections::HashSet::new();

    for &(d_start, h_pos, end_pos) in &cid_spans {
        // 注册所有合法项目号数字起点（兼容 Go `.rodata` 前序字符串末尾恰好是数字的情形）
        let latest_start = h_pos.saturating_sub(10);
        let earliest_start = d_start.max(h_pos.saturating_sub(20));
        for s in earliest_start..=latest_start {
            if let Ok(cid_str) = std::str::from_utf8(&bytes[s..end_pos]) {
                let vaddr = layout.file_off_to_vaddr(s);
                target_vaddrs.insert(vaddr, TargetEntry::ClientId(cid_str.to_string()));
                target_pages.insert(vaddr & !0xfff);
            }
        }
    }
    for (idx, (off, _)) in secs.iter().enumerate() {
        let vaddr = layout.file_off_to_vaddr(*off);
        target_vaddrs.insert(vaddr, TargetEntry::ClientSecret(idx));
        target_pages.insert(vaddr & !0xfff);
    }

    let mut xrefs: Vec<(usize, TargetEntry)> = Vec::new();

    // 1. ARM64 ADRP + ADD 交叉引用配对
    for &(range_start, range_end) in &layout.exec_ranges {
        let mut pc = (range_start + 3) & !3;
        while pc + 8 <= range_end {
            let insn = u32::from_le_bytes([bytes[pc], bytes[pc + 1], bytes[pc + 2], bytes[pc + 3]]);
            if (insn & 0x9f00_0000) == 0x9000_0000 {
                let immlo = ((insn >> 29) & 0x3) as i64;
                let immhi = ((insn >> 5) & 0x7ffff) as i64;
                let mut imm = (immhi << 2) | immlo;
                if (imm & (1 << 20)) != 0 {
                    imm -= 1 << 21;
                }
                let pc_page = (layout.file_off_to_vaddr(pc) & !0xfff) as i64;
                let page = pc_page.wrapping_add(imm << 12) as u64;
                if target_pages.contains(&page) {
                    let rd = insn & 0x1f;
                    for k in 1..=5 {
                        let next_pc = pc + 4 * k;
                        if next_pc + 4 > range_end {
                            break;
                        }
                        let insn2 = u32::from_le_bytes([
                            bytes[next_pc],
                            bytes[next_pc + 1],
                            bytes[next_pc + 2],
                            bytes[next_pc + 3],
                        ]);
                        if (insn2 & 0xffc0_0000) == 0x9100_0000 && ((insn2 >> 5) & 0x1f) == rd {
                            let imm12 = ((insn2 >> 10) & 0xfff) as u64;
                            let addr = page | imm12;
                            if let Some(entry) = target_vaddrs.get(&addr) {
                                xrefs.push((pc, entry.clone()));
                            }
                        }
                    }
                }
            }
            pc += 4;
        }
    }

    // 2. x86_64 RIP-relative LEA (`REX.W 8D ModRM disp32`) 交叉引用配对
    if xrefs.is_empty() {
        for &(range_start, range_end) in &layout.exec_ranges {
            if range_end.saturating_sub(range_start) < 7 {
                continue;
            }
            for i in range_start..=(range_end - 7) {
                let rex = bytes[i];
                if (rex & 0xf8) == 0x48 && bytes[i + 1] == 0x8d && (bytes[i + 2] & 0xc7) == 0x05 {
                    let disp = i32::from_le_bytes([bytes[i + 3], bytes[i + 4], bytes[i + 5], bytes[i + 6]]) as i64;
                    let next_vaddr = layout.file_off_to_vaddr(i + 7) as i64;
                    let addr = next_vaddr.wrapping_add(disp) as u64;
                    if let Some(entry) = target_vaddrs.get(&addr) {
                        xrefs.push((i, entry.clone()));
                    }
                }
            }
        }
    }

    // 按指令间距升序做一对一贪心配对，防止相邻结构体字面量的首尾指令串位误配
    let mut candidate_pairs: Vec<(usize, usize, usize, String, usize)> = Vec::new();
    for i in 0..xrefs.len().saturating_sub(1) {
        let (pc1, ref e1) = xrefs[i];
        let (pc2, ref e2) = xrefs[i + 1];
        let dist = pc2.saturating_sub(pc1);
        if dist <= 128 {
            match (e1, e2) {
                (TargetEntry::ClientId(cid), TargetEntry::ClientSecret(sec_idx))
                | (TargetEntry::ClientSecret(sec_idx), TargetEntry::ClientId(cid)) => {
                    candidate_pairs.push((dist, i, i + 1, cid.clone(), *sec_idx));
                }
                _ => {}
            }
        }
    }
    candidate_pairs.sort_by_key(|c| c.0);
    let mut used_xref = vec![false; xrefs.len()];
    for (_, i1, i2, cid, sec_idx) in candidate_pairs {
        if !used_xref[i1] && !used_xref[i2] {
            used_xref[i1] = true;
            used_xref[i2] = true;
            paired.entry(cid).or_insert_with(|| secs[sec_idx].1.clone());
        }
    }

    // 3. 静态数据段相邻 64-bit `GoString` (`{ptr, len}`) 结构体字段配对
    if paired.len() < cid_spans.len() && bytes.len() >= 32 {
        let sec_len = (secret_prefix.len() + secret_body_len) as u64;
        let mut off = 0usize;
        while off + 32 <= bytes.len() {
            let p1 = u64::from_le_bytes(bytes[off..off + 8].try_into().unwrap());
            if let Some(TargetEntry::ClientId(cid)) = target_vaddrs.get(&p1) {
                let l1 = u64::from_le_bytes(bytes[off + 8..off + 16].try_into().unwrap());
                let p2 = u64::from_le_bytes(bytes[off + 16..off + 24].try_into().unwrap());
                let l2 = u64::from_le_bytes(bytes[off + 24..off + 32].try_into().unwrap());
                if l1 == cid.len() as u64
                    && l2 == sec_len
                    && let Some(TargetEntry::ClientSecret(sec_idx)) = target_vaddrs.get(&p2)
                {
                    paired.entry(cid.clone()).or_insert_with(|| secs[*sec_idx].1.clone());
                }
            }
            off += 8;
        }
    }

    // 4. 等长顺序回退兜底
    let default_cids = scan_google_client_ids(bytes);
    if paired.len() < default_cids.len() && default_cids.len() == secs.len() {
        for (i, (_, cid)) in default_cids.iter().enumerate() {
            paired.entry(cid.clone()).or_insert_with(|| secs[i].1.clone());
        }
    }

    paired
}

fn extract_sign_in_client_ids_from_asar(asar_bytes: &[u8]) -> Option<(String, String)> {
    const MARKER: &[u8] = b"SIGN_IN_CLIENTS";
    let pos = asar_bytes.windows(MARKER.len()).position(|w| w == MARKER)?;
    let window_end = (pos + 2048).min(asar_bytes.len());
    let hits = scan_google_client_ids(&asar_bytes[pos..window_end]);
    let personal = hits.first()?.1.clone();
    let gcp = hits.get(1).map(|x| x.1.clone()).unwrap_or_else(|| personal.clone());
    Some((personal, gcp))
}

pub fn resolve_oauth_clients() -> anyhow::Result<AntigravityOAuthClientPair> {
    if let (Some(pid), Some(psec)) = (
        non_empty_env("SWIVOX_AG_OAUTH_CLIENT_ID"),
        non_empty_env("SWIVOX_AG_OAUTH_CLIENT_SECRET"),
    ) {
        let gid = non_empty_env("SWIVOX_AG_GCP_CLIENT_ID").unwrap_or_else(|| pid.clone());
        let gsec = non_empty_env("SWIVOX_AG_GCP_CLIENT_SECRET").unwrap_or_else(|| psec.clone());
        return Ok(AntigravityOAuthClientPair {
            personal_client_id: pid,
            personal_client_secret: psec,
            gcp_client_id: gid,
            gcp_client_secret: gsec,
            source_fingerprint: "env".to_string(),
        });
    }

    let cache_path = crate::paths::default_antigravity_oauth_clients_cache_path();
    let disk_cached: Option<AntigravityOAuthClientPair> = std::fs::read_to_string(&cache_path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok());

    let candidate_dirs = locate_antigravity_resources_dirs();
    let found_bundle = candidate_dirs.into_iter().find_map(|res_dir| {
        let asar = res_dir.join("app.asar");
        let ls = find_language_server_binary(&res_dir)?;
        if asar.is_file() {
            Some((asar, ls))
        } else {
            None
        }
    });

    if let Some((asar_path, ls_path)) = found_bundle {
        let fp = file_fingerprint(&ls_path).unwrap_or_default();
        if let Ok(guard) = oauth_clients_slot().lock()
            && let Some(ref mem) = *guard
            && mem.source_fingerprint == fp
        {
            return Ok(mem.clone());
        }
        if let Some(ref cached) = disk_cached
            && !fp.is_empty()
            && cached.source_fingerprint == fp
            && !cached.personal_client_id.is_empty()
            && !cached.personal_client_secret.is_empty()
        {
            if let Ok(mut guard) = oauth_clients_slot().lock() {
                *guard = Some(cached.clone());
            }
            return Ok(cached.clone());
        }

        let asar_bytes = std::fs::read(&asar_path)
            .with_context(|| format!("读取 Antigravity app.asar 失败: {}", asar_path.display()))?;
        let (personal_cid, gcp_cid) = extract_sign_in_client_ids_from_asar(&asar_bytes)
            .context("未能从 Antigravity app.asar 解析 SIGN_IN_CLIENTS 客户端 ID")?;

        let ls_bytes = std::fs::read(&ls_path)
            .with_context(|| format!("读取 Antigravity language_server 失败: {}", ls_path.display()))?;
        let sec_prefix = [b'G', b'O', b'C', b'S', b'P', b'X', b'-'];
        let paired = pair_client_ids_and_secrets_in_binary(&ls_bytes, &sec_prefix, 28);

        let personal_sec = paired
            .get(&personal_cid)
            .cloned()
            .context("未能从本机 Antigravity language_server 配对个人版 OAuth 参数")?;
        let gcp_sec = paired
            .get(&gcp_cid)
            .cloned()
            .unwrap_or_else(|| personal_sec.clone());

        let extracted = AntigravityOAuthClientPair {
            personal_client_id: personal_cid,
            personal_client_secret: personal_sec,
            gcp_client_id: gcp_cid,
            gcp_client_secret: gcp_sec,
            source_fingerprint: fp,
        };
        if let Ok(json_bytes) = serde_json::to_vec_pretty(&extracted) {
            let _ = crate::settings::atomic_write(&cache_path, &json_bytes);
        }
        if let Ok(mut guard) = oauth_clients_slot().lock() {
            *guard = Some(extracted.clone());
        }
        return Ok(extracted);
    }

    if let Some(cached) = disk_cached
        && !cached.personal_client_id.is_empty()
        && !cached.personal_client_secret.is_empty()
    {
        return Ok(cached);
    }

    bail!("未检测到本机安装的 Antigravity 客户端，请先安装 Antigravity 后重试，或配置环境变量 SWIVOX_AG_OAUTH_CLIENT_ID / SWIVOX_AG_OAUTH_CLIENT_SECRET。")
}

pub fn client_credentials_for(client_key: Option<&str>) -> anyhow::Result<(&'static str, String, String)> {
    let pair = resolve_oauth_clients()?;
    Ok(match client_key.unwrap_or("personal").trim() {
        "gcp" | "antigravity_gcp" | "enterprise" => (
            "gcp",
            pair.gcp_client_id,
            pair.gcp_client_secret,
        ),
        _ => (
            "personal",
            pair.personal_client_id,
            pair.personal_client_secret,
        ),
    })
}

fn alternate_client_credentials(primary_key: &str) -> anyhow::Result<(&'static str, String, String)> {
    let pair = resolve_oauth_clients()?;
    Ok(if primary_key == "gcp" {
        (
            "personal",
            pair.personal_client_id,
            pair.personal_client_secret,
        )
    } else {
        (
            "gcp",
            pair.gcp_client_id,
            pair.gcp_client_secret,
        )
    })
}

pub fn build_http_client_for_proxy(proxy_spec: Option<&str>, timeout_secs: u64) -> anyhow::Result<reqwest::Client> {
    let mut builder = reqwest::Client::builder()
        .timeout(Duration::from_secs(timeout_secs.max(5)));
    let fallback_spec = if proxy_spec.map(str::trim).filter(|s| !s.is_empty()).is_none() {
        crate::network_proxies::resolve_client_proxy_spec("antigravity_desktop")
    } else {
        None
    };
    let effective = proxy_spec
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .or(fallback_spec.as_deref());
    match effective {
        Some(spec) if spec.eq_ignore_ascii_case("direct") => {
            builder = builder.no_proxy();
        }
        Some(spec) => {
            let proxy = reqwest::Proxy::all(spec)
                .with_context(|| format!("无效的代理地址: {spec}"))?
                .no_proxy(reqwest::NoProxy::from_string("localhost,127.0.0.1,::1"));
            builder = builder.proxy(proxy);
        }
        None => {
            builder = builder.no_proxy();
        }
    }
    Ok(builder.user_agent(OAUTH_USER_AGENT).build()?)
}

/// 解析用户手动粘贴的完整回调 URL、查询串或纯授权码（支持 URL 编码的 `4%2F0A...`）。
pub fn parse_manual_oauth_input(raw_input: &str, expected_state: Option<&str>) -> anyhow::Result<String> {
    let trimmed = raw_input.trim();
    if trimmed.is_empty() {
        bail!("请输入回调链接或授权码。");
    }

    let as_url = if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        Url::parse(trimmed).ok()
    } else if trimmed.starts_with("localhost:")
        || trimmed.starts_with("127.0.0.1:")
        || trimmed.starts_with("[::1]:")
    {
        Url::parse(&format!("http://{trimmed}")).ok()
    } else if is_oauth_callback_path(trimmed) || trimmed.starts_with('?') {
        let prefix = if trimmed.starts_with('?') {
            format!("http://localhost{ANTIGRAVITY_OAUTH_CALLBACK_PATH}{trimmed}")
        } else {
            format!("http://localhost{trimmed}")
        };
        Url::parse(&prefix).ok()
    } else {
        None
    };

    if let Some(url) = as_url {
        if let Some((_, err)) = url.query_pairs().find(|(k, _)| k == "error") {
            bail!("Google 授权返回错误: {err}");
        }
        if let Some(exp) = expected_state.filter(|s| !s.is_empty())
            && let Some((_, got_state)) = url.query_pairs().find(|(k, _)| k == "state")
            && got_state != exp
        {
            bail!("OAuth state 校验不匹配，请重新发起登录。");
        }
        if let Some((_, code)) = url.query_pairs().find(|(k, _)| k == "code") {
            let code_trimmed = code.trim();
            if !code_trimmed.is_empty() {
                return Ok(code_trimmed.to_string());
            }
        }
        bail!("回调地址中未找到有效的 code 参数。");
    }

    // 纯 code 字符串（兼容 4%2F0A...）
    let decoded = url::form_urlencoded::parse(format!("code={trimmed}").as_bytes())
        .find(|(k, _)| k == "code")
        .map(|(_, v)| v.into_owned())
        .unwrap_or_else(|| trimmed.to_string());
    Ok(decoded.trim().to_string())
}

fn pkce_challenge(verifier: &str) -> String {
    use base64::Engine as _;
    use sha2::{Digest, Sha256};
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

fn callback_state_matches(path: &str, expected_state: &str) -> bool {
    let Ok(url) = Url::parse(&format!("http://localhost{path}")) else {
        return false;
    };
    !expected_state.is_empty()
        && url
            .query_pairs()
            .find(|(key, _)| key == "state")
            .is_some_and(|(_, value)| value == expected_state)
}

fn html_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

pub fn cancel_pending_oauth_session() {
    if let Ok(mut guard) = pending_slot().lock()
        && let Some(mut session) = guard.take()
        && let Some(cancel) = session.cancel_tx.take()
    {
        let _ = cancel.send(());
    }
}

/// 启动本地双栈回环 OAuth 监听并生成 Google 授权链接。
pub async fn start_oauth_login(client_key: Option<&str>) -> anyhow::Result<AntigravityOAuthStartResponse> {
    cancel_pending_oauth_session();

    let (normalized_key, client_id, _) = client_credentials_for(client_key)?;

    // 尝试双栈绑定，使 localhost 在 IPv4 与 IPv6 下均可命中
    let v4_listener = TcpListener::bind("127.0.0.1:0")
        .await
        .context("绑定本地 OAuth 回环端口失败")?;
    let port = v4_listener.local_addr()?.port();
    let v6_listener = TcpListener::bind(format!("[::1]:{port}")).await.ok();

    let redirect_uri = if v6_listener.is_some() {
        format!("http://localhost:{port}{ANTIGRAVITY_OAUTH_CALLBACK_PATH}")
    } else {
        format!("http://127.0.0.1:{port}{ANTIGRAVITY_OAUTH_CALLBACK_PATH}")
    };

    let state = Uuid::new_v4().to_string();
    let login_id = Uuid::new_v4().to_string();
    // PKCE（RFC 7636，S256）：即使授权码在回调环节被截获，也无法脱离本进程换取令牌。
    let code_verifier = format!(
        "{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    );
    let code_challenge = pkce_challenge(&code_verifier);

    let mut auth_url = Url::parse(GOOGLE_AUTH_URL)?;
    // 参数集合与官方 golang.org/x/oauth2 AuthCodeURL（含 PKCE）一致：
    // access_type / client_id / code_challenge / code_challenge_method / prompt / redirect_uri /
    // response_type / scope / state，不多不少。
    auth_url
        .query_pairs_mut()
        .append_pair("access_type", "offline")
        .append_pair("client_id", &client_id)
        .append_pair("code_challenge", &code_challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("prompt", "consent")
        .append_pair("redirect_uri", &redirect_uri)
        .append_pair("response_type", "code")
        .append_pair("scope", &oauth_scopes_for_client(normalized_key).join(" "))
        .append_pair("state", &state);

    let (code_tx, code_rx) = oneshot::channel::<Result<String, String>>();
    let (cancel_tx, mut cancel_rx) = oneshot::channel::<()>();
    let expected_state = state.clone();

    tokio::spawn(async move {
        let accept_once = async {
            loop {
                let accepted = if let Some(ref v6) = v6_listener {
                    tokio::select! {
                        r = v4_listener.accept() => r.ok().map(|(s, _)| s),
                        r = v6.accept() => r.ok().map(|(s, _)| s),
                    }
                } else {
                    v4_listener.accept().await.ok().map(|(s, _)| s)
                };
                let Some(mut stream) = accepted else {
                    break Err("回环监听器已关闭".to_string());
                };
                let mut buf = vec![0u8; 8192];
                let n = stream.read(&mut buf).await.unwrap_or(0);
                if n == 0 {
                    continue;
                }
                let req_str = String::from_utf8_lossy(&buf[..n]);
                let first_line = req_str.lines().next().unwrap_or("");
                let path = first_line.split_whitespace().nth(1).unwrap_or("/");
                if !is_oauth_callback_path(path) {
                    let resp = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                    let _ = stream.write_all(resp.as_bytes()).await;
                    continue;
                }
                // 回环回调必须携带与发起时一致的 state；缺失即拒绝（防登录 CSRF）。
                // 校验失败不终止监听，避免本机其他网页抢先打一枪就把正常登录搅黄。
                if !callback_state_matches(path, &expected_state) {
                    let html = "<!doctype html><html><head><meta charset='utf-8'><title>授权失败</title></head><body><h3>Antigravity 授权失败</h3><p>OAuth state 缺失或不匹配，请从 Swivox 重新发起登录。</p></body></html>";
                    let resp = format!(
                        "HTTP/1.1 400 Bad Request\r\nContent-Type: text/html; charset=utf-8\r\nContent-Security-Policy: default-src 'none'\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        html.len(),
                        html
                    );
                    let _ = stream.write_all(resp.as_bytes()).await;
                    continue;
                }
                match parse_manual_oauth_input(path, Some(&expected_state)) {
                    Ok(code) => {
                        let html = "<!doctype html><html><head><meta charset='utf-8'><title>Swivox - Antigravity 授权成功</title><style>body{background:#0b0f19;color:#f3f4f6;font-family:-apple-system,BlinkMacSystemFont,sans-serif;display:flex;align-items:center;justify-content:center;height:100vh;margin:0}.card{background:#131a2a;border:1px solid #26324d;border-radius:14px;padding:32px 40px;text-align:center;box-shadow:0 16px 40px rgba(0,0,0,.4)}h2{margin:0 0 8px;color:#38bdf8}p{margin:0;color:#9ca3af;font-size:14px}</style></head><body><div class='card'><h2>Antigravity 登录成功</h2><p>您可以关闭此浏览器标签页并返回 Swivox。</p></div></body></html>";
                        let resp = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            html.len(),
                            html
                        );
                        let _ = stream.write_all(resp.as_bytes()).await;
                        let _ = stream.flush().await;
                        break Ok(code);
                    }
                    Err(err) => {
                        let html = format!(
                            "<!doctype html><html><head><meta charset='utf-8'><title>授权失败</title></head><body><h3>Antigravity 授权失败</h3><p>{}</p></body></html>",
                            html_escape(&err.to_string())
                        );
                        let resp = format!(
                            "HTTP/1.1 400 Bad Request\r\nContent-Type: text/html; charset=utf-8\r\nContent-Security-Policy: default-src 'none'\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            html.len(),
                            html
                        );
                        let _ = stream.write_all(resp.as_bytes()).await;
                        break Err(err.to_string());
                    }
                }
            }
        };

        let outcome = tokio::select! {
            res = accept_once => res,
            _ = &mut cancel_rx => return,
            _ = tokio::time::sleep(Duration::from_secs(300)) => Err("等待浏览器授权超时（5 分钟）。".to_string()),
        };
        let _ = code_tx.send(outcome);
    });

    if let Ok(mut guard) = pending_slot().lock() {
        *guard = Some(PendingOAuthSession {
            login_id: login_id.clone(),
            state,
            redirect_uri: redirect_uri.clone(),
            client_key: normalized_key.to_string(),
            code_verifier,
            code_rx: Some(code_rx),
            cancel_tx: Some(cancel_tx),
        });
    }

    Ok(AntigravityOAuthStartResponse {
        login_id,
        verification_uri: auth_url.to_string(),
        callback_url: redirect_uri,
        client_key: normalized_key.to_string(),
    })
}

pub async fn wait_for_oauth_callback(
    login_id: &str,
    proxy_spec: Option<&str>,
) -> anyhow::Result<(ExchangedGoogleTokens, GoogleUserInfo)> {
    let (rx, redirect_uri, client_key, code_verifier) = {
        let mut guard = pending_slot()
            .lock()
            .map_err(|_| anyhow::anyhow!("OAuth 会话锁异常"))?;
        let Some(session) = guard.as_mut() else {
            bail!("当前没有进行中的 Antigravity OAuth 登录会话。");
        };
        if session.login_id != login_id {
            bail!("OAuth 登录会话已过期或被替换。");
        }
        let Some(rx) = session.code_rx.take() else {
            bail!("正在等待该 OAuth 会话完成，请勿重复提交。");
        };
        (
            rx,
            session.redirect_uri.clone(),
            session.client_key.clone(),
            session.code_verifier.clone(),
        )
    };

    let code = rx
        .await
        .map_err(|_| anyhow::anyhow!("OAuth 登录已取消。"))?
        .map_err(|e| anyhow::anyhow!(e))?;

    cancel_pending_oauth_session();
    let tokens = exchange_authorization_code_with_verifier(
        &code,
        &redirect_uri,
        Some(&client_key),
        proxy_spec,
        Some(&code_verifier),
    )
    .await?;
    let user = fetch_google_userinfo(&tokens.access_token, proxy_spec).await?;
    Ok((tokens, user))
}

pub async fn complete_manual_oauth_callback(
    raw_input: &str,
    proxy_spec: Option<&str>,
) -> anyhow::Result<(ExchangedGoogleTokens, GoogleUserInfo)> {
    let (expected_state, redirect_uri, client_key, code_verifier) = {
        let guard = pending_slot()
            .lock()
            .map_err(|_| anyhow::anyhow!("OAuth 会话锁异常"))?;
        let Some(session) = guard.as_ref() else {
            bail!("请先点击「OAuth 浏览器登录」生成授权会话后再粘贴回调地址。");
        };
        (
            session.state.clone(),
            session.redirect_uri.clone(),
            session.client_key.clone(),
            session.code_verifier.clone(),
        )
    };

    let code = parse_manual_oauth_input(raw_input, Some(&expected_state))?;
    cancel_pending_oauth_session();
    let tokens = exchange_authorization_code_with_verifier(
        &code,
        &redirect_uri,
        Some(&client_key),
        proxy_spec,
        Some(&code_verifier),
    )
    .await?;
    let user = fetch_google_userinfo(&tokens.access_token, proxy_spec).await?;
    Ok((tokens, user))
}

#[derive(Debug, Deserialize)]
struct TokenEndpointResponse {
    access_token: String,
    #[serde(default)]
    expires_in: Option<i64>,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    id_token: Option<String>,
}

pub async fn exchange_authorization_code(
    code: &str,
    redirect_uri: &str,
    client_key: Option<&str>,
    proxy_spec: Option<&str>,
) -> anyhow::Result<ExchangedGoogleTokens> {
    exchange_authorization_code_with_verifier(code, redirect_uri, client_key, proxy_spec, None).await
}

async fn exchange_authorization_code_with_verifier(
    code: &str,
    redirect_uri: &str,
    client_key: Option<&str>,
    proxy_spec: Option<&str>,
    code_verifier: Option<&str>,
) -> anyhow::Result<ExchangedGoogleTokens> {
    let (norm_key, client_id, client_secret) = client_credentials_for(client_key)?;
    let client = build_http_client_for_proxy(proxy_spec, 25)?;
    let mut form = vec![
        ("client_id", client_id.as_str()),
        ("client_secret", client_secret.as_str()),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("grant_type", "authorization_code"),
    ];
    if let Some(verifier) = code_verifier.filter(|value| !value.is_empty()) {
        form.push(("code_verifier", verifier));
    }
    let resp = client
        .post(GOOGLE_TOKEN_URL)
        .header("User-Agent", OAUTH_USER_AGENT)
        .form(&form)
        .send()
        .await
        .context("请求 Google OAuth Token 接口失败")?;

    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        bail!("Google OAuth 换取令牌失败 (HTTP {}): {}", status.as_u16(), text);
    }
    let parsed: TokenEndpointResponse =
        serde_json::from_str(&text).context("解析 Google Token 响应失败")?;
    let refresh_token = parsed
        .refresh_token
        .filter(|s| !s.trim().is_empty())
        .context("Google OAuth 未返回 refresh_token，请重试并确认授予离线访问权限")?;
    let expires_in = parsed.expires_in.unwrap_or(3599).max(60);
    let expiry_timestamp = Utc::now().timestamp() + expires_in;
    Ok(ExchangedGoogleTokens {
        access_token: parsed.access_token,
        refresh_token,
        expires_in,
        expiry_timestamp,
        id_token: parsed.id_token,
        client_key: norm_key.to_string(),
    })
}

/// 使用 `refresh_token` 换取新的 `access_token`（带瞬时 `invalid_grant` 500ms 重试确认与个人/GCP 双客户端回退）。
pub async fn refresh_google_access_token(
    refresh_token: &str,
    preferred_client_key: Option<&str>,
    proxy_spec: Option<&str>,
) -> anyhow::Result<ExchangedGoogleTokens> {
    let (primary_key, primary_id, primary_secret) = client_credentials_for(preferred_client_key)?;
    let (alt_key, alt_id, alt_secret) = alternate_client_credentials(primary_key)?;
    let client = build_http_client_for_proxy(proxy_spec, 25)?;

    let candidates = [
        (primary_key, primary_id.as_str(), primary_secret.as_str()),
        (alt_key, alt_id.as_str(), alt_secret.as_str()),
    ];

    let mut last_err = String::new();
    for (ckey, cid, csecret) in candidates {
        for attempt in 0..2 {
            let resp = client
                .post(GOOGLE_TOKEN_URL)
                .header("User-Agent", OAUTH_USER_AGENT)
                .form(&[
                    ("client_id", cid),
                    ("client_secret", csecret),
                    ("refresh_token", refresh_token.trim()),
                    ("grant_type", "refresh_token"),
                ])
                .send()
                .await
                .context("请求 Google Token 刷新接口失败")?;

            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            if status.is_success() {
                let parsed: TokenEndpointResponse =
                    serde_json::from_str(&body).context("解析刷新后的 Google Token 失败")?;
                let expires_in = parsed.expires_in.unwrap_or(3599).max(60);
                return Ok(ExchangedGoogleTokens {
                    access_token: parsed.access_token,
                    refresh_token: parsed
                        .refresh_token
                        .filter(|s| !s.trim().is_empty())
                        .unwrap_or_else(|| refresh_token.trim().to_string()),
                    expires_in,
                    expiry_timestamp: Utc::now().timestamp() + expires_in,
                    id_token: parsed.id_token,
                    client_key: ckey.to_string(),
                });
            }

            last_err = format!("HTTP {}: {}", status.as_u16(), body);
            let lower = body.to_ascii_lowercase();
            if attempt == 0 && lower.contains("invalid_grant") {
                tokio::time::sleep(Duration::from_millis(500)).await;
                continue;
            }
            // 若是客户端不匹配（unauthorized_client / invalid_client / invalid_grant），尝试备用内置客户端
            break;
        }
    }

    bail!("刷新 Antigravity Token 失败: {last_err}")
}

pub async fn fetch_google_userinfo(
    access_token: &str,
    proxy_spec: Option<&str>,
) -> anyhow::Result<GoogleUserInfo> {
    let client = build_http_client_for_proxy(proxy_spec, 20)?;
    let resp = client
        .get(GOOGLE_USERINFO_URL)
        .bearer_auth(access_token.trim())
        .send()
        .await
        .context("请求 Google UserInfo 失败")?;
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        bail!("获取 Google 账号资料失败 (HTTP {}): {}", status.as_u16(), body);
    }
    let info: GoogleUserInfo =
        serde_json::from_str(&body).context("解析 Google 账号资料响应失败")?;
    if info.email.trim().is_empty() {
        bail!("Google UserInfo 未返回有效邮箱地址。");
    }
    Ok(info)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_callback_requires_matching_state() {
        assert!(callback_state_matches("/auth/callback?state=s1&code=c", "s1"));
        assert!(!callback_state_matches("/auth/callback?code=attacker", "s1"));
        assert!(!callback_state_matches("/auth/callback?state=other&code=c", "s1"));
    }

    #[test]
    fn scopes_match_official_clients_exactly() {
        let personal = oauth_scopes_for_client("personal");
        assert_eq!(personal.len(), 6);
        assert_eq!(personal.last(), Some(&ANTIGRAVITY_PERSONAL_EXTRA_SCOPE));
        assert!(!personal.contains(&"openid"));
        let gcp = oauth_scopes_for_client("gcp");
        assert_eq!(gcp, ANTIGRAVITY_OAUTH_SCOPES.to_vec());
        assert!(is_oauth_callback_path("/auth/callback?code=x"));
        assert!(is_oauth_callback_path("/oauth-callback?code=x"));
    }

    #[test]
    fn pkce_challenge_matches_rfc7636_example() {
        assert_eq!(
            pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
        assert_eq!(html_escape("<b>&\"'"), "&lt;b&gt;&amp;&quot;&#39;");
    }

    #[test]
    fn parses_manual_callback_url_and_encoded_code() {
        let code = parse_manual_oauth_input(
            "http://localhost:54321/oauth-callback?state=abc-123&code=4%2F0AfJohXl_test&scope=email",
            Some("abc-123"),
        )
        .unwrap();
        assert_eq!(code, "4/0AfJohXl_test");

        let bare = parse_manual_oauth_input("4%2F0AfJohXl_bare", None).unwrap();
        assert_eq!(bare, "4/0AfJohXl_bare");
    }

    fn encode_arm64_adrp(rd: u32, pc: usize, target: usize) -> [u8; 4] {
        let imm = ((target >> 12) as i64) - ((pc >> 12) as i64);
        let immlo = ((imm & 0x3) as u32) << 29;
        let immhi = (((imm >> 2) & 0x7ffff) as u32) << 5;
        (0x9000_0000 | immlo | immhi | (rd & 0x1f)).to_le_bytes()
    }

    fn encode_arm64_add_imm12(rd: u32, rn: u32, target: usize) -> [u8; 4] {
        let imm12 = ((target & 0xfff) as u32) << 10;
        (0x9100_0000 | imm12 | ((rn & 0x1f) << 5) | (rd & 0x1f)).to_le_bytes()
    }

    fn encode_x64_lea_rip(pc: usize, target: usize, high_reg: bool) -> [u8; 7] {
        let rex = if high_reg { 0x4c } else { 0x48 };
        let disp = ((target as i64) - ((pc + 7) as i64)) as i32;
        let d = disp.to_le_bytes();
        [rex, 0x8d, 0x05, d[0], d[1], d[2], d[3]]
    }

    #[test]
    fn extracts_sign_in_clients_from_asar_snippet() {
        let cid_personal = "100000000001-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.apps.googleusercontent.com";
        let cid_gcp = "200000000002-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb.apps.googleusercontent.com";
        let js = format!(
            "const OTHER = '999999999999-zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz.apps.googleusercontent.com';\n\
             const SIGN_IN_CLIENTS = [{{ clientId: '{cid_personal}' }}, {{ clientId: '{cid_gcp}' }}];"
        );
        let (got_personal, got_gcp) = extract_sign_in_client_ids_from_asar(js.as_bytes()).unwrap();
        assert_eq!(got_personal, cid_personal);
        assert_eq!(got_gcp, cid_gcp);
    }

    #[test]
    fn pairs_client_ids_and_secrets_via_arm64_adrp_add_in_packed_go_rodata() {
        let cid1 = "100000000001-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.apps.googleusercontent.com";
        let cid2 = "200000000002-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb.apps.googleusercontent.com";
        let prefix = b"TESTPX-";
        let sec1 = "TESTPX-1111111111111111111111111111";
        let sec2 = "TESTPX-2222222222222222222222222222";
        let sec_unused = "TESTPX-9999999999999999999999999999";

        let mut buf = vec![0u8; 0x3000];
        // 模拟 Go .rodata 无 \0 紧邻字符串（包含前序字母与数字噪声，且 secret 顺序故意打乱）
        let mut ro = 0x2000usize;
        buf[ro..ro + 7].copy_from_slice(b"noise99");
        ro += 7;
        let cid1_off = ro;
        buf[ro..ro + cid1.len()].copy_from_slice(cid1.as_bytes());
        ro += cid1.len();
        buf[ro..ro + 6].copy_from_slice(b"tailzz");
        ro += 6;
        let cid2_off = ro;
        buf[ro..ro + cid2.len()].copy_from_slice(cid2.as_bytes());
        ro += cid2.len();
        let sec_unused_off = ro;
        buf[sec_unused_off..sec_unused_off + sec_unused.len()].copy_from_slice(sec_unused.as_bytes());
        ro += sec_unused.len();
        let sec2_off = ro;
        buf[sec2_off..sec2_off + sec2.len()].copy_from_slice(sec2.as_bytes());
        ro += sec2.len();
        let sec1_off = ro;
        buf[sec1_off..sec1_off + sec1.len()].copy_from_slice(sec1.as_bytes());

        // 构造两组相邻 ARM64 ADRP + ADD 指令对：(cid1, sec1) 与 (cid2, sec2)
        let mut pc = 0x100usize;
        buf[pc..pc + 4].copy_from_slice(&encode_arm64_adrp(0, pc, cid1_off));
        buf[pc + 4..pc + 8].copy_from_slice(&encode_arm64_add_imm12(0, 0, cid1_off));
        pc += 16;
        buf[pc..pc + 4].copy_from_slice(&encode_arm64_adrp(1, pc, sec1_off));
        buf[pc + 4..pc + 8].copy_from_slice(&encode_arm64_add_imm12(1, 1, sec1_off));

        pc = 0x400usize;
        buf[pc..pc + 4].copy_from_slice(&encode_arm64_adrp(2, pc, cid2_off));
        buf[pc + 4..pc + 8].copy_from_slice(&encode_arm64_add_imm12(2, 2, cid2_off));
        pc += 12;
        buf[pc..pc + 4].copy_from_slice(&encode_arm64_adrp(3, pc, sec2_off));
        buf[pc + 4..pc + 8].copy_from_slice(&encode_arm64_add_imm12(3, 3, sec2_off));

        let paired = pair_client_ids_and_secrets_in_binary(&buf, prefix, 28);
        assert_eq!(paired.get(cid1).map(String::as_str), Some(sec1));
        assert_eq!(paired.get(cid2).map(String::as_str), Some(sec2));
    }

    #[test]
    fn pairs_client_ids_and_secrets_via_x64_rip_lea() {
        let cid1 = "100000000001-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.apps.googleusercontent.com";
        let cid2 = "200000000002-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb.apps.googleusercontent.com";
        let prefix = b"TESTPX-";
        let sec1 = "TESTPX-aaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let sec2 = "TESTPX-bbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        let sec_extra = "TESTPX-cccccccccccccccccccccccccccc";

        let mut buf = vec![0u8; 0x2000];
        let mut ro = 0x1200usize;
        let sec2_off = ro;
        buf[ro..ro + sec2.len()].copy_from_slice(sec2.as_bytes());
        ro += sec2.len();
        let sec_extra_off = ro;
        buf[sec_extra_off..sec_extra_off + sec_extra.len()].copy_from_slice(sec_extra.as_bytes());
        ro += sec_extra.len();
        let sec1_off = ro;
        buf[ro..ro + sec1.len()].copy_from_slice(sec1.as_bytes());
        ro += sec1.len();
        let cid1_off = ro;
        buf[ro..ro + cid1.len()].copy_from_slice(cid1.as_bytes());
        ro += cid1.len();
        let cid2_off = ro;
        buf[ro..ro + cid2.len()].copy_from_slice(cid2.as_bytes());

        let mut pc = 0x80usize;
        buf[pc..pc + 7].copy_from_slice(&encode_x64_lea_rip(pc, cid1_off, false));
        pc += 10;
        buf[pc..pc + 7].copy_from_slice(&encode_x64_lea_rip(pc, sec1_off, true));

        pc = 0x200usize;
        buf[pc..pc + 7].copy_from_slice(&encode_x64_lea_rip(pc, cid2_off, false));
        pc += 14;
        buf[pc..pc + 7].copy_from_slice(&encode_x64_lea_rip(pc, sec2_off, false));

        let paired = pair_client_ids_and_secrets_in_binary(&buf, prefix, 28);
        assert_eq!(paired.get(cid1).map(String::as_str), Some(sec1));
        assert_eq!(paired.get(cid2).map(String::as_str), Some(sec2));
    }

    #[test]
    fn extracts_oauth_client_pairs_from_local_antigravity_when_present() {
        let Some((asar_path, ls_path)) = locate_antigravity_resources_dirs().into_iter().find_map(|res| {
            let asar = res.join("app.asar");
            let ls = find_language_server_binary(&res)?;
            asar.is_file().then_some((asar, ls))
        }) else {
            // CI 环境未安装 Antigravity 时跳过实机包体校验
            return;
        };

        let asar_bytes = std::fs::read(&asar_path).expect("read local app.asar");
        let (personal_cid, gcp_cid) =
            extract_sign_in_client_ids_from_asar(&asar_bytes).expect("extract SIGN_IN_CLIENTS");
        assert!(personal_cid.ends_with(".apps.googleusercontent.com"));
        assert!(gcp_cid.ends_with(".apps.googleusercontent.com"));
        assert_ne!(personal_cid, gcp_cid);

        let ls_bytes = std::fs::read(&ls_path).expect("read local language_server");
        let sec_prefix = [b'G', b'O', b'C', b'S', b'P', b'X', b'-'];
        let paired = pair_client_ids_and_secrets_in_binary(&ls_bytes, &sec_prefix, 28);
        let personal_sec = paired.get(&personal_cid).expect("personal secret paired");
        let gcp_sec = paired.get(&gcp_cid).expect("gcp secret paired");
        assert_eq!(personal_sec.len(), 35);
        assert_eq!(gcp_sec.len(), 35);
        assert_ne!(personal_sec, gcp_sec);
    }
}
