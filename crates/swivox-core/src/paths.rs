use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

const APP_STATE_DIR: &str = ".swivox";
const SETTINGS_FILE: &str = "settings.json";
const LATEST_STATUS_FILE: &str = "latest-status.json";
const DIAGNOSTIC_LOG_FILE: &str = "swivox.log";
const PENDING_PROVIDER_IMPORT_FILE: &str = "pending-provider-import.json";
const PENDING_SESSION_SHARE_FILE: &str = "pending-session-share.txt";
const PENDING_REMOTE_CONTROL_RECOVERY_FILE: &str = "pending-remote-control-recovery.json";
const SKILLS_STATE_FILE: &str = "skills.json";
const SKILLS_DIR: &str = "skills";
const SKILL_BACKUPS_DIR: &str = "skill-backups";
const PENDING_MANAGER_NAVIGATION_FILE: &str = "pending-manager-navigation.json";
const OFFICIAL_ACCOUNTS_FILE: &str = "official-accounts.json";
const ANTIGRAVITY_STATE_FILE: &str = "antigravity-accounts.json";
const ANTIGRAVITY_DEVICE_BASELINE_FILE: &str = "antigravity-device-original.json";
const LAMPLIGHTER_DIR: &str = "lamplighter";
const USER_SCRIPTS_DIR: &str = "user_scripts";
const USER_SCRIPTS_CONFIG_FILE: &str = "user_scripts.json";

/// 显式指定状态目录的环境变量（Windows 多开隔离的必需项）。
pub const STATE_DIR_ENV: &str = "SWIVOX_STATE_DIR";

/// 解析显式状态目录覆盖；未设置或设置为空时返回 `None`。
pub fn state_dir_override_from(value: Option<&std::ffi::OsStr>) -> Option<PathBuf> {
    let value = value?;
    let text = value.to_string_lossy();
    let trimmed = text.trim();
    (!trimmed.is_empty()).then(|| PathBuf::from(trimmed))
}

pub fn state_dir_override() -> Option<PathBuf> {
    state_dir_override_from(std::env::var_os(STATE_DIR_ENV).as_deref())
}

/// 返回 Swivox 独立状态根目录（默认 `~/.swivox`，不读取或迁移上游 `codex++` 旧状态目录）。
pub fn default_app_state_dir() -> PathBuf {
    if let Some(state_dir) = state_dir_override() {
        return state_dir;
    }

    if let Some(home_dir) = directories::BaseDirs::new().map(|dirs| dirs.home_dir().to_path_buf()) {
        return home_dir.join(APP_STATE_DIR);
    }

    PathBuf::from(APP_STATE_DIR)
}

pub fn default_settings_path() -> PathBuf {
    if let Some(path) = settings_path_for_tests() {
        return path;
    }
    default_app_state_dir().join(SETTINGS_FILE)
}

pub fn default_latest_status_path() -> PathBuf {
    default_app_state_dir().join(LATEST_STATUS_FILE)
}

pub fn default_diagnostic_log_path() -> PathBuf {
    default_app_state_dir().join(DIAGNOSTIC_LOG_FILE)
}

pub fn default_pending_provider_import_path() -> PathBuf {
    default_app_state_dir().join(PENDING_PROVIDER_IMPORT_FILE)
}

pub fn default_pending_session_share_path() -> PathBuf {
    default_app_state_dir().join(PENDING_SESSION_SHARE_FILE)
}

pub fn default_pending_remote_control_recovery_path() -> PathBuf {
    default_app_state_dir().join(PENDING_REMOTE_CONTROL_RECOVERY_FILE)
}

/// Skills 的「单一事实来源」目录。已安装的 skill 目录都放这里，
/// 启用时再软链到 `$CODEX_HOME/skills/<id>`，停用只删链接、源目录留着。
pub fn default_skills_source_dir() -> PathBuf {
    default_app_state_dir().join(SKILLS_DIR)
}

pub fn default_skills_state_path() -> PathBuf {
    default_app_state_dir().join(SKILLS_STATE_FILE)
}

/// 卸载 skill 时把源目录整体移到这里，方便反悔。不自动轮转删除。
pub fn default_skill_backups_dir() -> PathBuf {
    default_app_state_dir().join(SKILL_BACKUPS_DIR)
}

pub fn default_pending_manager_navigation_path() -> PathBuf {
    default_app_state_dir().join(PENDING_MANAGER_NAVIGATION_FILE)
}

pub fn default_official_accounts_path() -> PathBuf {
    default_app_state_dir().join(OFFICIAL_ACCOUNTS_FILE)
}

/// 用户自定义拓展脚本目录（统一收拢在 `~/.swivox/user_scripts`）。
pub fn default_user_scripts_dir() -> PathBuf {
    default_app_state_dir().join(USER_SCRIPTS_DIR)
}

/// 用户拓展脚本开关配置文件路径（统一收拢在 `~/.swivox/user_scripts.json`）。
pub fn default_user_scripts_config_path() -> PathBuf {
    default_app_state_dir().join(USER_SCRIPTS_CONFIG_FILE)
}

/// Antigravity 账号、配额、增强配置与设备指纹持久化路径（`~/.swivox/antigravity-accounts.json`）。
pub fn default_antigravity_state_path() -> PathBuf {
    default_app_state_dir().join(ANTIGRAVITY_STATE_FILE)
}

/// Antigravity 本机原始设备指纹基线备份路径（`~/.swivox/antigravity-device-original.json`）。
pub fn default_antigravity_device_baseline_path() -> PathBuf {
    default_app_state_dir().join(ANTIGRAVITY_DEVICE_BASELINE_FILE)
}

