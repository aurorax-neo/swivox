use std::collections::HashMap;

use chatgptx_backend::i18n::t;
use chatgptx_backend::phase2_rules::{
    account_initial, format_plan_display_name, multi_launch_instance_failure,
    multi_launch_instance_label, multi_launch_instance_running, multi_launch_instance_status,
    multi_launch_supported, provider_initial, relay_profile_config_brief, relay_profile_mode_label,
    relay_protocol_label,
};
use chatgptx_core::settings::RelayMode;
use gpui_kit::assets::IconName;
use gpui_kit::component::StyledExt as _;
use gpui_kit::component::theme::ActiveTheme as _;
use gpui_kit::*;

use crate::store::AppStore;
use crate::views::{avatar_badge, panel_card, status_badge};
use crate::widgets::AppButton;

pub fn render_multi_launch(store: &Entity<AppStore>, cx: &App) -> impl IntoElement {
    let data = store.read(cx);
    let instances_loading = data.multi_instances_loading;
    let multi_launch_pending = data.multi_launch_pending.clone();
    let multi_action_pending = data.multi_action_pending.clone();
    let profiles = data.settings.relay_profiles.clone();
    let aggregate_map: HashMap<String, _> = data
        .settings
        .aggregate_relay_profiles
        .iter()
        .map(|a| (a.id.clone(), a.clone()))
        .collect();
    let official_accounts = data
        .official_snapshot
        .as_ref()
        .map(|s| s.accounts.clone())
        .unwrap_or_default();

    let store_refresh_top = store.clone();
    let store_refresh_running = store.clone();

    // 1. 当前运行实例列表
    let running_body: AnyElement = if data.multi_instances.is_empty() {
        div()
            .py_6()
            .w_full()
            .flex()
            .items_center()
            .justify_center()
            .text_sm()
            .text_color(cx.theme().muted_foreground)
            .child(if instances_loading {
                t("正在读取运行实例…")
            } else {
                t("当前没有已发现的 Codex 实例。")
            })
            .into_any_element()
    } else {
        let rows: Vec<AnyElement> = data
            .multi_instances
            .iter()
            .map(|inst| {
                let status = multi_launch_instance_status(inst);
                let running = multi_launch_instance_running(inst);
                let pending = multi_action_pending.as_deref() == Some(inst.instance_id.as_str());
                let label = multi_launch_instance_label(inst);
                let failure = multi_launch_instance_failure(inst);

                let mut detail_parts = Vec::new();
                if inst.is_main {
                    detail_parts.push(t("主进程"));
                } else {
                    detail_parts.push(inst.instance_id.clone());
                }
                if let Some(pid) = inst.pid {
                    detail_parts.push(format!("PID {pid}"));
                }
                if inst.debug_port > 0 {
                    detail_parts.push(format!("CDP {}", inst.debug_port));
                }
                if inst.helper_port > 0 {
                    detail_parts.push(format!("API {}", inst.helper_port));
                }
                if let Some(err) = &failure {
                    detail_parts.push(format!("{}：{err}", t("启动失败")));
                }
                let detail = if detail_parts.is_empty() {
                    t("未提供进程详情")
                } else {
                    detail_parts.join(" · ")
                };

                let id_restart = inst.instance_id.clone();
                let id_stop = inst.instance_id.clone();
                let store_restart = store.clone();
                let store_stop = store.clone();

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
                    .border_color(if failure.is_some() {
                        cx.theme().danger.opacity(0.5)
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
                            .child(avatar_badge(cx, provider_initial(&label)))
                            .child(
                                div()
                                    .v_flex()
                                    .gap_1()
                                    .flex_1()
                                    .min_w_0()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_semibold()
                                            .text_color(cx.theme().foreground)
                                            .child(label),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(if failure.is_some() {
                                                cx.theme().danger
                                            } else {
                                                cx.theme().muted_foreground
                                            })
                                            .child(detail),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .gap_2()
                            .child(status_badge(&status))
                            .child(
                                AppButton::new(SharedString::from(format!(
                                    "multi-restart-{}",
                                    inst.instance_id
                                )))
                                .primary()
                                .small()
                                .disabled(pending)
                                .icon(IconName::RotateCcw)
                                .label(if pending { t("处理中") } else { t("重启") })
                                .tooltip(t("重启此 Codex 实例"))
                                .on_click(move |_, _, cx| {
                                    let id = id_restart.clone();
                                    store_restart.update(cx, |s, cx| s.restart_multi_instance(id, cx));
                                }),
                            )
                            .child(
                                AppButton::new(SharedString::from(format!(
                                    "multi-stop-{}",
                                    inst.instance_id
                                )))
                                .secondary()
                                .small()
                                .disabled(!running || pending)
                                .icon(IconName::PowerOff)
                                .label(t("结束"))
                                .tooltip(if running {
                                    t("结束此 Codex 实例")
                                } else {
                                    t("实例当前未运行")
                                })
                                .on_click(move |_, _, cx| {
                                    let id = id_stop.clone();
                                    store_stop.update(cx, |s, cx| s.stop_multi_instance(id, cx));
                                }),
                            ),
                    )
                    .into_any_element()
            })
            .collect();
        div().v_flex().w_full().gap_2p5().children(rows).into_any_element()
    };

    // 2. 选择官方账号启动 Codex
    let official_body: AnyElement = if official_accounts.is_empty() {
        div()
            .py_6()
            .w_full()
            .flex()
            .items_center()
            .justify_center()
            .text_sm()
            .text_color(cx.theme().muted_foreground)
            .child(t("暂无已保存的官方账号，请先在「官方账号」页面添加。"))
            .into_any_element()
    } else {
        let rows: Vec<AnyElement> = official_accounts
            .iter()
            .map(|acc| {
                let pending_key = format!("official:{}", acc.id);
                let pending = multi_launch_pending.as_deref() == Some(pending_key.as_str());
                let any_pending = multi_launch_pending.is_some();
                let plan_raw = acc
                    .plan_type
                    .as_deref()
                    .or_else(|| acc.quota.as_ref().and_then(|q| q.plan_type.as_deref()));
                let plan_label = format_plan_display_name(plan_raw);
                let bound_relay = acc
                    .bound_relay_profile_id
                    .as_deref()
                    .and_then(|id| profiles.iter().find(|p| p.id == id));

                let mut parts = Vec::new();
                parts.push(if plan_label.is_empty() {
                    t("官方账号")
                } else {
                    plan_label
                });
                if let Some(lbl) = acc.label.as_deref().filter(|s| !s.trim().is_empty()) {
                    parts.push(lbl.to_string());
                }
                if let Some(relay) = bound_relay {
                    parts.push(format!("{}：{}", t("已绑定供应商"), relay.name));
                } else {
                    parts.push(t("纯官方登录"));
                }
                let detail = parts.join(" · ");

                let acc_id = acc.id.clone();
                let store_launch = store.clone();

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
                            .h_flex()
                            .items_center()
                            .gap_3()
                            .flex_1()
                            .min_w_0()
                            .child(avatar_badge(cx, account_initial(&acc.email)))
                            .child(
                                div()
                                    .v_flex()
                                    .gap_1()
                                    .flex_1()
                                    .min_w_0()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_semibold()
                                            .text_color(cx.theme().foreground)
                                            .child(acc.email.clone()),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(detail),
                                    ),
                            ),
                    )
                    .child(
                        AppButton::new(SharedString::from(format!("multi-launch-official-{}", acc.id)))
                            .primary()
                            .small()
                            .disabled(any_pending)
                            .icon(IconName::Rocket)
                            .label(if pending { t("启动中") } else { t("启动") })
                            .tooltip(t("启动此官方账号的独立 Codex 实例"))
                            .on_click(move |_, _, cx| {
                                let id = acc_id.clone();
                                store_launch.update(cx, |s, cx| s.launch_multi_official_account(id, cx));
                            }),
                    )
                    .into_any_element()
            })
            .collect();
        div().v_flex().w_full().gap_2p5().children(rows).into_any_element()
    };

    // 3. 选择供应商身份启动 Codex
    let provider_body: AnyElement = if profiles.is_empty() {
        div()
            .py_6()
            .w_full()
            .flex()
            .items_center()
            .justify_center()
            .text_sm()
            .text_color(cx.theme().muted_foreground)
            .child(t("暂无供应商，请先添加供应商配置。"))
            .into_any_element()
    } else {
        let rows: Vec<AnyElement> = profiles
            .iter()
            .map(|profile| {
                let is_agg = profile.relay_mode == RelayMode::Aggregate;
                let supported = multi_launch_supported(profile);
                let pending = multi_launch_pending.as_deref() == Some(profile.id.as_str());
                let any_pending = multi_launch_pending.is_some();
                let agg_cfg = aggregate_map.get(&profile.id);

                let display_name = if profile.name.trim().is_empty() {
                    t("未命名供应商")
                } else {
                    profile.name.clone()
                };
                let subtitle = format!(
                    "{} · {} · {}",
                    relay_profile_mode_label(profile, agg_cfg),
                    relay_protocol_label(profile.protocol),
                    relay_profile_config_brief(profile, agg_cfg)
                );
                let tip = if is_agg {
                    t("聚合供应商暂不支持独立多开")
                } else if !supported {
                    t("仅支持单一 Responses 或 Chat Completions 供应商，且不能配置模型路由")
                } else {
                    t("启动此供应商的独立 Codex 实例")
                };

                let profile_id = profile.id.clone();
                let store_launch = store.clone();

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
                            .h_flex()
                            .items_center()
                            .gap_3()
                            .flex_1()
                            .min_w_0()
                            .child(avatar_badge(cx, provider_initial(&display_name)))
                            .child(
                                div()
                                    .v_flex()
                                    .gap_1()
                                    .flex_1()
                                    .min_w_0()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_semibold()
                                            .text_color(cx.theme().foreground)
                                            .child(display_name),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(subtitle),
                                    ),
                            ),
                    )
                    .child(
                        AppButton::new(SharedString::from(format!(
                            "multi-launch-profile-{}",
                            profile.id
                        )))
                        .primary()
                        .small()
                        .disabled(!supported || any_pending)
                        .icon(IconName::Rocket)
                        .label(if pending { t("启动中") } else { t("启动") })
                        .tooltip(tip)
                        .on_click(move |_, _, cx| {
                            let id = profile_id.clone();
                            store_launch.update(cx, |s, cx| s.launch_multi_profile(id, cx));
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
                .justify_end()
                .child(
                    AppButton::new("multi-launch-refresh-all")
                        .secondary()
                        .small()
                        .icon(IconName::RefreshCw)
                        .label(t("刷新列表"))
                        .on_click(move |_, _, cx| {
                            store_refresh_top.update(cx, |s, cx| s.refresh_current(cx));
                        }),
                ),
        )
        .child(panel_card(
            cx,
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
                                .v_flex()
                                .gap_1()
                                .child(
                                    div()
                                        .text_base()
                                        .font_semibold()
                                        .text_color(cx.theme().foreground)
                                        .child(t("当前运行实例")),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(t("这里列出主 Codex 和已经启动的多开实例，可单独重启或结束。")),
                                ),
                        )
                        .child(
                            AppButton::new("multi-launch-refresh-running")
                                .secondary()
                                .small()
                                .disabled(instances_loading)
                                .icon(IconName::RefreshCw)
                                .label(t("刷新"))
                                .on_click(move |_, _, cx| {
                                    store_refresh_running
                                        .update(cx, |s, cx| s.refresh_multi_instances(true, cx));
                                }),
                        ),
                )
                .child(running_body),
        ))
        .child(panel_card(
            cx,
            div()
                .v_flex()
                .w_full()
                .gap_4()
                .child(
                    div()
                        .v_flex()
                        .gap_1()
                        .child(
                            div()
                                .text_base()
                                .font_semibold()
                                .text_color(cx.theme().foreground)
                                .child(t("选择官方账号启动 Codex")),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(t("每个官方账号使用独立的配置、会话和用户数据目录，支持纯官方登录或与绑定的第三方 API 供应商组合多开。")),
                        ),
                )
                .child(official_body),
        ))
        .child(panel_card(
            cx,
            div()
                .v_flex()
                .w_full()
                .gap_4()
                .child(
                    div()
                        .v_flex()
                        .gap_1()
                        .child(
                            div()
                                .text_base()
                                .font_semibold()
                                .text_color(cx.theme().foreground)
                                .child(t("选择供应商身份启动 Codex")),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(t("每个身份使用独立的配置、会话和用户数据目录，可与其他单一 Responses 或 Chat Completions 供应商同时运行。")),
                        ),
                )
                .child(provider_body),
        ))
}
