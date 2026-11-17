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
    ok(
        "已加载 Lamplighter 状态。",
        LamplighterPayload {
            snapshot: lamplighter::get_lamplighter_snapshot_with_hours(history_hours),
        },
    )
}

pub async fn refresh_lamplighter(history_hours: u64) -> CommandResult<LamplighterPayload> {
    match lamplighter::refresh_all_now().await {
        Ok(_) => ok(
            "已刷新全部监控账号配额与点火计划。",
            LamplighterPayload {
                snapshot: lamplighter::get_lamplighter_snapshot_with_hours(history_hours),
            },
        ),
        Err(err) => failed(
            err.to_string(),
            LamplighterPayload {
                snapshot: lamplighter::get_lamplighter_snapshot_with_hours(history_hours),
            },
        ),
    }
}

pub async fn ignite_lamplighter_group(
    group_key: String,
    history_hours: u64,
) -> CommandResult<LamplighterPayload> {
    match lamplighter::manual_ignite_group(&group_key).await {
        Ok(msg) => ok(
            msg,
            LamplighterPayload {
                snapshot: lamplighter::get_lamplighter_snapshot_with_hours(history_hours),
            },
        ),
        Err(err) => failed(
            err.to_string(),
            LamplighterPayload {
                snapshot: lamplighter::get_lamplighter_snapshot_with_hours(history_hours),
            },
        ),
    }
}

pub fn save_lamplighter_config(
    mut config: LamplighterConfig,
    history_hours: u64,
) -> CommandResult<LamplighterPayload> {
    match lamplighter::store::save_config(&mut config) {
        Ok(()) => ok(
            "已保存 Lamplighter 设置并立即生效。",
            LamplighterPayload {
                snapshot: lamplighter::get_lamplighter_snapshot_with_hours(history_hours),
            },
        ),
        Err(err) => failed(
            err.to_string(),
            LamplighterPayload {
                snapshot: lamplighter::get_lamplighter_snapshot_with_hours(history_hours),
            },
        ),
    }
}

pub async fn test_lamplighter_notification(
    config: LamplighterConfig,
    history_hours: u64,
) -> CommandResult<LamplighterPayload> {
    match lamplighter::send_test_notification(Some(config)).await {
        Ok(msg) => ok(
            msg,
            LamplighterPayload {
                snapshot: lamplighter::get_lamplighter_snapshot_with_hours(history_hours),
            },
        ),
        Err(err) => failed(
            err.to_string(),
            LamplighterPayload {
                snapshot: lamplighter::get_lamplighter_snapshot_with_hours(history_hours),
            },
        ),
    }
}

pub fn add_lamplighter_external_account(
    provider: String,
    email: String,
    label: Option<String>,
    access_token: String,
    refresh_token: Option<String>,
    domain: Option<String>,
    proxy_url: Option<String>,
    history_hours: u64,
) -> CommandResult<LamplighterPayload> {
    match lamplighter::upsert_external_account(
        provider,
        email,
        label,
        access_token,
        refresh_token,
        domain,
        proxy_url,
    ) {
        Ok(()) => ok(
            "已添加监控账号凭据。",
            LamplighterPayload {
                snapshot: lamplighter::get_lamplighter_snapshot_with_hours(history_hours),
            },
        ),
        Err(err) => failed(
            err.to_string(),
            LamplighterPayload {
                snapshot: lamplighter::get_lamplighter_snapshot_with_hours(history_hours),
            },
        ),
    }
}

pub fn delete_lamplighter_external_account(
    id: String,
    history_hours: u64,
) -> CommandResult<LamplighterPayload> {
    match lamplighter::delete_external_account(&id) {
        Ok(()) => ok(
            "已移除监控账号凭据。",
            LamplighterPayload {
                snapshot: lamplighter::get_lamplighter_snapshot_with_hours(history_hours),
            },
        ),
        Err(err) => failed(
            err.to_string(),
            LamplighterPayload {
                snapshot: lamplighter::get_lamplighter_snapshot_with_hours(history_hours),
            },
        ),
    }
}
