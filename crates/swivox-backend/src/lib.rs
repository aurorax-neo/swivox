pub mod antigravity_commands;
pub mod auto_takeover;
pub mod commands;
pub mod i18n;
mod i18n_en;
pub mod install;
pub mod lamplighter_commands;
pub mod phase2_rules;
pub mod phase3_rules;
pub mod phase4_rules;
pub mod phase5_rules;
pub mod runtime_health;
pub mod ui_prefs;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

static APP_EXITING: AtomicBool = AtomicBool::new(false);
static SHUTDOWN_STARTED: AtomicBool = AtomicBool::new(false);
static SHUTDOWN_COMPLETE: AtomicBool = AtomicBool::new(false);

pub fn app_is_exiting() -> bool {
    APP_EXITING.load(Ordering::SeqCst)
}

pub fn mark_app_exiting() {
    APP_EXITING.store(true, Ordering::SeqCst);
}

pub fn shutdown_started() -> bool {
    SHUTDOWN_STARTED.load(Ordering::SeqCst)
}

pub fn shutdown_complete() -> bool {
    SHUTDOWN_COMPLETE.load(Ordering::SeqCst)
}

/// 执行优雅停机（停止自动接管守护并关闭 launcher 及其子服务），幂等安全。
pub fn run_graceful_shutdown() {
    if !SHUTDOWN_STARTED.swap(true, Ordering::SeqCst) {
        APP_EXITING.store(true, Ordering::SeqCst);
        auto_takeover::stop_supervisor();
        if let Err(error) = swivox_launcher::shutdown() {
            let _ = swivox_core::diagnostic_log::append_diagnostic_log(
                "app.shutdown_failed",
                serde_json::json!({ "message": error.to_string() }),
            );
        }
        SHUTDOWN_COMPLETE.store(true, Ordering::SeqCst);
    }
}

/// CLI 早期拦截结果：若返回 `Handled`，进程应直接按退出码结束，不再拉起 GUI 窗口。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CliInterceptOutcome {
    ContinueToGui,
    Handled(i32),
}

/// 处理与 Tauri 版一致的命令行早期分支（多开 worker、旧兼容壳、开机自启跳过、更新 helper、URL 导入）。
pub fn handle_cli_intercepts() -> CliInterceptOutcome {
    let args: Vec<_> = std::env::args_os().collect();
    if let Some(options_path) = args
        .windows(2)
        .find(|window| window[0] == "--swivox-multi-worker")
        .and_then(|window| window.get(1))
    {
        if let Err(error) = commands::run_codex_multi_worker(std::path::Path::new(options_path)) {
            eprintln!("Codex 多开 worker 失败：{error:#}");
            return CliInterceptOutcome::Handled(1);
        }
        return CliInterceptOutcome::Handled(0);
    }

    #[cfg(target_os = "macos")]
    if launched_from_legacy_manager_compat_bundle() {
        return CliInterceptOutcome::Handled(0);
    }

    if should_skip_autostart(
        std::env::args(),
        swivox_core::watcher::default_watcher_disabled_flag().exists(),
    ) {
        return CliInterceptOutcome::Handled(0);
    }

    #[cfg(target_os = "macos")]
    if args
        .get(1)
        .is_some_and(|arg| arg == "--apply-swivox-update")
    {
        let result = if args.len() == 3 {
            swivox_core::update::macos::run_update_helper(std::path::Path::new(&args[2]))
        } else {
            Err(anyhow::anyhow!(
                "更新 helper 需要且仅接受一个计划文件路径。"
            ))
        };
        if let Err(error) = result {
            eprintln!("更新 helper 失败：{error:#}");
            let _ = swivox_core::diagnostic_log::append_diagnostic_log(
                "update.macos.helper_failed",
                serde_json::json!({ "message": error.to_string() }),
            );
            return CliInterceptOutcome::Handled(1);
        }
        return CliInterceptOutcome::Handled(0);
    }

    for arg in std::env::args() {
        if arg.starts_with("swivox://") {
            match swivox_core::provider_import::save_pending_provider_import_from_url(&arg) {
                Ok(request) => {
                    let _ = swivox_core::diagnostic_log::append_diagnostic_log(
                        "manager.provider_import_url.pending",
                        serde_json::json!({
                            "name": request.name,
                            "baseUrl": request.base_url
                        }),
                    );
                    focus_existing_manager_window();
                }
                Err(error) => {
                    let _ = swivox_core::diagnostic_log::append_diagnostic_log(
                        "manager.provider_import_url.failed",
                        serde_json::json!({
                            "error": error.to_string()
                        }),
                    );
                }
            }
        }
    }

    if std::env::args().any(|arg| arg == "--show-update") {
        unsafe {
            std::env::set_var("SWIVOX_SHOW_UPDATE", "1");
        }
    }

    CliInterceptOutcome::ContinueToGui
}

#[cfg(target_os = "macos")]
fn launched_from_legacy_manager_compat_bundle() -> bool {
    std::env::current_exe()
        .ok()
        .map(|path| path.to_string_lossy().contains("Swivox 管理工具.app"))
        .unwrap_or(false)
}

pub fn should_skip_autostart(args: impl IntoIterator<Item = String>, disabled: bool) -> bool {
    disabled && args.into_iter().any(|arg| arg == "--autostart")
}

