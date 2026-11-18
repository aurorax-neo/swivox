//! Antigravity 桌面版 (`Antigravity`) 与编辑器版 (`Antigravity IDE`) 发现、进程分类、免 TUN 代理启动、冲突清理与无感热切号。

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};

use super::db::AntigravityClientKind;

pub const DEFAULT_ANTIGRAVITY_CDP_PORT: u16 = 9333;
const CHROMIUM_PROXY_BYPASS: &str = "localhost;127.0.0.1;[::1]";
const ENV_NO_PROXY: &str = "localhost,127.0.0.1,::1,[::1]";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AntigravityInstallation {
    pub kind: String,
    pub found: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_or_exe_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executable_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub running: bool,
    pub managed_cdp_ready: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_cdp_port: Option<u16>,
    #[serde(default)]
    pub proxy_injected: bool,
}

#[derive(Debug, Clone)]
pub struct RunningAgProcess {
    pub pid: u32,
    pub ppid: u32,
    pub is_ide: bool,
    pub is_language_server: bool,
    pub is_helper: bool,
    pub command_line: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AntigravityLaunchOutcome {
    pub launched: bool,
    pub conflict: bool,
    pub target_kind: String,
    pub cdp_port: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effective_proxy: Option<String>,
    pub message: String,
}

fn read_macos_bundle_exec_and_version(app_bundle: &Path) -> Option<(PathBuf, Option<String>)> {
    if !app_bundle.is_dir() {
        return None;
    }
    let plist = app_bundle.join("Contents").join("Info.plist");
    let mut exec_name = None;
    let mut version = None;
    #[cfg(target_os = "macos")]
    {
        if plist.is_file() {
            if let Ok(out) = Command::new("/usr/libexec/PlistBuddy")
                .args(["-c", "Print :CFBundleExecutable", &plist.to_string_lossy()])
                .output()
                && out.status.success()
            {
                let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !s.is_empty() {
                    exec_name = Some(s);
                }
            }
            if let Ok(out) = Command::new("/usr/libexec/PlistBuddy")
                .args([
                    "-c",
                    "Print :CFBundleShortVersionString",
                    &plist.to_string_lossy(),
                ])
                .output()
                && out.status.success()
            {
                let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !s.is_empty() {
                    version = Some(s);
                }
            }
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = plist;

    let macos_dir = app_bundle.join("Contents").join("MacOS");
    let fallback_stem = app_bundle
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Antigravity");
    let candidate = macos_dir.join(exec_name.as_deref().unwrap_or(fallback_stem));
    if candidate.is_file() {
        return Some((candidate, version));
    }
    let electron = macos_dir.join("Electron");
    if electron.is_file() {
        return Some((electron, version));
    }
    None
}

/// 探测本机安装的桌面版 (`Desktop`) 或编辑器版 (`Ide`) 路径。
pub fn discover_installation(
    kind: AntigravityClientKind,
    custom_path: Option<&str>,
    cdp_port: u16,
) -> AntigravityInstallation {
    let target = match kind {
        AntigravityClientKind::Ide => AntigravityClientKind::Ide,
        _ => AntigravityClientKind::Desktop,
    };
    let is_ide = target == AntigravityClientKind::Ide;

    let mut resolved_bundle_or_exe: Option<PathBuf> = None;
    let mut resolved_exe: Option<PathBuf> = None;
    let mut version: Option<String> = None;

    if let Some(custom) = custom_path.map(str::trim).filter(|s| !s.is_empty()) {
        let p = PathBuf::from(custom);
        if p.extension().is_some_and(|e| e == "app") {
            if let Some((exe, ver)) = read_macos_bundle_exec_and_version(&p) {
                resolved_bundle_or_exe = Some(p);
                resolved_exe = Some(exe);
                version = ver;
            }
        } else if p.is_file() {
            resolved_bundle_or_exe = Some(p.clone());
            resolved_exe = Some(p);
        }
    }

    if resolved_exe.is_none() {
        #[cfg(target_os = "macos")]
        {
            let app_name = if is_ide {
                "Antigravity IDE.app"
            } else {
                "Antigravity.app"
            };
            let mut candidates = vec![PathBuf::from("/Applications").join(app_name)];
            if let Some(base) = directories::BaseDirs::new() {
                candidates.push(base.home_dir().join("Applications").join(app_name));
            }
            for bundle in candidates {
                if let Some((exe, ver)) = read_macos_bundle_exec_and_version(&bundle) {
                    resolved_bundle_or_exe = Some(bundle);
                    resolved_exe = Some(exe);
                    version = ver;
                    break;
                }
            }
        }

        #[cfg(windows)]
        {
            let mut candidates = Vec::new();
            if let Ok(local) = std::env::var("LOCALAPPDATA") {
                if is_ide {
                    candidates.push(
                        PathBuf::from(&local)
                            .join("Programs")
                            .join("Antigravity IDE")
                            .join("Antigravity IDE.exe"),
                    );
                    candidates.push(
                        PathBuf::from(&local)
                            .join("Programs")
                            .join("Antigravity IDE")
                            .join("Antigravity.exe"),
                    );
                } else {
                    candidates.push(
                        PathBuf::from(&local)
                            .join("Programs")
                            .join("antigravity")
                            .join("Antigravity.exe"),
                    );
                }
            }
            if let Ok(pf) = std::env::var("ProgramFiles") {
                if is_ide {
                    candidates.push(
                        PathBuf::from(&pf)
                            .join("Antigravity IDE")
                            .join("Antigravity IDE.exe"),
                    );
                } else {
                    candidates.push(PathBuf::from(&pf).join("antigravity").join("Antigravity.exe"));
                }
            }
            for exe in candidates {
                if exe.is_file() {
                    resolved_bundle_or_exe = Some(exe.clone());
                    resolved_exe = Some(exe);
                    break;
                }
            }
        }

        #[cfg(all(unix, not(target_os = "macos")))]
        {
            let candidates = if is_ide {
                vec![
                    PathBuf::from("/usr/bin/antigravity-ide"),
                    PathBuf::from("/opt/Antigravity IDE/antigravity-ide"),
                ]
            } else {
                vec![
                    PathBuf::from("/usr/bin/antigravity"),
                    PathBuf::from("/opt/Antigravity/antigravity"),
                ]
            };
            for exe in candidates {
                if exe.is_file() {
                    resolved_bundle_or_exe = Some(exe.clone());
                    resolved_exe = Some(exe);
                    break;
                }
            }
        }
    }

    let procs = enumerate_antigravity_processes();
    let main_running: Vec<&RunningAgProcess> = procs
        .iter()
        .filter(|p| p.is_ide == is_ide && !p.is_helper && !p.is_language_server)
        .collect();
    let running = !main_running.is_empty();
    let proxy_injected = running
        && main_running.iter().any(|p| {
            p.command_line.contains("--proxy-server=")
                || p.command_line.contains("--no-proxy-server")
                || p.command_line.contains("--remote-debugging-address=")
        });
    let user_data_dir = if is_ide {
        super::db::ide_user_data_dir()
    } else {
        super::db::desktop_user_data_dir()
    };
    let active_cdp_port = if running {
        read_devtools_active_port(&user_data_dir)
            .or_else(|| {
                main_running
                    .iter()
                    .find_map(|p| extract_nonzero_cdp_port(&p.command_line))
                    .filter(|port| cdp_port_responding(*port))
            })
            .or_else(|| cdp_port_responding(cdp_port).then_some(cdp_port))
    } else {
        None
    };
    let managed_cdp_ready = active_cdp_port.is_some();

    AntigravityInstallation {
        kind: target.as_str().to_string(),
        found: resolved_exe.is_some(),
        app_or_exe_path: resolved_bundle_or_exe.map(|p| p.to_string_lossy().to_string()),
        executable_path: resolved_exe.map(|p| p.to_string_lossy().to_string()),
        version,
        running,
        managed_cdp_ready,
        active_cdp_port,
        proxy_injected,
    }
}

/// 读取 Chromium/Electron 自动写入 `<UserDataDir>/DevToolsActivePort` 的随机回环调试端口。
/// Antigravity 2.x 默认以 `--remote-debugging-port=0` 启动并将真实端口写入该文件首行。
pub fn read_devtools_active_port(user_data_dir: &Path) -> Option<u16> {
    let raw = std::fs::read_to_string(user_data_dir.join("DevToolsActivePort")).ok()?;
    let port = raw.lines().next()?.trim().parse::<u16>().ok()?;
    if port == 0 || !cdp_port_responding(port) {
        return None;
    }
    Some(port)
}

fn extract_nonzero_cdp_port(cmdline: &str) -> Option<u16> {
    let marker = "--remote-debugging-port=";
    let idx = cmdline.find(marker)?;
    let digits: String = cmdline[idx + marker.len()..]
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    let port = digits.parse::<u16>().ok()?;
    (port > 0).then_some(port)
}

/// 汇总当前运行中的 Antigravity（桌面版与 IDE）所有可达的本地回环 CDP 端口。
pub fn discover_active_cdp_ports(configured_port: u16) -> Vec<u16> {
    let procs = enumerate_antigravity_processes();
    let desktop_running = procs
        .iter()
        .any(|p| !p.is_ide && !p.is_helper && !p.is_language_server);
    let ide_running = procs
        .iter()
        .any(|p| p.is_ide && !p.is_helper && !p.is_language_server);

    let mut ports = Vec::new();
    let mut push_unique = |p: u16| {
        if p > 0 && !ports.contains(&p) {
            ports.push(p);
        }
    };

    if desktop_running
        && let Some(p) = read_devtools_active_port(&super::db::desktop_user_data_dir())
    {
        push_unique(p);
    }
    if ide_running
        && let Some(p) = read_devtools_active_port(&super::db::ide_user_data_dir())
    {
        push_unique(p);
    }
    for proc in procs.iter().filter(|p| !p.is_helper && !p.is_language_server) {
        if let Some(p) = extract_nonzero_cdp_port(&proc.command_line)
            && cdp_port_responding(p)
        {
            push_unique(p);
        }
    }
    if configured_port > 0 && (desktop_running || ide_running) && cdp_port_responding(configured_port)
    {
        push_unique(configured_port);
    }
    ports
}

pub fn cdp_port_responding(port: u16) -> bool {
    if port == 0 {
        return false;
    }
    std::net::TcpStream::connect_timeout(
        &std::net::SocketAddr::from(([127, 0, 0, 1], port)),
        Duration::from_millis(250),
    )
    .is_ok()
}

/// 枚举本机 Antigravity 相关进程，严格按可执行文件路径分类，绝不误匹配普通终端或编辑器。
pub fn enumerate_antigravity_processes() -> Vec<RunningAgProcess> {
    let self_pid = std::process::id();
    let mut results = Vec::new();

    #[cfg(unix)]
    {
        if let Ok(output) = Command::new("/bin/ps")
            .args(["-axo", "pid=,ppid=,command="])
            .output()
            && output.status.success()
        {
            let text = String::from_utf8_lossy(&output.stdout);
            for line in text.lines() {
                let Some((pid_p, ppid_p, cmdline)) = parse_unix_ps_line(line) else {
                    continue;
                };
                if pid_p == self_pid {
                    continue;
                }
                if let Some(classified) = classify_command_line(pid_p, ppid_p, cmdline) {
                    results.push(classified);
                }
            }
        }
    }

    #[cfg(windows)]
    {
        for proc in crate::windows_integration::enumerate_processes() {
            if proc.process_id == self_pid {
                continue;
            }
            let lower_exe = proc.exe_file.to_ascii_lowercase();
            let probe_path = proc
                .executable_path
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|| proc.exe_file.clone());
            let lower_probe = probe_path.to_ascii_lowercase();
            if lower_exe.contains("swivox")
                || lower_exe.contains("antigravity-manager")
                || lower_exe.contains("antigravity-tools")
                || lower_exe.contains("easyag")
                || lower_probe.contains("swivox")
                || lower_probe.contains("antigravity-manager")
                || lower_probe.contains("antigravity-tools")
                || lower_probe.contains("easyag")
            {
                continue;
            }
            let is_ls = lower_exe.contains("language_server") || lower_exe == "agy.exe";
            if !lower_exe.contains("antigravity") && !lower_probe.contains("antigravity") && !is_ls {
                continue;
            }
            let is_ide = lower_probe.contains("antigravity ide")
                || lower_probe.contains("antigravity-ide")
                || lower_probe.contains("antigravity_ide");
            results.push(RunningAgProcess {
                pid: proc.process_id,
                ppid: proc.parent_process_id,
                is_ide,
                is_language_server: is_ls,
                is_helper: is_ls || lower_exe.contains("crashpad"),
                command_line: probe_path,
            });
        }
    }

    results
}

#[cfg(any(unix, test))]
fn parse_unix_ps_line(line: &str) -> Option<(u32, u32, &str)> {
    let trimmed = line.trim();
    let (pid_str, after_pid) = trimmed.split_once(char::is_whitespace)?;
    let pid = pid_str.parse::<u32>().ok()?;
    let after_pid = after_pid.trim_start();
    let (ppid_str, cmdline_raw) = after_pid.split_once(char::is_whitespace)?;
    let ppid = ppid_str.parse::<u32>().ok()?;
    let cmdline = cmdline_raw.trim();
    if cmdline.is_empty() {
        return None;
    }
    Some((pid, ppid, cmdline))
}

fn classify_command_line(pid: u32, ppid: u32, cmdline: &str) -> Option<RunningAgProcess> {
    let lower = cmdline.to_ascii_lowercase();
    // 提取可执行路径部分（处理 macOS .app/Contents/... 中带空格路径）
    let exe_probe = if let Some(idx) = lower.find(" --") {
        &lower[..idx]
    } else {
        lower.as_str()
    };

    if exe_probe.contains("swivox")
        || exe_probe.contains("antigravity-manager")
        || exe_probe.contains("antigravity-tools")
        || exe_probe.contains("easyag")
        || exe_probe.contains(".gemini/antigravity")
    {
        return None;
    }

    let is_language_server = exe_probe.contains("language_server")
        || exe_probe.ends_with("/agy")
        || lower.contains("language_server_");

    let is_app_binary = exe_probe.contains("/antigravity.app/contents/")
        || exe_probe.contains("/antigravity ide.app/contents/")
        || exe_probe.ends_with("/antigravity")
        || exe_probe.ends_with("/antigravity-ide");

    if !is_app_binary && !is_language_server {
        return None;
    }

    let is_ide = exe_probe.contains("antigravity ide")
        || exe_probe.contains("antigravity-ide")
        || exe_probe.contains("antigravity_ide")
        || lower.contains("--subclient_type=ide");

    let is_helper = is_language_server
        || lower.contains("--type=")
        || lower.contains("antigravity helper")
        || lower.contains("crashpad")
        || lower.contains("node-ipc")
        || lower.contains("--standalone");

    Some(RunningAgProcess {
        pid,
        ppid,
        is_ide,
        is_language_server,
        is_helper,
        command_line: cmdline.to_string(),
    })
}

/// 解析最终启动目标：优先显式设置，否则选运行中或已安装的客户端。
pub fn resolve_effective_target(
    preferred: AntigravityClientKind,
    custom_desktop_path: Option<&str>,
    custom_ide_path: Option<&str>,
    cdp_port: u16,
) -> (AntigravityClientKind, AntigravityInstallation) {
    match preferred {
        AntigravityClientKind::Desktop => (
            AntigravityClientKind::Desktop,
            discover_installation(AntigravityClientKind::Desktop, custom_desktop_path, cdp_port),
        ),
        AntigravityClientKind::Ide => (
            AntigravityClientKind::Ide,
            discover_installation(AntigravityClientKind::Ide, custom_ide_path, cdp_port),
        ),
        AntigravityClientKind::Auto => {
            let desktop =
                discover_installation(AntigravityClientKind::Desktop, custom_desktop_path, cdp_port);
            let ide = discover_installation(AntigravityClientKind::Ide, custom_ide_path, cdp_port);
            if desktop.running {
                (AntigravityClientKind::Desktop, desktop)
            } else if ide.running {
                (AntigravityClientKind::Ide, ide)
            } else if desktop.found {
                (AntigravityClientKind::Desktop, desktop)
            } else if ide.found {
                (AntigravityClientKind::Ide, ide)
            } else {
                (AntigravityClientKind::Desktop, desktop)
            }
        }
    }
}

/// 优雅停止指定类型的 Antigravity 主进程（超时后回退强杀并清理孤儿 `language_server`）。
pub fn stop_antigravity_target(is_ide: bool) -> anyhow::Result<usize> {
    let procs = enumerate_antigravity_processes();
    let main_pids: Vec<u32> = procs
        .iter()
        .filter(|p| p.is_ide == is_ide && !p.is_helper && !p.is_language_server)
        .map(|p| p.pid)
        .collect();

    #[cfg(unix)]
    {
        for pid in &main_pids {
            let _ = Command::new("/bin/kill")
                .args(["-TERM", &pid.to_string()])
                .status();
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            let remaining = enumerate_antigravity_processes()
                .into_iter()
                .any(|p| p.is_ide == is_ide && !p.is_helper && !p.is_language_server);
            if !remaining {
                break;
            }
            std::thread::sleep(Duration::from_millis(160));
        }
        // 清理仍未退出的主进程与孤儿 language_server
        for p in enumerate_antigravity_processes() {
            if p.is_ide == is_ide && (!p.is_helper || p.is_language_server) {
                let _ = Command::new("/bin/kill")
                    .args(["-KILL", &p.pid.to_string()])
                    .status();
            }
        }
    }

    #[cfg(windows)]
    {
        for pid in &main_pids {
            let mut cmd = Command::new("taskkill");
            cmd.args(["/F", "/T", "/PID", &pid.to_string()]);
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt as _;
                cmd.creation_flags(0x0800_0000);
            }
            let _ = cmd.status();
        }
    }

    Ok(main_pids.len())
}

/// 桌面版无感热切号：仅重启主进程后代的 `language_server` / `agy` 子进程，保留主窗口与对话上下文。
/// 返回 `true` 表示热切成功；返回 `false` 表示未找到语言服务子进程或重生超时，调用方可降级为完整重启。
pub fn hot_reload_desktop_language_server(reapply_credentials: impl Fn()) -> bool {
    let procs = enumerate_antigravity_processes();
    let desktop_running = procs
        .iter()
        .any(|p| !p.is_ide && !p.is_helper && !p.is_language_server);
    if !desktop_running {
        return false;
    }

    let ls_pids: Vec<u32> = procs
        .iter()
        .filter(|p| !p.is_ide && p.is_language_server)
        .map(|p| p.pid)
        .collect();
    if ls_pids.is_empty() {
        return false;
    }

    #[cfg(unix)]
    for pid in &ls_pids {
        let _ = Command::new("/bin/kill")
            .args(["-KILL", &pid.to_string()])
            .status();
    }

    #[cfg(windows)]
    for pid in &ls_pids {
        let mut cmd = Command::new("taskkill");
        cmd.args(["/F", "/PID", &pid.to_string()]);
        use std::os::windows::process::CommandExt as _;
        cmd.creation_flags(0x0800_0000);
        let _ = cmd.status();
    }

    // 立即补写一次凭据，消除旧 language_server 退出瞬间回写旧 Token 的竞态
    reapply_credentials();

    let deadline = Instant::now() + Duration::from_secs(12);
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(450));
        let respawned = enumerate_antigravity_processes()
            .into_iter()
            .any(|p| !p.is_ide && p.is_language_server && !ls_pids.contains(&p.pid));
        if respawned {
            return true;
        }
    }
    false
}

