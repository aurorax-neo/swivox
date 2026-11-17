use chatgptx_backend::i18n::{Language, t};
use chatgptx_backend::ui_prefs::UiThemeMode;
use gpui_kit::assets::IconName;
use gpui_kit::component::input::{Input, InputState, Textarea, TextareaState};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::theme::ActiveTheme as _;
use gpui_kit::component::StyledExt as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::store::AppStore;
use crate::views::panel_card;
use crate::widgets::{AppBadge, AppButton};

pub struct SettingsView {
    store: Entity<AppStore>,
    proxy_mode_draft: String,
    proxy_url_input: Entity<InputState>,
    extra_args_textarea: Entity<TextareaState>,
}

impl SettingsView {
    pub fn new(store: Entity<AppStore>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let data = store.read(cx);
        let init_mode = data
            .official_snapshot
            .as_ref()
            .map(|s| s.global_proxy_mode.clone())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "system".to_string());
        let init_url = data
            .official_snapshot
            .as_ref()
            .map(|s| s.global_proxy_url.clone())
            .unwrap_or_default();
        let init_args = data.settings.codex_extra_args.join("\n");

        let proxy_url_input = cx.new(|cx| {
            let mut st = InputState::new(window, cx)
                .placeholder("http://127.0.0.1:7890 / socks5://127.0.0.1:1080");
            if !init_url.is_empty() {
                st.set_value(init_url, window, cx);
            }
            st
        });
        let extra_args_textarea = cx.new(|cx| {
            let mut st = TextareaState::new(window, cx)
                .placeholder(t("每行一个额外 CLI 启动参数，例如：\n--enable-features=Vulkan"));
            if !init_args.is_empty() {
                st.set_value(init_args, window, cx);
            }
            st
        });

        cx.observe(&store, |_, _, cx| cx.notify()).detach();

        Self {
            store,
            proxy_mode_draft: init_mode,
            proxy_url_input,
            extra_args_textarea,
        }
    }
}

