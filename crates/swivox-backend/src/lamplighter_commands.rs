//! Lamplighter（点灯人）后端命令集。

use serde::Serialize;
use swivox_core::lamplighter::{self, LamplighterConfig, LamplighterSnapshot};

use crate::commands::CommandResult;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LamplighterPayload {
    pub snapshot: LamplighterSnapshot,
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

pub fn load_lamplighter_snapshot(history_hours: u64) -> CommandResult<LamplighterPayload> {
    load_lamplighter_client_snapshot(None, history_hours)
}

pub fn load_lamplighter_client_snapshot(
    client_id: Option<String>,
    history_hours: u64,
) -> CommandResult<LamplighterPayload> {
    ok(
        "已加载 Lamplighter 状态。",
        LamplighterPayload {
            snapshot: lamplighter::get_lamplighter_snapshot_for_client(
                client_id.as_deref(),
                history_hours,
            ),
        },
    )
}

pub async fn refresh_lamplighter(history_hours: u64) -> CommandResult<LamplighterPayload> {
    refresh_lamplighter_client(None, history_hours).await
}

pub async fn refresh_lamplighter_client(
    client_id: Option<String>,
    history_hours: u64,
) -> CommandResult<LamplighterPayload> {
    match lamplighter::refresh_client_now(client_id.as_deref()).await {
        Ok(_) => ok(
            "已刷新当前客户端监控账号配额与点火计划。",
            LamplighterPayload {
                snapshot: lamplighter::get_lamplighter_snapshot_for_client(
                    client_id.as_deref(),
                    history_hours,
                ),
            },
        ),
        Err(err) => failed(
            err.to_string(),
            LamplighterPayload {
                snapshot: lamplighter::get_lamplighter_snapshot_for_client(
                    client_id.as_deref(),
                    history_hours,
                ),
            },
        ),
    }
}

pub async fn ignite_lamplighter_group(
    group_key: String,
    history_hours: u64,
) -> CommandResult<LamplighterPayload> {
    ignite_lamplighter_group_for_client(group_key, None, history_hours).await
}

pub async fn ignite_lamplighter_group_for_client(
    group_key: String,
    client_id: Option<String>,
    history_hours: u64,
) -> CommandResult<LamplighterPayload> {
    match lamplighter::manual_ignite_group(&group_key).await {
        Ok(msg) => ok(
            msg,
            LamplighterPayload {
                snapshot: lamplighter::get_lamplighter_snapshot_for_client(
                    client_id.as_deref(),
                    history_hours,
                ),
            },
        ),
        Err(err) => failed(
            err.to_string(),
            LamplighterPayload {
                snapshot: lamplighter::get_lamplighter_snapshot_for_client(
                    client_id.as_deref(),
                    history_hours,
                ),
            },
        ),
    }
}

pub fn save_lamplighter_config(
    config: LamplighterConfig,
    history_hours: u64,
) -> CommandResult<LamplighterPayload> {
    save_lamplighter_config_for_client(config, None, history_hours)
}

pub fn save_lamplighter_config_for_client(
    mut config: LamplighterConfig,
    client_id: Option<String>,
    history_hours: u64,
) -> CommandResult<LamplighterPayload> {
    match lamplighter::store::save_config(&mut config) {
        Ok(()) => ok(
            "已保存 Lamplighter 设置并立即生效。",
            LamplighterPayload {
                snapshot: lamplighter::get_lamplighter_snapshot_for_client(
                    client_id.as_deref(),
                    history_hours,
                ),
            },
        ),
        Err(err) => failed(
            err.to_string(),
            LamplighterPayload {
                snapshot: lamplighter::get_lamplighter_snapshot_for_client(
                    client_id.as_deref(),
                    history_hours,
                ),
            },
        ),
    }
}

pub async fn test_lamplighter_notification(
    config: LamplighterConfig,
    history_hours: u64,
) -> CommandResult<LamplighterPayload> {
    test_lamplighter_notification_for_client(config, None, history_hours).await
}

pub async fn test_lamplighter_notification_for_client(
    config: LamplighterConfig,
    client_id: Option<String>,
    history_hours: u64,
) -> CommandResult<LamplighterPayload> {
    match lamplighter::send_test_notification(Some(config)).await {
        Ok(msg) => ok(
            msg,
            LamplighterPayload {
                snapshot: lamplighter::get_lamplighter_snapshot_for_client(
                    client_id.as_deref(),
                    history_hours,
                ),
            },
        ),
        Err(err) => failed(
            err.to_string(),
            LamplighterPayload {
                snapshot: lamplighter::get_lamplighter_snapshot_for_client(
                    client_id.as_deref(),
                    history_hours,
                ),
            },
        ),
    }
}