pub fn install_panic_logger() {
    std::panic::set_hook(Box::new(|panic_info| {
        let payload = panic_info
            .payload()
            .downcast_ref::<&str>()
            .map(|message| (*message).to_string())
            .or_else(|| panic_info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "非字符串 panic payload".to_string());
        let location = panic_info.location().map(|location| {
            serde_json::json!({
                "file": location.file(),
                "line": location.line(),
                "column": location.column()
            })
        });
        let _ = swivox_core::diagnostic_log::append_diagnostic_log(
            "manager.panic",
            serde_json::json!({
                "payload": payload,
                "location": location
            }),
        );
    }));
}

/// 窗口启动前执行的标准初始化动作（日志、清理已下载安装包、官方账号同步、旧包迁移）。
pub fn run_startup_housekeeping() {
    let _ = swivox_core::diagnostic_log::append_diagnostic_log(
        "manager.start",
        serde_json::json!({
            "version": swivox_core::version::VERSION,
            "frontend": "gpui"
        }),
    );
    let update_download_dir = swivox_core::paths::default_app_state_dir().join("updates");
    let removed_installers =
        swivox_core::update::cleanup_downloaded_installers(&update_download_dir);
    if removed_installers > 0 {
        let _ = swivox_core::diagnostic_log::append_diagnostic_log(
            "manager.update_installers_cleaned",
            serde_json::json!({
                "directory": update_download_dir,
                "count": removed_installers
            }),
        );
    }
    commands::sync_bound_official_account_on_startup();
    #[cfg(target_os = "macos")]
    if let Some(root) = swivox_core::install::default_install_root() {
        match swivox_core::install::macos::migrate_legacy_manager_bundle(&root) {
            Ok(Some(archive)) => {
                let _ = swivox_core::diagnostic_log::append_diagnostic_log(
                    "manager.legacy_manager_migrated",
                    serde_json::json!({ "archive": archive }),
                );
            }
            Ok(None) => {}
            Err(error) => {
                let _ = swivox_core::diagnostic_log::append_diagnostic_log(
                    "manager.legacy_manager_migration_failed",
                    serde_json::json!({ "message": error.to_string() }),
                );
            }
        }
    }
}

pub fn window_activation_request_path() -> PathBuf {
    swivox_core::paths::default_app_state_dir().join(format!(
        "window-activation-{}.request",
        swivox_core::ports::manager_guard_port()
    ))
}

/// 唤醒并聚焦已运行的管理器主窗口。
pub fn focus_existing_manager_window() {
    let path = window_activation_request_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let request = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .to_string();
    let _ = std::fs::write(path, request);

    #[cfg(windows)]
    {
        let current_process_id = std::process::id();
        for process in swivox_core::windows_enumerate_processes() {
            if process.process_id == current_process_id {
                continue;
            }
            if ["swivox.exe", "swivox-manager.exe", "swivox-gpui.exe"]
                .iter()
                .any(|name| process.exe_file.eq_ignore_ascii_case(name))
                && swivox_core::windows_activate_process_window(process.process_id)
            {
                break;
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Ok(executable) = std::env::current_exe()
            && let Ok(app) =
                swivox_core::install::macos::native_application_bundle_from_executable(
                    &executable,
                )
        {
            let _ = std::process::Command::new("/usr/bin/open")
                .arg(app)
                .status();
        }
    }
}

pub fn acquire_single_instance_guard() -> Option<swivox_core::ports::LoopbackPortGuard> {
    match swivox_core::ports::acquire_resilient_loopback_port_guard(
        swivox_core::ports::manager_guard_port(),
    ) {
        Ok(guard) => {
            if let Some(fallback_lock_path) = guard.fallback_path() {
                let _ = swivox_core::diagnostic_log::append_diagnostic_log(
                    "manager.guard_fallback",
                    serde_json::json!({
                        "requested_guard_port": swivox_core::ports::manager_guard_port(),
                        "fallback_lock_path": fallback_lock_path
                    }),
                );
            }
            Some(guard)
        }
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::AddrInUse | std::io::ErrorKind::WouldBlock
            ) =>
        {
            let _ = swivox_core::diagnostic_log::append_diagnostic_log(
                "manager.already_running",
                serde_json::json!({
                    "guard_port": swivox_core::ports::manager_guard_port()
                }),
            );
            focus_existing_manager_window();
            None
        }
        Err(error) => {
            let _ = swivox_core::diagnostic_log::append_diagnostic_log(
                "manager.guard_failed",
                serde_json::json!({
                    "guard_port": swivox_core::ports::manager_guard_port(),
                    "error": error.to_string()
                }),
            );
            match std::net::TcpListener::bind(("127.0.0.1", 0)) {
                Ok(listener) => Some(swivox_core::ports::LoopbackPortGuard::listener(listener)),
                Err(fallback_error) => {
                    let _ = swivox_core::diagnostic_log::append_diagnostic_log(
                        "manager.guard_fallback_failed",
                        serde_json::json!({
                            "error": fallback_error.to_string()
                        }),
                    );
                    None
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::should_skip_autostart;

    #[test]
    fn disabling_login_startup_still_allows_opening_the_gui() {
        for args in [
            vec!["swivox"],
            vec!["swivox", "--background"],
            vec!["swivox", "--helper-only"],
        ] {
            assert!(!should_skip_autostart(
                args.into_iter().map(String::from),
                true
            ));
        }
        assert!(should_skip_autostart(
            ["swivox", "--autostart"].map(String::from),
            true
        ));
        assert!(!should_skip_autostart(
            ["swivox", "--autostart"].map(String::from),
            false
        ));
    }
}
