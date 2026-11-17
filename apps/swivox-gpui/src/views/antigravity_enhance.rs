//! Antigravity 增强与免 TUN 代理启动、可逆界面汉化、上下文监控圆环与设备指纹保护视图。

use gpui_kit::assets::IconName;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::theme::ActiveTheme as _;
use gpui_kit::component::StyledExt as _;
use gpui_kit::*;
use swivox_backend::i18n::t;

use crate::store::AppStore;
use crate::views::panel_card;
use crate::widgets::{AppBadge, AppButton};

pub struct AntigravityEnhanceView {
    store: Entity<AppStore>,
    desktop_path_input: Entity<InputState>,
    ide_path_input: Entity<InputState>,
    proxy_url_input: Entity<InputState>,
    cdp_port_input: Entity<InputState>,
    target_mode: String,
    proxy_mode: String,
    enable_i18n: bool,
    enable_context_ring: bool,
}

impl AntigravityEnhanceView {
    pub fn new(store: Entity<AppStore>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let snap = store.read(cx).ag_snapshot.clone();
        let target_mode = snap
            .as_ref()
            .map(|s| s.preferred_target.clone())
            .unwrap_or_else(|| "auto".to_string());
        let proxy_mode = snap
            .as_ref()
            .map(|s| s.launch_proxy_mode.clone())
            .unwrap_or_else(|| "inherit".to_string());
        let enable_i18n = snap.as_ref().is_some_and(|s| s.enable_ui_localization);
        let enable_context_ring = snap
            .as_ref()
            .map(|s| s.enable_context_ring)
            .unwrap_or(true);
        let init_desktop = snap
            .as_ref()
            .map(|s| s.custom_desktop_path.clone())
            .unwrap_or_default();
        let init_ide = snap
            .as_ref()
            .map(|s| s.custom_ide_path.clone())
            .unwrap_or_default();
        let init_proxy = snap
            .as_ref()
            .map(|s| s.launch_proxy_url.clone())
            .unwrap_or_default();
        let init_port = snap
            .as_ref()
            .map(|s| s.cdp_port.to_string())
            .unwrap_or_else(|| "9333".to_string());

        let desktop_path_input = cx.new(|cx| {
            let mut st = InputState::new(window, cx)
                .placeholder(t("留空自动探测 /Applications/Antigravity.app 或系统默认位置"));
            st.set_value(init_desktop, window, cx);
            st
        });
        let ide_path_input = cx.new(|cx| {
            let mut st = InputState::new(window, cx)
                .placeholder(t("留空自动探测 /Applications/Antigravity IDE.app 或系统默认位置"));
            st.set_value(init_ide, window, cx);
            st
        });
        let proxy_url_input = cx.new(|cx| {
            let mut st =
                InputState::new(window, cx).placeholder("http://127.0.0.1:7890");
            st.set_value(init_proxy, window, cx);
            st
        });
        let cdp_port_input = cx.new(|cx| {
            let mut st = InputState::new(window, cx).placeholder("9333");
            st.set_value(init_port, window, cx);
            st
        });

        cx.observe(&store, |_, _, cx| cx.notify()).detach();

        Self {
            store,
            desktop_path_input,
            ide_path_input,
            proxy_url_input,
            cdp_port_input,
            target_mode,
            proxy_mode,
            enable_i18n,
            enable_context_ring,
        }
    }

    fn persist_now(&self, cx: &mut Context<Self>) {
        let dp = self.desktop_path_input.read(cx).value().to_string();
        let ip = self.ide_path_input.read(cx).value().to_string();
        let pu = self.proxy_url_input.read(cx).value().to_string();
        let port = self
            .cdp_port_input
            .read(cx)
            .value()
            .trim()
            .parse::<u16>()
            .unwrap_or(9333);
        let tm = self.target_mode.clone();
        let pm = self.proxy_mode.clone();
        let i18n = self.enable_i18n;
        let ring = self.enable_context_ring;
        self.store.update(cx, |s, cx| {
            s.save_antigravity_enhance(tm, dp, ip, pm, pu, i18n, ring, port, cx);
        });
    }

