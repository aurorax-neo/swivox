//! Lamplighter（点灯人）原生 GPUI 管理视图：多服务额度卡片、5h 自动点火计划与立即点火、额度变化趋势柱线图、事件流与完整推送/MCP 设置。

use chrono::{Local, TimeZone};
use gpui_kit::assets::IconName;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::theme::ActiveTheme as _;
use gpui_kit::component::StyledExt as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use swivox_backend::i18n::{t, tf};
use swivox_core::lamplighter::{
    HistorySeries, LamplighterConfig, MonitoredGroupView, generate_mcp_bearer_key,
};

use crate::store::AppStore;
use crate::views::panel_card;
use crate::widgets::{AppBadge, AppButton};

pub struct LamplighterView {
    store: Entity<AppStore>,
    bark_url_input: Entity<InputState>,
    webhook_url_input: Entity<InputState>,
    poll_secs_input: Entity<InputState>,
    start_hour_input: Entity<InputState>,
    end_hour_input: Entity<InputState>,
    notice_th_input: Entity<InputState>,
    low_th_input: Entity<InputState>,
    crit_th_input: Entity<InputState>,
    mcp_listen_input: Entity<InputState>,
    ext_email_input: Entity<InputState>,
    ext_label_input: Entity<InputState>,
    ext_token_input: Entity<InputState>,
    ext_proxy_input: Entity<InputState>,
    draft_cfg: LamplighterConfig,
}