/// 按客户端类型（桌面版 `antigravity_desktop` 与编辑器 `antigravity_ide` 完全隔离）解析免 TUN 启动代理。
pub fn resolve_launch_proxy_for_target(
    kind: AntigravityClientKind,
    proxy_mode: &str,
    custom_proxy_url: &str,
) -> Option<String> {
    let trimmed = proxy_mode.trim();
    match trimmed {
        "direct" => Some("direct".to_string()),
        m if m.starts_with("node:") => {
            let cfg = crate::network_proxies::load_network_proxy_config();
            cfg.find_node(m)
                .map(|n| n.url.clone())
                .or_else(|| crate::network_proxies::resolve_client_proxy_spec(if kind == AntigravityClientKind::Ide {
                    "antigravity_ide"
                } else {
                    "antigravity_desktop"
                }))
        }
        "custom" if !custom_proxy_url.trim().is_empty() => {
            Some(custom_proxy_url.trim().to_string())
        }
        _ => {
            let client_key = if kind == AntigravityClientKind::Ide {
                "antigravity_ide"
            } else {
                "antigravity_desktop"
            };
            crate::network_proxies::resolve_client_proxy_spec(client_key)
        }
    }
}

pub fn resolve_launch_proxy(proxy_mode: &str, custom_proxy_url: &str) -> Option<String> {
    resolve_launch_proxy_for_target(AntigravityClientKind::Desktop, proxy_mode, custom_proxy_url)
}

