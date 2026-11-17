//! Antigravity 桌面管理器后端命令层。

use serde::Serialize;
use swivox_core::antigravity::{
    self, AntigravityClientKind, AntigravityDeviceProfile, AntigravityEnhanceStatus,
    AntigravityLaunchOutcome, AntigravityOAuthStartResponse, AntigravitySnapshot,
    DEFAULT_ANTIGRAVITY_CDP_PORT,
};

use crate::commands::CommandResult;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AntigravitySnapshotPayload {
    pub snapshot: AntigravitySnapshot,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AntigravityOAuthStartPayload {
    pub session: Option<AntigravityOAuthStartResponse>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AntigravityExportPayload {
    pub json: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AntigravityLaunchPayload {
    pub outcome: Option<AntigravityLaunchOutcome>,
    pub snapshot: AntigravitySnapshot,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AntigravityDevicePayload {
    pub profile: Option<AntigravityDeviceProfile>,
    pub snapshot: AntigravitySnapshot,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AntigravityEnhanceSyncPayload {
    pub enhance: AntigravityEnhanceStatus,
    pub snapshot: AntigravitySnapshot,
}

fn ok<T: Serialize>(message: impl Into<String>, payload: T) -> CommandResult<T> {
    CommandResult {
        status: "ok".to_string(),
        message: message.into(),
        payload,
    }
}

fn failed<T: Serialize>(message: impl Into<String>, payload: T) -> CommandResult<T> {
    CommandResult {
        status: "failed".to_string(),
        message: message.into(),
        payload,
    }
}

pub fn load_antigravity_snapshot() -> CommandResult<AntigravitySnapshotPayload> {
    ok(
        "已加载 Antigravity 状态。",
        AntigravitySnapshotPayload {
            snapshot: antigravity::build_antigravity_snapshot(),
        },
    )
}

pub async fn start_antigravity_oauth(
    client_key: Option<String>,
) -> CommandResult<AntigravityOAuthStartPayload> {
    match antigravity::start_oauth_login(client_key.as_deref()).await {
        Ok(session) => ok(
            "已生成 Google OAuth 授权地址，请在浏览器中完成登录。",
            AntigravityOAuthStartPayload {
                session: Some(session),
            },
        ),
        Err(err) => failed(
            err.to_string(),
            AntigravityOAuthStartPayload { session: None },
        ),
    }
}

pub async fn complete_antigravity_oauth(
    login_id: String,
) -> CommandResult<AntigravitySnapshotPayload> {
    let state = antigravity::load_antigravity_state();
    let proxy = antigravity::effective_proxy_for_ag_account(None, &state)
        .ok()
        .flatten();
    match antigravity::wait_for_oauth_callback(&login_id, proxy.as_deref()).await {
        Ok((tokens, user)) => {
            let name = user.display_name();
            match antigravity::upsert_account_from_tokens(
                user.email.clone(),
                name,
                tokens.access_token,
                tokens.refresh_token,
                tokens.expiry_timestamp,
                tokens.id_token,
                tokens.client_key,
                None,
                false,
            )
            .await
            {
                Ok(_) => ok(
                    format!("已成功添加 Antigravity 账号：{}", user.email),
                    AntigravitySnapshotPayload {
                        snapshot: antigravity::build_antigravity_snapshot(),
                    },
                ),
                Err(err) => failed(
                    err.to_string(),
                    AntigravitySnapshotPayload {
                        snapshot: antigravity::build_antigravity_snapshot(),
                    },
                ),
            }
        }
        Err(err) => failed(
            err.to_string(),
            AntigravitySnapshotPayload {
                snapshot: antigravity::build_antigravity_snapshot(),
            },
        ),
    }
}

pub async fn submit_antigravity_manual_callback(
    raw_input: String,
) -> CommandResult<AntigravitySnapshotPayload> {
    let state = antigravity::load_antigravity_state();
    let proxy = antigravity::effective_proxy_for_ag_account(None, &state)
        .ok()
        .flatten();
    match antigravity::complete_manual_oauth_callback(&raw_input, proxy.as_deref()).await {
        Ok((tokens, user)) => {
            let name = user.display_name();
            match antigravity::upsert_account_from_tokens(
                user.email.clone(),
                name,
                tokens.access_token,
                tokens.refresh_token,
                tokens.expiry_timestamp,
                tokens.id_token,
                tokens.client_key,
                None,
                false,
            )
            .await
            {
                Ok(_) => ok(
                    format!("已验证回调并添加 Antigravity 账号：{}", user.email),
                    AntigravitySnapshotPayload {
                        snapshot: antigravity::build_antigravity_snapshot(),
                    },
                ),
                Err(err) => failed(
                    err.to_string(),
                    AntigravitySnapshotPayload {
                        snapshot: antigravity::build_antigravity_snapshot(),
                    },
                ),
            }
        }
        Err(err) => failed(
            err.to_string(),
            AntigravitySnapshotPayload {
                snapshot: antigravity::build_antigravity_snapshot(),
            },
        ),
    }
}

pub fn cancel_antigravity_oauth() {
    antigravity::cancel_pending_oauth_session();
}

pub async fn import_local_antigravity_accounts() -> CommandResult<AntigravitySnapshotPayload> {
    match antigravity::scan_and_import_local_accounts().await {
        Ok(count) => ok(
            format!("已从本机导入 {count} 个 Antigravity 账号。"),
            AntigravitySnapshotPayload {
                snapshot: antigravity::build_antigravity_snapshot(),
            },
        ),
        Err(err) => failed(
            err.to_string(),
            AntigravitySnapshotPayload {
                snapshot: antigravity::build_antigravity_snapshot(),
            },
        ),
    }
}

pub async fn import_antigravity_accounts_text(
    raw_text: String,
) -> CommandResult<AntigravitySnapshotPayload> {
    match antigravity::import_accounts_from_text(&raw_text).await {
        Ok(count) => ok(
            format!("已成功导入 {count} 个 Antigravity 账号。"),
            AntigravitySnapshotPayload {
                snapshot: antigravity::build_antigravity_snapshot(),
            },
        ),
        Err(err) => failed(
            err.to_string(),
            AntigravitySnapshotPayload {
                snapshot: antigravity::build_antigravity_snapshot(),
            },
        ),
    }
}

pub fn export_antigravity_accounts() -> CommandResult<AntigravityExportPayload> {
    match antigravity::export_accounts_json() {
        Ok(json) => ok(
            "已导出 Antigravity 账号 JSON。",
            AntigravityExportPayload { json },
        ),
        Err(err) => failed(
            err.to_string(),
            AntigravityExportPayload {
                json: String::new(),
            },
        ),
    }
}

pub async fn switch_antigravity_account(
    account_id: String,
) -> CommandResult<AntigravitySnapshotPayload> {
    switch_antigravity_account_with_mode(account_id, "desktop_hot".to_string()).await
}

pub async fn switch_antigravity_account_with_mode(
    account_id: String,
    switch_mode: String,
) -> CommandResult<AntigravitySnapshotPayload> {
    match antigravity::switch_active_account_with_mode(&account_id, &switch_mode).await {
        Ok(msg) => ok(
            msg,
            AntigravitySnapshotPayload {
                snapshot: antigravity::build_antigravity_snapshot(),
            },
        ),
        Err(err) => failed(
            err.to_string(),
            AntigravitySnapshotPayload {
                snapshot: antigravity::build_antigravity_snapshot(),
            },
        ),
    }
}

pub async fn refresh_antigravity_account_quota(
    account_id: String,
) -> CommandResult<AntigravitySnapshotPayload> {
    match antigravity::refresh_account_quota(&account_id).await {
        Ok(_) => ok(
            "已刷新 Antigravity 账号配额。",
            AntigravitySnapshotPayload {
                snapshot: antigravity::build_antigravity_snapshot(),
            },
        ),
        Err(err) => failed(
            err.to_string(),
            AntigravitySnapshotPayload {
                snapshot: antigravity::build_antigravity_snapshot(),
            },
        ),
    }
}

pub async fn refresh_all_antigravity_quotas() -> CommandResult<AntigravitySnapshotPayload> {
    let (ok_cnt, fail_cnt) = antigravity::refresh_all_account_quotas().await;
    let status_msg = if fail_cnt == 0 {
        format!("已刷新全部 {ok_cnt} 个 Antigravity 账号配额。")
    } else {
        format!("配额刷新完成：成功 {ok_cnt} 个，失败 {fail_cnt} 个。")
    };
    ok(
        status_msg,
        AntigravitySnapshotPayload {
            snapshot: antigravity::build_antigravity_snapshot(),
        },
    )
}

pub fn update_antigravity_account_meta(
    account_id: String,
    label: Option<String>,
    proxy_mode: Option<String>,
    egress_proxy_url: Option<String>,
) -> CommandResult<AntigravitySnapshotPayload> {
    match antigravity::update_account_metadata(&account_id, label, proxy_mode, egress_proxy_url) {
        Ok(()) => ok(
            "已更新 Antigravity 账号设置。",
            AntigravitySnapshotPayload {
                snapshot: antigravity::build_antigravity_snapshot(),
            },
        ),
        Err(err) => failed(
            err.to_string(),
            AntigravitySnapshotPayload {
                snapshot: antigravity::build_antigravity_snapshot(),
            },
        ),
    }
}

pub fn delete_antigravity_account(account_id: String) -> CommandResult<AntigravitySnapshotPayload> {
    match antigravity::delete_account(&account_id) {
        Ok(()) => ok(
            "已删除 Antigravity 账号。",
            AntigravitySnapshotPayload {
                snapshot: antigravity::build_antigravity_snapshot(),
            },
        ),
        Err(err) => failed(
            err.to_string(),
            AntigravitySnapshotPayload {
                snapshot: antigravity::build_antigravity_snapshot(),
            },
        ),
    }
}

pub async fn save_antigravity_enhance_settings(
    preferred_target: String,
    custom_desktop_path: String,
    custom_ide_path: String,
    launch_proxy_mode: String,
    launch_proxy_url: String,
    enable_ui_localization: bool,
    enable_context_ring: bool,
    cdp_port: u16,
) -> CommandResult<AntigravityEnhanceSyncPayload> {
    let mut state = antigravity::load_antigravity_state();
    state.preferred_target = AntigravityClientKind::parse(&preferred_target);
    state.custom_desktop_path = custom_desktop_path.trim().to_string();
    state.custom_ide_path = custom_ide_path.trim().to_string();
    state.launch_proxy_mode = match launch_proxy_mode.trim() {
        "direct" => "direct".to_string(),
        "custom" => "custom".to_string(),
        m if m.starts_with("node:") => m.to_string(),
        _ => "inherit".to_string(),
    };
    state.launch_proxy_url = launch_proxy_url.trim().to_string();
    state.enable_ui_localization = enable_ui_localization;
    state.enable_context_ring = enable_context_ring;
    state.cdp_port = if cdp_port == 0 {
        DEFAULT_ANTIGRAVITY_CDP_PORT
    } else {
        cdp_port
    };

    if let Err(err) = antigravity::save_antigravity_state(&state) {
        return failed(
            err.to_string(),
            AntigravityEnhanceSyncPayload {
                enhance: AntigravityEnhanceStatus::default(),
                snapshot: antigravity::build_antigravity_snapshot(),
            },
        );
    }

    // 立即向运行中页面热同步（关闭汉化开关时瞬间还原英文原文，零重启！）
    let enhance = antigravity::apply_cdp_enhancements_once(
        state.cdp_port,
        state.enable_ui_localization,
        state.enable_context_ring,
    )
    .await;

    ok(
        "已保存 Antigravity 增强与免 TUN 代理设置。",
        AntigravityEnhanceSyncPayload {
            enhance,
            snapshot: antigravity::build_antigravity_snapshot(),
        },
    )
}

pub async fn launch_antigravity(
    force_clean_conflict: bool,
) -> CommandResult<AntigravityLaunchPayload> {
    let preferred = antigravity::load_antigravity_state().preferred_target;
    let target_str = match preferred {
        AntigravityClientKind::Ide => "ide",
        _ => "desktop",
    };
    launch_antigravity_target(target_str.to_string(), force_clean_conflict).await
}

pub async fn launch_antigravity_target(
    target_kind: String,
    force_clean_conflict: bool,
) -> CommandResult<AntigravityLaunchPayload> {
    let explicit_kind = match target_kind.trim().to_ascii_lowercase().as_str() {
        "ide" | "antigravity_ide" => AntigravityClientKind::Ide,
        "desktop" | "antigravity_desktop" => AntigravityClientKind::Desktop,
        _ => antigravity::load_antigravity_state().preferred_target,
    };
    let state = antigravity::load_antigravity_state();
    let res = tokio::task::spawn_blocking(move || {
        antigravity::launch_antigravity_instance(
            explicit_kind,
            Some(&state.custom_desktop_path),
            Some(&state.custom_ide_path),
            &state.launch_proxy_mode,
            &state.launch_proxy_url,
            state.cdp_port,
            force_clean_conflict,
        )
    })
    .await;

    match res {
        Ok(Ok(outcome)) => {
            if outcome.conflict {
                CommandResult {
                    status: "conflict".to_string(),
                    message: outcome.message.clone(),
                    payload: AntigravityLaunchPayload {
                        outcome: Some(outcome),
                        snapshot: antigravity::build_antigravity_snapshot(),
                    },
                }
            } else {
                let st = antigravity::load_antigravity_state();
                if st.enable_ui_localization || st.enable_context_ring {
                    let port = outcome.cdp_port;
                    tokio::spawn(async move {
                        for _ in 0..8 {
                            tokio::time::sleep(std::time::Duration::from_millis(900)).await;
                            let r = antigravity::apply_cdp_enhancements_once(
                                port,
                                st.enable_ui_localization,
                                st.enable_context_ring,
                            )
                            .await;
                            if r.connected {
                                break;
                            }
                        }
                    });
                }
                ok(
                    outcome.message.clone(),
                    AntigravityLaunchPayload {
                        outcome: Some(outcome),
                        snapshot: antigravity::build_antigravity_snapshot(),
                    },
                )
            }
        }
        Ok(Err(err)) => failed(
            err.to_string(),
            AntigravityLaunchPayload {
                outcome: None,
                snapshot: antigravity::build_antigravity_snapshot(),
            },
        ),
        Err(err) => failed(
            err.to_string(),
            AntigravityLaunchPayload {
                outcome: None,
                snapshot: antigravity::build_antigravity_snapshot(),
            },
        ),
    }
}

pub async fn stop_antigravity() -> CommandResult<AntigravitySnapshotPayload> {
    let state = antigravity::load_antigravity_state();
    let (target, _) = antigravity::resolve_effective_target(
        state.preferred_target,
        Some(&state.custom_desktop_path),
        Some(&state.custom_ide_path),
        state.cdp_port,
    );
    let target_str = if target == AntigravityClientKind::Ide {
        "ide"
    } else {
        "desktop"
    };
    stop_antigravity_target(target_str.to_string()).await
}

pub async fn stop_antigravity_target(
    target_kind: String,
) -> CommandResult<AntigravitySnapshotPayload> {
    let is_ide = matches!(
        target_kind.trim().to_ascii_lowercase().as_str(),
        "ide" | "antigravity_ide"
    );
    let label = if is_ide {
        "Antigravity IDE"
    } else {
        "Antigravity 桌面版"
    };
    let res = tokio::task::spawn_blocking(move || antigravity::stop_antigravity_target(is_ide)).await;
    match res {
        Ok(Ok(_)) => ok(
            format!("已停止 {label}。"),
            AntigravitySnapshotPayload {
                snapshot: antigravity::build_antigravity_snapshot(),
            },
        ),
        Ok(Err(err)) => failed(
            err.to_string(),
            AntigravitySnapshotPayload {
                snapshot: antigravity::build_antigravity_snapshot(),
            },
        ),
        Err(err) => failed(
            err.to_string(),
            AntigravitySnapshotPayload {
                snapshot: antigravity::build_antigravity_snapshot(),
            },
        ),
    }
}

pub fn regenerate_antigravity_device_fingerprint(
    account_id: Option<String>,
) -> CommandResult<AntigravityDevicePayload> {
    match antigravity::regenerate_and_apply_device_profile(account_id.as_deref()) {
        Ok(prof) => ok(
            "已生成并应用新的隔离设备指纹。",
            AntigravityDevicePayload {
                profile: Some(prof),
                snapshot: antigravity::build_antigravity_snapshot(),
            },
        ),
        Err(err) => failed(
            err.to_string(),
            AntigravityDevicePayload {
                profile: None,
                snapshot: antigravity::build_antigravity_snapshot(),
            },
        ),
    }
}

pub fn restore_antigravity_device_baseline() -> CommandResult<AntigravityDevicePayload> {
    match antigravity::restore_original_device_baseline() {
        Ok(prof) => ok(
            "已恢复本机原始设备指纹基线。",
            AntigravityDevicePayload {
                profile: Some(prof),
                snapshot: antigravity::build_antigravity_snapshot(),
            },
        ),
        Err(err) => failed(
            err.to_string(),
            AntigravityDevicePayload {
                profile: None,
                snapshot: antigravity::build_antigravity_snapshot(),
            },
        ),
    }
}