impl Render for SettingsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let data = self.store.read(cx);
        let theme = data.prefs.theme;
        let lang = data.prefs.language;
        let auto_models_dev = data.settings.auto_update_models_dev;
        let proxy_testing = data.proxy_testing;
        let reset_confirm = data.reset_confirm_open;
        let effective_proxy = data
            .official_snapshot
            .as_ref()
            .map(|s| s.effective_launch_proxy.clone())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| t("直连 / 系统默认"));

        let cur_mode = self.proxy_mode_draft.clone();

        let store_dark = self.store.clone();
        let store_light = self.store.clone();
        let store_zh = self.store.clone();
        let store_en = self.store.clone();
        let store_models_dev = self.store.clone();
        let store_reset_open = self.store.clone();

        let ent_proxy_test = cx.entity();
        let ent_proxy_save = cx.entity();
        let ent_args_save = cx.entity();

        let proxy_modes = [
            ("system", t("跟随系统代理")),
            ("custom", t("指定代理地址")),
            ("direct", t("强制直连 (禁用代理)")),
        ];
        let mode_btns: Vec<AnyElement> = proxy_modes
            .into_iter()
            .map(|(m, lbl)| {
                let ent = cx.entity();
                AppButton::new(SharedString::from(format!("set-pm-{m}")))
                    .outline()
                    .small()
                    .selected(cur_mode == m)
                    .label(lbl)
                    .on_click(move |_, _, cx| {
                        ent.update(cx, |v, cx| {
                            v.proxy_mode_draft = m.to_string();
                            cx.notify();
                        });
                    })
                    .into_any_element()
            })
            .collect();

        let reset_banner: Option<AnyElement> = reset_confirm.then(|| {
            let store_cancel = self.store.clone();
            let store_exec = self.store.clone();
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
                        .child(t("确认恢复默认设置？")),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(t("此操作会将增强开关与常规偏好重置为出厂默认值（已保存的官方账号凭据不受影响）。")),
                )
                .child(
                    div()
                        .h_flex()
                        .justify_end()
                        .gap_2()
                        .child(
                            AppButton::new("set-reset-cancel")
                                .secondary()
                                .small()
                                .label(t("取消"))
                                .on_click(move |_, _, cx| {
                                    store_cancel.update(cx, |s, cx| {
                                        s.reset_confirm_open = false;
                                        cx.notify();
                                    });
                                }),
                        )
                        .child(
                            AppButton::new("set-reset-exec")
                                .danger()
                                .small()
                                .icon(IconName::Trash)
                                .label(t("确认重置"))
                                .on_click(move |_, _, cx| {
                                    store_exec.update(cx, |s, cx| s.reset_all_settings_now(cx));
                                }),
                        ),
                )
                .into_any_element()
        });

        div()
            .v_flex()
            .w_full()
            .gap_4()
            .when_some(reset_banner, |d, b| d.child(b))
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
                            .child(t("界面外观与语言偏好")),
                    )
                    .child(
                        div()
                            .h_flex()
                            .w_full()
                            .items_center()
                            .justify_between()
                            .gap_4()
                            .child(
                                div()
                                    .h_flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        AppButton::new("set-theme-dark")
                                            .outline()
                                            .small()
                                            .selected(theme == UiThemeMode::Dark)
                                            .icon(IconName::Moon)
                                            .label(t("深色主题"))
                                            .on_click(move |_, _, cx| {
                                                store_dark.update(cx, |s, cx| {
                                                    if s.prefs.theme != UiThemeMode::Dark {
                                                        s.toggle_theme(cx);
                                                    }
                                                });
                                            }),
                                    )
                                    .child(
                                        AppButton::new("set-theme-light")
                                            .outline()
                                            .small()
                                            .selected(theme == UiThemeMode::Light)
                                            .icon(IconName::Sun)
                                            .label(t("浅色主题"))
                                            .on_click(move |_, _, cx| {
                                                store_light.update(cx, |s, cx| {
                                                    if s.prefs.theme != UiThemeMode::Light {
                                                        s.toggle_theme(cx);
                                                    }
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
                                        AppButton::new("set-lang-zh")
                                            .outline()
                                            .small()
                                            .selected(lang == Language::Zh)
                                            .icon(IconName::Languages)
                                            .label("简体中文")
                                            .on_click(move |_, _, cx| {
                                                store_zh.update(cx, |s, cx| {
                                                    if s.prefs.language != Language::Zh {
                                                        s.toggle_language(cx);
                                                    }
                                                });
                                            }),
                                    )
                                    .child(
                                        AppButton::new("set-lang-en")
                                            .outline()
                                            .small()
                                            .selected(lang == Language::En)
                                            .icon(IconName::Languages)
                                            .label("English")
                                            .on_click(move |_, _, cx| {
                                                store_en.update(cx, |s, cx| {
                                                    if s.prefs.language != Language::En {
                                                        s.toggle_language(cx);
                                                    }
                                                });
                                            }),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .h_flex()
                            .w_full()
                            .items_center()
                            .justify_between()
                            .gap_4()
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
                                            .child(t("启动时自动同步 models.dev 模型目录")),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(t("后台更新主流大模型默认上下文窗口与多模态能力索引")),
                                    ),
                            )
                            .child(
                                Switch::new("set-models-dev-sw")
                                    .checked(auto_models_dev)
                                    .on_change(move |val, _, cx| {
                                        let v = *val;
                                        store_models_dev.update(cx, |s, cx| {
                                            let mut next = s.settings.clone();
                                            next.auto_update_models_dev = v;
                                            s.persist_settings_with_notice(next, "models.dev", cx);
                                        });
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
                                    .text_sm()
                                    .font_semibold()
                                    .text_color(cx.theme().foreground)
                                    .child(t("全局启动与配额查询代理")),
                            )
                            .child(AppBadge::info(format!(
                                "{}：{effective_proxy}",
                                t("当前生效")
                            ))),
                    )
                    .child(div().h_flex().flex_wrap().items_center().gap_2().children(mode_btns))
                    .when(cur_mode == "custom", |d| {
                        d.child(Input::new(&self.proxy_url_input))
                    })
                    .child(
                        div()
                            .h_flex()
                            .justify_end()
                            .gap_2()
                            .child(
                                AppButton::new("set-proxy-test")
                                    .secondary()
                                    .small()
                                    .disabled(proxy_testing)
                                    .icon(IconName::FlaskConical)
                                    .label(if proxy_testing {
                                        t("测试中…")
                                    } else {
                                        t("测试代理连通性")
                                    })
                                    .on_click(move |_, _, cx| {
                                        ent_proxy_test.update(cx, |v, cx| {
                                            let u = v
                                                .proxy_url_input
                                                .read(cx)
                                                .value()
                                                .trim()
                                                .to_string();
                                            v.store.update(cx, |s, cx| {
                                                s.test_global_proxy_url(u, cx)
                                            });
                                        });
                                    }),
                            )
                            .child(
                                AppButton::new("set-proxy-save")
                                    .primary()
                                    .small()
                                    .icon(IconName::Save)
                                    .label(t("保存全局代理"))
                                    .on_click(move |_, _, cx| {
                                        ent_proxy_save.update(cx, |v, cx| {
                                            let m = v.proxy_mode_draft.clone();
                                            let u = v
                                                .proxy_url_input
                                                .read(cx)
                                                .value()
                                                .trim()
                                                .to_string();
                                            v.store.update(cx, |s, cx| {
                                                s.save_global_proxy_settings(m, u, cx)
                                            });
                                        });
                                    }),
                            ),
                    ),
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
                            .child(t("Codex 附加启动参数与重置")),
                    )
                    .child(Textarea::new(&self.extra_args_textarea).h(px(95.)))
                    .child(
                        div()
                            .h_flex()
                            .w_full()
                            .items_center()
                            .justify_between()
                            .child(
                                AppButton::new("set-reset-trigger")
                                    .outline()
                                    .small()
                                    .icon(IconName::Trash)
                                    .label(t("恢复默认设置"))
                                    .on_click(move |_, _, cx| {
                                        store_reset_open.update(cx, |s, cx| {
                                            s.reset_confirm_open = true;
                                            cx.notify();
                                        });
                                    }),
                            )
                            .child(
                                AppButton::new("set-save-extra-args")
                                    .primary()
                                    .small()
                                    .icon(IconName::Save)
                                    .label(t("保存启动参数"))
                                    .on_click(move |_, _, cx| {
                                        ent_args_save.update(cx, |v, cx| {
                                            let lines: Vec<String> = v
                                                .extra_args_textarea
                                                .read(cx)
                                                .value()
                                                .lines()
                                                .map(str::trim)
                                                .filter(|s| !s.is_empty())
                                                .map(str::to_string)
                                                .collect();
                                            v.store.update(cx, |s, cx| {
                                                let mut next = s.settings.clone();
                                                next.codex_extra_args = lines;
                                                s.persist_settings_with_notice(
                                                    next,
                                                    t("启动参数"),
                                                    cx,
                                                );
                                            });
                                        });
                                    }),
                            ),
                    ),
            ))
    }
}
