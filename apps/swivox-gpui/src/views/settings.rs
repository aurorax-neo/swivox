use gpui_kit::assets::IconName;
use gpui_kit::component::StyledExt as _;
use gpui_kit::component::input::{Input, InputState, Textarea, TextareaState};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::theme::ActiveTheme as _;
use gpui_kit::*;
use swivox_backend::i18n::{Language, t};
use swivox_backend::ui_prefs::UiThemeMode;
use swivox_core::network_proxies::ProxyNodeEntry;

use crate::store::AppStore;
use crate::views::panel_card;
use crate::widgets::{AppBadge, AppButton};

pub struct SettingsView {
    store: Entity<AppStore>,
    node_name_input: Entity<InputState>,
    node_url_input: Entity<InputState>,
    extra_args_textarea: Entity<TextareaState>,
}

impl SettingsView {
    pub fn new(store: Entity<AppStore>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let init_args = store.read(cx).settings.codex_extra_args.join("\n");

        let node_name_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder(t("节点名称（如：Clash 本地）"))
        });
        let node_url_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("http://127.0.0.1:7890 / socks5://127.0.0.1:1080")
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
            node_name_input,
            node_url_input,
            extra_args_textarea,
        }
    }

    fn render_proxy_assignment_row(
        &self,
        cx: &App,
        slot: &'static str,
        title: String,
        subtitle: String,
        current_selection: &str,
        effective_desc: &str,
        allow_follow_app: bool,
        nodes: &[ProxyNodeEntry],
    ) -> Div {
        let mut choices: Vec<(String, String)> = vec![("system".to_string(), t("跟随系统代理"))];
        if allow_follow_app {
            choices.push(("app".to_string(), t("跟随本程序")));
        }
        choices.push(("direct".to_string(), t("强制直连")));
        for node in nodes {
            choices.push((format!("node:{}", node.id), node.name.clone()));
        }

        let btns: Vec<AnyElement> = choices
            .into_iter()
            .map(|(sel_val, label)| {
                let selected = current_selection == sel_val;
                let st = self.store.clone();
                let b = AppButton::new(SharedString::from(format!("set-px-slot-{slot}-{sel_val}")))
                    .small()
                    .label(label);
                let b = if selected {
                    b.selected(true)
                } else {
                    b.outline()
                };
                b.on_click(move |_, _, cx| {
                    let next_sel = sel_val.clone();
                    st.update(cx, |s, cx| {
                        s.update_self_or_client_proxy_assignment(slot, next_sel, cx);
                    });
                })
                .into_any_element()
            })
            .collect();

        div()
            .v_flex()
            .w_full()
            .gap_2()
            .p_3p5()
            .rounded(px(10.))
            .bg(cx.theme().secondary.opacity(0.24))
            .border_1()
            .border_color(cx.theme().border)
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
                                    .text_xs()
                                    .font_semibold()
                                    .text_color(cx.theme().foreground)
                                    .child(title),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(subtitle),
                            ),
                    )
                    .child(AppBadge::info(format!("{}: {effective_desc}", t("生效")))),
            )
            .child(div().h_flex().flex_wrap().items_center().gap_1p5().children(btns))
    }
}