impl LamplighterView {
    pub fn new(store: Entity<AppStore>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let draft_cfg = store
            .read(cx)
            .lamplighter_snapshot
            .as_ref()
            .map(|s| s.config.clone())
            .unwrap_or_default();

        let init_bark = draft_cfg.bark_url.clone();
        let init_wh = draft_cfg.webhook.url.clone();
        let init_poll = draft_cfg.poll_interval_seconds.to_string();
        let init_sh = draft_cfg.ignition.start_hour.to_string();
        let init_eh = draft_cfg.ignition.end_hour.to_string();
        let init_nth = format!("{:.0}", draft_cfg.notice_threshold);
        let init_lth = format!("{:.0}", draft_cfg.low_threshold);
        let init_cth = format!("{:.0}", draft_cfg.critical_threshold);
        let init_mcp = draft_cfg.mcp_listen.clone();

        let bark_url_input = cx.new(|cx| {
            let mut st =
                InputState::new(window, cx).placeholder("https://api.day.app/your_device_key");
            st.set_value(init_bark, window, cx);
            st
        });
        let webhook_url_input = cx.new(|cx| {
            let mut st = InputState::new(window, cx)
                .placeholder("https://open.feishu.cn/open-apis/bot/v2/hook/...");
            st.set_value(init_wh, window, cx);
            st
        });
        let poll_secs_input = cx.new(|cx| {
            let mut st = InputState::new(window, cx).placeholder("300");
            st.set_value(init_poll, window, cx);
            st
        });
        let start_hour_input = cx.new(|cx| {
            let mut st = InputState::new(window, cx).placeholder("7");
            st.set_value(init_sh, window, cx);
            st
        });
        let end_hour_input = cx.new(|cx| {
            let mut st = InputState::new(window, cx).placeholder("22");
            st.set_value(init_eh, window, cx);
            st
        });
        let notice_th_input = cx.new(|cx| {
            let mut st = InputState::new(window, cx).placeholder("50");
            st.set_value(init_nth, window, cx);
            st
        });
        let low_th_input = cx.new(|cx| {
            let mut st = InputState::new(window, cx).placeholder("20");
            st.set_value(init_lth, window, cx);
            st
        });
        let crit_th_input = cx.new(|cx| {
            let mut st = InputState::new(window, cx).placeholder("10");
            st.set_value(init_cth, window, cx);
            st
        });
        let mcp_listen_input = cx.new(|cx| {
            let mut st = InputState::new(window, cx).placeholder("127.0.0.1:58318");
            st.set_value(init_mcp, window, cx);
            st
        });

        let ext_email_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("user@example.com"));
        let ext_label_input =
            cx.new(|cx| InputState::new(window, cx).placeholder(t("备注名称（可选，如主力号）")));
        let ext_token_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder("sk-ant-oat01-... / Bearer access_token")
        });
        let ext_proxy_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder(t("专属代理 URL（留空跟随全局）"))
        });

        cx.observe(&store, |_, _, cx| cx.notify()).detach();

        Self {
            store,
            bark_url_input,
            webhook_url_input,
            poll_secs_input,
            start_hour_input,
            end_hour_input,
            notice_th_input,
            low_th_input,
            crit_th_input,
            mcp_listen_input,
            ext_email_input,
            ext_label_input,
            ext_token_input,
            ext_proxy_input,
            draft_cfg,
        }
    }

    fn collect_config_from_inputs(&mut self, cx: &App) -> LamplighterConfig {
        let mut cfg = self.draft_cfg.clone();
        cfg.bark_url = self.bark_url_input.read(cx).value().to_string();
        cfg.webhook.url = self.webhook_url_input.read(cx).value().to_string();
        if let Ok(v) = self.poll_secs_input.read(cx).value().trim().parse::<u64>() {
            cfg.poll_interval_seconds = v;
        }
        if let Ok(v) = self.start_hour_input.read(cx).value().trim().parse::<u8>() {
            cfg.ignition.start_hour = v;
        }
        if let Ok(v) = self.end_hour_input.read(cx).value().trim().parse::<u8>() {
            cfg.ignition.end_hour = v;
        }
        if let Ok(v) = self.notice_th_input.read(cx).value().trim().parse::<f64>() {
            cfg.notice_threshold = v;
        }
        if let Ok(v) = self.low_th_input.read(cx).value().trim().parse::<f64>() {
            cfg.low_threshold = v;
        }
        if let Ok(v) = self.crit_th_input.read(cx).value().trim().parse::<f64>() {
            cfg.critical_threshold = v;
        }
        cfg.mcp_listen = self.mcp_listen_input.read(cx).value().to_string();
        cfg.normalize();
        self.draft_cfg = cfg.clone();
        cfg
    }

    fn format_clock(epoch: Option<i64>) -> String {
        let Some(ts) = epoch.filter(|v| *v > 0) else {
            return "-".to_string();
        };
        Local
            .timestamp_opt(ts, 0)
            .single()
            .map(|dt| dt.format("%m/%d %H:%M").to_string())
            .unwrap_or_else(|| "-".to_string())
    }

    fn redact_email(email: &str, hide: bool) -> String {
        if !hide {
            return email.to_string();
        }
        let local = email.split('@').next().unwrap_or(email);
        let chars: Vec<char> = local.chars().collect();
        let tail: String = if chars.len() <= 2 {
            chars.into_iter().collect()
        } else {
            chars[chars.len() - 2..].iter().collect()
        };
        format!("***{tail}")
    }

    fn render_group_card(
        &self,
        grp: &MonitoredGroupView,
        hide_emails: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let igniting = self
            .store
            .read(cx)
            .lamplighter_igniting_key
            .as_deref()
            == Some(grp.group_key.as_str());
        let store_ig = self.store.clone();
        let gkey = grp.group_key.clone();

        let bars: Vec<AnyElement> = grp
            .windows
            .iter()
            .map(|w| {
                let pct = w.remaining_percent.clamp(0.0, 100.0);
                let color = if pct <= 15.0 {
                    cx.theme().danger
                } else if pct <= 45.0 {
                    cx.theme().warning
                } else {
                    cx.theme().success
                };
                div()
                    .v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_1()
                    .child(
                        div()
                            .h_flex()
                            .w_full()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_xs()
                                    .font_medium()
                                    .child(format!("{}  {:.0}%", w.short_label, pct)),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(Self::format_clock(w.reset_at)),
                            ),
                    )
                    .child(
                        div()
                            .w_full()
                            .h(px(6.))
                            .rounded_full()
                            .bg(cx.theme().secondary)
                            .overflow_hidden()
                            .child(
                                div()
                                    .h_full()
                                    .rounded_full()
                                    .w(relative((pct / 100.0) as f32))
                                    .bg(color),
                            ),
                    )
                    .into_any_element()
            })
            .collect();

        let ig_status = if !grp.ignite_supported {
            t("周期自动切换，无需点火")
        } else if grp.seven_day_exhausted {
            t("7 天额度已用完，暂停自动点火")
        } else if let Some(ref reason) = grp.paused_reason {
            format!("{}: {reason}", t("熔断保护"))
        } else if let Some(next_at) = grp.next_ignition_at {
            format!("{}: {}", t("下次自动点火"), Self::format_clock(Some(next_at)))
        } else {
            t("自动点火未开启")
        };

        div()
            .v_flex()
            .w_full()
            .gap_3()
            .p_4()
            .rounded(px(10.))
            .bg(cx.theme().background)
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
                            .h_flex()
                            .items_center()
                            .gap_2()
                            .flex_wrap()
                            .child(
                                div()
                                    .text_sm()
                                    .font_semibold()
                                    .child(grp.display_label.clone()),
                            )
                            .when(!grp.plan_name.is_empty(), |r| {
                                r.child(AppBadge::info(grp.plan_name.clone()))
                            })
                            .when((grp.weight - 1.0).abs() > 0.01, |r| {
                                r.child(AppBadge::new(format!("×{}", grp.weight)))
                            })
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(Self::redact_email(&grp.account_email, hide_emails)),
                            ),
                    )
                    .when(grp.ignite_supported, |row| {
                        row.child(
                            AppButton::new(SharedString::from(format!(
                                "ll-ignite-{}",
                                grp.group_key
                            )))
                            .outline()
                            .small()
                            .disabled(igniting)
                            .icon(IconName::Zap)
                            .label(if igniting {
                                t("点火中...")
                            } else {
                                t("立即点火")
                            })
                            .on_click(move |_, _, cx| {
                                let k = gkey.clone();
                                store_ig.update(cx, |s, cx| {
                                    s.manual_ignite_lamplighter_group(k, cx)
                                });
                            }),
                        )
                    }),
            )
            .child(if bars.is_empty() {
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(t("等待首次主动轮询或点击右上角「立即刷新全部」读取配额"))
                    .into_any_element()
            } else {
                div().h_flex().w_full().gap_4().children(bars).into_any_element()
            })
            .child(
                div()
                    .h_flex()
                    .w_full()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(ig_status),
                    )
                    .when_some(grp.last_ignition_model.as_ref(), |r, m| {
                        r.child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(format!(
                                    "{}: {} ({})",
                                    t("最近点火"),
                                    m,
                                    Self::format_clock(grp.last_ignition_at)
                                )),
                        )
                    }),
            )
    }

    fn render_history_chart(cx: &App, series: &[HistorySeries], window_filter: &str) -> Div {
        let filtered: Vec<&HistorySeries> = series
            .iter()
            .filter(|s| s.window_id == window_filter)
            .collect();

        if filtered.is_empty() {
            return panel_card(
                cx,
                div()
                    .v_flex()
                    .w_full()
                    .py_8()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .font_medium()
                            .child(t("暂无额度历史采样数据")),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(t("每次主动轮询或经过本地协议代理的请求都会自动记录额度曲线样本。")),
                    ),
            );
        }

        let rows: Vec<AnyElement> = filtered
            .into_iter()
            .map(|ser| {
                // 取最近最多 36 个采样点渲染等宽柱状趋势条
                let len = ser.points.len();
                let slice = if len > 36 {
                    &ser.points[len - 36..]
                } else {
                    &ser.points[..]
                };
                let latest = slice.last().map(|p| p.remaining).unwrap_or(0.0);
                let bars: Vec<AnyElement> = slice
                    .iter()
                    .map(|pt| {
                        let h_pct = (pt.remaining.clamp(2.0, 100.0) / 100.0) as f32;
                        let col = if pt.remaining <= 15.0 {
                            cx.theme().danger
                        } else if pt.remaining <= 45.0 {
                            cx.theme().warning
                        } else {
                            cx.theme().primary
                        };
                        div()
                            .flex_1()
                            .h_full()
                            .flex()
                            .items_end()
                            .child(
                                div()
                                    .w_full()
                                    .h(relative(h_pct))
                                    .rounded(px(2.))
                                    .bg(col.opacity(0.85)),
                            )
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
                            .child(
                                div()
                                    .text_xs()
                                    .font_semibold()
                                    .child(format!("{} ({})", ser.label, ser.window_id)),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .font_semibold()
                                    .text_color(cx.theme().primary)
                                    .child(format!("{}: {:.1}%", t("最新剩余"), latest)),
                            ),
                    )
                    .child(
                        div()
                            .h_flex()
                            .w_full()
                            .h(px(76.))
                            .gap_1()
                            .items_end()
                            .p_2()
                            .rounded(px(6.))
                            .bg(cx.theme().background)
                            .children(bars),
                    )
                    .into_any_element()
            })
            .collect();

        panel_card(
            cx,
            div()
                .v_flex()
                .w_full()
                .gap_3()
                .child(
                    div()
                        .text_base()
                        .font_semibold()
                        .child(t("额度变化历史趋势")),
                )
                .children(rows),
        )
    }

    fn render_settings_tab(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let enabled = self.draft_cfg.enabled;
        let ig_enabled = self.draft_cfg.ignition.enabled;
        let codex_ig = self.draft_cfg.provider_rule("codex").ignite;
        let ag_ig = self.draft_cfg.provider_rule("antigravity").ignite;
        let claude_ig = self.draft_cfg.provider_rule("claude").ignite;
        let reset_sig = self.draft_cfg.codex_reset_updates.enabled;
        let mcp_en = self.draft_cfg.mcp_enabled;
        let mcp_key = self.draft_cfg.mcp_key.clone();
        let preset_now = self.draft_cfg.webhook.preset.clone();

        div()
            .v_flex()
            .w_full()
            .gap_4()
            // 1. 监控总开关与自动点火日程
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
                                    .v_flex()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_base()
                                            .font_semibold()
                                            .child(t("后台监控与 5 小时窗口自动点火")),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(t("在每个 5 小时额度窗口重置后 3 秒自动发送极小 OK 请求（首包 2 秒即断流），让下一周期无缝开始计时")),
                                    ),
                            )
                            .child(
                                Switch::new("ll-cfg-enabled")
                                    .checked(enabled)
                                    .on_click(cx.listener(|v, checked: &bool, _, cx| {
                                        v.draft_cfg.enabled = *checked;
                                        let cfg = v.collect_config_from_inputs(cx);
                                        v.store.update(cx, |s, cx| s.save_lamplighter_config(cfg, cx));
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .h_flex()
                            .w_full()
                            .gap_3()
                            .items_center()
                            .flex_wrap()
                            .child(
                                div()
                                    .h_flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(t("自动点火开关：")),
                                    )
                                    .child(
                                        Switch::new("ll-cfg-ig-en")
                                            .checked(ig_enabled)
                                            .on_click(cx.listener(|v, checked: &bool, _, cx| {
                                                v.draft_cfg.ignition.enabled = *checked;
                                                cx.notify();
                                            })),
                                    ),
                            )
                            .child(
                                div()
                                    .h_flex()
                                    .items_center()
                                    .gap_1p5()
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(t("开始小时：")),
                                    )
                                    .child(div().w(px(72.)).child(Input::new(&self.start_hour_input))),
                            )
                            .child(
                                div()
                                    .h_flex()
                                    .items_center()
                                    .gap_1p5()
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(t("结束小时：")),
                                    )
                                    .child(div().w(px(72.)).child(Input::new(&self.end_hour_input))),
                            )
                            .child(
                                div()
                                    .h_flex()
                                    .items_center()
                                    .gap_1p5()
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(t("轮询间隔(秒)：")),
                                    )
                                    .child(div().w(px(88.)).child(Input::new(&self.poll_secs_input))),
                            ),
                    )
                    .child(
                        div()
                            .h_flex()
                            .w_full()
                            .gap_3()
                            .flex_wrap()
                            .children(
                                [
                                    ("codex", "GPT / Codex 自动点火", codex_ig),
                                    ("antigravity", "Antigravity (Gemini) 自动点火", ag_ig),
                                    ("claude", "Claude 自动点火", claude_ig),
                                ]
                                .into_iter()
                                .map(|(pid, lbl, cur)| {
                                    div()
                                        .h_flex()
                                        .flex_1()
                                        .min_w(px(220.))
                                        .items_center()
                                        .justify_between()
                                        .p_3()
                                        .rounded(px(8.))
                                        .bg(cx.theme().secondary.opacity(0.24))
                                        .border_1()
                                        .border_color(cx.theme().border)
                                        .child(div().text_xs().font_medium().child(t(lbl)))
                                        .child(
                                            Switch::new(SharedString::from(format!(
                                                "ll-prov-ig-{pid}"
                                            )))
                                            .checked(cur)
                                            .on_click(cx.listener(move |v, checked: &bool, _, cx| {
                                                let entry = v
                                                    .draft_cfg
                                                    .providers
                                                    .entry(pid.to_string())
                                                    .or_default();
                                                entry.ignite = *checked;
                                                cx.notify();
                                            })),
                                        )
                                }),
                            ),
                    ),
            ))
            // 2. Bark 与通用 Webhook 推送 + 三档提醒阈值
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
                                    .text_base()
                                    .font_semibold()
                                    .child(t("Bark 与通用 Webhook 提醒推送")),
                            )
                            .child(
                                AppButton::new("ll-test-push")
                                    .outline()
                                    .small()
                                    .icon(IconName::Send)
                                    .label(t("发送测试推送"))
                                    .on_click(cx.listener(|v, _, _, cx| {
                                        let cfg = v.collect_config_from_inputs(cx);
                                        v.store.update(cx, |s, cx| s.test_lamplighter_push(cfg, cx));
                                    })),
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
                                    .child(t("Bark 推送地址（留空不启用 Bark）")),
                            )
                            .child(Input::new(&self.bark_url_input))
                            .child(
                                div()
                                    .h_flex()
                                    .items_center()
                                    .justify_between()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_medium()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(t("通用 Webhook 模板与地址（飞书 / 钉钉 / Telegram / ntfy）")),
                                    )
                                    .child(
                                        div().h_flex().gap_1p5().children(
                                            [
                                                ("feishu", "飞书"),
                                                ("dingtalk", "钉钉"),
                                                ("telegram", "Telegram"),
                                                ("ntfy", "ntfy"),
                                            ]
                                            .into_iter()
                                            .map(|(p, lbl)| {
                                                let active = preset_now == p;
                                                let b = AppButton::new(SharedString::from(
                                                    format!("ll-wh-preset-{p}"),
                                                ))
                                                .small()
                                                .label(lbl);
                                                let b = if active {
                                                    b.selected(true)
                                                } else {
                                                    b.outline()
                                                };
                                                b.on_click(cx.listener(move |v, _, _, cx| {
                                                    v.draft_cfg.webhook.preset = p.to_string();
                                                    cx.notify();
                                                }))
                                            }),
                                        ),
                                    ),
                            )
                            .child(Input::new(&self.webhook_url_input)),
                    )
                    .child(
                        div()
                            .h_flex()
                            .w_full()
                            .items_center()
                            .justify_between()
                            .gap_3()
                            .flex_wrap()
                            .child(
                                div()
                                    .h_flex()
                                    .items_center()
                                    .gap_3()
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(t("三档提醒阈值(%)：")),
                                    )
                                    .child(div().w(px(72.)).child(Input::new(&self.notice_th_input)))
                                    .child(div().w(px(72.)).child(Input::new(&self.low_th_input)))
                                    .child(div().w(px(72.)).child(Input::new(&self.crit_th_input))),
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
                                            .child(t("转发 Did Codex Reset 排期信号")),
                                    )
                                    .child(
                                        Switch::new("ll-did-codex-reset")
                                            .checked(reset_sig)
                                            .on_click(cx.listener(|v, checked: &bool, _, cx| {
                                                v.draft_cfg.codex_reset_updates.enabled = *checked;
                                                cx.notify();
                                            })),
                                    ),
                            ),
                    ),
            ))
            // 3. 本地 Streamable HTTP /mcp 服务与保存栏
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
                                    .v_flex()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_base()
                                            .font_semibold()
                                            .child(t("本地 Streamable HTTP MCP 脱敏配额接口")),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(t("供编码 Agent 查询实时脱敏配额与近期重置事件（常量时间 SHA-256 Bearer 鉴权）")),
                                    ),
                            )
                            .child(
                                Switch::new("ll-mcp-en")
                                    .checked(mcp_en)
                                    .on_click(cx.listener(|v, checked: &bool, _, cx| {
                                        v.draft_cfg.mcp_enabled = *checked;
                                        if *checked && v.draft_cfg.mcp_key.trim().is_empty() {
                                            v.draft_cfg.mcp_key = generate_mcp_bearer_key();
                                        }
                                        cx.notify();
                                    })),
                            ),
                    )
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
                                    .flex_1()
                                    .child(div().w(px(210.)).child(Input::new(&self.mcp_listen_input)))
                                    .when(!mcp_key.is_empty(), |r| {
                                        let key_cp = mcp_key.clone();
                                        r.child(
                                            AppButton::new("ll-copy-mcp-key")
                                                .outline()
                                                .small()
                                                .icon(IconName::Copy)
                                                .label(t("复制 MCP 密钥"))
                                                .on_click(move |_, _, cx| {
                                                    cx.write_to_clipboard(
                                                        ClipboardItem::new_string(key_cp.clone()),
                                                    );
                                                }),
                                        )
                                    })
                                    .child(
                                        AppButton::new("ll-regen-mcp-key")
                                            .outline()
                                            .small()
                                            .icon(IconName::RefreshCw)
                                            .label(t("重置密钥"))
                                            .on_click(cx.listener(|v, _, _, cx| {
                                                v.draft_cfg.mcp_key = generate_mcp_bearer_key();
                                                cx.notify();
                                            })),
                                    ),
                            )
                            .child(
                                AppButton::new("ll-save-all-cfg")
                                    .primary()
                                    .small()
                                    .icon(IconName::Save)
                                    .label(t("保存全部设置"))
                                    .on_click(cx.listener(|v, _, _, cx| {
                                        let cfg = v.collect_config_from_inputs(cx);
                                        v.store.update(cx, |s, cx| s.save_lamplighter_config(cfg, cx));
                                    })),
                            ),
                    ),
            ))
    }
}

