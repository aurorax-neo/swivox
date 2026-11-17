use chatgptx_backend::i18n::{t, tf};
use gpui_kit::assets::IconName;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::theme::ActiveTheme as _;
use gpui_kit::component::StyledExt as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::store::AppStore;
use crate::views::panel_card;
use crate::widgets::{AppBadge, AppButton};

pub fn render_user_scripts(store: &Entity<AppStore>, cx: &App) -> impl IntoElement {
    let data = store.read(cx);
    let inv = data.user_scripts_inventory.clone();
    let market = data.market_scripts.clone();
    let market_loading = data.script_market_loading;
    let installing_id = data.script_installing_id.clone();

    let store_reload = store.clone();
    let store_refresh = store.clone();
    let store_master = store.clone();

    let local_rows: AnyElement = if inv.scripts.is_empty() {
        div()
            .p_4()
            .w_full()
            .rounded(px(9.))
            .bg(cx.theme().secondary.opacity(0.24))
            .border_1()
            .border_color(cx.theme().border)
            .text_xs()
            .text_color(cx.theme().muted_foreground)
            .child(format!(
                "{}（{}：{}）",
                t("当前还没有安装任何本地用户脚本"),
                t("脚本目录"),
                inv.user_dir
            ))
            .into_any_element()
    } else {
        let items: Vec<AnyElement> = inv
            .scripts
            .iter()
            .map(|sc| {
                let key_sw = sc.key.clone();
                let store_sw = store.clone();
                let key_del = sc.key.clone();
                let store_del = store.clone();
                let is_user = sc.source == "user";

                let status_badge = match sc.status.as_str() {
                    "loaded" | "ok" => AppBadge::good(t("已加载")),
                    "failed" => AppBadge::bad(t("加载失败")),
                    "disabled" => AppBadge::secondary(t("已禁用")),
                    _ => AppBadge::outline(t("待注入")),
                };

                div()
                    .h_flex()
                    .w_full()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .p_3p5()
                    .rounded(px(10.))
                    .bg(cx.theme().secondary.opacity(0.28))
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
                                    .h_flex()
                                    .flex_wrap()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_semibold()
                                            .text_color(cx.theme().foreground)
                                            .child(sc.name.clone()),
                                    )
                                    .child(status_badge)
                                    .when(!sc.version.is_empty(), |d| {
                                        d.child(AppBadge::outline(format!("v{}", sc.version)))
                                    }),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(if sc.error.is_empty() {
                                        cx.theme().muted_foreground
                                    } else {
                                        cx.theme().danger
                                    })
                                    .child(if sc.error.is_empty() {
                                        sc.key.clone()
                                    } else {
                                        sc.error.clone()
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .h_flex()
                            .flex_shrink_0()
                            .items_center()
                            .gap_2()
                            .child(
                                Switch::new(SharedString::from(format!("us-sw-{}", sc.key)))
                                    .checked(sc.enabled)
                                    .on_change(move |val, _, cx| {
                                        let k = key_sw.clone();
                                        let v = *val;
                                        store_sw.update(cx, |s, cx| {
                                            s.set_single_user_script(k, v, cx)
                                        });
                                    }),
                            )
                            .when(is_user, |d| {
                                d.child(
                                    AppButton::new(SharedString::from(format!(
                                        "us-del-{}",
                                        sc.key
                                    )))
                                    .ghost()
                                    .small_icon()
                                    .icon(IconName::Trash)
                                    .tooltip(t("卸载用户脚本"))
                                    .on_click(move |_, _, cx| {
                                        let k = key_del.clone();
                                        store_del.update(cx, |s, cx| {
                                            s.delete_local_user_script(k, cx)
                                        });
                                    }),
                                )
                            }),
                    )
                    .into_any_element()
            })
            .collect();
        div().v_flex().w_full().gap_2p5().children(items).into_any_element()
    };

    let market_rows: AnyElement = if market.is_empty() {
        div()
            .p_4()
            .w_full()
            .rounded(px(9.))
            .bg(cx.theme().secondary.opacity(0.24))
            .border_1()
            .border_color(cx.theme().border)
            .text_xs()
            .text_color(cx.theme().muted_foreground)
            .child(if market_loading {
                t("正在拉取社区拓展脚本市场清单…")
            } else {
                t("暂无脚本市场数据，点击右上角「刷新市场」获取最新用户脚本。")
            })
            .into_any_element()
    } else {
        let cards: Vec<AnyElement> = market
            .into_iter()
            .map(|ms| {
                let busy = installing_id.as_deref() == Some(ms.id.as_str());
                let id_inst = ms.id.clone();
                let store_inst = store.clone();
                let hp = ms.homepage.clone();
                let store_hp = store.clone();

                let badge = if ms.update_available {
                    AppBadge::warn(t("可更新"))
                } else if ms.installed {
                    AppBadge::good(tf("已安装 {0}", &[&ms.installed_version]))
                } else {
                    AppBadge::outline(format!("v{}", ms.version))
                };

                div()
                    .h_flex()
                    .w_full()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .p_3p5()
                    .rounded(px(10.))
                    .bg(cx.theme().secondary.opacity(0.28))
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
                                    .h_flex()
                                    .flex_wrap()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_semibold()
                                            .text_color(cx.theme().foreground)
                                            .child(ms.name),
                                    )
                                    .child(badge)
                                    .when(!ms.author.is_empty(), |d| {
                                        d.child(
                                            div()
                                                .text_xs()
                                                .text_color(cx.theme().muted_foreground)
                                                .child(format!("by {}", ms.author)),
                                        )
                                    }),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(ms.description),
                            ),
                    )
                    .child(
                        div()
                            .h_flex()
                            .flex_shrink_0()
                            .items_center()
                            .gap_2()
                            .when(!hp.is_empty(), |d| {
                                d.child(
                                    AppButton::new(SharedString::from(format!("ms-hp-{}", ms.id)))
                                        .ghost()
                                        .small_icon()
                                        .icon(IconName::Globe)
                                        .on_click(move |_, _, cx| {
                                            let u = hp.clone();
                                            store_hp.update(cx, |s, cx| s.open_external_url(u, cx));
                                        }),
                                )
                            })
                            .child(
                                AppButton::new(SharedString::from(format!("ms-inst-{}", ms.id)))
                                    .primary()
                                    .small()
                                    .disabled(busy)
                                    .icon(IconName::Download)
                                    .label(if busy {
                                        t("安装中…")
                                    } else if ms.update_available {
                                        t("更新")
                                    } else if ms.installed {
                                        t("重新安装")
                                    } else {
                                        t("安装")
                                    })
                                    .on_click(move |_, _, cx| {
                                        let id = id_inst.clone();
                                        store_inst.update(cx, |s, cx| {
                                            s.install_market_user_script(id, cx)
                                        });
                                    }),
                            ),
                    )
                    .into_any_element()
            })
            .collect();
        div().v_flex().w_full().gap_2p5().children(cards).into_any_element()
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
                .justify_end()
                .gap_2()
                .child(
                    AppButton::new("us-reload-live")
                        .secondary()
                        .small()
                        .icon(IconName::RefreshCw)
                        .label(t("热重载运行脚本"))
                        .on_click(move |_, _, cx| {
                            store_reload.update(cx, |s, cx| s.reload_live_user_scripts(cx));
                        }),
                )
                .child(
                    AppButton::new("us-refresh-market")
                        .primary()
                        .small()
                        .disabled(market_loading)
                        .icon(IconName::Download)
                        .label(t("刷新脚本市场"))
                        .on_click(move |_, _, cx| {
                            store_refresh.update(cx, |s, cx| s.refresh_user_scripts_page(true, cx));
                        }),
                ),
        )
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
                        .gap_4()
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
                                .gap_0p5()
                                .child(
                                    div()
                                        .text_sm()
                                        .font_semibold()
                                        .text_color(cx.theme().foreground)
                                        .child(t("启用第三方用户脚本拓展 (window.chatgptx)")),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(format!("{}：{}", t("本地目录"), inv.user_dir)),
                                ),
                        )
                        .child(
                            Switch::new("us-master-sw")
                                .checked(inv.enabled)
                                .on_change(move |val, _, cx| {
                                    let v = *val;
                                    store_master
                                        .update(cx, |s, cx| s.set_user_scripts_master(v, cx));
                                }),
                        ),
                )
                .child(
                    div()
                        .text_sm()
                        .font_semibold()
                        .text_color(cx.theme().foreground)
                        .child(t("已安装脚本列表")),
                )
                .child(local_rows),
        ))
        .child(panel_card(
            cx,
            div()
                .v_flex()
                .w_full()
                .gap_3()
                .child(
                    div()
                        .text_sm()
                        .font_semibold()
                        .text_color(cx.theme().foreground)
                        .child(t("社区拓展脚本市场")),
                )
                .child(market_rows),
        ))
}
