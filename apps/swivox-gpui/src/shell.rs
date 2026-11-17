use swivox_backend::i18n::{Language, t};
use swivox_backend::ui_prefs::UiThemeMode;
use gpui_kit::assets::IconName;
use gpui_kit::component::Icon;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::theme::ActiveTheme as _;
use gpui_kit::component::{Sizable as _, StyledExt as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::routes::{
    GLOBAL_NAVIGATION_ROUTES, NAVIGATION_SECTIONS, Route, SectionPlacement, StatusTone,
    status_label, status_tone, tool_tooltip,
};
use crate::store::AppStore;
use crate::views;
use crate::views::antigravity_accounts::AntigravityAccountsView;
use crate::views::antigravity_enhance::AntigravityEnhanceView;
use crate::views::enhance::EnhanceView;
use crate::views::lamplighter::LamplighterView;
use crate::views::maintenance::MaintenanceView;
use crate::views::mcp::McpView;
use crate::views::official_accounts::OfficialAccountsView;
use crate::views::plugin_market::PluginMarketView;
use crate::views::relay::RelayView;
use crate::views::sessions::SessionsView;
use crate::views::settings::SettingsView;
use crate::views::skills::SkillsView;
use crate::widgets::{AppBadge, AppButton};

const CHATGPT_AGENT_SVG: &[u8] = include_bytes!("../assets/chatgpt.svg");
const ANTIGRAVITY_AGENT_SVG: &[u8] = include_bytes!("../assets/antigravity.svg");

pub struct ManagerShell {
    store: Entity<AppStore>,
    last_lang: Language,
    official_accounts_view: Entity<OfficialAccountsView>,
    antigravity_accounts_view: Entity<AntigravityAccountsView>,
    antigravity_enhance_view: Entity<AntigravityEnhanceView>,
    lamplighter_view: Entity<LamplighterView>,
    relay_view: Entity<RelayView>,
    mcp_view: Entity<McpView>,
    skills_view: Entity<SkillsView>,
    sessions_view: Entity<SessionsView>,
    enhance_view: Entity<EnhanceView>,
    plugin_market_view: Entity<PluginMarketView>,
    maintenance_view: Entity<MaintenanceView>,
    settings_view: Entity<SettingsView>,
}

impl ManagerShell {
    pub fn new(store: Entity<AppStore>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let initial_lang = store.read(cx).prefs.language;
        let official_accounts_view =
            cx.new(|cx| OfficialAccountsView::new(store.clone(), window, cx));
        let antigravity_accounts_view =
            cx.new(|cx| AntigravityAccountsView::new(store.clone(), window, cx));
        let antigravity_enhance_view =
            cx.new(|cx| AntigravityEnhanceView::new(store.clone(), window, cx));
        let lamplighter_view = cx.new(|cx| LamplighterView::new(store.clone(), window, cx));
        let relay_view = cx.new(|cx| RelayView::new(store.clone(), window, cx));
        let mcp_view = cx.new(|cx| McpView::new(store.clone(), window, cx));
        let skills_view = cx.new(|cx| SkillsView::new(store.clone(), window, cx));
        let sessions_view = cx.new(|cx| SessionsView::new(store.clone(), window, cx));
        let enhance_view = cx.new(|cx| EnhanceView::new(store.clone(), window, cx));
        let plugin_market_view = cx.new(|cx| PluginMarketView::new(store.clone(), window, cx));
        let maintenance_view = cx.new(|cx| MaintenanceView::new(store.clone(), window, cx));
        let settings_view = cx.new(|cx| SettingsView::new(store.clone(), window, cx));
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        Self {
            store,
            last_lang: initial_lang,
            official_accounts_view,
            antigravity_accounts_view,
            antigravity_enhance_view,
            lamplighter_view,
            relay_view,
            mcp_view,
            skills_view,
            sessions_view,
            enhance_view,
            plugin_market_view,
            maintenance_view,
            settings_view,
        }
    }

    fn rebuild_localized_views(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let store = self.store.clone();
        self.official_accounts_view =
            cx.new(|cx| OfficialAccountsView::new(store.clone(), window, cx));
        self.antigravity_accounts_view =
            cx.new(|cx| AntigravityAccountsView::new(store.clone(), window, cx));
        self.antigravity_enhance_view =
            cx.new(|cx| AntigravityEnhanceView::new(store.clone(), window, cx));
        self.lamplighter_view = cx.new(|cx| LamplighterView::new(store.clone(), window, cx));
        self.relay_view = cx.new(|cx| RelayView::new(store.clone(), window, cx));
        self.mcp_view = cx.new(|cx| McpView::new(store.clone(), window, cx));
        self.skills_view = cx.new(|cx| SkillsView::new(store.clone(), window, cx));
        self.sessions_view = cx.new(|cx| SessionsView::new(store.clone(), window, cx));
        self.enhance_view = cx.new(|cx| EnhanceView::new(store.clone(), window, cx));
        self.plugin_market_view = cx.new(|cx| PluginMarketView::new(store.clone(), window, cx));
        self.maintenance_view = cx.new(|cx| MaintenanceView::new(store.clone(), window, cx));
        self.settings_view = cx.new(|cx| SettingsView::new(store, window, cx));
    }

    fn render_application_rail(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let data = self.store.read(cx);
        let active_tool = data.active_tool.clone();
        let codex_running = data.codex_app_running();
        let ag_running = data.antigravity_app_running();
        let current_route = data.route;
        let show_tool_selection = !current_route.is_global();
        let theme_mode = data.prefs.theme;
        let lang = data.prefs.language;

        let tool_buttons: Vec<AnyElement> = data
            .tool_entries
            .iter()
            .map(|tool| {
                let id = tool.id.clone();
                let selected = show_tool_selection && tool.id == active_tool;
                let running = (tool.id == "codex" && codex_running)
                    || (tool.id == "antigravity" && ag_running);
                let tip = tool_tooltip(
                    &tool.name,
                    &tool.home_dir,
                    tool.switchable,
                    tool.relay_count,
                    running,
                );
                let store = self.store.clone();
                let switchable = tool.switchable;

                let icon_el = if tool.id == "codex" {
                    Icon::default().data(CHATGPT_AGENT_SVG)
                } else if tool.id == "antigravity" {
                    Icon::default().data(ANTIGRAVITY_AGENT_SVG)
                } else {
                    Icon::new(IconName::Bot)
                };

                div()
                    .relative()
                    .child(
                        AppButton::new(SharedString::from(format!("rail-tool-{}", tool.id)))
                            .ghost()
                            .rail_icon()
                            .selected(selected)
                            .disabled(!switchable)
                            .icon(icon_el)
                            .tooltip(tip)
                            .on_click(move |_, _, cx| {
                                let id = id.clone();
                                store.update(cx, |s, cx| s.switch_tool(id, cx));
                            }),
                    )
                    .when(running, |el| {
                        el.child(
                            div()
                                .absolute()
                                .right(px(3.))
                                .bottom(px(3.))
                                .size(px(7.))
                                .rounded_full()
                                .bg(cx.theme().success),
                        )
                    })
                    .into_any_element()
            })
            .collect();

        let global_nav_buttons: Vec<AnyElement> = GLOBAL_NAVIGATION_ROUTES
            .iter()
            .copied()
            .map(|route_id| {
                let selected = current_route == route_id;
                let store = self.store.clone();
                AppButton::new(SharedString::from(format!("rail-global-{route_id:?}")))
                    .ghost()
                    .rail_icon()
                    .selected(selected)
                    .icon(route_id.icon())
                    .tooltip(route_id.title())
                    .on_click(move |_, _, cx| {
                        store.update(cx, |s, cx| {
                            if s.route == route_id {
                                let back = s.last_workspace_route;
                                s.navigate(back, cx);
                            } else {
                                s.navigate(route_id, cx);
                            }
                        });
                    })
                    .into_any_element()
            })
            .collect();

        let language_label = if lang == Language::En {
            t("切换到中文")
        } else {
            t("切换到英文")
        };
        let theme_label = if theme_mode == UiThemeMode::Dark {
            t("切换到浅色")
        } else {
            t("切换到深色")
        };
        let theme_icon = if theme_mode == UiThemeMode::Dark {
            IconName::Sun
        } else {
            IconName::Moon
        };

        let store_lang = self.store.clone();
        let store_theme = self.store.clone();

        div()
            .v_flex()
            .w(px(56.))
            .h_full()
            .py_3()
            .px_2()
            .items_center()
            .justify_between()
            .bg(cx.theme().sidebar)
            .border_r_1()
            .border_color(cx.theme().sidebar_border)
            .child(div().v_flex().items_center().gap_2().children(tool_buttons))
            .child(
                div()
                    .v_flex()
                    .items_center()
                    .gap_2()
                    .children(global_nav_buttons)
                    .child(
                        div()
                            .w(px(28.))
                            .h(px(1.))
                            .my_1()
                            .bg(cx.theme().sidebar_border),
                    )
                    .child(
                        AppButton::new("rail-toggle-lang")
                            .ghost()
                            .rail_icon()
                            .icon(IconName::Languages)
                            .tooltip(language_label)
                            .on_click(move |_, _, cx| {
                                store_lang.update(cx, |s, cx| s.toggle_language(cx));
                            }),
                    )
                    .child(
                        AppButton::new("rail-toggle-theme")
                            .ghost()
                            .rail_icon()
                            .icon(theme_icon)
                            .tooltip(theme_label)
                            .on_click(move |_, _, cx| {
                                store_theme.update(cx, |s, cx| s.toggle_theme(cx));
                            }),
                    ),
            )
    }

    fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let data = self.store.read(cx);
        let active_tool = data.active_tool.clone();
        let current_route = data.route;

        let mut top_sections = Vec::new();
        let mut bottom_sections = Vec::new();

        for section in NAVIGATION_SECTIONS {
            let visible: Vec<Route> = section
                .routes
                .iter()
                .copied()
                .filter(|r| r.visible_for_tool(&active_tool))
                .collect();
            if visible.is_empty() {
                continue;
            }

            let items: Vec<AnyElement> = visible
                .into_iter()
                .map(|route_id| {
                    let selected = current_route == route_id;
                    let store = self.store.clone();
                    let bg = if selected {
                        cx.theme().sidebar_accent
                    } else {
                        cx.theme().transparent
                    };
                    let fg = if selected {
                        cx.theme().sidebar_accent_foreground
                    } else {
                        cx.theme().sidebar_foreground
                    };

                    div()
                        .id(SharedString::from(format!("nav-item-{route_id:?}")))
                        .h_flex()
                        .w_full()
                        .h(px(34.))
                        .items_center()
                        .gap_2p5()
                        .px_3()
                        .rounded(px(8.))
                        .cursor_pointer()
                        .bg(bg)
                        .hover(|s| s.bg(cx.theme().sidebar_accent.opacity(0.65)))
                        .on_click(move |_, _, cx| {
                            store.update(cx, |s, cx| s.navigate(route_id, cx));
                        })
                        .child(
                            div()
                                .flex()
                                .size(px(16.))
                                .items_center()
                                .justify_center()
                                .child(
                                    Icon::new(route_id.icon())
                                        .with_size(gpui_kit::component::Size::Size(px(15.)))
                                        .text_color(fg),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .text_size(px(13.))
                                .line_height(px(13.))
                                .when(selected, |d| d.font_medium())
                                .text_color(fg)
                                .child(route_id.title()),
                        )
                        .into_any_element()
                })
                .collect();

            let block = div()
                .v_flex()
                .w_full()
                .gap_1()
                .child(
                    div()
                        .px_3()
                        .py_1()
                        .text_xs()
                        .font_semibold()
                        .text_color(cx.theme().muted_foreground)
                        .child(t(section.label_zh)),
                )
                .children(items);

            match section.placement {
                SectionPlacement::Top => top_sections.push(block),
                SectionPlacement::Bottom => bottom_sections.push(block),
            }
        }

        div()
            .v_flex()
            .w(px(216.))
            .h_full()
            .p_3()
            .justify_between()
            .bg(cx.theme().sidebar)
            .border_r_1()
            .border_color(cx.theme().sidebar_border)
            .child(div().v_flex().w_full().gap_4().children(top_sections))
            .child(div().v_flex().w_full().gap_2().children(bottom_sections))
    }

    fn render_topbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let data = self.store.read(cx);
        let route = data.route;
        let show_codex_controls =
            data.active_tool == "codex" && !route.is_global() && route != Route::Lamplighter;
        let show_ag_controls =
            data.active_tool == "antigravity" && !route.is_global() && route != Route::Lamplighter;
        let codex_status = data
            .overview
            .as_ref()
            .map(|ov| ov.runtime_health.codex_app.status)
            .unwrap_or("not_checked");
        let codex_running = codex_status == "running";
        let controls_disabled = data.launch_pending || data.overview.is_none();
        let ag_running = data.antigravity_app_running();
        let ag_pending = data.ag_launch_pending;
        let ag_conflict = data.ag_launch_conflict;

        let store_stop = self.store.clone();
        let store_launch = self.store.clone();
        let store_ag_stop = self.store.clone();
        let store_ag_launch = self.store.clone();
        let store_ag_clean = self.store.clone();

        div()
            .h_flex()
            .w_full()
            .px_6()
            .py_4()
            .items_center()
            .justify_between()
            .gap_4()
            .bg(cx.theme().background)
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_1()
                    .child(
                        div()
                            .text_lg()
                            .font_semibold()
                            .text_color(cx.theme().foreground)
                            .child(route.title()),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(route.subtitle()),
                    ),
            )
            .when(show_codex_controls, |bar| {
                let badge_text = format!("Codex {}", status_label(codex_status));
                let badge = match status_tone(codex_status) {
                    StatusTone::Good => AppBadge::good(badge_text),
                    StatusTone::Bad => AppBadge::bad(badge_text),
                    StatusTone::Warn => AppBadge::warn(badge_text),
                };

                bar.child(
                    div()
                        .h_flex()
                        .flex_shrink_0()
                        .items_center()
                        .gap_2p5()
                        .child(badge)
                        .when(codex_running, |row| {
                            row.child(
                                AppButton::new("topbar-stop-codex")
                                    .secondary()
                                    .small()
                                    .disabled(controls_disabled)
                                    .icon(IconName::PowerOff)
                                    .label(t("停止 Codex"))
                                    .on_click(move |_, _, cx| {
                                        store_stop.update(cx, |s, cx| s.stop_codex(cx));
                                    }),
                            )
                        })
                        .child(
                            AppButton::new("topbar-launch-or-restart-codex")
                                .primary()
                                .small()
                                .disabled(controls_disabled)
                                .icon(IconName::Rocket)
                                .label(if codex_running {
                                    t("重启 Codex")
                                } else {
                                    t("启动 Codex")
                                })
                                .on_click(move |_, _, cx| {
                                    store_launch.update(cx, |s, cx| {
                                        if codex_running {
                                            s.restart_codex(cx);
                                        } else {
                                            s.launch_codex(cx);
                                        }
                                    });
                                }),
                        ),
                )
            })
            .when(show_ag_controls, |bar| {
                let status_str = if ag_running { "running" } else { "stopped" };
                let badge = if ag_running {
                    AppBadge::good(format!("Antigravity {}", status_label(status_str)))
                } else {
                    AppBadge::warn(format!("Antigravity {}", status_label(status_str)))
                };
                bar.child(
                    div()
                        .h_flex()
                        .flex_shrink_0()
                        .items_center()
                        .gap_2p5()
                        .child(badge)
                        .when(ag_running, |row| {
                            row.child(
                                AppButton::new("topbar-stop-ag")
                                    .secondary()
                                    .small()
                                    .disabled(ag_pending)
                                    .icon(IconName::PowerOff)
                                    .label(t("停止 Antigravity"))
                                    .on_click(move |_, _, cx| {
                                        store_ag_stop.update(cx, |s, cx| s.stop_antigravity(cx));
                                    }),
                            )
                        })
                        .when(ag_conflict, |row| {
                            row.child(
                                AppButton::new("topbar-clean-restart-ag")
                                    .danger()
                                    .small()
                                    .disabled(ag_pending)
                                    .icon(IconName::RotateCcw)
                                    .label(t("清理冲突并重启"))
                                    .on_click(move |_, _, cx| {
                                        store_ag_clean
                                            .update(cx, |s, cx| s.launch_antigravity(true, cx));
                                    }),
                            )
                        })
                        .child(
                            AppButton::new("topbar-launch-ag")
                                .primary()
                                .small()
                                .disabled(ag_pending)
                                .icon(IconName::Rocket)
                                .label(if ag_running {
                                    t("重启 Antigravity")
                                } else {
                                    t("免 TUN 启动 Antigravity")
                                })
                                .on_click(move |_, _, cx| {
                                    store_ag_launch.update(cx, |s, cx| {
                                        s.launch_antigravity(ag_running, cx);
                                    });
                                }),
                        ),
                )
            })
    }

    fn render_floating_toast(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let notice = self.store.read(cx).notice.clone()?;
        let store_dismiss = self.store.clone();
        let is_dark = cx.theme().is_dark();
        let (border_color, bg_color, icon_color, icon_name) = match notice.tone {
            StatusTone::Good => (
                cx.theme().success.opacity(0.65),
                if is_dark {
                    hsla(146.0 / 360.0, 0.34, 0.13, 0.97)
                } else {
                    hsla(146.0 / 360.0, 0.45, 0.95, 0.98)
                },
                cx.theme().success,
                IconName::CircleCheck,
            ),
            StatusTone::Bad => (
                cx.theme().danger.opacity(0.65),
                if is_dark {
                    hsla(0.0, 0.38, 0.14, 0.97)
                } else {
                    hsla(0.0, 0.55, 0.96, 0.98)
                },
                cx.theme().danger,
                IconName::CircleX,
            ),
            StatusTone::Warn => (
                cx.theme().warning.opacity(0.65),
                if is_dark {
                    hsla(38.0 / 360.0, 0.40, 0.14, 0.97)
                } else {
                    hsla(38.0 / 360.0, 0.65, 0.95, 0.98)
                },
                cx.theme().warning,
                IconName::Info,
            ),
        };

        Some(
            deferred(
                div()
                    .absolute()
                    .top(px(14.))
                    .left_0()
                    .right_0()
                    .h_flex()
                    .justify_center()
                    .px_6()
                    .child(
                        div()
                            .id("floating-toast-card")
                            .occlude()
                            .h_flex()
                            .min_w(px(320.))
                            .max_w(px(560.))
                            .items_center()
                            .justify_between()
                            .gap_3()
                            .px_4()
                            .py_2p5()
                            .rounded(px(10.))
                            .bg(bg_color)
                            .border_1()
                            .border_color(border_color)
                            .shadow_lg()
                            .child(
                                div()
                                    .h_flex()
                                    .flex_1()
                                    .min_w_0()
                                    .items_center()
                                    .gap_2p5()
                                    .child(Icon::new(icon_name).small().text_color(icon_color))
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_semibold()
                                            .text_color(cx.theme().foreground)
                                            .child(notice.title),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(notice.message),
                                    ),
                            )
                            .child(
                                AppButton::new("dismiss-floating-notice")
                                    .ghost()
                                    .small_icon()
                                    .icon(IconName::X)
                                    .on_click(move |_, _, cx| {
                                        store_dismiss.update(cx, |s, cx| s.clear_notice(cx));
                                    }),
                            ),
                    ),
            )
            .with_priority(10)
            .into_any_element(),
        )
    }

    fn render_active_screen(&self, cx: &mut Context<Self>) -> AnyElement {
        let route = self.store.read(cx).route;
        match route {
            Route::Overview => views::overview::render_overview(&self.store, cx).into_any_element(),
            Route::OfficialAccounts => self.official_accounts_view.clone().into_any_element(),
            Route::AntigravityAccounts => {
                self.antigravity_accounts_view.clone().into_any_element()
            }
            Route::AntigravityEnhance => {
                self.antigravity_enhance_view.clone().into_any_element()
            }
            Route::Lamplighter => self.lamplighter_view.clone().into_any_element(),
            Route::Relay => self.relay_view.clone().into_any_element(),
            Route::MultiLaunch => {
                views::multi_launch::render_multi_launch(&self.store, cx).into_any_element()
            }
            Route::Context => self.mcp_view.clone().into_any_element(),
            Route::Skills => self.skills_view.clone().into_any_element(),
            Route::Sessions => self.sessions_view.clone().into_any_element(),
            Route::Enhance => self.enhance_view.clone().into_any_element(),
            Route::UserScripts => {
                views::user_scripts::render_user_scripts(&self.store, cx).into_any_element()
            }
            Route::PluginMarket => self.plugin_market_view.clone().into_any_element(),
            Route::AgentCache => {
                views::agent_cache::render_agent_cache(&self.store, cx).into_any_element()
            }
            Route::Maintenance => self.maintenance_view.clone().into_any_element(),
            Route::About => views::about::render_about(&self.store, cx).into_any_element(),
            Route::Settings => self.settings_view.clone().into_any_element(),
            Route::RelayEnvironment => {
                views::relay_env::render_relay_environment(&self.store, cx).into_any_element()
            }
        }
    }
}

impl Render for ManagerShell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let current_lang = self.store.read(cx).prefs.language;
        if current_lang != self.last_lang {
            self.last_lang = current_lang;
            self.rebuild_localized_views(window, cx);
        }
        let floating_toast = self.render_floating_toast(cx);
        let canvas_bg = if cx.theme().is_dark() {
            cx.theme().background
        } else {
            hsla(220.0 / 360.0, 0.25, 0.968, 1.0)
        };

        div()
            .h_flex()
            .size_full()
            .bg(canvas_bg)
            .text_color(cx.theme().foreground)
            .child(self.render_application_rail(cx))
            .child(self.render_sidebar(cx))
            .child(
                div()
                    .relative()
                    .v_flex()
                    .flex_1()
                    .h_full()
                    .min_w_0()
                    .child(self.render_topbar(cx))
                    .child(
                        div()
                            .v_flex()
                            .flex_1()
                            .w_full()
                            .overflow_y_scrollbar()
                            .p_6()
                            .gap_4()
                            .child(self.render_active_screen(cx)),
                    )
                    .when_some(floating_toast, |pane, toast| pane.child(toast)),
            )
    }
}