    fn kv_row(cx: &App, label: String, val: String) -> Div {
        div()
            .h_flex()
            .w_full()
            .items_center()
            .justify_between()
            .gap_3()
            .py_1p5()
            .border_b_1()
            .border_color(cx.theme().border.opacity(0.5))
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(label),
            )
            .child(
                div()
                    .text_xs()
                    .font_semibold()
                    .truncate()
                    .text_color(cx.theme().foreground)
                    .child(val),
            )
    }
}

impl Render for AntigravityEnhanceView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (snap, px_snap) = {
            let s = self.store.read(cx);
            (s.ag_snapshot.clone(), s.network_proxy_snapshot.clone())
        };
        let eff_desk_proxy = snap
            .as_ref()
            .and_then(|s| s.effective_desktop_proxy.clone())
            .unwrap_or_else(|| px_snap.effective_antigravity_desktop.clone());
        let eff_ide_proxy = snap
            .as_ref()
            .and_then(|s| s.effective_ide_proxy.clone())
            .unwrap_or_else(|| px_snap.effective_antigravity_ide.clone());
        let dev_profile = snap.as_ref().and_then(|s| s.current_device_profile.clone());
        let has_baseline = snap.as_ref().is_some_and(|s| s.has_device_baseline);
        let nodes = px_snap.config.nodes.clone();

        let st_regen = self.store.clone();
        let st_restore = self.store.clone();
        let st_goto_settings = self.store.clone();

        let build_slot_btns = |slot: &'static str, cur_sel: &str| -> Vec<AnyElement> {
            let mut opts = vec![
                ("system".to_string(), t("跟随系统代理")),
                ("app".to_string(), t("跟随本程序")),
                ("direct".to_string(), t("强制直连")),
            ];
            for n in &nodes {
                opts.push((format!("node:{}", n.id), n.name.clone()));
            }
            opts.into_iter()
                .map(|(sel, lbl)| {
                    let selected = cur_sel == sel;
                    let st = self.store.clone();
                    let b = AppButton::new(SharedString::from(format!("ag-enh-px-{slot}-{sel}")))
                        .small()
                        .label(lbl);
                    let b = if selected { b.selected(true) } else { b.outline() };
                    b.on_click(move |_, _, cx| {
                        let next_sel = sel.clone();
                        st.update(cx, |s, cx| {
                            s.update_self_or_client_proxy_assignment(slot, next_sel, cx);
                        });
                    })
                    .into_any_element()
                })
                .collect()
        };

        let desk_btns = build_slot_btns(
            "antigravity_desktop",
            &px_snap.config.clients.antigravity_desktop,
        );
        let ide_btns = build_slot_btns("antigravity_ide", &px_snap.config.clients.antigravity_ide);

        div()
            .v_flex()
            .w_full()
            .gap_4()
            // 卡片 1：桌面版与 IDE 免 TUN 独立隔离代理启动
            .child(panel_card(
                cx,
                div()
                    .v_flex()
                    .w_full()
                    .gap_3p5()
                    .child(
                        div()
                            .h_flex()
                            .flex_wrap()
                            .w_full()
                            .items_center()
                            .justify_between()
                            .gap_3()
                            .child(
                                div()
                                    .v_flex()
                                    .flex_1()
                                    .min_w(px(220.))
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_base()
                                            .font_semibold()
                                            .child(t("免 TUN 隔离代理启动（桌面版与 IDE 各自独立）")),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(t("启动时同时向 Electron 渲染层与后台 language_server 注入所选代理，无需系统 TUN 网卡；Agy 点火自动复用桌面版代理")),
                                    ),
                            )
                            .child(
                                AppButton::new("ag-enh-manage-proxy-pool")
                                    .outline()
                                    .small()
                                    .icon(IconName::Settings)
                                    .label(t("管理代理列表"))
                                    .on_click(move |_, _, cx| {
                                        st_goto_settings.update(cx, |s, cx| {
                                            s.navigate(crate::routes::Route::Settings, cx);
                                        });
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .v_flex()
                            .w_full()
                            .gap_2()
                            .p_3()
                            .rounded(px(8.))
                            .bg(cx.theme().secondary.opacity(0.24))
                            .border_1()
                            .border_color(cx.theme().border)
                            .child(
                                div()
                                    .h_flex()
                                    .flex_wrap()
                                    .w_full()
                                    .items_center()
                                    .justify_between()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_semibold()
                                            .child(t("Antigravity 桌面版与 Agy 点火代理")),
                                    )
                                    .child(AppBadge::info(format!(
                                        "{}: {eff_desk_proxy}",
                                        t("当前生效")
                                    ))),
                            )
                            .child(div().h_flex().flex_wrap().gap_1p5().children(desk_btns)),
                    )
                    .child(
                        div()
                            .v_flex()
                            .w_full()
                            .gap_2()
                            .p_3()
                            .rounded(px(8.))
                            .bg(cx.theme().secondary.opacity(0.24))
                            .border_1()
                            .border_color(cx.theme().border)
                            .child(
                                div()
                                    .h_flex()
                                    .flex_wrap()
                                    .w_full()
                                    .items_center()
                                    .justify_between()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_semibold()
                                            .child(t("Antigravity IDE 编辑器代理")),
                                    )
                                    .child(AppBadge::info(format!(
                                        "{}: {eff_ide_proxy}",
                                        t("当前生效")
                                    ))),
                            )
                            .child(div().h_flex().flex_wrap().gap_1p5().children(ide_btns)),
                    ),
            ))
            // 卡片 2：可逆界面汉化与上下文监控圆环
            .child(panel_card(
                cx,
                div()
                    .v_flex()
                    .w_full()
                    .gap_3p5()
                    .child(
                        div()
                            .v_flex()
                            .gap_1()
                            .child(
                                div()
                                    .text_base()
                                    .font_semibold()
                                    .child(t("CDP 动态增强（可逆汉化与上下文圆环）")),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(t("完全通过本地回环 CDP 实时挂载，不触碰 .app 安装包内任何文件，不破坏 macOS 代码签名")),
                            ),
                    )
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
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_medium()
                                            .child(t("可逆界面汉化（仅桌面版）")),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(t("内置 3,376 条精校词条，自动避开代码块与消息正文；关闭开关后无需重启瞬间恢复英文")),
                                    ),
                            )
                            .child(
                                Switch::new("ag-toggle-i18n")
                                    .checked(self.enable_i18n)
                                    .on_click(cx.listener(|v, checked: &bool, _, cx| {
                                        v.enable_i18n = *checked;
                                        v.persist_now(cx);
                                    })),
                            ),
                    )
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
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_medium()
                                            .child(t("上下文占用圆环与输出速度监控（桌面版 + IDE）")),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(t("在输入框模型选择器旁显示实时上下文百分比圆环，点击查看底层端点模型、tok/s 速度与首字等待")),
                                    ),
                            )
                            .child(
                                Switch::new("ag-toggle-context-ring")
                                    .checked(self.enable_context_ring)
                                    .on_click(cx.listener(|v, checked: &bool, _, cx| {
                                        v.enable_context_ring = *checked;
                                        v.persist_now(cx);
                                    })),
                            ),
                    ),
            ))
            // 卡片 3：启动目标与自定义路径
            .child(panel_card(
                cx,
                div()
                    .v_flex()
                    .w_full()
                    .gap_3p5()
                    .child(
                        div()
                            .text_base()
                            .font_semibold()
                            .child(t("启动目标选择与应用路径")),
                    )
                    .child(
                        div()
                            .h_flex()
                            .flex_wrap()
                            .gap_2()
                            .children(
                                [
                                    ("auto", t("自动探测（推荐）")),
                                    ("desktop", t("Antigravity 桌面版")),
                                    ("ide", t("Antigravity IDE 编辑器")),
                                ]
                                .into_iter()
                                .map(|(m, lbl)| {
                                    let active = self.target_mode == m;
                                    let b = AppButton::new(SharedString::from(format!(
                                        "ag-target-{m}"
                                    )))
                                    .small()
                                    .label(lbl);
                                    let b = if active { b.selected(true) } else { b.outline() };
                                    b.on_click(cx.listener(move |v, _, _, cx| {
                                        v.target_mode = m.to_string();
                                        cx.notify();
                                    }))
                                }),
                            ),
                    )
                    .child(
                        div()
                            .v_flex()
                            .gap_2()
                            .child(
                                div()
                                    .text_xs()
                                    .font_medium()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(t("桌面版自定义路径（可选）")),
                            )
                            .child(Input::new(&self.desktop_path_input))
                            .child(
                                div()
                                    .text_xs()
                                    .font_medium()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(t("IDE 版自定义路径（可选）")),
                            )
                            .child(Input::new(&self.ide_path_input))
                            .child(
                                div()
                                    .h_flex()
                                    .flex_wrap()
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
                                                    .text_xs()
                                                    .text_color(cx.theme().muted_foreground)
                                                    .child(t("本地 CDP 调试端口：")),
                                            )
                                            .child(
                                                div()
                                                    .w(px(110.))
                                                    .child(Input::new(&self.cdp_port_input)),
                                            ),
                                    )
                                    .child(
                                        AppButton::new("ag-save-enhance-all")
                                            .primary()
                                            .small()
                                            .icon(IconName::Save)
                                            .label(t("保存启动与增强配置"))
                                            .on_click(cx.listener(|v, _, _, cx| {
                                                v.persist_now(cx);
                                            })),
                                    ),
                            ),
                    ),
            ))
            // 卡片 4：设备指纹保护
            .child(panel_card(
                cx,
                div()
                    .v_flex()
                    .w_full()
                    .gap_3p5()
                    .child(
                        div()
                            .h_flex()
                            .flex_wrap()
                            .w_full()
                            .items_center()
                            .justify_between()
                            .gap_3()
                            .child(
                                div()
                                    .v_flex()
                                    .flex_1()
                                    .min_w(px(220.))
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_base()
                                            .font_semibold()
                                            .child(t("设备指纹（Machine ID）单号隔离与基线恢复")),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(t("每个账号切号时自动绑定独立机器码并同步 storage.json 与 state.vscdb，随时可一键恢复本机原始指纹")),
                                    ),
                            )
                            .child(
                                div()
                                    .h_flex()
                                    .flex_wrap()
                                    .gap_2()
                                    .child(
                                        AppButton::new("ag-dev-restore")
                                            .outline()
                                            .small()
                                            .disabled(!has_baseline)
                                            .icon(IconName::RotateCcw)
                                            .label(t("恢复原始基线"))
                                            .on_click(move |_, _, cx| {
                                                st_restore.update(cx, |s, cx| {
                                                    s.request_action_confirm(
                                                        crate::store::PendingActionConfirm::RestoreAgFingerprintBaseline,
                                                        cx,
                                                    )
                                                });
                                            }),
                                    )
                                    .child(
                                        AppButton::new("ag-dev-regen")
                                            .primary()
                                            .small()
                                            .icon(IconName::RefreshCw)
                                            .label(t("生成随机新指纹"))
                                            .on_click(move |_, _, cx| {
                                                st_regen.update(cx, |s, cx| {
                                                    s.request_action_confirm(
                                                        crate::store::PendingActionConfirm::RegenerateAgFingerprint,
                                                        cx,
                                                    )
                                                });
                                            }),
                                    ),
                            ),
                    )
                    .child(if let Some(dev) = dev_profile {
                        div()
                            .v_flex()
                            .w_full()
                            .p_3()
                            .rounded(px(8.))
                            .bg(cx.theme().secondary.opacity(0.24))
                            .border_1()
                            .border_color(cx.theme().border)
                            .child(Self::kv_row(cx, "telemetry.machineId".to_string(), dev.machine_id))
                            .child(Self::kv_row(
                                cx,
                                "telemetry.macMachineId".to_string(),
                                dev.mac_machine_id,
                            ))
                            .child(Self::kv_row(
                                cx,
                                "storage.serviceMachineId".to_string(),
                                dev.dev_device_id,
                            ))
                            .child(Self::kv_row(cx, "telemetry.sqmId".to_string(), dev.sqm_id))
                            .into_any_element()
                    } else {
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(t("当前尚未在目标目录读取到 storage.json，点击「生成随机新指纹」可立即初始化写入。"))
                            .into_any_element()
                    }),
            ))
    }
}
