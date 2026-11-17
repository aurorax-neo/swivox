use swivox_backend::i18n::{Language, t, tf};
use swivox_backend::phase5_rules::format_bytes_human;
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
        let ag_desktop_running = data
            .ag_snapshot
            .as_ref()
            .is_some_and(|s| s.desktop_install.running);
        let ag_ide_running = data
            .ag_snapshot
            .as_ref()
            .is_some_and(|s| s.ide_install.running);
        let ag_pending = data.ag_launch_pending;
        let ag_conflict = data.ag_launch_conflict;

        let store_stop = self.store.clone();
        let store_launch = self.store.clone();
        let store_ag_desk_stop = self.store.clone();
        let store_ag_desk_launch = self.store.clone();
        let store_ag_ide_stop = self.store.clone();
        let store_ag_ide_launch = self.store.clone();
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
                let desk_status_str = if ag_desktop_running { "running" } else { "stopped" };
                let desk_badge = if ag_desktop_running {
                    AppBadge::good(format!("{} {}", t("桌面版"), status_label(desk_status_str)))
                } else {
                    AppBadge::warn(format!("{} {}", t("桌面版"), status_label(desk_status_str)))
                };
                let ide_status_str = if ag_ide_running { "running" } else { "stopped" };
                let ide_badge = if ag_ide_running {
                    AppBadge::good(format!("IDE {}", status_label(ide_status_str)))
                } else {
                    AppBadge::warn(format!("IDE {}", status_label(ide_status_str)))
                };

                bar.child(
                    div()
                        .h_flex()
                        .flex_shrink_0()
                        .items_center()
                        .gap_2()
                        .when(ag_conflict, |row| {
                            row.child(
                                AppButton::new("topbar-clean-restart-ag")
                                    .danger()
                                    .small()
                                    .disabled(ag_pending)
                                    .icon(IconName::RotateCcw)
                                    .label(t("清理冲突"))
                                    .on_click(move |_, _, cx| {
                                        store_ag_clean.update(cx, |s, cx| {
                                            s.launch_antigravity_by_target("desktop", true, cx)
                                        });
                                    }),
                            )
                        })
                        .child(desk_badge)
                        .when(ag_desktop_running, |row| {
                            row.child(
                                AppButton::new("topbar-stop-ag-desktop")
                                    .outline()
                                    .small()
                                    .disabled(ag_pending)
                                    .icon(IconName::PowerOff)
                                    .label(t("停止桌面版"))
                                    .on_click(move |_, _, cx| {
                                        store_ag_desk_stop.update(cx, |s, cx| {
                                            s.stop_antigravity_by_target("desktop", cx)
                                        });
                                    }),
                            )
                        })
                        .child(
                            AppButton::new("topbar-launch-ag-desktop")
                                .primary()
                                .small()
                                .disabled(ag_pending)
                                .icon(IconName::Rocket)
                                .label(if ag_desktop_running {
                                    t("重启桌面版")
                                } else {
                                    t("启动桌面版")
                                })
                                .on_click(move |_, _, cx| {
                                    store_ag_desk_launch.update(cx, |s, cx| {
                                        s.launch_antigravity_by_target(
                                            "desktop",
                                            ag_desktop_running,
                                            cx,
                                        );
                                    });
                                }),
                        )
                        .child(
                            div()
                                .w(px(1.))
                                .h(px(18.))
                                .mx_0p5()
                                .bg(cx.theme().border),
                        )
                        .child(ide_badge)
                        .when(ag_ide_running, |row| {
                            row.child(
                                AppButton::new("topbar-stop-ag-ide")
                                    .outline()
                                    .small()
                                    .disabled(ag_pending)
                                    .icon(IconName::PowerOff)
                                    .label(t("停止 IDE"))
                                    .on_click(move |_, _, cx| {
                                        store_ag_ide_stop.update(cx, |s, cx| {
                                            s.stop_antigravity_by_target("ide", cx)
                                        });
                                    }),
                            )
                        })
                        .child(
                            AppButton::new("topbar-launch-ag-ide")
                                .secondary()
                                .small()
                                .disabled(ag_pending)
                                .icon(IconName::Rocket)
                                .label(if ag_ide_running {
                                    t("重启 IDE")
                                } else {
                                    t("启动 IDE")
                                })
                                .on_click(move |_, _, cx| {
                                    store_ag_ide_launch.update(cx, |s, cx| {
                                        s.launch_antigravity_by_target("ide", ag_ide_running, cx);
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

    fn render_confirm_modal_overlay(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let data = self.store.read(cx);

        // 1. Codex 官方账号切换 / 重新应用确认弹窗
        if let Some(target) = data.use_confirm_account.clone() {
            let email = target.email.clone();
            let is_reapply = target.is_active;
            let id_only = target.id.clone();
            let id_restart = target.id.clone();
            let st_dismiss = self.store.clone();
            let st_cancel = self.store.clone();
            let st_only = self.store.clone();
            let st_restart = self.store.clone();

            let title = if is_reapply {
                t("重新应用官方账号")
            } else {
                t("切换官方账号")
            };
            let body = tf("请选择官方账号「{0}」的生效方式：", &[&email]);

            return Some(views::modal_dialog_overlay(
                cx,
                "modal-use-official-account",
                title,
                body,
                false,
                move |_, cx| {
                    st_dismiss.update(cx, |s, cx| s.dismiss_all_confirm_modals(cx));
                },
                vec![
                    AppButton::new("confirm-use-cancel")
                        .outline()
                        .small()
                        .label(t("取消"))
                        .on_click(move |_, _, cx| {
                            st_cancel.update(cx, |s, cx| {
                                s.use_confirm_account = None;
                                cx.notify();
                            });
                        })
                        .into_any_element(),
                    AppButton::new("confirm-use-only")
                        .secondary()
                        .small()
                        .icon(IconName::CircleCheck)
                        .label(t("仅切换（重启生效）"))
                        .on_click(move |_, _, cx| {
                            let id = id_only.clone();
                            st_only.update(cx, |s, cx| s.switch_official_account(id, false, cx));
                        })
                        .into_any_element(),
                    AppButton::new("confirm-use-and-restart")
                        .primary()
                        .small()
                        .icon(IconName::Rocket)
                        .label(t("切换并重启 Codex"))
                        .on_click(move |_, _, cx| {
                            let id = id_restart.clone();
                            st_restart.update(cx, |s, cx| s.switch_official_account(id, true, cx));
                        })
                        .into_any_element(),
                ],
            ));
        }

        // 2. Codex 官方账号删除确认弹窗
        if let Some(target) = data.delete_confirm_account.clone() {
            let email = target.email.clone();
            let st_dismiss = self.store.clone();
            let st_cancel = self.store.clone();
            let st_exec = self.store.clone();

            return Some(views::modal_dialog_overlay(
                cx,
                "modal-delete-official-account",
                t("删除官方账号"),
                tf("确定删除官方账号「{0}」吗？删除后将从本地账号列表中移除。", &[&email]),
                true,
                move |_, cx| {
                    st_dismiss.update(cx, |s, cx| s.dismiss_all_confirm_modals(cx));
                },
                vec![
                    AppButton::new("confirm-del-cancel")
                        .outline()
                        .small()
                        .label(t("取消"))
                        .on_click(move |_, _, cx| {
                            st_cancel.update(cx, |s, cx| {
                                s.delete_confirm_account = None;
                                cx.notify();
                            });
                        })
                        .into_any_element(),
                    AppButton::new("confirm-del-execute")
                        .danger()
                        .small()
                        .icon(IconName::Trash)
                        .label(t("确认删除"))
                        .on_click(move |_, _, cx| {
                            let acc = target.clone();
                            st_exec.update(cx, |s, cx| s.delete_official_account(acc, cx));
                        })
                        .into_any_element(),
                ],
            ));
        }

        // 3. Codex 供应商配置切换 / 重新应用确认弹窗
        if let Some(target_id) = data.use_confirm_relay_id.clone() {
            let is_reapply = data.settings.active_relay_id == target_id;
            let display_name = data
                .settings
                .relay_profiles
                .iter()
                .find(|p| p.id == target_id)
                .map(|p| {
                    if p.name.trim().is_empty() {
                        p.id.clone()
                    } else {
                        p.name.clone()
                    }
                })
                .unwrap_or_else(|| target_id.clone());
            let id_only = target_id.clone();
            let id_restart = target_id;
            let st_dismiss = self.store.clone();
            let st_cancel = self.store.clone();
            let st_only = self.store.clone();
            let st_restart = self.store.clone();

            let title = if is_reapply {
                t("重新应用供应商配置")
            } else {
                t("切换供应商配置")
            };
            let body = tf("请选择供应商「{0}」的生效方式：", &[&display_name]);

            return Some(views::modal_dialog_overlay(
                cx,
                "modal-use-relay-profile",
                title,
                body,
                false,
                move |_, cx| {
                    st_dismiss.update(cx, |s, cx| s.dismiss_all_confirm_modals(cx));
                },
                vec![
                    AppButton::new("relay-use-cancel")
                        .outline()
                        .small()
                        .label(t("取消"))
                        .on_click(move |_, _, cx| {
                            st_cancel.update(cx, |s, cx| {
                                s.use_confirm_relay_id = None;
                                cx.notify();
                            });
                        })
                        .into_any_element(),
                    AppButton::new("relay-use-only")
                        .secondary()
                        .small()
                        .icon(IconName::CircleCheck)
                        .label(t("仅切换（重启生效）"))
                        .on_click(move |_, _, cx| {
                            let id = id_only.clone();
                            st_only.update(cx, |s, cx| s.switch_relay_profile(id, false, cx));
                        })
                        .into_any_element(),
                    AppButton::new("relay-use-restart")
                        .primary()
                        .small()
                        .icon(IconName::Rocket)
                        .label(t("切换并重启 Codex"))
                        .on_click(move |_, _, cx| {
                            let id = id_restart.clone();
                            st_restart.update(cx, |s, cx| s.switch_relay_profile(id, true, cx));
                        })
                        .into_any_element(),
                ],
            ));
        }

        // 4. Codex 供应商删除确认弹窗
        if let Some(del_id) = data.delete_confirm_relay_id.clone() {
            let display_name = data
                .settings
                .relay_profiles
                .iter()
                .find(|p| p.id == del_id)
                .map(|p| {
                    if p.name.trim().is_empty() {
                        p.id.clone()
                    } else {
                        p.name.clone()
                    }
                })
                .unwrap_or_else(|| del_id.clone());
            let st_dismiss = self.store.clone();
            let st_cancel = self.store.clone();
            let st_del = self.store.clone();

            return Some(views::modal_dialog_overlay(
                cx,
                "modal-delete-relay-profile",
                t("删除供应商"),
                tf("确定删除供应商「{0}」吗？", &[&display_name]),
                true,
                move |_, cx| {
                    st_dismiss.update(cx, |s, cx| s.dismiss_all_confirm_modals(cx));
                },
                vec![
                    AppButton::new("relay-del-cancel")
                        .outline()
                        .small()
                        .label(t("取消"))
                        .on_click(move |_, _, cx| {
                            st_cancel.update(cx, |s, cx| {
                                s.delete_confirm_relay_id = None;
                                cx.notify();
                            });
                        })
                        .into_any_element(),
                    AppButton::new("relay-del-confirm")
                        .danger()
                        .small()
                        .icon(IconName::Trash)
                        .label(t("确认删除"))
                        .on_click(move |_, _, cx| {
                            let id = del_id.clone();
                            st_del.update(cx, |s, cx| s.delete_relay(id, cx));
                        })
                        .into_any_element(),
                ],
            ));
        }

        // 5. Antigravity 账号切换 / 重新应用确认弹窗
        if let Some(target) = data.ag_use_confirm_account.clone() {
            let email = target.email.clone();
            let is_reapply = target.is_active;
            let id_write = target.id.clone();
            let id_desk = target.id.clone();
            let id_ide = target.id.clone();
            let st_dismiss = self.store.clone();
            let st_cancel = self.store.clone();
            let st_write = self.store.clone();
            let st_desk = self.store.clone();
            let st_ide = self.store.clone();

            let title = if is_reapply {
                t("重新应用 Antigravity 账号")
            } else {
                t("切换 Antigravity 账号")
            };
            let body = tf(
                "请选择 Antigravity 账号「{0}」的生效方式（将同步写入独立设备指纹、state.vscdb 与系统钥匙串）：",
                &[&email],
            );

            return Some(views::modal_dialog_overlay(
                cx,
                "modal-use-ag-account",
                title,
                body,
                false,
                move |_, cx| {
                    st_dismiss.update(cx, |s, cx| s.dismiss_all_confirm_modals(cx));
                },
                vec![
                    AppButton::new("ag-confirm-use-cancel")
                        .outline()
                        .small()
                        .label(t("取消"))
                        .on_click(move |_, _, cx| {
                            st_cancel.update(cx, |s, cx| {
                                s.ag_use_confirm_account = None;
                                cx.notify();
                            });
                        })
                        .into_any_element(),
                    AppButton::new("ag-confirm-use-write-only")
                        .secondary()
                        .small()
                        .icon(IconName::CircleCheck)
                        .label(t("仅写入凭据（重启生效）"))
                        .on_click(move |_, _, cx| {
                            let id = id_write.clone();
                            st_write.update(cx, |s, cx| {
                                s.switch_antigravity_account_with_mode(id, "write_only", cx)
                            });
                        })
                        .into_any_element(),
                    AppButton::new("ag-confirm-use-ide")
                        .outline()
                        .small()
                        .icon(IconName::Rocket)
                        .label(t("切换并重启 IDE"))
                        .on_click(move |_, _, cx| {
                            let id = id_ide.clone();
                            st_ide.update(cx, |s, cx| {
                                s.switch_antigravity_account_with_mode(id, "ide_restart", cx)
                            });
                        })
                        .into_any_element(),
                    AppButton::new("ag-confirm-use-desktop")
                        .primary()
                        .small()
                        .icon(IconName::Zap)
                        .label(t("切换并热重载桌面版"))
                        .on_click(move |_, _, cx| {
                            let id = id_desk.clone();
                            st_desk.update(cx, |s, cx| {
                                s.switch_antigravity_account_with_mode(id, "desktop_hot", cx)
                            });
                        })
                        .into_any_element(),
                ],
            ));
        }

        // 6. Antigravity 账号删除确认弹窗
        if let Some(del_id) = data.ag_delete_confirm_id.clone() {
            let email = data
                .ag_snapshot
                .as_ref()
                .and_then(|s| s.accounts.iter().find(|a| a.id == del_id))
                .map(|a| a.email.clone())
                .unwrap_or_else(|| del_id.clone());
            let st_dismiss = self.store.clone();
            let st_cancel = self.store.clone();
            let st_exec = self.store.clone();

            return Some(views::modal_dialog_overlay(
                cx,
                "modal-delete-ag-account",
                t("删除 Antigravity 账号"),
                tf(
                    "确定从 Swivox 移除 Antigravity 账号「{0}」吗？",
                    &[&email],
                ),
                true,
                move |_, cx| {
                    st_dismiss.update(cx, |s, cx| s.dismiss_all_confirm_modals(cx));
                },
                vec![
                    AppButton::new("ag-del-cancel")
                        .outline()
                        .small()
                        .label(t("取消"))
                        .on_click(move |_, _, cx| {
                            st_cancel.update(cx, |s, cx| {
                                s.ag_delete_confirm_id = None;
                                cx.notify();
                            });
                        })
                        .into_any_element(),
                    AppButton::new("ag-del-confirm")
                        .danger()
                        .small()
                        .icon(IconName::Trash)
                        .label(t("确认删除"))
                        .on_click(move |_, _, cx| {
                            let id = del_id.clone();
                            st_exec.update(cx, |s, cx| s.delete_antigravity_account(id, cx));
                        })
                        .into_any_element(),
                ],
            ));
        }

        // 7. 设置页恢复默认设置确认弹窗
        if data.reset_confirm_open {
            let st_dismiss = self.store.clone();
            let st_cancel = self.store.clone();
            let st_exec = self.store.clone();

            return Some(views::modal_dialog_overlay(
                cx,
                "modal-reset-settings",
                t("确认恢复默认设置？"),
                t("此操作会将增强开关与常规偏好重置为出厂默认值（已保存的账号凭据与代理列表不受影响）。"),
                true,
                move |_, cx| {
                    st_dismiss.update(cx, |s, cx| s.dismiss_all_confirm_modals(cx));
                },
                vec![
                    AppButton::new("set-reset-cancel")
                        .outline()
                        .small()
                        .label(t("取消"))
                        .on_click(move |_, _, cx| {
                            st_cancel.update(cx, |s, cx| {
                                s.reset_confirm_open = false;
                                cx.notify();
                            });
                        })
                        .into_any_element(),
                    AppButton::new("set-reset-exec")
                        .danger()
                        .small()
                        .icon(IconName::Trash)
                        .label(t("确认重置"))
                        .on_click(move |_, _, cx| {
                            st_exec.update(cx, |s, cx| s.reset_all_settings_now(cx));
                        })
                        .into_any_element(),
                ],
            ));
        }

        // 8. 本地会话删除确认弹窗
        if let Some(sess) = data.session_delete_confirm.clone() {
            let title_disp = if sess.title.trim().is_empty() {
                sess.id.clone()
            } else {
                sess.title.clone()
            };
            let st_dismiss = self.store.clone();
            let st_cancel = self.store.clone();
            let st_exec = self.store.clone();

            return Some(views::modal_dialog_overlay(
                cx,
                "modal-delete-session",
                format!("{}：「{title_disp}」", t("确认删除本地会话")),
                t("删除前将自动归档至本地备份存储，并同步移除 SQLite 与 session_index 记录。"),
                true,
                move |_, cx| {
                    st_dismiss.update(cx, |s, cx| s.dismiss_all_confirm_modals(cx));
                },
                vec![
                    AppButton::new("sess-del-cancel")
                        .outline()
                        .small()
                        .label(t("取消"))
                        .on_click(move |_, _, cx| {
                            st_cancel.update(cx, |s, cx| {
                                s.session_delete_confirm = None;
                                cx.notify();
                            });
                        })
                        .into_any_element(),
                    AppButton::new("sess-del-ok")
                        .danger()
                        .small()
                        .icon(IconName::Trash)
                        .label(t("确认删除"))
                        .on_click(move |_, _, cx| {
                            let target = sess.clone();
                            st_exec.update(cx, |s, cx| s.confirm_delete_session(target, cx));
                        })
                        .into_any_element(),
                ],
            ));
        }

        // 9. Agent 缓存清理确认弹窗
        if data.cache_confirm_open {
            let (selected_files, selected_bytes) = data
                .cache_report
                .as_ref()
                .map(|r| {
                    r.groups
                        .iter()
                        .filter(|g| data.cache_selected_ids.contains(&g.id))
                        .fold((0usize, 0u64), |(fc, bc), g| {
                            (fc + g.eligible_files, bc + g.eligible_bytes)
                        })
                })
                .unwrap_or((0, 0));
            let st_dismiss = self.store.clone();
            let st_cancel = self.store.clone();
            let st_exec = self.store.clone();

            return Some(views::modal_dialog_overlay(
                cx,
                "modal-clean-cache",
                t("确认执行缓存清理"),
                tf(
                    "即将清理 {0} 个过期缓存文件，预计释放 {1}（自动保留最近 24 小时内的活跃文件，绝不触碰会话与凭据）。",
                    &[
                        &selected_files.to_string(),
                        &format_bytes_human(selected_bytes),
                    ],
                ),
                true,
                move |_, cx| {
                    st_dismiss.update(cx, |s, cx| s.dismiss_all_confirm_modals(cx));
                },
                vec![
                    AppButton::new("cache-confirm-cancel")
                        .outline()
                        .small()
                        .label(t("取消"))
                        .on_click(move |_, _, cx| {
                            st_cancel.update(cx, |s, cx| {
                                s.cache_confirm_open = false;
                                cx.notify();
                            });
                        })
                        .into_any_element(),
                    AppButton::new("cache-confirm-exec")
                        .danger()
                        .small()
                        .icon(IconName::Trash)
                        .label(t("确认清理"))
                        .on_click(move |_, _, cx| {
                            st_exec.update(cx, |s, cx| s.clean_selected_agent_cache(cx));
                        })
                        .into_any_element(),
                ],
            ));
        }

        None
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
        let confirm_modal = self.render_confirm_modal_overlay(cx);
        let canvas_bg = if cx.theme().is_dark() {
            cx.theme().background
        } else {
            hsla(220.0 / 360.0, 0.25, 0.968, 1.0)
        };

        div()
            .relative()
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
            .when_some(confirm_modal, |root, modal| root.child(modal))
    }
}
