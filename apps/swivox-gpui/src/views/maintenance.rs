use swivox_backend::i18n::t;
use gpui_kit::assets::IconName;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::theme::ActiveTheme as _;
use gpui_kit::component::StyledExt as _;
use gpui_kit::*;

use crate::routes::status_label;
use crate::store::AppStore;
use crate::views::{health_item_row, panel_card};
use crate::widgets::{AppBadge, AppButton};

pub struct MaintenanceView {
    store: Entity<AppStore>,
    codex_path_input: Entity<InputState>,
}

impl MaintenanceView {
    pub fn new(store: Entity<AppStore>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let initial_path = store.read(cx).settings.codex_app_path.clone();
        let codex_path_input = cx.new(|cx| {
            let mut st = InputState::new(window, cx)
                .placeholder(t("留空自动探测 /Applications/Codex.app 或系统默认安装位置"));
            if !initial_path.is_empty() {
                st.set_value(initial_path, window, cx);
            }
            st
        });
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        Self {
            store,
            codex_path_input,
        }
    }
}

impl Render for MaintenanceView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let data = self.store.read(cx);
        let ov = data.overview.as_ref();
        let watcher_enabled = data
            .watcher_state
            .as_ref()
            .is_some_and(|w| w.enabled);
        let takeover = data.auto_takeover_state.clone();
        let takeover_enabled = data.settings.codex_app_auto_takeover;
        let takeover_policy = data.settings.codex_app_auto_takeover_when_active.clone();
        let repairing = data.shortcuts_repairing;
        let sync_busy = data.provider_sync_busy;

        let (silent_ok, silent_status, silent_path) = ov
            .map(|o| {
                let ok = matches!(o.silent_shortcut.status.as_str(), "found" | "ok");
                (
                    ok,
                    o.silent_shortcut.status.clone(),
                    o.silent_shortcut
                        .path
                        .clone()
                        .unwrap_or_else(|| t("未创建静默启动入口")),
                )
            })
            .unwrap_or((false, "not_checked".to_string(), t("正在检测…")));

        let (mgmt_ok, mgmt_status, mgmt_path) = ov
            .map(|o| {
                let ok = matches!(o.management_shortcut.status.as_str(), "found" | "ok");
                (
                    ok,
                    o.management_shortcut.status.clone(),
                    o.management_shortcut
                        .path
                        .clone()
                        .unwrap_or_else(|| t("未创建管理面板入口")),
                )
            })
            .unwrap_or((false, "not_checked".to_string(), t("正在检测…")));

        let store_repair = self.store.clone();
        let store_watcher = self.store.clone();
        let store_takeover_sw = self.store.clone();
        let store_policy_defer = self.store.clone();
        let store_policy_imm = self.store.clone();
        let store_takeover_now = self.store.clone();
        let store_takeover_pause = self.store.clone();
        let store_sync_hist = self.store.clone();

        let ent_pick_app = cx.entity();
        let ent_save_path = cx.entity();

        let takeover_state_str = takeover.as_ref().map(|tk| tk.state).unwrap_or("disabled");

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
                            .child(
                                div()
                                    .text_sm()
                                    .font_semibold()
                                    .text_color(cx.theme().foreground)
                                    .child(t("快捷入口与开机自启")),
                            )
                            .child(
                                AppButton::new("maint-repair-shortcuts")
                                    .primary()
                                    .small()
                                    .disabled(repairing)
                                    .icon(IconName::Wrench)
                                    .label(if repairing {
                                        t("修复中…")
                                    } else {
                                        t("检测并修复快捷方式")
                                    })
                                    .on_click(move |_, _, cx| {
                                        store_repair
                                            .update(cx, |s, cx| s.repair_shortcuts_now(cx));
                                    }),
                            ),
                    )
                    .child(health_item_row(
                        cx,
                        silent_ok,
                        t("静默启动快捷方式"),
                        silent_path,
                        &silent_status,
                    ))
                    .child(health_item_row(
                        cx,
                        mgmt_ok,
                        t("管理界面快捷方式"),
                        mgmt_path,
                        &mgmt_status,
                    ))
                    .child(
                        div()
                            .h_flex()
                            .w_full()
                            .items_center()
                            .justify_between()
                            .gap_4()
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
                                    .gap_0p5()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_medium()
                                            .text_color(cx.theme().foreground)
                                            .child(t("系统登录时后台自启守护")),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(t("开机自动拉起 Swivox 驻留托盘与接管守护")),
                                    ),
                            )
                            .child(
                                Switch::new("maint-watcher-sw")
                                    .checked(watcher_enabled)
                                    .on_change(move |val, _, cx| {
                                        let v = *val;
                                        store_watcher
                                            .update(cx, |s, cx| s.set_login_startup(v, cx));
                                    }),
                            ),
                    ),
            ))
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
                                    .h_flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_semibold()
                                            .text_color(cx.theme().foreground)
                                            .child(t("官方 Codex App 自动接管")),
                                    )
                                    .child(AppBadge::info(status_label(takeover_state_str))),
                            )
                            .child(
                                Switch::new("maint-takeover-sw")
                                    .checked(takeover_enabled)
                                    .on_change(move |val, _, cx| {
                                        let v = *val;
                                        store_takeover_sw.update(cx, |s, cx| {
                                            s.set_auto_takeover_enabled(v, cx)
                                        });
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(t("开启后当您直接从 Dock 或访达启动官方 Codex 时，Swivox 将自动平滑接管并挂载增强与本地协议代理。")),
                    )
                    .child(
                        div()
                            .h_flex()
                            .flex_wrap()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .child(
                                div()
                                    .h_flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        AppButton::new("maint-pol-defer")
                                            .outline()
                                            .small()
                                            .selected(takeover_policy != "immediate")
                                            .label(t("活跃时延后接管 (推荐)"))
                                            .on_click(move |_, _, cx| {
                                                store_policy_defer.update(cx, |s, cx| {
                                                    let mut next = s.settings.clone();
                                                    next.codex_app_auto_takeover_when_active =
                                                        "defer".to_string();
                                                    s.persist_settings_with_notice(
                                                        next,
                                                        t("自动接管策略"),
                                                        cx,
                                                    );
                                                });
                                            }),
                                    )
                                    .child(
                                        AppButton::new("maint-pol-imm")
                                            .outline()
                                            .small()
                                            .selected(takeover_policy == "immediate")
                                            .label(t("立即重启接管"))
                                            .on_click(move |_, _, cx| {
                                                store_policy_imm.update(cx, |s, cx| {
                                                    let mut next = s.settings.clone();
                                                    next.codex_app_auto_takeover_when_active =
                                                        "immediate".to_string();
                                                    s.persist_settings_with_notice(
                                                        next,
                                                        t("自动接管策略"),
                                                        cx,
                                                    );
                                                });
                                            }),
                                    ),
                            )
                            .child(
                                div()
                                    .h_flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        AppButton::new("maint-pause-takeover")
                                            .secondary()
                                            .small()
                                            .label(t("本轮暂停接管"))
                                            .on_click(move |_, _, cx| {
                                                store_takeover_pause.update(cx, |s, cx| {
                                                    s.pause_takeover_instance(cx)
                                                });
                                            }),
                                    )
                                    .child(
                                        AppButton::new("maint-now-takeover")
                                            .primary()
                                            .small()
                                            .icon(IconName::Rocket)
                                            .label(t("立即接管运行实例"))
                                            .on_click(move |_, _, cx| {
                                                store_takeover_now
                                                    .update(cx, |s, cx| s.trigger_takeover_now(cx));
                                            }),
                                    ),
                            ),
                    ),
            ))
            .child(panel_card(
                cx,
                div()
                    .v_flex()
                    .w_full()
                    .gap_3p5()
                    .child(
                        div()
                            .text_sm()
                            .font_semibold()
                            .text_color(cx.theme().foreground)
                            .child(t("Codex 应用路径与历史会话供应商同步")),
                    )
                    .child(
                        div()
                            .h_flex()
                            .w_full()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .child(Input::new(&self.codex_path_input)),
                            )
                            .child(
                                AppButton::new("maint-pick-app")
                                    .secondary()
                                    .small()
                                    .icon(IconName::FolderOpen)
                                    .label(t("浏览定位"))
                                    .on_click(move |_, window, cx| {
                                        let win_h = window.window_handle();
                                        let ent = ent_pick_app.clone();
                                        cx.spawn(async move |cx| {
                                            if let Some(dir) =
                                                rfd::AsyncFileDialog::new().pick_folder().await
                                            {
                                                let p = dir.path().to_string_lossy().to_string();
                                                cx.update(|app| {
                                                    let _ = win_h.update(app, |_, win, cx| {
                                                        ent.update(cx, |v, cx| {
                                                            v.codex_path_input.update(
                                                                cx,
                                                                |inp, cx| inp.set_value(p, win, cx),
                                                            );
                                                        });
                                                    });
                                                });
                                            }
                                        })
                                        .detach();
                                    }),
                            )
                            .child(
                                AppButton::new("maint-save-app-path")
                                    .primary()
                                    .small()
                                    .icon(IconName::Save)
                                    .label(t("保存路径"))
                                    .on_click(move |_, _, cx| {
                                        ent_save_path.update(cx, |v, cx| {
                                            let p = v
                                                .codex_path_input
                                                .read(cx)
                                                .value()
                                                .trim()
                                                .to_string();
                                            v.store.update(cx, |s, cx| {
                                                let mut next = s.settings.clone();
                                                next.codex_app_path = p;
                                                s.persist_settings_with_notice(
                                                    next,
                                                    t("Codex 应用"),
                                                    cx,
                                                );
                                            });
                                        });
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .h_flex()
                            .w_full()
                            .items_center()
                            .justify_between()
                            .gap_3()
                            .pt_2()
                            .border_t_1()
                            .border_color(cx.theme().border)
                            .child(
                                div()
                                    .v_flex()
                                    .flex_1()
                                    .min_w_0()
                                    .gap_0p5()
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_semibold()
                                            .text_color(cx.theme().foreground)
                                            .child(t("历史会话供应商归属修复")),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(t("切换供应商后若旧会话未出现在列表，可一键对齐数据库与 rollout 文件的供应商标识（自动备份）。")),
                                    ),
                            )
                            .child(
                                AppButton::new("maint-sync-providers")
                                    .secondary()
                                    .small()
                                    .disabled(sync_busy)
                                    .icon(IconName::RefreshCw)
                                    .label(if sync_busy {
                                        t("正在修复…")
                                    } else {
                                        t("一键对齐历史会话")
                                    })
                                    .on_click(move |_, _, cx| {
                                        store_sync_hist
                                            .update(cx, |s, cx| s.sync_history_providers_now(cx));
                                    }),
                            ),
                    ),
            ))
    }
}