impl Render for SettingsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let data = self.store.read(cx);
        let theme = data.prefs.theme;
        let lang = data.prefs.language;
        let auto_models_dev = data.settings.auto_update_models_dev;
        let proxy_testing = data.proxy_testing;
        let px_snap = data.network_proxy_snapshot.clone();
        let nodes = px_snap.config.nodes.clone();

        let store_dark = self.store.clone();
        let store_light = self.store.clone();
        let store_zh = self.store.clone();
        let store_en = self.store.clone();
        let store_models_dev = self.store.clone();
        let store_reset_open = self.store.clone();

        let ent_node_test = cx.entity();
        let ent_node_add = cx.entity();
        let ent_args_save = cx.entity();

        let node_rows: AnyElement = if nodes.is_empty() {
            div()
                .p_3p5()
                .w_full()
                .rounded(px(8.))
                .bg(cx.theme().secondary.opacity(0.22))
                .border_1()
                .border_color(cx.theme().border)
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(t("代理列表暂为空。在上方填入节点名称与 HTTP / SOCKS5 地址点击「添加到代理列表」，即可供本程序、各客户端与自动点火独立选取。"))
                .into_any_element()
        } else {
            let rows: Vec<AnyElement> = nodes
                .iter()
                .map(|node| {
                    let node_id_del = node.id.clone();
                    let node_url_test = node.url.clone();
                    let st_test = self.store.clone();
                    let st_del = self.store.clone();

                    div()
                        .h_flex()
                        .w_full()
                        .items_center()
                        .justify_between()
                        .gap_3()
                        .px_3p5()
                        .py_2p5()
                        .rounded(px(8.))
                        .bg(cx.theme().secondary.opacity(0.28))
                        .border_1()
                        .border_color(cx.theme().border)
                        .child(
                            div()
                                .h_flex()
                                .flex_1()
                                .min_w_0()
                                .items_center()
                                .gap_2p5()
                                .child(
                                    div()
                                        .text_xs()
                                        .font_semibold()
                                        .text_color(cx.theme().foreground)
                                        .child(node.name.clone()),
                                )
                                .child(AppBadge::secondary(node.url.clone())),
                        )
                        .child(
                            div()
                                .h_flex()
                                .flex_shrink_0()
                                .items_center()
                                .gap_1p5()
                                .child(
                                    AppButton::new(SharedString::from(format!(
                                        "set-px-node-test-{}",
                                        node.id
                                    )))
                                    .outline()
                                    .small()
                                    .disabled(proxy_testing)
                                    .icon(IconName::FlaskConical)
                                    .label(t("测速"))
                                    .on_click(move |_, _, cx| {
                                        let u = node_url_test.clone();
                                        st_test.update(cx, |s, cx| s.test_global_proxy_url(u, cx));
                                    }),
                                )
                                .child(
                                    AppButton::new(SharedString::from(format!(
                                        "set-px-node-del-{}",
                                        node.id
                                    )))
                                    .ghost()
                                    .small_icon()
                                    .icon(IconName::Trash)
                                    .tooltip(t("删除代理节点"))
                                    .on_click(move |_, _, cx| {
                                        let id = node_id_del.clone();
                                        st_del.update(cx, |s, cx| s.remove_proxy_node(id, cx));
                                    }),
                                ),
                        )
                        .into_any_element()
                })
                .collect();
            div().v_flex().w_full().gap_2().children(rows).into_any_element()
        };

        let quick_presets = [
            ("Clash 7890", "Clash 本地", "http://127.0.0.1:7890"),
            ("Verge 7897", "Clash Verge", "http://127.0.0.1:7897"),
            ("SOCKS5 10808", "本地 SOCKS5", "socks5://127.0.0.1:10808"),
        ];
        let name_inp_quick = self.node_name_input.clone();
        let url_inp_quick = self.node_url_input.clone();

        div()
            .v_flex()
            .w_full()
            .gap_4()
            // 1. 外观与语言
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
            // 2. 代理节点列表（供本程序、各客户端与点火统一选取）
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
                                    .gap_0p5()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_semibold()
                                            .text_color(cx.theme().foreground)
                                            .child(t("代理节点列表（Proxy Pool）")),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(t("统一维护 HTTP / HTTPS / SOCKS5 代理节点，供 Swivox 本程序、各客户端与自动点火按需独立选择")),
                                    ),
                            )
                            .child(AppBadge::secondary(format!(
                                "{} {}",
                                nodes.len(),
                                t("个节点")
                            ))),
                    )
                    .child(
                        div()
                            .h_flex()
                            .w_full()
                            .items_center()
                            .gap_2()
                            .child(div().w(px(250.)).child(Input::new(&self.node_name_input)))
                            .child(div().flex_1().child(Input::new(&self.node_url_input)))
                            .child(
                                AppButton::new("set-proxy-test")
                                    .outline()
                                    .small()
                                    .disabled(proxy_testing)
                                    .icon(IconName::FlaskConical)
                                    .label(if proxy_testing {
                                        t("测试中…")
                                    } else {
                                        t("测试连通性")
                                    })
                                    .on_click(move |_, _, cx| {
                                        ent_node_test.update(cx, |v, cx| {
                                            let u = v
                                                .node_url_input
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
                                AppButton::new("set-proxy-add-node")
                                    .primary()
                                    .small()
                                    .icon(IconName::Plus)
                                    .label(t("添加到代理列表"))
                                    .on_click(move |_, window, cx| {
                                        ent_node_add.update(cx, |v, cx| {
                                            let nm = v
                                                .node_name_input
                                                .read(cx)
                                                .value()
                                                .trim()
                                                .to_string();
                                            let u = v
                                                .node_url_input
                                                .read(cx)
                                                .value()
                                                .trim()
                                                .to_string();
                                            v.store.update(cx, |s, cx| {
                                                s.add_or_update_proxy_node(
                                                    None,
                                                    nm,
                                                    u.clone(),
                                                    cx,
                                                );
                                            });
                                            if !u.is_empty() {
                                                v.node_name_input.update(cx, |inp, cx| {
                                                    inp.set_value("", window, cx);
                                                });
                                                v.node_url_input.update(cx, |inp, cx| {
                                                    inp.set_value("", window, cx);
                                                });
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
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(t("常用本地端口快捷填入：")),
                            )
                            .children(quick_presets.into_iter().map(|(btn_lbl, def_name, def_url)| {
                                let ni = name_inp_quick.clone();
                                let ui = url_inp_quick.clone();
                                AppButton::new(SharedString::from(format!(
                                    "set-px-quick-{btn_lbl}"
                                )))
                                .outline()
                                .small()
                                .label(btn_lbl)
                                .on_click(move |_, window, cx| {
                                    ni.update(cx, |inp, cx| {
                                        if inp.value().trim().is_empty() {
                                            inp.set_value(t(def_name), window, cx);
                                        }
                                    });
                                    ui.update(cx, |inp, cx| {
                                        inp.set_value(def_url.to_string(), window, cx);
                                    });
                                })
                            })),
                    )
                    .child(node_rows),
            ))
            // 3. Swivox 本程序与各客户端独立隔离代理（点火就用对应客户端代理）
            .child(panel_card(
                cx,
                div()
                    .v_flex()
                    .w_full()
                    .gap_3()
                    .child(
                        div()
                            .v_flex()
                            .gap_0p5()
                            .child(
                                div()
                                    .text_sm()
                                    .font_semibold()
                                    .text_color(cx.theme().foreground)
                                    .child(t("本程序与各客户端隔离代理分配（点火自动使用对应客户端代理）")),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(t("Swivox 本程序、Codex、Antigravity 桌面版、Antigravity IDE 与扩展监控各自从上方代理列表中独立选择、互不串流；Lamplighter 配额轮询与 5h 点火严格复用所属客户端及账号代理")),
                            ),
                    )
                    .child(self.render_proxy_assignment_row(
                        cx,
                        "app_self",
                        t("Swivox 本程序自身代理"),
                        t("用于插件市场、脚本市场、models.dev 目录更新、Bark / Webhook 推送与软件更新检查"),
                        &px_snap.config.app_self_proxy,
                        &px_snap.effective_app_self,
                        false,
                        &nodes,
                    ))
                    .child(self.render_proxy_assignment_row(
                        cx,
                        "codex",
                        t("Codex 客户端与 Codex 点火代理"),
                        t("用于 Codex 桌面启动、OpenAI OAuth 登录、官方账号配额刷新及 Lamplighter Codex 点火"),
                        &px_snap.config.clients.codex,
                        &px_snap.effective_codex,
                        true,
                        &nodes,
                    ))
                    .child(self.render_proxy_assignment_row(
                        cx,
                        "antigravity_desktop",
                        t("Antigravity 桌面版与 Agy 点火代理"),
                        t("用于 Antigravity 桌面版免 TUN 启动、Google OAuth、三端点配额查询及 Lamplighter Agy 点火"),
                        &px_snap.config.clients.antigravity_desktop,
                        &px_snap.effective_antigravity_desktop,
                        true,
                        &nodes,
                    ))
                    .child(self.render_proxy_assignment_row(
                        cx,
                        "antigravity_ide",
                        t("Antigravity IDE 编辑器代理"),
                        t("用于 Antigravity IDE 免 TUN 独立代理启动（与桌面版代理完全隔离）"),
                        &px_snap.config.clients.antigravity_ide,
                        &px_snap.effective_antigravity_ide,
                        true,
                        &nodes,
                    ))
                    .child(self.render_proxy_assignment_row(
                        cx,
                        "external",
                        t("扩展监控凭据（Claude / Kimi / Grok）代理"),
                        t("用于独立扩展凭据的额度查询与 Claude 自动点火"),
                        &px_snap.config.clients.external,
                        &px_snap.effective_external,
                        true,
                        &nodes,
                    )),
            ))
            // 4. Codex 附加参数与恢复默认
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
