use chatgptx_backend::i18n::{t, tf};
use chatgptx_backend::phase5_rules::{format_bytes_human, recommended_cache_selection};
use gpui_kit::assets::IconName;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::theme::ActiveTheme as _;
use gpui_kit::component::StyledExt as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::store::AppStore;
use crate::views::panel_card;
use crate::widgets::{AppBadge, AppButton};

pub fn render_agent_cache(store: &Entity<AppStore>, cx: &App) -> impl IntoElement {
    let data = store.read(cx);
    let scanning = data.cache_scanning;
    let cleaning = data.cache_cleaning;
    let confirm_open = data.cache_confirm_open;
    let selected_ids = data.cache_selected_ids.clone();
    let report = data.cache_report.clone();

    let groups = report.as_ref().map(|r| r.groups.clone()).unwrap_or_default();
    let total_bytes: u64 = groups.iter().map(|g| g.total_bytes).sum();
    let selected_bytes: u64 = groups
        .iter()
        .filter(|g| selected_ids.contains(&g.id))
        .map(|g| g.eligible_bytes)
        .sum();
    let selected_files: usize = groups
        .iter()
        .filter(|g| selected_ids.contains(&g.id))
        .map(|g| g.eligible_files)
        .sum();

    let store_scan = store.clone();
    let store_sel_rec = store.clone();
    let store_sel_all = store.clone();
    let store_sel_none = store.clone();
    let store_open_confirm = store.clone();

    let confirm_banner: Option<AnyElement> = confirm_open.then(|| {
        let store_cancel = store.clone();
        let store_exec = store.clone();
        div()
            .v_flex()
            .w_full()
            .gap_3()
            .p_4()
            .rounded(px(11.))
            .bg(cx.theme().popover)
            .border_1()
            .border_color(cx.theme().danger.opacity(0.6))
            .child(
                div()
                    .text_sm()
                    .font_semibold()
                    .text_color(cx.theme().foreground)
                    .child(t("确认执行缓存清理")),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(tf(
                        "即将清理 {0} 个过期缓存文件，预计释放 {1}（自动保留最近 24 小时内的活跃文件，绝不触碰会话与凭据）。",
                        &[
                            &selected_files.to_string(),
                            &format_bytes_human(selected_bytes),
                        ],
                    )),
            )
            .child(
                div()
                    .h_flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        AppButton::new("cache-confirm-cancel")
                            .secondary()
                            .small()
                            .label(t("取消"))
                            .on_click(move |_, _, cx| {
                                store_cancel.update(cx, |s, cx| {
                                    s.cache_confirm_open = false;
                                    cx.notify();
                                });
                            }),
                    )
                    .child(
                        AppButton::new("cache-confirm-exec")
                            .danger()
                            .small()
                            .icon(IconName::Trash)
                            .label(t("确认清理"))
                            .on_click(move |_, _, cx| {
                                store_exec
                                    .update(cx, |s, cx| s.clean_selected_agent_cache(cx));
                            }),
                    ),
            )
            .into_any_element()
    });

    let group_rows: AnyElement = if groups.is_empty() {
        div()
            .p_6()
            .w_full()
            .rounded(px(10.))
            .bg(cx.theme().secondary.opacity(0.24))
            .border_1()
            .border_color(cx.theme().border)
            .text_xs()
            .text_color(cx.theme().muted_foreground)
            .child(if scanning {
                t("正在扫描本机 AI Agent 缓存白名单目录…")
            } else {
                t("尚未发现可统计的缓存目录，点击右上角「重新扫描」开始检测。")
            })
            .into_any_element()
    } else {
        let rows: Vec<AnyElement> = groups
            .iter()
            .map(|g| {
                let can_pick = g.cleanable && g.eligible_files > 0;
                let checked = selected_ids.contains(&g.id);
                let gid = g.id.clone();
                let store_chk = store.clone();

                div()
                    .h_flex()
                    .w_full()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .p_3p5()
                    .rounded(px(10.))
                    .bg(cx.theme().secondary.opacity(if checked { 0.38 } else { 0.24 }))
                    .border_1()
                    .border_color(if checked {
                        cx.theme().primary.opacity(0.5)
                    } else {
                        cx.theme().border
                    })
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .gap_3()
                            .flex_1()
                            .min_w_0()
                            .child(
                                Checkbox::new(SharedString::from(format!("cache-chk-{}", g.id)))
                                    .checked(checked)
                                    .on_change(move |_, _, cx| {
                                        if !can_pick {
                                            return;
                                        }
                                        let id = gid.clone();
                                        store_chk.update(cx, |s, cx| {
                                            if !s.cache_selected_ids.remove(&id) {
                                                s.cache_selected_ids.insert(id);
                                            }
                                            cx.notify();
                                        });
                                    }),
                            )
                            .child(
                                div()
                                    .v_flex()
                                    .flex_1()
                                    .min_w_0()
                                    .gap_0p5()
                                    .child(
                                        div()
                                            .h_flex()
                                            .items_center()
                                            .gap_2()
                                            .child(
                                                div()
                                                    .text_sm()
                                                    .font_semibold()
                                                    .text_color(cx.theme().foreground)
                                                    .child(format!("{} · {}", g.app, g.kind)),
                                            )
                                            .child(AppBadge::secondary(tf(
                                                "{0} · {1} 个可清文件",
                                                &[
                                                    &format_bytes_human(g.eligible_bytes),
                                                    &g.eligible_files.to_string(),
                                                ],
                                            ))),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .whitespace_nowrap()
                                            .overflow_hidden()
                                            .child(g.path.clone()),
                                    ),
                            ),
                    )
                    .child(AppBadge::outline(format!(
                        "{}: {}",
                        t("总占用"),
                        format_bytes_human(g.total_bytes)
                    )))
                    .into_any_element()
            })
            .collect();
        div().v_flex().w_full().gap_2p5().children(rows).into_any_element()
    };

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
                        .h_flex()
                        .items_center()
                        .gap_2()
                        .child(AppBadge::secondary(format!(
                            "{}：{}",
                            t("缓存总占用"),
                            format_bytes_human(total_bytes)
                        )))
                        .child(AppBadge::good(format!(
                            "{}：{} ({selected_files})",
                            t("已勾选可释放"),
                            format_bytes_human(selected_bytes)
                        ))),
                )
                .child(
                    div()
                        .h_flex()
                        .flex_shrink_0()
                        .items_center()
                        .gap_2()
                        .child(
                            AppButton::new("cache-rescan-btn")
                                .secondary()
                                .small()
                                .disabled(scanning || cleaning)
                                .icon(IconName::RefreshCw)
                                .label(if scanning {
                                    t("扫描中…")
                                } else {
                                    t("重新扫描")
                                })
                                .on_click(move |_, _, cx| {
                                    store_scan.update(cx, |s, cx| s.scan_agent_cache_now(cx));
                                }),
                        )
                        .child(
                            AppButton::new("cache-clean-btn")
                                .danger()
                                .small()
                                .disabled(selected_ids.is_empty() || cleaning || scanning)
                                .icon(IconName::Trash)
                                .label(if cleaning {
                                    t("正在清理…")
                                } else {
                                    t("清理已选缓存")
                                })
                                .on_click(move |_, _, cx| {
                                    store_open_confirm.update(cx, |s, cx| {
                                        s.cache_confirm_open = true;
                                        cx.notify();
                                    });
                                }),
                        ),
                ),
        )
        .when_some(confirm_banner, |d, b| d.child(b))
        .child(panel_card(
            cx,
            div()
                .v_flex()
                .w_full()
                .gap_3p5()
                .child(
                    div()
                        .h_flex()
                        .items_center()
                        .gap_2()
                        .child(
                            AppButton::new("cache-sel-rec")
                                .outline()
                                .small()
                                .label(t("推荐选择（保留日志）"))
                                .on_click(move |_, _, cx| {
                                    store_sel_rec.update(cx, |s, cx| {
                                        if let Some(r) = &s.cache_report {
                                            s.cache_selected_ids =
                                                recommended_cache_selection(&r.groups, false);
                                            cx.notify();
                                        }
                                    });
                                }),
                        )
                        .child(
                            AppButton::new("cache-sel-all")
                                .outline()
                                .small()
                                .label(t("全选可清理项"))
                                .on_click(move |_, _, cx| {
                                    store_sel_all.update(cx, |s, cx| {
                                        if let Some(r) = &s.cache_report {
                                            s.cache_selected_ids =
                                                recommended_cache_selection(&r.groups, true);
                                            cx.notify();
                                        }
                                    });
                                }),
                        )
                        .child(
                            AppButton::new("cache-sel-none")
                                .ghost()
                                .small()
                                .label(t("清空选择"))
                                .on_click(move |_, _, cx| {
                                    store_sel_none.update(cx, |s, cx| {
                                        s.cache_selected_ids.clear();
                                        cx.notify();
                                    });
                                }),
                        ),
                )
                .child(group_rows),
        ))
}
