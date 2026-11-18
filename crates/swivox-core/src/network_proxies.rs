//! 全局代理节点列表、Swivox 本程序自身代理与已支持客户端（Codex / Antigravity Desktop / Antigravity IDE）互相隔离的代理分配引擎。

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::official_accounts::{CodexLaunchProxy, normalize_proxy_url_input};

static PROXY_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn proxy_lock() -> &'static Mutex<()> {
    PROXY_LOCK.get_or_init(|| Mutex::new(()))
}

thread_local! {
    static TEST_PROXIES_FILE_PATH: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

/// 仅供测试在当前线程内隔离 `network-proxies.json` 读写路径。
#[doc(hidden)]
pub fn set_network_proxies_path_for_tests(path: Option<PathBuf>) -> Option<PathBuf> {
    TEST_PROXIES_FILE_PATH.with(|slot| std::mem::replace(&mut *slot.borrow_mut(), path))
}

fn proxies_file_path() -> PathBuf {
    if let Some(p) = TEST_PROXIES_FILE_PATH.with(|slot| slot.borrow().clone()) {
        return p;
    }
    crate::paths::default_network_proxies_path()
}

fn default_system_selection() -> String {
    "system".to_string()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyNodeEntry {
    pub id: String,
    pub name: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// 各客户端独立隔离的代理选择器。
///
/// 取值约定：
/// - `"system"`：跟随操作系统代理
/// - `"direct"`：强制直连（注入 `--no-proxy-server` / `no_proxy()`）
/// - `"app"`：跟随 Swivox 本程序自身代理
/// - `"node:<id>"`：选用 `nodes` 列表中的指定节点（彼此完全隔离）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientProxyAssignments {
    #[serde(default = "default_system_selection")]
    pub codex: String,
    #[serde(default = "default_system_selection")]
    pub codex_ignition: String,
    #[serde(default = "default_system_selection")]
    pub antigravity_desktop: String,
    #[serde(default = "default_system_selection")]
    pub antigravity_ide: String,
    #[serde(default = "default_system_selection")]
    pub antigravity_ignition: String,
}

impl Default for ClientProxyAssignments {
    fn default() -> Self {
        Self {
            codex: default_system_selection(),
            codex_ignition: default_system_selection(),
            antigravity_desktop: default_system_selection(),
            antigravity_ide: default_system_selection(),
            antigravity_ignition: default_system_selection(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkProxyConfig {
    /// 命名代理节点列表（供本程序、各客户端与单账号自由选取）
    #[serde(default)]
    pub nodes: Vec<ProxyNodeEntry>,
    /// Swivox 本程序自身代理（`"system"` | `"direct"` | `"node:<id>"`）
    #[serde(default = "default_system_selection")]
    pub app_self_proxy: String,
    /// 各客户端独立隔离的代理分配
    #[serde(default)]
    pub clients: ClientProxyAssignments,
}

impl Default for NetworkProxyConfig {
    fn default() -> Self {
        Self {
            nodes: Vec::new(),
            app_self_proxy: default_system_selection(),
            clients: ClientProxyAssignments::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkProxySnapshot {
    pub config: NetworkProxyConfig,
    pub effective_app_self: String,
    pub effective_codex: String,
    pub effective_codex_ignition: String,
    pub effective_antigravity_desktop: String,
    pub effective_antigravity_ide: String,
    pub effective_antigravity_ignition: String,
}

fn sanitize_selection(raw: &str, allow_app: bool, nodes: &[ProxyNodeEntry]) -> String {
    let trimmed = raw.trim();
    match trimmed {
        "direct" => "direct".to_string(),
        "app" if allow_app => "app".to_string(),
        _ if trimmed.starts_with("node:") => {
            let id = trimmed.trim_start_matches("node:").trim();
            if nodes.iter().any(|n| n.id == id) {
                format!("node:{id}")
            } else {
                "system".to_string()
            }
        }
        _ => "system".to_string(),
    }
}

impl NetworkProxyConfig {
    pub fn normalize(&mut self) {
        let mut cleaned_nodes: Vec<ProxyNodeEntry> = Vec::with_capacity(self.nodes.len());
        for mut node in self.nodes.drain(..) {
            let Ok(norm_url) = normalize_proxy_url_input(&node.url) else {
                continue;
            };
            node.url = norm_url;
            node.name = node.name.trim().to_string();
            if node.name.is_empty() {
                node.name = node.url.clone();
            }
            if node.id.trim().is_empty() {
                node.id = format!("px_{}", &Uuid::new_v4().simple().to_string()[..8]);
            }
            node.note = node
                .note
                .take()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            if !cleaned_nodes.iter().any(|existing| existing.id == node.id) {
                cleaned_nodes.push(node);
            }
        }
        self.nodes = cleaned_nodes;
        self.app_self_proxy = sanitize_selection(&self.app_self_proxy, false, &self.nodes);
        self.clients.codex = sanitize_selection(&self.clients.codex, true, &self.nodes);
        self.clients.codex_ignition =
            sanitize_selection(&self.clients.codex_ignition, true, &self.nodes);
        self.clients.antigravity_desktop =
            sanitize_selection(&self.clients.antigravity_desktop, true, &self.nodes);
        self.clients.antigravity_ide =
            sanitize_selection(&self.clients.antigravity_ide, true, &self.nodes);
        self.clients.antigravity_ignition =
            sanitize_selection(&self.clients.antigravity_ignition, true, &self.nodes);
    }

    pub fn find_node(&self, node_id: &str) -> Option<&ProxyNodeEntry> {
        let target = node_id.trim().trim_start_matches("node:");
        self.nodes.iter().find(|n| n.id == target)
    }

    pub fn client_selection(&self, client_key: &str) -> &str {
        match client_key.trim().to_ascii_lowercase().as_str() {
            "codex" => &self.clients.codex,
            "codex_ignition" => &self.clients.codex_ignition,
            "antigravity" | "antigravity_desktop" | "desktop" => &self.clients.antigravity_desktop,
            "antigravity_ide" | "ide" => &self.clients.antigravity_ide,
            "antigravity_ignition" => &self.clients.antigravity_ignition,
            _ => &self.clients.codex,
        }
    }
}

fn ensure_migrated_node(
    nodes: &mut Vec<ProxyNodeEntry>,
    name_hint: &str,
    raw_url: &str,
) -> Option<String> {
    let norm = normalize_proxy_url_input(raw_url).ok()?;
    if let Some(existing) = nodes.iter().find(|n| n.url.eq_ignore_ascii_case(&norm)) {
        return Some(format!("node:{}", existing.id));
    }
    let id = format!("px_{}", &Uuid::new_v4().simple().to_string()[..8]);
    let name = if name_hint.trim().is_empty() {
        norm.clone()
    } else {
        name_hint.trim().to_string()
    };
    nodes.push(ProxyNodeEntry {
        id: id.clone(),
        name,
        url: norm,
        note: None,
    });
    Some(format!("node:{id}"))
}

fn migrate_from_legacy_states() -> NetworkProxyConfig {
    let mut cfg = NetworkProxyConfig::default();

    // 1. 迁移 official-accounts.json 中的节点池与旧全局代理（但不读取此处的 network_proxies 以防递归）
    let oa_path = crate::paths::default_official_accounts_path();
    if let Ok(raw) = std::fs::read_to_string(&oa_path)
        && let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw)
    {
        if let Some(arr) = val.get("proxyResources").and_then(serde_json::Value::as_array) {
            for (idx, item) in arr.iter().enumerate() {
                let url = item
                    .get("url")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("");
                let name = item
                    .get("note")
                    .or_else(|| item.get("id"))
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("");
                let label = if name.trim().is_empty() {
                    format!("代理节点 {}", idx + 1)
                } else {
                    name.to_string()
                };
                let _ = ensure_migrated_node(&mut cfg.nodes, &label, url);
            }
        }
        let g_mode = val
            .get("globalProxyMode")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("inherit");
        let g_url = val
            .get("globalProxyUrl")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        match g_mode.trim() {
            "direct" => {
                cfg.clients.codex = "direct".to_string();
                cfg.clients.codex_ignition = "direct".to_string();
            }
            "custom" => {
                if let Some(sel) = ensure_migrated_node(&mut cfg.nodes, "默认本地代理", g_url) {
                    cfg.app_self_proxy = sel.clone();
                    cfg.clients.codex = sel.clone();
                    cfg.clients.codex_ignition = sel;
                }
            }
            _ => {}
        }
    }

    // 2. 迁移 antigravity-accounts.json 中的旧启动代理
    let ag_path = crate::paths::default_antigravity_state_path();
    if let Ok(raw) = std::fs::read_to_string(&ag_path)
        && let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw)
    {
        let ag_mode = val
            .get("launchProxyMode")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("inherit");
        let ag_url = val
            .get("launchProxyUrl")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        match ag_mode.trim() {
            "direct" => {
                cfg.clients.antigravity_desktop = "direct".to_string();
                cfg.clients.antigravity_ide = "direct".to_string();
                cfg.clients.antigravity_ignition = "direct".to_string();
            }
            "custom" => {
                if let Some(sel) =
                    ensure_migrated_node(&mut cfg.nodes, "Antigravity 代理", ag_url)
                {
                    cfg.clients.antigravity_desktop = sel.clone();
                    cfg.clients.antigravity_ide = sel.clone();
                    cfg.clients.antigravity_ignition = sel;
                }
            }
            _ => {}
        }
    }

    cfg.normalize();
    cfg
}

pub fn load_network_proxy_config() -> NetworkProxyConfig {
    let _guard = proxy_lock().lock().unwrap_or_else(|e| e.into_inner());
    let path = proxies_file_path();
    if let Ok(raw) = std::fs::read_to_string(&path)
        && !raw.trim().is_empty()
    {
        match serde_json::from_str::<NetworkProxyConfig>(&raw) {
            Ok(mut parsed) => {
                parsed.normalize();
                return parsed;
            }
            Err(error) => crate::settings::quarantine_corrupt_file(&path, &error.to_string()),
        }
    }
    migrate_from_legacy_states()
}

pub fn save_network_proxy_config(cfg: &mut NetworkProxyConfig) -> anyhow::Result<()> {
    let _guard = proxy_lock().lock().unwrap_or_else(|e| e.into_inner());
    cfg.normalize();
    let path = proxies_file_path();
    let bytes = serde_json::to_vec_pretty(cfg).context("序列化网络代理配置失败")?;
    crate::settings::atomic_write(&path, &bytes)
}

/// 添加或更新一条代理列表节点。
pub fn upsert_proxy_node(
    id: Option<String>,
    name: String,
    raw_url: String,
    note: Option<String>,
) -> anyhow::Result<NetworkProxyConfig> {
    let norm_url = normalize_proxy_url_input(&raw_url)?;
    let trimmed_name = name.trim();
    let final_name = if trimmed_name.is_empty() {
        norm_url.clone()
    } else {
        trimmed_name.to_string()
    };
    let mut cfg = load_network_proxy_config();
    if let Some(target_id) = id.as_deref().map(str::trim).filter(|s| !s.is_empty())
        && let Some(existing) = cfg.nodes.iter_mut().find(|n| n.id == target_id)
    {
        existing.name = final_name;
        existing.url = norm_url;
        existing.note = note.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    } else {
        if cfg
            .nodes
            .iter()
            .any(|n| n.name == final_name && n.url.eq_ignore_ascii_case(&norm_url))
        {
            bail!("同名同地址的代理节点已存在。");
        }
        cfg.nodes.push(ProxyNodeEntry {
            id: format!("px_{}", &Uuid::new_v4().simple().to_string()[..8]),
            name: final_name,
            url: norm_url,
            note: note.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
        });
    }
    save_network_proxy_config(&mut cfg)?;
    Ok(cfg)
}

pub fn delete_proxy_node(node_id: &str) -> anyhow::Result<NetworkProxyConfig> {
    let mut cfg = load_network_proxy_config();
    let before = cfg.nodes.len();
    cfg.nodes.retain(|n| n.id != node_id.trim());
    if cfg.nodes.len() == before {
        bail!("未找到要删除的代理节点。");
    }
    save_network_proxy_config(&mut cfg)?;
    Ok(cfg)
}

pub fn update_proxy_assignments(
    app_self_proxy: String,
    clients: ClientProxyAssignments,
) -> anyhow::Result<NetworkProxyConfig> {
    let mut cfg = load_network_proxy_config();
    cfg.app_self_proxy = app_self_proxy;
    cfg.clients = clients;
    save_network_proxy_config(&mut cfg)?;
    Ok(cfg)
}

fn detect_system_or_env_proxy() -> Option<String> {
    let env_map: std::collections::HashMap<String, String> = std::env::vars().collect();
    if crate::proxy::has_proxy_environment(&env_map) {
        for key in ["HTTPS_PROXY", "https_proxy", "HTTP_PROXY", "http_proxy", "ALL_PROXY", "all_proxy"] {
            if let Some(v) = env_map.get(key).map(|s| s.trim()).filter(|s| !s.is_empty()) {
                return Some(v.to_string());
            }
        }
        None
    } else {
        crate::proxy::detect_system_proxy()
    }
}

/// 解析 Swivox 本程序自身使用的代理：返回 `Some("direct")`、`Some("http://...")` 或 `None`（直连系统无代理）。
pub fn resolve_app_self_proxy_spec_from(cfg: &NetworkProxyConfig) -> Option<String> {
    match cfg.app_self_proxy.trim() {
        "direct" => Some("direct".to_string()),
        sel if sel.starts_with("node:") => cfg
            .find_node(sel)
            .map(|n| n.url.clone())
            .or_else(detect_system_or_env_proxy),
        _ => detect_system_or_env_proxy(),
    }
}

pub fn resolve_app_self_proxy_spec() -> Option<String> {
    let cfg = load_network_proxy_config();
    resolve_app_self_proxy_spec_from(&cfg)
}

/// 解析指定隔离客户端（`"codex"` | `"antigravity_desktop"` | `"antigravity_ide"`）的生效代理。
pub fn resolve_client_proxy_spec_from(cfg: &NetworkProxyConfig, client_key: &str) -> Option<String> {
    match cfg.client_selection(client_key).trim() {
        "direct" => Some("direct".to_string()),
        "app" => resolve_app_self_proxy_spec_from(cfg),
        sel if sel.starts_with("node:") => cfg
            .find_node(sel)
            .map(|n| n.url.clone())
            .or_else(detect_system_or_env_proxy),
        _ => detect_system_or_env_proxy(),
    }
}

pub fn resolve_client_proxy_spec(client_key: &str) -> Option<String> {
    let cfg = load_network_proxy_config();
    resolve_client_proxy_spec_from(&cfg, client_key)
}

/// 将指定客户端的代理选择转换为 Electron 启动策略 `CodexLaunchProxy`。
pub fn resolve_client_launch_proxy(client_key: &str) -> CodexLaunchProxy {
    let cfg = load_network_proxy_config();
    match cfg.client_selection(client_key).trim() {
        "direct" => CodexLaunchProxy::Direct,
        "app" => match resolve_app_self_proxy_spec_from(&cfg) {
            Some(s) if s.eq_ignore_ascii_case("direct") => CodexLaunchProxy::Direct,
            Some(url) => CodexLaunchProxy::Proxy(url),
            None => CodexLaunchProxy::Inherit,
        },
        sel if sel.starts_with("node:") => match cfg.find_node(sel) {
            Some(n) => CodexLaunchProxy::Proxy(n.url.clone()),
            None => CodexLaunchProxy::Inherit,
        },
        _ => CodexLaunchProxy::Inherit,
    }
}

/// 解析账号专属代理（支持 `"direct"`、`"node:<id>"`、`"custom"`，否则继承对应客户端隔离代理）。
pub fn resolve_account_or_client_proxy_spec(
    proxy_mode: &str,
    custom_url: Option<&str>,
    client_key: &str,
) -> Option<String> {
    let cfg = load_network_proxy_config();
    let mode = proxy_mode.trim();
    if mode.eq_ignore_ascii_case("direct") {
        return Some("direct".to_string());
    }
    if mode.starts_with("node:") {
        if let Some(node) = cfg.find_node(mode) {
            return Some(node.url.clone());
        }
    }
    if mode.eq_ignore_ascii_case("custom") {
        if let Some(u) = custom_url.map(str::trim).filter(|s| !s.is_empty()) {
            if u.starts_with("node:") {
                if let Some(node) = cfg.find_node(u) {
                    return Some(node.url.clone());
                }
            } else if let Ok(norm) = normalize_proxy_url_input(u) {
                return Some(norm);
            }
        }
    }
    resolve_client_proxy_spec_from(&cfg, client_key)
}

pub fn describe_selection(cfg: &NetworkProxyConfig, selection: &str) -> String {
    match selection.trim() {
        "direct" => "direct".to_string(),
        "app" => match resolve_app_self_proxy_spec_from(cfg) {
            Some(s) if s == "direct" => "app (direct)".to_string(),
            Some(u) => format!("app ({u})"),
            None => "app (system)".to_string(),
        },
        sel if sel.starts_with("node:") => match cfg.find_node(sel) {
            Some(n) => format!("{} ({})", n.name, n.url),
            None => "system".to_string(),
        },
        _ => match detect_system_or_env_proxy() {
            Some(u) => format!("system ({u})"),
            None => "system (direct)".to_string(),
        },
    }
}

pub fn get_network_proxy_snapshot() -> NetworkProxySnapshot {
    let cfg = load_network_proxy_config();
    NetworkProxySnapshot {
        effective_app_self: describe_selection(&cfg, &cfg.app_self_proxy),
        effective_codex: describe_selection(&cfg, &cfg.clients.codex),
        effective_codex_ignition: describe_selection(&cfg, &cfg.clients.codex_ignition),
        effective_antigravity_desktop: describe_selection(&cfg, &cfg.clients.antigravity_desktop),
        effective_antigravity_ide: describe_selection(&cfg, &cfg.clients.antigravity_ide),
        effective_antigravity_ignition: describe_selection(&cfg, &cfg.clients.antigravity_ignition),
        config: cfg,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn isolates_app_self_and_each_client_proxy_selection() {
        let dir = tempfile::tempdir().unwrap();
        let prev = set_network_proxies_path_for_tests(Some(dir.path().join("network-proxies.json")));

        let cfg = upsert_proxy_node(
            None,
            "日本高速".to_string(),
            "http://127.0.0.1:7891".to_string(),
            None,
        )
        .unwrap();
        let jp_id = cfg.nodes[0].id.clone();

        let cfg = upsert_proxy_node(
            None,
            "美区独享".to_string(),
            "socks5://127.0.0.1:1088".to_string(),
            None,
        )
        .unwrap();
        let us_id = cfg.nodes[1].id.clone();

        update_proxy_assignments(
            format!("node:{jp_id}"),
            ClientProxyAssignments {
                codex: format!("node:{us_id}"),
                codex_ignition: "direct".to_string(),
                antigravity_desktop: "direct".to_string(),
                antigravity_ide: format!("node:{jp_id}"),
                antigravity_ignition: format!("node:{us_id}"),
            },
        )
        .unwrap();

        assert_eq!(
            resolve_app_self_proxy_spec().as_deref(),
            Some("http://127.0.0.1:7891")
        );
        assert_eq!(
            resolve_client_proxy_spec("codex").as_deref(),
            Some("socks5://127.0.0.1:1088")
        );
        assert_eq!(
            resolve_client_proxy_spec("codex_ignition").as_deref(),
            Some("direct")
        );
        assert_eq!(
            resolve_client_proxy_spec("antigravity_desktop").as_deref(),
            Some("direct")
        );
        assert_eq!(
            resolve_client_proxy_spec("antigravity_ide").as_deref(),
            Some("http://127.0.0.1:7891")
        );
        assert_eq!(
            resolve_client_proxy_spec("antigravity_ignition").as_deref(),
            Some("socks5://127.0.0.1:1088")
        );

        set_network_proxies_path_for_tests(prev);
    }
}