/// Lamplighter（点灯人）状态与额度历史根目录（`~/.swivox/lamplighter`）。
pub fn default_lamplighter_dir() -> PathBuf {
    default_app_state_dir().join(LAMPLIGHTER_DIR)
}

pub fn default_lamplighter_config_path() -> PathBuf {
    default_lamplighter_dir().join("config.json")
}

pub fn default_lamplighter_state_path() -> PathBuf {
    default_lamplighter_dir().join("state.json")
}

pub fn default_lamplighter_accounts_path() -> PathBuf {
    default_lamplighter_dir().join("accounts.json")
}

pub fn default_lamplighter_history_dir() -> PathBuf {
    default_lamplighter_dir().join("history")
}

fn settings_path_for_tests() -> Option<PathBuf> {
    SETTINGS_PATH_FOR_TESTS
        .get_or_init(|| Mutex::new(None))
        .lock()
        .ok()
        .and_then(|path| path.clone())
}

static SETTINGS_PATH_FOR_TESTS: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();

#[cfg(test)]
static SETTINGS_PATH_TEST_GUARD: OnceLock<Mutex<()>> = OnceLock::new();

#[cfg(test)]
pub(crate) fn settings_path_test_guard() -> std::sync::MutexGuard<'static, ()> {
    SETTINGS_PATH_TEST_GUARD
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

pub fn set_settings_path_for_tests(path: Option<PathBuf>) -> Option<PathBuf> {
    SETTINGS_PATH_FOR_TESTS
        .get_or_init(|| Mutex::new(None))
        .lock()
        .ok()
        .and_then(|mut current| std::mem::replace(&mut *current, path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_settings_path_uses_app_state_directory() {
        let _guard = settings_path_test_guard();
        let path = default_settings_path();

        assert!(path.ends_with(".swivox/settings.json"));
    }

    #[test]
    fn default_latest_status_path_uses_app_state_directory() {
        let _guard = settings_path_test_guard();
        let path = default_latest_status_path();

        assert!(path.ends_with(".swivox/latest-status.json"));
    }

    #[test]
    fn default_diagnostic_log_path_uses_app_state_directory() {
        let _guard = settings_path_test_guard();
        let path = default_diagnostic_log_path();

        assert!(path.ends_with(".swivox/swivox.log"));
    }

    #[test]
    fn default_pending_provider_import_path_uses_app_state_directory() {
        let _guard = settings_path_test_guard();
        let path = default_pending_provider_import_path();

        assert!(path.ends_with(".swivox/pending-provider-import.json"));
    }

    #[test]
    fn default_pending_session_share_path_uses_app_state_directory() {
        let _guard = settings_path_test_guard();
        let path = default_pending_session_share_path();

        assert!(path.ends_with(".swivox/pending-session-share.txt"));
    }

    #[test]
    fn default_pending_remote_control_recovery_path_uses_app_state_directory() {
        let _guard = settings_path_test_guard();
        let path = default_pending_remote_control_recovery_path();

        assert!(path.ends_with(".swivox/pending-remote-control-recovery.json"));
    }

    #[test]
    fn default_pending_manager_navigation_path_uses_app_state_directory() {
        let _guard = settings_path_test_guard();
        let path = default_pending_manager_navigation_path();

        assert!(path.ends_with(".swivox/pending-manager-navigation.json"));
    }

    #[test]
    fn state_dir_override_accepts_non_empty_paths_only() {
        assert_eq!(
            state_dir_override_from(Some(std::ffi::OsStr::new("/tmp/instance-state"))),
            Some(PathBuf::from("/tmp/instance-state"))
        );
        assert_eq!(
            state_dir_override_from(Some(std::ffi::OsStr::new("  /tmp/padded  "))),
            Some(PathBuf::from("/tmp/padded"))
        );
        assert_eq!(
            state_dir_override_from(Some(std::ffi::OsStr::new(""))),
            None
        );
        assert_eq!(
            state_dir_override_from(Some(std::ffi::OsStr::new("   "))),
            None
        );
        assert_eq!(state_dir_override_from(None), None);
    }

    #[test]
    fn default_app_state_dir_honours_explicit_override() {
        let _guard = settings_path_test_guard();
        let scope = StateDirEnvScope::set("/tmp/swivox-instance-state");

        assert_eq!(
            default_app_state_dir(),
            PathBuf::from("/tmp/swivox-instance-state")
        );

        drop(scope);
        assert!(default_app_state_dir().ends_with(".swivox"));
    }

    #[test]
    fn user_scripts_paths_live_inside_app_state_directory() {
        let _guard = settings_path_test_guard();
        let scope = StateDirEnvScope::set("/tmp/swivox-isolated-state");

        assert_eq!(
            default_user_scripts_dir(),
            PathBuf::from("/tmp/swivox-isolated-state/user_scripts")
        );
        assert_eq!(
            default_user_scripts_config_path(),
            PathBuf::from("/tmp/swivox-isolated-state/user_scripts.json")
        );

        drop(scope);
        assert!(default_user_scripts_dir().ends_with(".swivox/user_scripts"));
        assert!(default_user_scripts_config_path().ends_with(".swivox/user_scripts.json"));
    }

    struct StateDirEnvScope {
        previous: Option<std::ffi::OsString>,
    }

    impl StateDirEnvScope {
        fn set(value: &str) -> Self {
            let previous = std::env::var_os(STATE_DIR_ENV);
            unsafe { std::env::set_var(STATE_DIR_ENV, value) };
            Self { previous }
        }
    }

    impl Drop for StateDirEnvScope {
        fn drop(&mut self) {
            match self.previous.take() {
                Some(previous) => unsafe { std::env::set_var(STATE_DIR_ENV, previous) },
                None => unsafe { std::env::remove_var(STATE_DIR_ENV) },
            }
        }
    }
}
