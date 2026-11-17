use chatgptx_backend::i18n::t;
use gpui_kit::assets::IconName;
use gpui_kit::component::theme::ActiveTheme as _;
use gpui_kit::component::StyledExt as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::store::AppStore;
use crate::views::panel_card;
use crate::widgets::{AppBadge, AppButton};

const REPO_URL: &str = "https://github.com/aurorax-neo/chatgptx";

pub fn render_about(store: &Entity<AppStore>, cx: &App) -> impl IntoElement {
    let data = store.read(cx);
    let current_ver = data
        .overview
        .as_ref()
        .map(|o| o.current_version.clone())
        .unwrap_or_else(|| chatgptx_core::version::VERSION.to_string());
    let codex_ver = data
        .overview
        .as_ref()
        .and_then(|o| o.codex_version.clone())
        .unwrap_or_else(|| t("未检测"));
    let logs = data.logs_payload.clone();
    let upd = data.update_info.clone();
    let checking = data.checking_update;
    let installing = data.installing_update;

    let store_gh = store.clone();
    let store_chk_upd = store.clone();
    let store_inst_upd = store.clone();
    let store_diag = store.clone();
    let store_ref_log = store.clone();
    let store_copy_log = store.clone();
    let store_clear_log = store.clone();

    let log_text = logs
        .as_ref()
        .map(|l| l.text.clone())
        .unwrap_or_default();
    let log_text_for_copy = log_text.clone();
    let log_path = logs
        .as_ref()
        .map(|l| l.path.clone())
        .unwrap_or_default();

    div()
        .v_flex()
        .w_full()
        .gap_4()
        .child(panel_card(
            cx,
            div()
                .v_flex()
                .w_full()
                .gap_3p5()
                .child(
                    div()
                        .h_flex()
                        .w_full()
                        .items_center()
                        .justify_between()
                        .gap_3()
                        .child(
                            div()
                                .v_flex()
                                .gap_1()
                                .child(
                                    div()
                                        .h_flex()
                                        .items_center()
                                        .gap_2()
                                        .child(
                                            div()
                                                .text_base()
                                                .font_semibold()
                                                .text_color(cx.theme().foreground)
                                                .child("ChatGPTX (GPUI Edition)"),
                                        )
                                        .child(AppBadge::good(format!("v{current_ver}")))
                                        .child(AppBadge::secondary(format!("Codex {codex_ver}"))),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(t("原生 Rust + GPUI 驱动的多账号、多供应商、每模型窗口目录与客户端增强管理中心")),
                                ),
                        )
                        .child(
                            div()
                                .h_flex()
                                .flex_shrink_0()
                                .items_center()
                                .gap_2()
                                .child(
                                    AppButton::new("about-open-github")
                                        .outline()
                                        .small()
                                        .icon(IconName::Globe)
                                        .label("GitHub")
                                        .on_click(move |_, _, cx| {
                                            store_gh.update(cx, |s, cx| {
                                                s.open_external_url(REPO_URL.to_string(), cx)
                                            });
                                        }),
                                )
                                .child(
                                    AppButton::new("about-copy-diag")
                                        .secondary()
                                        .small()
                                        .icon(IconName::Copy)
                                        .label(t("复制诊断报告"))
                                        .on_click(move |_, _, cx| {
                                            store_diag
                                                .update(cx, |s, cx| s.copy_diagnostics_report(cx));
                                        }),
                                )
                                .child(
                                    AppButton::new("about-check-update")
                                        .primary()
                                        .small()
                                        .disabled(checking || installing)
                                        .icon(IconName::RefreshCw)
                                        .label(if checking {
                                            t("正在检查…")
                                        } else {
                                            t("检查更新")
                                        })
                                        .on_click(move |_, _, cx| {
                                            store_chk_upd
                                                .update(cx, |s, cx| s.check_for_updates_now(cx));
                                        }),
                                ),
                        ),
                )
                .when_some(upd, |d, info| {
                    d.child(
                        div()
                            .h_flex()
                            .w_full()
                            .items_center()
                            .justify_between()
                            .gap_3()
                            .p_3p5()
                            .rounded(px(10.))
                            .bg(cx.theme().secondary.opacity(0.32))
                            .border_1()
                            .border_color(cx.theme().border)
                            .child(
                                div()
                                    .v_flex()
                                    .flex_1()
                                    .min_w_0()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_semibold()
                                            .text_color(cx.theme().foreground)
                                            .child(if info.update_available {
                                                format!(
                                                    "{}：v{}",
                                                    t("发现新版本"),
                                                    info.latest_version.unwrap_or_default()
                                                )
                                            } else {
                                                t("当前已是最新版本")
                                            }),
                                    )
                                    .when(!info.release_summary.is_empty(), |col| {
                                        col.child(
                                            div()
                                                .text_xs()
                                                .text_color(cx.theme().muted_foreground)
                                                .child(info.release_summary),
                                        )
                                    }),
                            )
                            .when(info.update_available, |row| {
                                row.child(
                                    AppButton::new("about-install-update")
                                        .primary()
                                        .small()
                                        .disabled(installing)
                                        .icon(IconName::Download)
                                        .label(if installing {
                                            t("正在下载安装…")
                                        } else {
                                            t("立即下载并更新")
                                        })
                                        .on_click(move |_, _, cx| {
                                            store_inst_upd
                                                .update(cx, |s, cx| s.install_available_update(cx));
                                        }),
                                )
                            }),
                    )
                }),
        ))
        .child(panel_card(
            cx,
            div()
                .v_flex()
                .w_full()
                .gap_3()
                .child(
                    div()
                        .h_flex()
                        .w_full()
                        .items_center()
                        .justify_between()
                        .gap_3()
                        .child(
                            div()
                                .v_flex()
                                .flex_1()
                                .min_w_0()
                                .gap_0p5()
                                .child(
                                    div()
                                        .text_sm()
                                        .font_semibold()
                                        .text_color(cx.theme().foreground)
                                        .child(t("运行与诊断日志 (最近 200 行)")),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .whitespace_nowrap()
                                        .overflow_hidden()
                                        .child(log_path),
                                ),
                        )
                        .child(
                            div()
                                .h_flex()
                                .flex_shrink_0()
                                .items_center()
                                .gap_2()
                                .child(
                                    AppButton::new("about-log-refresh")
                                        .secondary()
                                        .small()
                                        .icon(IconName::RefreshCw)
                                        .label(t("刷新日志"))
                                        .on_click(move |_, _, cx| {
                                            store_ref_log
                                                .update(cx, |s, cx| s.refresh_about_page(true, cx));
                                        }),
                                )
                                .child(
                                    AppButton::new("about-log-copy")
                                        .secondary()
                                        .small()
                                        .icon(IconName::Copy)
                                        .label(t("复制日志"))
                                        .on_click(move |_, _, cx| {
                                            cx.write_to_clipboard(ClipboardItem::new_string(
                                                log_text_for_copy.clone(),
                                            ));
                                            store_copy_log.update(cx, |s, cx| {
                                                s.set_notice(
                                                    t("运行日志"),
                                                    t("已复制日志内容到剪贴板。"),
                                                    "ok",
                                                    cx,
                                                );
                                            });
                                        }),
                                )
                                .child(
                                    AppButton::new("about-log-clear")
                                        .outline()
                                        .small()
                                        .icon(IconName::Trash)
                                        .label(t("清空日志"))
                                        .on_click(move |_, _, cx| {
                                            store_clear_log
                                                .update(cx, |s, cx| s.clear_app_logs(cx));
                                        }),
                                ),
                        ),
                )
                .child(
                    div()
                        .w_full()
                        .p_3p5()
                        .rounded(px(9.))
                        .bg(cx.theme().secondary.opacity(0.32))
                        .border_1()
                        .border_color(cx.theme().border)
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(if log_text.trim().is_empty() {
                            t("当前日志文件为空。")
                        } else {
                            log_text
                        }),
                ),
        ))
}