impl Render for LamplighterView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (snap, tab, hours, win_filter, hide_emails, loading, add_open, new_prov) = {
            let s = self.store.read(cx);
            (
                s.lamplighter_snapshot.clone(),
                s.lamplighter_tab,
                s.lamplighter_history_hours,
                s.lamplighter_window_filter,
                s.lamplighter_hide_emails,
                s.lamplighter_loading,
                s.lamplighter_add_account_open,
                s.lamplighter_new_provider,
            )
        };

        let groups = snap.as_ref().map(|s| s.groups.clone()).unwrap_or_default();
        let events = snap.as_ref().map(|s| s.events.clone()).unwrap_or_default();
        let series = snap
            .as_ref()
            .map(|s| s.history_series.clone())
            .unwrap_or_default();
        let ext_accounts = snap
            .as_ref()
            .map(|s| s.external_accounts.clone())
            .unwrap_or_default();
        let next_poll = snap.as_ref().map(|s| s.next_poll_at).unwrap_or(0);

        let st_tab = self.store.clone();
        let st_eye = self.store.clone();
        let st_ref = self.store.clone();

        div()
            .v_flex()
            .w_full()
            .gap_4()
            // 顶部页签与快捷动作条
            .child(panel_card(
                cx,
                div()
                    .h_flex()
                    .w_full()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .flex_wrap()
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .gap_2()
                            .children(
                                [
                                    ("overview", t("额度与自动点火")),
                                    ("history", t("趋势曲线与事件")),
                                    ("settings", t("推送与规则设置")),
                                ]
                                .into_iter()
                                .map(|(id, lbl)| {
                                    let active = tab == id;
                                    let st = st_tab.clone();
                                    let b = AppButton::new(SharedString::from(format!(
                                        "ll-tab-{id}"
                                    )))
                                    .small()
                                    .label(lbl);
                                    let b = if active { b.selected(true) } else { b.outline() };
                                    b.on_click(move |_, _, cx| {
                                        st.update(cx, |s, cx| {
                                            s.lamplighter_tab = id;
                                            cx.notify();
                                        });
                                    })
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
                                    .child(tf(
                                        "已监控 {0} 个额度组 · 下次轮询 {1}",
                                        &[
                                            &groups.len().to_string(),
                                            &Self::format_clock(Some(next_poll)),
                                        ],
                                    )),
                            )
                            .child(
                                AppButton::new("ll-toggle-privacy")
                                    .outline()
                                    .small()
                                    .icon(if hide_emails {
                                        IconName::EyeOff
                                    } else {
                                        IconName::Eye
                                    })
                                    .label(if hide_emails {
                                        t("显示邮箱")
                                    } else {
                                        t("隐藏邮箱")
                                    })
                                    .on_click(move |_, _, cx| {
                                        st_eye.update(cx, |s, cx| {
                                            s.lamplighter_hide_emails = !s.lamplighter_hide_emails;
                                            cx.notify();
                                        });
                                    }),
                            )
                            .child(
                                AppButton::new("ll-refresh-now")
                                    .primary()
                                    .small()
                                    .disabled(loading)
                                    .icon(IconName::RefreshCw)
                                    .label(if loading {
                                        t("刷新中...")
                                    } else {
                                        t("立即刷新全部")
                                    })
                                    .on_click(move |_, _, cx| {
                                        st_ref.update(cx, |s, cx| s.refresh_lamplighter_remote(cx));
                                    }),
                            ),
                    ),
            ))
            // 主体区域
            .child(match tab {
                "history" => {
                    let st_hr = self.store.clone();
                    let st_wf = self.store.clone();
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
                                .child(
                                    div().h_flex().gap_2().children(
                                        [
                                            ("five-hour", t("5 小时窗口")),
                                            ("seven-day", t("7 天窗口")),
                                        ]
                                        .into_iter()
                                        .map(|(wf, lbl)| {
                                            let active = win_filter == wf;
                                            let st = st_wf.clone();
                                            let b = AppButton::new(SharedString::from(format!(
                                                "ll-wf-{wf}"
                                            )))
                                            .small()
                                            .label(lbl);
                                            let b = if active {
                                                b.selected(true)
                                            } else {
                                                b.outline()
                                            };
                                            b.on_click(move |_, _, cx| {
                                                st.update(cx, |s, cx| {
                                                    s.lamplighter_window_filter = wf;
                                                    cx.notify();
                                                });
                                            })
                                        }),
                                    ),
                                )
                                .child(
                                    div().h_flex().gap_2().children(
                                        [
                                            (12u64, "12H"),
                                            (24u64, "24H"),
                                            (168u64, "7D"),
                                            (720u64, "30D"),
                                        ]
                                        .into_iter()
                                        .map(|(h, lbl)| {
                                            let active = hours == h;
                                            let st = st_hr.clone();
                                            let b = AppButton::new(SharedString::from(format!(
                                                "ll-hrs-{h}"
                                            )))
                                            .small()
                                            .label(lbl);
                                            let b = if active {
                                                b.selected(true)
                                            } else {
                                                b.outline()
                                            };
                                            b.on_click(move |_, _, cx| {
                                                st.update(cx, |s, cx| {
                                                    s.set_lamplighter_history_hours(h, cx);
                                                });
                                            })
                                        }),
                                    ),
                                ),
                        )
                        .child(Self::render_history_chart(cx, &series, win_filter))
                        .child(panel_card(
                            cx,
                            div()
                                .v_flex()
                                .w_full()
                                .gap_3()
                                .child(
                                    div()
                                        .text_base()
                                        .font_semibold()
                                        .child(t("最近点火与告警事件记录")),
                                )
                                .child(if events.is_empty() {
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(t("暂无历史事件记录。"))
                                        .into_any_element()
                                } else {
                                    let ev_rows: Vec<AnyElement> = events
                                        .iter()
                                        .take(25)
                                        .map(|ev| {
                                            div()
                                                .h_flex()
                                                .w_full()
                                                .items_center()
                                                .justify_between()
                                                .gap_3()
                                                .p_2p5()
                                                .rounded(px(8.))
                                                .bg(cx.theme().secondary.opacity(0.24))
                                                .child(
                                                    div()
                                                        .text_xs()
                                                        .text_color(cx.theme().foreground)
                                                        .child(ev.message.clone()),
                                                )
                                                .child(
                                                    div()
                                                        .text_xs()
                                                        .text_color(cx.theme().muted_foreground)
                                                        .child(Self::format_clock(Some(
                                                            ev.timestamp,
                                                        ))),
                                                )
                                                .into_any_element()
                                        })
                                        .collect();
                                    div().v_flex().w_full().gap_2().children(ev_rows).into_any_element()
                                }),
                        ))
                        .into_any_element()
                }
                "settings" => self.render_settings_tab(cx).into_any_element(),
                _ => {
                    let st_open_add = self.store.clone();
                    let st_prov = self.store.clone();
                    let st_do_add = self.store.clone();
                    let em_i = self.ext_email_input.clone();
                    let lb_i = self.ext_label_input.clone();
                    let tk_i = self.ext_token_input.clone();
                    let px_i = self.ext_proxy_input.clone();

                    div()
                        .v_flex()
                        .w_full()
                        .gap_4()
                        .child(if groups.is_empty() {
                            panel_card(
                                cx,
                                div()
                                    .v_flex()
                                    .w_full()
                                    .py_8()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_medium()
                                            .child(t("尚未发现可监控的账号")),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(t("Lamplighter 会自动同步 Swivox 中的 Codex 官方账号与 Antigravity 账号；您也可以在下方添加独立的 Claude / Kimi / Grok 凭据。")),
                                    ),
                            )
                            .into_any_element()
                        } else {
                            let cards: Vec<AnyElement> = groups
                                .iter()
                                .map(|g| {
                                    self.render_group_card(g, hide_emails, cx)
                                        .into_any_element()
                                })
                                .collect();
                            div().v_flex().w_full().gap_3().children(cards).into_any_element()
                        })
                        // 扩展多服务凭据卡片
                        .child(panel_card(
                            cx,
                            div()
                                .v_flex()
                                .w_full()
                                .gap_3()
                                .child(
                                    div()
                                        .h_flex()
                                        .w_full()
                                        .items_center()
                                        .justify_between()
                                        .child(
                                            div()
                                                .v_flex()
                                                .gap_1()
                                                .child(
                                                    div()
                                                        .text_sm()
                                                        .font_semibold()
                                                        .child(t("扩展监控凭据（Claude / Kimi / Grok）")),
                                                )
                                                .child(
                                                    div()
                                                        .text_xs()
                                                        .text_color(cx.theme().muted_foreground)
                                                        .child(t("Codex 与 Antigravity 账号已自动从 Swivox 读取，无需重复添加")),
                                                ),
                                        )
                                        .child(
                                            AppButton::new("ll-toggle-add-ext")
                                                .outline()
                                                .small()
                                                .icon(IconName::Plus)
                                                .label(if add_open {
                                                    t("收起")
                                                } else {
                                                    t("添加凭据")
                                                })
                                                .on_click(move |_, _, cx| {
                                                    st_open_add.update(cx, |s, cx| {
                                                        s.lamplighter_add_account_open =
                                                            !s.lamplighter_add_account_open;
                                                        cx.notify();
                                                    });
                                                }),
                                        ),
                                )
                                .when(add_open, |col| {
                                    col.child(
                                        div()
                                            .v_flex()
                                            .w_full()
                                            .gap_2p5()
                                            .p_3()
                                            .rounded(px(8.))
                                            .bg(cx.theme().secondary.opacity(0.24))
                                            .child(
                                                div().h_flex().gap_2().children(
                                                    [
                                                        ("claude", "Claude"),
                                                        ("kimi", "Kimi"),
                                                        ("xai", "Grok (xAI)"),
                                                    ]
                                                    .into_iter()
                                                    .map(|(p, lbl)| {
                                                        let active = new_prov == p;
                                                        let st = st_prov.clone();
                                                        let b = AppButton::new(SharedString::from(
                                                            format!("ll-new-prov-{p}"),
                                                        ))
                                                        .small()
                                                        .label(lbl);
                                                        let b = if active {
                                                            b.selected(true)
                                                        } else {
                                                            b.outline()
                                                        };
                                                        b.on_click(move |_, _, cx| {
                                                            st.update(cx, |s, cx| {
                                                                s.lamplighter_new_provider = p;
                                                                cx.notify();
                                                            });
                                                        })
                                                    }),
                                                ),
                                            )
                                            .child(
                                                div()
                                                    .h_flex()
                                                    .w_full()
                                                    .gap_2()
                                                    .child(
                                                        div()
                                                            .flex_1()
                                                            .child(Input::new(&self.ext_email_input)),
                                                    )
                                                    .child(
                                                        div()
                                                            .flex_1()
                                                            .child(Input::new(&self.ext_label_input)),
                                                    ),
                                            )
                                            .child(Input::new(&self.ext_token_input))
                                            .child(
                                                div()
                                                    .h_flex()
                                                    .w_full()
                                                    .gap_2()
                                                    .child(
                                                        div()
                                                            .flex_1()
                                                            .child(Input::new(&self.ext_proxy_input)),
                                                    )
                                                    .child(
                                                        AppButton::new("ll-confirm-add-ext")
                                                            .primary()
                                                            .small()
                                                            .label(t("确认添加"))
                                                            .on_click(move |_, _, cx| {
                                                                let em = em_i
                                                                    .read(cx)
                                                                    .value()
                                                                    .to_string();
                                                                let lb = lb_i
                                                                    .read(cx)
                                                                    .value()
                                                                    .to_string();
                                                                let tk = tk_i
                                                                    .read(cx)
                                                                    .value()
                                                                    .to_string();
                                                                let px = px_i
                                                                    .read(cx)
                                                                    .value()
                                                                    .to_string();
                                                                let prov = new_prov.to_string();
                                                                st_do_add.update(cx, |s, cx| {
                                                                    s.add_lamplighter_external_account(
                                                                        prov,
                                                                        em,
                                                                        (!lb.trim().is_empty())
                                                                            .then_some(lb),
                                                                        tk,
                                                                        (!px.trim().is_empty())
                                                                            .then_some(px),
                                                                        cx,
                                                                    );
                                                                });
                                                            }),
                                                    ),
                                            ),
                                    )
                                })
                                .children(ext_accounts.into_iter().map(|ext| {
                                    let id = ext.id.clone();
                                    let st_del = self.store.clone();
                                    div()
                                        .h_flex()
                                        .w_full()
                                        .items_center()
                                        .justify_between()
                                        .p_2p5()
                                        .rounded(px(8.))
                                        .bg(cx.theme().secondary.opacity(0.18))
                                        .child(
                                            div()
                                                .h_flex()
                                                .items_center()
                                                .gap_2()
                                                .child(AppBadge::new(ext.provider.to_uppercase()))
                                                .child(
                                                    div()
                                                        .text_xs()
                                                        .font_medium()
                                                        .child(Self::redact_email(
                                                            &ext.email,
                                                            hide_emails,
                                                        )),
                                                ),
                                        )
                                        .child(
                                            AppButton::new(SharedString::from(format!(
                                                "ll-del-ext-{}",
                                                ext.id
                                            )))
                                            .ghost()
                                            .small_icon()
                                            .icon(IconName::Trash)
                                            .on_click(move |_, _, cx| {
                                                let id = id.clone();
                                                st_del.update(cx, |s, cx| {
                                                    s.remove_lamplighter_external_account(id, cx)
                                                });
                                            }),
                                        )
                                })),
                        ))
                        .into_any_element()
                }
            })
    }
}
