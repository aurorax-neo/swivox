use chatgptx_backend::commands::OverviewPayload;
use chatgptx_backend::i18n::{t, tf};
use chatgptx_backend::runtime_health::ServiceState;
use gpui_kit::assets::IconName;
use gpui_kit::component::StyledExt as _;
use gpui_kit::component::theme::ActiveTheme as _;
use gpui_kit::*;

use crate::store::AppStore;
use crate::views::{health_item_row, panel_card};
use crate::widgets::AppButton;

struct HealthCardData {
    title: String,
    detail: String,
    status: String,
    ok: bool,
}

fn build_codex_health_items(overview: Option<&OverviewPayload>) -> Vec<HealthCardData> {
    let conversion_detail = overview
        .and_then(|ov| ov.runtime_health.protocol_conversion_enabled)
        .map(|enabled| {
            if enabled {
                t("协议转换已启用")
            } else {
                t("协议转换未启用")
            }
        });

    let service_item =
        |title: String, state: Option<&ServiceState>, extra: Option<String>| -> HealthCardData {
            match state {
                Some(st) => {
                    let mut parts = Vec::new();
                    if let Some(addr) = &st.address
                        && !addr.is_empty()
                    {
                        parts.push(addr.clone());
                    }
                    let msg = t(st.message);
                    if !msg.is_empty() {
                        parts.push(msg);
                    }
                    if let Some(ext) = extra
                        && !ext.is_empty()
                    {
                        parts.push(ext);
                    }
                    HealthCardData {
                        title,
                        status: st.status.to_string(),
                        ok: matches!(st.status, "running" | "connected" | "disabled"),
                        detail: parts.join(" · "),
                    }
                }
                None => HealthCardData {
                    title,
                    status: "not_checked".to_string(),
                    ok: false,
                    detail: t("等待状态检查"),
                },
            }
        };

    let codex_version = overview.and_then(|ov| ov.codex_version.clone());
    let version_card = HealthCardData {
        title: t("Codex 版本"),
        status: if codex_version.is_some() {
            "ok".to_string()
        } else {
            "not_checked".to_string()
        },
        ok: codex_version.is_some(),
        detail: codex_version.unwrap_or_else(|| t("未检测到 Codex 应用版本。")),
    };

    let app_card = HealthCardData {
        title: t("Codex 应用"),
        status: overview
            .map(|ov| ov.codex_app.status.clone())
            .unwrap_or_else(|| "not_checked".to_string()),
        ok: overview.is_some_and(|ov| ov.codex_app.status == "found"),
        detail: overview
            .and_then(|ov| ov.codex_app.path.clone())
            .unwrap_or_else(|| t("尚未检查 Codex 应用路径。")),
    };

    let local_server_card = service_item(
        t("本地服务器状态"),
        overview.map(|ov| &ov.runtime_health.local_server),
        conversion_detail,
    );

    let debugger_card = service_item(
        t("调试连接状态"),
        overview.map(|ov| &ov.runtime_health.debugger),
        None,
    );

    vec![version_card, app_card, local_server_card, debugger_card]
}

pub fn render_overview(store: &Entity<AppStore>, cx: &App) -> impl IntoElement {
    let data = store.read(cx);
    let active_tool = data.active_tool.clone();

    if active_tool == "codex" {
        let cards = build_codex_health_items(data.overview.as_ref());
        let store_refresh = store.clone();

        let mut rows = Vec::new();
        for pair in cards.chunks(2) {
            let mut row = div().h_flex().w_full().gap_3().items_stretch();
            for item in pair {
                row = row.child(
                    div().flex_1().min_w_0().child(health_item_row(
                        cx,
                        item.ok,
                        item.title.clone(),
                        item.detail.clone(),
                        &item.status,
                    )),
                );
            }
            if pair.len() == 1 {
                row = row.child(div().flex_1());
            }
            rows.push(row);
        }

        panel_card(
            cx,
            div()
                .v_flex()
                .w_full()
                .gap_4()
                .child(
                    div()
                        .h_flex()
                        .w_full()
                        .items_center()
                        .justify_between()
                        .gap_3()
                        .child(
                            div()
                                .text_base()
                                .font_semibold()
                                .text_color(cx.theme().foreground)
                                .child(t("健康检查")),
                        )
                        .child(
                            div()
                                .h_flex()
                                .items_center()
                                .gap_3()
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(t("概览只展示关键问题，具体配置在对应页面处理")),
                                )
                                .child(
                                    AppButton::new("overview-refresh")
                                        .outline()
                                        .small_icon()
                                        .icon(IconName::RefreshCw)
                                        .tooltip(t("刷新状态"))
                                        .on_click(move |_, _, cx| {
                                            store_refresh
                                                .update(cx, |s, cx| s.refresh_current(cx));
                                        }),
                                ),
                        ),
                )
                .child(div().v_flex().w_full().gap_3().children(rows)),
        )
    } else {
        let tool = data
            .tool_entries
            .iter()
            .find(|entry| entry.id == active_tool);
        let tool_name = tool
            .map(|t_entry| t_entry.name.clone())
            .unwrap_or_else(|| t("工具"));
        let switchable = tool.is_some_and(|t_entry| t_entry.switchable);
        let relay_count = tool.map(|t_entry| t_entry.relay_count).unwrap_or(0);
        let home_dir = tool
            .map(|t_entry| t_entry.home_dir.clone())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| t("未配置目录"));

        panel_card(
            cx,
            div()
                .v_flex()
                .w_full()
                .gap_4()
                .child(
                    div()
                        .v_flex()
                        .gap_1()
                        .child(
                            div()
                                .text_base()
                                .font_semibold()
                                .text_color(cx.theme().foreground)
                                .child(tf("{0} 状态", &[&tool_name])),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(t("该工具由它自己的页签管理")),
                        ),
                )
                .child(
                    div()
                        .v_flex()
                        .gap_3()
                        .child(health_item_row(
                            cx,
                            switchable,
                            t("配置切换"),
                            if switchable {
                                t("已接入，可在该工具页切换供应商。")
                            } else {
                                t("尚未接入配置切换。")
                            },
                            if switchable { "ok" } else { "not_checked" },
                        ))
                        .child(health_item_row(
                            cx,
                            relay_count > 0,
                            t("供应商"),
                            tf("{0} 个已保存", &[&relay_count.to_string()]),
                            if relay_count > 0 { "ok" } else { "not_checked" },
                        ))
                        .child(health_item_row(cx, true, t("配置目录"), home_dir, "ok")),
                ),
        )
    }
}