/// 免 TUN 代理启动 Antigravity（同时注入 Chromium `--proxy-server`、8 项代理环境变量与本地回环 CDP 端口）。
pub fn launch_antigravity_instance(
    preferred_kind: AntigravityClientKind,
    custom_desktop_path: Option<&str>,
    custom_ide_path: Option<&str>,
    proxy_mode: &str,
    custom_proxy_url: &str,
    requested_cdp_port: u16,
    force_clean_conflict: bool,
) -> anyhow::Result<AntigravityLaunchOutcome> {
    let cdp_port = if requested_cdp_port == 0 {
        DEFAULT_ANTIGRAVITY_CDP_PORT
    } else {
        requested_cdp_port
    };
    let (target, install) = resolve_effective_target(
        preferred_kind,
        custom_desktop_path,
        custom_ide_path,
        cdp_port,
    );
    let is_ide = target == AntigravityClientKind::Ide;

    let Some(exe_str) = install.executable_path.clone() else {
        bail!("未检测到 Antigravity 安装位置，请在「Antigravity 增强」中指定应用路径。");
    };

    let effective_proxy = resolve_launch_proxy_for_target(target, proxy_mode, custom_proxy_url);

    if install.running && !force_clean_conflict {
        let needs_proxy_restart = effective_proxy.is_some() && !install.proxy_injected;
        if needs_proxy_restart || !install.managed_cdp_ready {
            return Ok(AntigravityLaunchOutcome {
                launched: false,
                conflict: true,
                target_kind: target.as_str().to_string(),
                cdp_port: install.active_cdp_port.unwrap_or(cdp_port),
                effective_proxy,
                message: if needs_proxy_restart {
                    "检测到运行中的 Antigravity 尚未注入免 TUN 代理参数，请点击「重启桌面版」或「清理冲突」使代理生效。".to_string()
                } else {
                    "检测到已有未挂载免 TUN 代理/CDP 的 Antigravity 进程在运行，请点击「清理冲突」以生效。".to_string()
                },
            });
        }
        #[cfg(target_os = "macos")]
        if let Some(ref bundle) = install.app_or_exe_path {
            let _ = Command::new("/usr/bin/open").arg(bundle).status();
        }
        return Ok(AntigravityLaunchOutcome {
            launched: true,
            conflict: false,
            target_kind: target.as_str().to_string(),
            cdp_port: install.active_cdp_port.unwrap_or(cdp_port),
            effective_proxy,
            message: "已聚焦运行中的受管 Antigravity 实例。".to_string(),
        });
    }

    if install.running && force_clean_conflict {
        let _ = stop_antigravity_target(is_ide);
    }

    let selected_port = if crate::ports::can_bind_loopback_port(cdp_port) || cdp_port_responding(cdp_port) {
        cdp_port
    } else {
        crate::ports::find_available_loopback_port()
    };

    let mut args = vec![
        format!("--remote-debugging-port={selected_port}"),
        "--remote-debugging-address=127.0.0.1".to_string(),
    ];

    // 直接启动可执行文件（包括 macOS .app/Contents/MacOS/Antigravity），确保 language_server 完整继承代理环境变量
    let mut cmd = Command::new(&exe_str);
    if let Some(parent) = Path::new(&exe_str).parent() {
        cmd.current_dir(parent);
    }

    match effective_proxy.as_deref() {
        Some("direct") => {
            args.push("--no-proxy-server".to_string());
            for k in [
                "HTTP_PROXY",
                "HTTPS_PROXY",
                "ALL_PROXY",
                "http_proxy",
                "https_proxy",
                "all_proxy",
            ] {
                cmd.env_remove(k);
            }
        }
        Some(proxy_url) if !proxy_url.trim().is_empty() => {
            args.push(format!("--proxy-server={proxy_url}"));
            args.push(format!("--proxy-bypass-list={CHROMIUM_PROXY_BYPASS}"));
            for k in [
                "HTTP_PROXY",
                "HTTPS_PROXY",
                "ALL_PROXY",
                "http_proxy",
                "https_proxy",
                "all_proxy",
            ] {
                cmd.env(k, proxy_url);
            }
            cmd.env("NO_PROXY", ENV_NO_PROXY);
            cmd.env("no_proxy", ENV_NO_PROXY);
        }
        _ => {}
    }

    cmd.args(&args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        cmd.creation_flags(0x0000_0008); // DETACHED_PROCESS
    }

    cmd.spawn()
        .with_context(|| format!("启动 Antigravity 失败: {exe_str}"))?;

    Ok(AntigravityLaunchOutcome {
        launched: true,
        conflict: false,
        target_kind: target.as_str().to_string(),
        cdp_port: selected_port,
        effective_proxy,
        message: "已启动 Antigravity（免 TUN 代理与本地 CDP 增强已就绪）。".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_unix_ps_line_keeps_cmdline_when_ppid_is_prefix_of_pid() {
        let line = "  1234   12 /Applications/Antigravity.app/Contents/MacOS/Electron --flag";
        let (pid, ppid, cmdline) = parse_unix_ps_line(line).unwrap();
        assert_eq!(pid, 1234);
        assert_eq!(ppid, 12);
        assert_eq!(
            cmdline,
            "/Applications/Antigravity.app/Contents/MacOS/Electron --flag"
        );
    }
}
