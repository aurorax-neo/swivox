use chatgptx_backend::i18n::t;
use chatgptx_backend::phase5_rules::{
    MarketFilter, MarketSource, filter_market_plugins, format_bytes_human,
};
use gpui_kit::assets::IconName;
use gpui_kit::component::Icon;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::theme::ActiveTheme as _;
use gpui_kit::component::{Sizable as _, StyledExt as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::store::AppStore;
use crate::views::{avatar_badge, panel_card};
use crate::widgets::{AppBadge, AppButton};

const PAGE_SIZE: usize = 25;

pub struct PluginMarketView {
    store: Entity<AppStore>,
    search_input: Entity<InputState>,
}

impl PluginMarketView {
    pub fn new(store: Entity<AppStore>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search_input =
            cx.new(|cx| InputState::new(window, cx).placeholder(t("搜索插件名称、描述、作者或标签")));
        let store_search = store.clone();
        cx.subscribe(&search_input, move |_, inp, ev: &InputEvent, cx| {
            if matches!(ev, InputEvent::Change) {
                let q = inp.read(cx).value().to_string();
                store_search.update(cx, |s, cx| {
                    s.plugin_query = q;
                    s.plugin_page = 0;
                    cx.notify();
                });
            }
        })
        .detach();
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        Self {
            store,
            search_input,
        }
    }
}

impl Render for PluginMarketView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let data = self.store.read(cx);
        let source = data.plugin_source;
        let filter = data.plugin_filter;
        let query = data.plugin_query.clone();
        let loading = data.plugin_loading;
        let installing_id = data.plugin_installing_id.clone();

        let catalog = match source {
            MarketSource::Public => data.plugin_catalog_public.clone(),
            MarketSource::Full => data.plugin_catalog_full.clone(),
        };
        let all_plugins = catalog.as_ref().map(|c| c.plugins.clone()).unwrap_or_default();
        let filtered = filter_market_plugins(&all_plugins, &query, filter);

        let total_pages = filtered.len().div_ceil(PAGE_SIZE).max(1);
        let cur_page = data.plugin_page.min(total_pages.saturating_sub(1));
        let visible: Vec<_> = filtered
            .iter()
            .skip(cur_page * PAGE_SIZE)
            .take(PAGE_SIZE)
            .cloned()
            .collect();

        let store_src_pub = self.store.clone();
        let store_src_full = self.store.clone();
        let store_refresh = self.store.clone();
        let store_prev = self.store.clone();
        let store_next = self.store.clone();

        let filter_btns: Vec<AnyElement> = [
            (MarketFilter::All, t("全部")),
            (MarketFilter::Available, t("未安装")),
            (MarketFilter::Installed, t("已安装")),
            (MarketFilter::Updates, t("可更新")),
            (MarketFilter::Auth, t("需授权")),
        ]
        .into_iter()
        .map(|(f, lbl)| {
            let st = self.store.clone();
            AppButton::new(SharedString::from(format!("pm-flt-{f:?}")))
                .outline()
                .small()
                .selected(filter == f)
                .label(lbl)
                .on_click(move |_, _, cx| {
                    st.update(cx, |s, cx| {
                        s.plugin_filter = f;
                        s.plugin_page = 0;
                        cx.notify();
                    });
                })
                .into_any_element()
        })
        .collect();

        let list_body: AnyElement = if visible.is_empty() {
            div()
                .p_6()
                .w_full()
                .rounded(px(10.))
                .bg(cx.theme().secondary.opacity(0.24))
                .border_1()
                .border_color(cx.theme().border)
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(if loading {
                    t("正在同步 Codex 插件市场索引…")
                } else {
                    t("当前筛选条件下没有匹配的插件。")
                })
                .into_any_element()
        } else {
            let rows: Vec<AnyElement> = visible
                .into_iter()
                .map(|pl| {
                    let is_busy = installing_id.as_deref() == Some(pl.id.as_str());
                    let pid = pl.id.clone();
                    let store_inst = self.store.clone();
                    let title = if pl.display_name.trim().is_empty() {
                        pl.name.clone()
                    } else {
                        pl.display_name.clone()
                    };
                    let initial = title
                        .chars()
                        .next()
                        .map(|c| c.to_uppercase().to_string())
                        .unwrap_or_else(|| "P".to_string());

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
                                .h_flex()
                                .items_center()
                                .gap_3()
                                .flex_1()
                                .min_w_0()
                                .child(avatar_badge(cx, initial))
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
                                                        .child(title),
                                                )
                                                .when(!pl.version.is_empty(), |d| {
                                                    d.child(AppBadge::outline(format!(
                                                        "v{}",
                                                        pl.version
                                                    )))
                                                })
                                                .when(pl.installed, |d| {
                                                    d.child(AppBadge::good(t("已安装")))
                                                })
                                                .when(pl.requires_auth, |d| {
                                                    d.child(AppBadge::info(t("OAuth")))
                                                })
                                                .when(pl.bytes > 0, |d| {
                                                    d.child(AppBadge::secondary(
                                                        format_bytes_human(pl.bytes),
                                                    ))
                                                }),
                                        )
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(cx.theme().muted_foreground)
                                                .child(pl.description),
                                        ),
                                ),
                        )
                        .child(
                            AppButton::new(SharedString::from(format!("pm-inst-{}", pl.id)))
                                .primary()
                                .small()
                                .disabled(is_busy || loading)
                                .icon(IconName::Download)
                                .label(if is_busy {
                                    t("安装中…")
                                } else if pl.update_available {
                                    t("更新")
                                } else if pl.installed {
                                    t("重新安装")
                                } else {
                                    t("安装")
                                })
                                .on_click(move |_, _, cx| {
                                    let id = pid.clone();
                                    store_inst.update(cx, |s, cx| s.install_plugin_item(id, cx));
                                }),
                        )
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
                            .child(
                                AppButton::new("pm-src-public")
                                    .outline()
                                    .small()
                                    .selected(source == MarketSource::Public)
                                    .label(t("官方精选目录 (Public)"))
                                    .on_click(move |_, _, cx| {
                                        store_src_pub.update(cx, |s, cx| {
                                            if s.plugin_catalog_public.is_some() {
                                                s.plugin_source = MarketSource::Public;
                                                s.plugin_page = 0;
                                                cx.notify();
                                            } else {
                                                s.load_plugin_market(
                                                    MarketSource::Public,
                                                    false,
                                                    cx,
                                                );
                                            }
                                        });
                                    }),
                            )
                            .child(
                                AppButton::new("pm-src-full")
                                    .outline()
                                    .small()
                                    .selected(source == MarketSource::Full)
                                    .label(t("完整仓库目录 (Full)"))
                                    .on_click(move |_, _, cx| {
                                        store_src_full.update(cx, |s, cx| {
                                            if s.plugin_catalog_full.is_some() {
                                                s.plugin_source = MarketSource::Full;
                                                s.plugin_page = 0;
                                                cx.notify();
                                            } else {
                                                s.load_plugin_market(MarketSource::Full, false, cx);
                                            }
                                        });
                                    }),
                            ),
                    )
                    .child(
                        AppButton::new("pm-refresh-btn")
                            .primary()
                            .small()
                            .disabled(loading || installing_id.is_some())
                            .icon(IconName::RefreshCw)
                            .label(if loading {
                                t("刷新中…")
                            } else {
                                t("刷新插件目录")
                            })
                            .on_click(move |_, _, cx| {
                                store_refresh.update(cx, |s, cx| {
                                    let src = s.plugin_source;
                                    s.load_plugin_market(src, true, cx);
                                });
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
                            .gap_3()
                            .child(
                                div().flex_1().min_w_0().child(
                                    Input::new(&self.search_input).prefix(
                                        Icon::new(IconName::Search)
                                            .with_size(gpui_kit::component::Size::Size(px(14.)))
                                            .text_color(cx.theme().muted_foreground),
                                    ),
                                ),
                            )
                            .child(
                                div()
                                    .h_flex()
                                    .flex_shrink_0()
                                    .items_center()
                                    .gap_1p5()
                                    .children(filter_btns),
                            ),
                    )
                    .child(list_body)
                    .when(filtered.len() > PAGE_SIZE, |d| {
                        d.child(
                            div()
                                .h_flex()
                                .w_full()
                                .items_center()
                                .justify_between()
                                .pt_2()
                                .border_t_1()
                                .border_color(cx.theme().border)
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(format!(
                                            "{} {} · {} {} / {}",
                                            t("共"),
                                            filtered.len(),
                                            t("页"),
                                            cur_page + 1,
                                            total_pages
                                        )),
                                )
                                .child(
                                    div()
                                        .h_flex()
                                        .gap_2()
                                        .child(
                                            AppButton::new("pm-prev-page")
                                                .secondary()
                                                .small()
                                                .disabled(cur_page == 0)
                                                .label(t("上一页"))
                                                .on_click(move |_, _, cx| {
                                                    store_prev.update(cx, |s, cx| {
                                                        s.plugin_page = cur_page.saturating_sub(1);
                                                        cx.notify();
                                                    });
                                                }),
                                        )
                                        .child(
                                            AppButton::new("pm-next-page")
                                                .secondary()
                                                .small()
                                                .disabled(cur_page + 1 >= total_pages)
                                                .label(t("下一页"))
                                                .on_click(move |_, _, cx| {
                                                    store_next.update(cx, |s, cx| {
                                                        s.plugin_page =
                                                            (cur_page + 1).min(total_pages - 1);
                                                        cx.notify();
                                                    });
                                                }),
                                        ),
                                ),
                        )
                    }),
            ))
    }
}
