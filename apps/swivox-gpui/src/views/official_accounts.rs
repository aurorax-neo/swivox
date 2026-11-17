use swivox_backend::i18n::t;
use swivox_backend::phase2_rules::{
    QuotaTone, account_initial, build_account_quota_windows, format_plan_display_name,
    resolve_official_account_mode,
};
use swivox_core::official_accounts::OfficialAccountView;
use gpui_kit::assets::IconName;
use gpui_kit::component::Icon;
use gpui_kit::component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use gpui_kit::component::theme::ActiveTheme as _;
use gpui_kit::component::{Sizable as _, StyledExt as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::store::AppStore;
use crate::views::{avatar_badge, panel_card};
use crate::widgets::{AppBadge, AppButton, AppSelectTrigger};

const PAGE_SIZE: usize = 8;

pub struct OfficialAccountsView {
    store: Entity<AppStore>,
    search_input: Entity<InputState>,
    oauth_label_input: Entity<InputState>,
    manual_callback_input: Entity<InputState>,
    import_label_input: Entity<InputState>,
    import_body_input: Entity<TextareaState>,
    edit_label_input: Entity<InputState>,
    edit_proxy_input: Entity<InputState>,
    edit_proxy_mode: String,
}

impl OfficialAccountsView {
    pub fn new(store: Entity<AppStore>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder(t("搜索邮箱、备注或计划"))
        });
        let oauth_label_input =
            cx.new(|cx| InputState::new(window, cx).placeholder(t("例如：主力 Plus / Team")));
        let manual_callback_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("http://localhost:1455/auth/callback?code=...")
        });
        let import_label_input =
            cx.new(|cx| InputState::new(window, cx).placeholder(t("例如：备用 Pro 账号")));
        let import_body_input = cx.new(|cx| {
            TextareaState::new(window, cx).placeholder(
                "{\"auth_mode\":\"chatgpt\",\"tokens\":{\"access_token\":\"...\",\"refresh_token\":\"...\"}}",
            )
        });
        let edit_label_input =
            cx.new(|cx| InputState::new(window, cx).placeholder(t("例如：主力 Plus")));
        let edit_proxy_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("http://127.0.0.1:7890"));

        let store_for_search = store.clone();
        cx.subscribe(&search_input, move |_this, input, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                let query = input.read(cx).value().to_string();
                store_for_search.update(cx, |s, cx| {
                    s.official_search_query = query;
                    s.official_page = 1;
                    cx.notify();
                });
            }
        })
        .detach();

        cx.observe(&store, |_, _, cx| cx.notify()).detach();

        Self {
            store,
            search_input,
            oauth_label_input,
            manual_callback_input,
            import_label_input,
            import_body_input,
            edit_label_input,
            edit_proxy_input,
            edit_proxy_mode: "inherit".to_string(),
        }
    }

    fn render_metric_card(
        cx: &App,
        label: impl Into<SharedString>,
        value: impl Into<SharedString>,
    ) -> Div {
        div()
            .v_flex()
            .flex_1()
            .min_w_0()
            .gap_1()
            .p_3p5()
            .rounded(px(10.))
            .bg(cx.theme().secondary.opacity(0.32))
            .border_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(label.into()),
            )
            .child(
                div()
                    .text_sm()
                    .font_semibold()
                    .text_color(cx.theme().foreground)
                    .child(value.into()),
            )
    }

    fn render_oauth_box(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let session = self.store.read(cx).oauth_session.clone()?;
        let auth_url_copy = session.auth_url.clone();
        let auth_url_open = session.auth_url.clone();
        let store_copy = self.store.clone();
        let store_open = self.store.clone();
        let store_cancel = self.store.clone();
        let store_submit = self.store.clone();
        let label_input = self.oauth_label_input.clone();
        let callback_input = self.manual_callback_input.clone();

        Some(
            div()
                .v_flex()
                .w_full()
                .gap_3()
                .p_4()
                .rounded(px(10.))
                .bg(cx.theme().info.opacity(0.08))
                .border_1()
                .border_color(cx.theme().info.opacity(0.4))
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
                                .child(format!(
                                    "{}（127.0.0.1:{}）",
                                    t("正在等待浏览器完成授权"),
                                    session.callback_port
                                )),
                        )
                        .child(
                            div()
                                .h_flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    AppButton::new("oauth-copy-url")
                                        .secondary()
                                        .small()
                                        .icon(IconName::Copy)
                                        .label(t("复制授权链接"))
                                        .on_click(move |_, _, cx| {
                                            cx.write_to_clipboard(ClipboardItem::new_string(
                                                auth_url_copy.clone(),
                                            ));
                                            store_copy.update(cx, |s, cx| {
                                                s.set_notice(
                                                    t("授权链接"),
                                                    t("已复制授权链接到剪贴板。"),
                                                    "ok",
                                                    cx,
                                                );
                                            });
                                        }),
                                )
                                .child(
                                    AppButton::new("oauth-open-browser")
                                        .secondary()
                                        .small()
                                        .icon(IconName::ExternalLink)
                                        .label(t("打开浏览器"))
                                        .on_click(move |_, _, cx| {
                                            let url = auth_url_open.clone();
                                            store_open
                                                .update(cx, |s, cx| s.open_external_url(url, cx));
                                        }),
                                )
                                .child(
                                    AppButton::new("oauth-cancel")
                                        .ghost()
                                        .small()
                                        .label(t("取消"))
                                        .on_click(move |_, _, cx| {
                                            store_cancel
                                                .update(cx, |s, cx| s.cancel_official_oauth(cx));
                                        }),
                                ),
                        ),
                )
                .child(
                    div()
                        .h_flex()
                        .w_full()
                        .gap_3()
                        .child(
                            div()
                                .v_flex()
                                .flex_1()
                                .gap_1p5()
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(t("账号备注（可选）")),
                                )
                                .child(Input::new(&self.oauth_label_input)),
                        )
                        .child(
                            div()
                                .v_flex()
                                .flex_1()
                                .gap_1p5()
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(t("手动回调链接（远程浏览器未自动跳回时粘贴）")),
                                )
                                .child(Input::new(&self.manual_callback_input)),
                        ),
                )
                .child(
                    div().h_flex().child(
                        AppButton::new("oauth-submit-manual-callback")
                            .primary()
                            .small()
                            .label(t("提交手动回调"))
                            .on_click(move |_, window, cx| {
                                let cb = callback_input.read(cx).value().to_string();
                                let lbl = label_input.read(cx).value().trim().to_string();
                                callback_input.update(cx, |inp, cx| inp.set_value("", window, cx));
                                store_submit.update(cx, |s, cx| {
                                    s.submit_manual_oauth_callback(
                                        cb,
                                        (!lbl.is_empty()).then_some(lbl),
                                        cx,
                                    );
                                });
                            }),
                    ),
                )
                .into_any_element(),
        )
    }

    fn render_import_box(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let data = self.store.read(cx);
        if !data.import_open {
            return None;
        }
        let importing = data.importing_official;
        let store_confirm = self.store.clone();
        let store_close = self.store.clone();
        let label_input = self.import_label_input.clone();
        let body_input = self.import_body_input.clone();

        Some(
            div()
                .v_flex()
                .w_full()
                .gap_3()
                .p_4()
                .rounded(px(10.))
                .bg(cx.theme().secondary.opacity(0.32))
                .border_1()
                .border_color(cx.theme().border)
                .child(
                    div()
                        .v_flex()
                        .gap_1p5()
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(t("备注名称（可选）")),
                        )
                        .child(Input::new(&self.import_label_input)),
                )
                .child(
                    div()
                        .v_flex()
                        .gap_1p5()
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(t("粘贴 auth.json、导出 JSON 数组、Refresh Token 或 Access Token")),
                        )
                        .child(Textarea::new(&self.import_body_input).h(px(110.))),
                )
                .child(
                    div()
                        .h_flex()
                        .items_center()
                        .gap_2()
                        .child(
                            AppButton::new("official-confirm-import")
                                .primary()
                                .small()
                                .disabled(importing)
                                .label(t("确认导入"))
                                .on_click(move |_, window, cx| {
                                    let text = body_input.read(cx).value().to_string();
                                    let lbl = label_input.read(cx).value().trim().to_string();
                                    if !text.trim().is_empty() {
                                        body_input
                                            .update(cx, |inp, cx| inp.set_value("", window, cx));
                                        label_input
                                            .update(cx, |inp, cx| inp.set_value("", window, cx));
                                    }
                                    store_confirm.update(cx, |s, cx| {
                                        s.import_official_from_text(
                                            text,
                                            (!lbl.is_empty()).then_some(lbl),
                                            cx,
                                        );
                                    });
                                }),
                        )
                        .child(
                            AppButton::new("official-close-import")
                                .secondary()
                                .small()
                                .label(t("收起"))
                                .on_click(move |_, _, cx| {
                                    store_close.update(cx, |s, cx| {
                                        s.import_open = false;
                                        cx.notify();
                                    });
                                }),
                        ),
                )
                .into_any_element(),
        )
    }

    fn render_export_box(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let text = self.store.read(cx).export_json_text.clone();
        if text.is_empty() {
            return None;
        }
        let store_close = self.store.clone();
        let preview = if text.chars().count() > 600 {
            let head: String = text.chars().take(600).collect();
            format!("{head}…")
        } else {
            text
        };

        Some(
            div()
                .v_flex()
                .w_full()
                .gap_2p5()
                .p_4()
                .rounded(px(10.))
                .bg(cx.theme().secondary.opacity(0.32))
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
                                .text_sm()
                                .font_semibold()
                                .text_color(cx.theme().foreground)
                                .child(t("账号导出 JSON（已复制到剪贴板）")),
                        )
                        .child(
                            AppButton::new("official-close-export")
                                .ghost()
                                .small_icon()
                                .icon(IconName::X)
                                .on_click(move |_, _, cx| {
                                    store_close.update(cx, |s, cx| {
                                        s.export_json_text.clear();
                                        cx.notify();
                                    });
                                }),
                        ),
                )
                .child(
                    div()
                        .p_3()
                        .rounded(px(8.))
                        .bg(cx.theme().background)
                        .border_1()
                        .border_color(cx.theme().border)
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(preview),
                )
                .into_any_element(),
        )
    }

    fn render_account_card(
        &self,
        acc: &OfficialAccountView,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let data = self.store.read(cx);
        let profiles = &data.settings.relay_profiles;
        let is_editing = data.editing_account_id.as_deref() == Some(acc.id.as_str());
        let dropdown_open = data.open_binding_dropdown_for.as_deref() == Some(acc.id.as_str());
        let switching = data.switching_account_id.as_deref() == Some(acc.id.as_str());
        let refreshing_this = data.refreshing_quota_id.as_deref() == Some(acc.id.as_str());
        let multi_pending = data
            .multi_launch_pending
            .as_deref()
            .is_some_and(|p| p == format!("official:{}", acc.id));

        let plan_raw = acc
            .plan_type
            .as_deref()
            .or_else(|| acc.quota.as_ref().and_then(|q| q.plan_type.as_deref()));
        let plan_label = format_plan_display_name(plan_raw);
        let quota_windows = build_account_quota_windows(acc.quota.as_ref());
        let bound_relay = acc
            .bound_relay_profile_id
            .as_deref()
            .and_then(|id| profiles.iter().find(|p| p.id == id));

        let mut sub_parts = Vec::new();
        if let Some(name) = acc.account_name.as_deref().filter(|s| !s.trim().is_empty()) {
            sub_parts.push(name.to_string());
        }
        sub_parts.push(if acc.has_refresh_token {
            t("可自动续期")
        } else {
            t("仅 Access Token")
        });
        sub_parts.push(match bound_relay {
            Some(r) => format!("{}：{}", t("已绑定"), r.name),
            None => t("纯官方模式"),
        });
        let sub_details = sub_parts.join(" · ");

        let current_binding_label = match bound_relay {
            Some(r) => {
                if r.name.trim().is_empty() {
                    r.id.clone()
                } else {
                    r.name.clone()
                }
            }
            None => t("不绑定（纯官方登录）"),
        };

        // 操作克隆句柄
        let acc_use = acc.clone();
        let store_use = self.store.clone();
        let acc_id_multi = acc.id.clone();
        let store_multi = self.store.clone();
        let acc_id_quota = acc.id.clone();
        let store_quota = self.store.clone();
        let acc_id_edit = acc.id.clone();
        let initial_edit_label = acc.label.clone().unwrap_or_default();
        let initial_edit_pmode = acc.proxy_mode.clone();
        let initial_edit_purl = acc.egress_proxy_url.clone().unwrap_or_default();
        let edit_input = self.edit_label_input.clone();
        let edit_proxy_inp = self.edit_proxy_input.clone();
        let store_edit_toggle = self.store.clone();
        let proxy_nodes = data.network_proxy_snapshot.config.nodes.clone();
        let acc_del = acc.clone();
        let store_del = self.store.clone();

        // 配额区域
        let quota_section: AnyElement = if let Some(err) = acc
            .quota
            .as_ref()
            .and_then(|q| q.error.as_deref())
            .filter(|e| !e.trim().is_empty())
        {
            div()
                .text_xs()
                .text_color(cx.theme().danger)
                .child(err.to_string())
                .into_any_element()
        } else if !quota_windows.is_empty() {
            let items: Vec<AnyElement> = quota_windows
                .into_iter()
                .map(|win| {
                    let bar_color = match win.tone {
                        QuotaTone::High => cx.theme().success,
                        QuotaTone::Medium => cx.theme().warning,
                        QuotaTone::Low => cx.theme().danger,
                    };
                    let pct_clamped = win.percentage.clamp(0, 100) as f32 / 100.0;
                    div()
                        .v_flex()
                        .min_w(px(190.))
                        .flex_1()
                        .gap_1p5()
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
                                        .text_color(cx.theme().foreground)
                                        .child(win.label),
                                )
                                .child(
                                    div()
                                        .h_flex()
                                        .items_center()
                                        .gap_1p5()
                                        .child(
                                            div()
                                                .text_xs()
                                                .font_semibold()
                                                .text_color(bar_color)
                                                .child(format!("{}%", win.percentage)),
                                        )
                                        .when(!win.reset_text.is_empty(), |d| {
                                            d.child(
                                                div()
                                                    .text_size(px(11.))
                                                    .text_color(cx.theme().muted_foreground)
                                                    .child(win.reset_text),
                                            )
                                        }),
                                ),
                        )
                        .child(
                            div()
                                .w_full()
                                .h(px(6.))
                                .rounded_full()
                                .overflow_hidden()
                                .bg(cx.theme().secondary)
                                .child(
                                    div()
                                        .h_full()
                                        .w(relative(pct_clamped))
                                        .rounded_full()
                                        .bg(bar_color),
                                ),
                        )
                        .into_any_element()
                })
                .collect();
            div().h_flex().flex_wrap().w_full().gap_4().children(items).into_any_element()
        } else {
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(t("尚未查询配额，点击右侧刷新按钮获取实时配额。"))
                .into_any_element()
        };

        // 绑定供应商下拉选项列表
        let acc_id_dropdown = acc.id.clone();
        let store_dropdown_toggle = self.store.clone();
        let dropdown_options: Option<AnyElement> = if dropdown_open {
            let mut option_rows: Vec<AnyElement> = Vec::new();
            let current_bound_id = acc.bound_relay_profile_id.clone().unwrap_or_default();

            let mut choices: Vec<(String, String)> =
                vec![(String::new(), t("不绑定（纯官方登录）"))];
            for p in profiles {
                let label = if p.name.trim().is_empty() {
                    p.id.clone()
                } else {
                    p.name.clone()
                };
                choices.push((p.id.clone(), label));
            }

            for (opt_val, opt_label) in choices {
                let selected = opt_val == current_bound_id;
                let acc_bind = acc.clone();
                let store_bind = self.store.clone();
                let val_clone = opt_val.clone();

                option_rows.push(
                    div()
                        .id(SharedString::from(format!(
                            "bind-opt-{}-{}",
                            acc.id,
                            if opt_val.is_empty() { "none" } else { &opt_val }
                        )))
                        .h_flex()
                        .w_full()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .px_3()
                        .py_1p5()
                        .rounded(px(6.))
                        .cursor_pointer()
                        .when(selected, |d| d.bg(cx.theme().accent))
                        .hover(|s| s.bg(cx.theme().accent.opacity(0.75)))
                        .on_click(move |_, _, cx| {
                            let target_acc = acc_bind.clone();
                            let next = if val_clone.is_empty() {
                                None
                            } else {
                                Some(val_clone.clone())
                            };
                            store_bind
                                .update(cx, |s, cx| s.bind_relay_for_account(target_acc, next, cx));
                        })
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().foreground)
                                .child(opt_label),
                        )
                        .when(selected, |d| {
                            d.child(
                                Icon::new(IconName::Check)
                                    .with_size(gpui_kit::component::Size::Size(px(13.)))
                                    .text_color(cx.theme().primary),
                            )
                        })
                        .into_any_element(),
                );
            }

            Some(
                div()
                    .v_flex()
                    .w_full()
                    .gap_0p5()
                    .p_1p5()
                    .mt_1p5()
                    .rounded(px(8.))
                    .bg(cx.theme().popover)
                    .border_1()
                    .border_color(cx.theme().border)
                    .children(option_rows)
                    .into_any_element(),
            )
        } else {
            None
        };

        // 编辑备注与专属代理内联框
        let inline_edit_box: Option<AnyElement> = if is_editing {
            let acc_save = acc.clone();
            let store_save = self.store.clone();
            let store_cancel_edit = self.store.clone();
            let edit_input_read = self.edit_label_input.clone();
            let edit_proxy_read = self.edit_proxy_input.clone();
            let cur_pmode = self.edit_proxy_mode.clone();

            let mut proxy_choices: Vec<(String, String)> = vec![
                ("inherit".to_string(), t("跟随 Codex 客户端代理")),
                ("direct".to_string(), t("强制直连")),
            ];
            for n in &proxy_nodes {
                proxy_choices.push((format!("node:{}", n.id), n.name.clone()));
            }
            proxy_choices.push(("custom".to_string(), t("自定义地址")));

            let pmode_btns: Vec<AnyElement> = proxy_choices
                .into_iter()
                .map(|(m, lbl)| {
                    let selected = self.edit_proxy_mode == m;
                    let btn = AppButton::new(SharedString::from(format!(
                        "official-pmode-{}-{}",
                        acc.id, m
                    )))
                    .small()
                    .label(lbl);
                    let btn = if selected {
                        btn.selected(true)
                    } else {
                        btn.outline()
                    };
                    btn.on_click(cx.listener(move |view, _, _, cx| {
                        view.edit_proxy_mode = m.clone();
                        cx.notify();
                    }))
                    .into_any_element()
                })
                .collect();

            Some(
                div()
                    .v_flex()
                    .w_full()
                    .gap_2p5()
                    .pt_3()
                    .border_t_1()
                    .border_color(cx.theme().border.opacity(0.6))
                    .child(
                        div()
                            .v_flex()
                            .gap_1()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(t("账号备注")),
                            )
                            .child(Input::new(&self.edit_label_input)),
                    )
                    .child(
                        div()
                            .v_flex()
                            .gap_1p5()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(t("账号专属配额与点火代理（可从设置页代理列表中选取）")),
                            )
                            .child(div().h_flex().flex_wrap().gap_1p5().children(pmode_btns))
                            .when(self.edit_proxy_mode == "custom", |col| {
                                col.child(Input::new(&self.edit_proxy_input))
                            }),
                    )
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .gap_2()
                            .child(
                                AppButton::new(SharedString::from(format!(
                                    "official-save-label-{}",
                                    acc.id
                                )))
                                .primary()
                                .small()
                                .label(t("保存"))
                                .on_click(move |_, _, cx| {
                                    let val = edit_input_read.read(cx).value().to_string();
                                    let purl = edit_proxy_read.read(cx).value().to_string();
                                    let target = acc_save.clone();
                                    let pm = cur_pmode.clone();
                                    store_save.update(cx, |s, cx| {
                                        s.save_official_account_meta(target, val, pm, purl, cx);
                                    });
                                }),
                            )
                            .child(
                                AppButton::new(SharedString::from(format!(
                                    "official-cancel-label-{}",
                                    acc.id
                                )))
                                .secondary()
                                .small()
                                .label(t("取消"))
                                .on_click(move |_, _, cx| {
                                    store_cancel_edit.update(cx, |s, cx| {
                                        s.editing_account_id = None;
                                        cx.notify();
                                    });
                                }),
                            ),
                    )
                    .into_any_element(),
            )
        } else {
            None
        };

        let active_border = if acc.is_active {
            cx.theme().primary.opacity(0.65)
        } else {
            cx.theme().border
        };

        div()
            .v_flex()
            .w_full()
            .gap_3p5()
            .p_4()
            .rounded(px(11.))
            .bg(cx.theme().secondary.opacity(if acc.is_active { 0.42 } else { 0.28 }))
            .border_1()
            .border_color(active_border)
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
                                            .h_flex()
                                            .items_center()
                                            .flex_wrap()
                                            .gap_2()
                                            .child(
                                                div()
                                                    .text_sm()
                                                    .font_semibold()
                                                    .text_color(cx.theme().foreground)
                                                    .child(acc.email.clone()),
                                            )
                                            .when(!plan_label.is_empty(), |d| {
                                                d.child(AppBadge::secondary(plan_label))
                                            })
                                            .when(acc.is_active, |d| {
                                                d.child(AppBadge::good(t("当前生效")))
                                            })
                                            .when_some(
                                                acc.label
                                                    .as_ref()
                                                    .filter(|s| !s.trim().is_empty())
                                                    .cloned(),
                                                |d, lbl| d.child(AppBadge::outline(lbl)),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(sub_details),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .gap_1p5()
                            .child(
                                if acc.is_active {
                                    AppButton::new(SharedString::from(format!(
                                        "official-use-{}",
                                        acc.id
                                    )))
                                    .secondary()
                                    .small()
                                    .disabled(switching)
                                    .icon(IconName::RefreshCw)
                                    .label(t("重新应用"))
                                } else {
                                    AppButton::new(SharedString::from(format!(
                                        "official-use-{}",
                                        acc.id
                                    )))
                                    .outline()
                                    .small()
                                    .disabled(switching)
                                    .icon(IconName::ArrowLeftRight)
                                    .label(t("切换使用"))
                                }
                                .on_click(move |_, _, cx| {
                                    let target = acc_use.clone();
                                    store_use.update(cx, |s, cx| {
                                        s.use_confirm_account = Some(target);
                                        cx.notify();
                                    });
                                }),
                            )
                            .child(
                                AppButton::new(SharedString::from(format!(
                                    "official-multi-{}",
                                    acc.id
                                )))
                                .ghost()
                                .small_icon()
                                .disabled(multi_pending)
                                .icon(IconName::Rocket)
                                .tooltip(t("启动独立 Codex 实例"))
                                .on_click(move |_, _, cx| {
                                    let id = acc_id_multi.clone();
                                    store_multi
                                        .update(cx, |s, cx| s.launch_multi_official_account(id, cx));
                                }),
                            )
                            .child(
                                AppButton::new(SharedString::from(format!(
                                    "official-quota-{}",
                                    acc.id
                                )))
                                .ghost()
                                .small_icon()
                                .disabled(refreshing_this)
                                .icon(IconName::RefreshCw)
                                .tooltip(t("刷新配额"))
                                .on_click(move |_, _, cx| {
                                    let id = acc_id_quota.clone();
                                    store_quota.update(cx, |s, cx| {
                                        s.refresh_official_quota(Some(id), cx)
                                    });
                                }),
                            )
                            .child(
                                AppButton::new(SharedString::from(format!(
                                    "official-edit-{}",
                                    acc.id
                                )))
                                .ghost()
                                .small_icon()
                                .selected(is_editing)
                                .icon(IconName::SquarePen)
                                .tooltip(t("备注与专属代理"))
                                .on_click(cx.listener(move |view, _, window, cx| {
                                    let id = acc_id_edit.clone();
                                    let draft = initial_edit_label.clone();
                                    let purl = initial_edit_purl.clone();
                                    view.edit_proxy_mode = initial_edit_pmode.clone();
                                    edit_input.update(cx, |inp, cx| {
                                        inp.set_value(draft, window, cx);
                                    });
                                    edit_proxy_inp.update(cx, |inp, cx| {
                                        inp.set_value(purl, window, cx);
                                    });
                                    store_edit_toggle.update(cx, |s, cx| {
                                        if s.editing_account_id.as_deref() == Some(id.as_str()) {
                                            s.editing_account_id = None;
                                        } else {
                                            s.editing_account_id = Some(id);
                                        }
                                        cx.notify();
                                    });
                                })),
                            )
                            .child(
                                AppButton::new(SharedString::from(format!(
                                    "official-del-{}",
                                    acc.id
                                )))
                                .ghost()
                                .small_icon()
                                .icon(IconName::Trash)
                                .tooltip(t("删除账号"))
                                .on_click(move |_, _, cx| {
                                    let target = acc_del.clone();
                                    store_del.update(cx, |s, cx| {
                                        s.delete_confirm_account = Some(target);
                                        cx.notify();
                                    });
                                }),
                            ),
                    ),
            )
            .child(
                div()
                    .h_flex()
                    .w_full()
                    .items_end()
                    .justify_between()
                    .gap_4()
                    .pt_2()
                    .border_t_1()
                    .border_color(cx.theme().border.opacity(0.5))
                    .child(div().flex_1().min_w_0().child(quota_section))
                    .child(
                        div()
                            .v_flex()
                            .w(px(230.))
                            .gap_1()
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(cx.theme().muted_foreground)
                                    .child(t("绑定供应商")),
                            )
                            .child(
                                AppSelectTrigger::new(
                                    SharedString::from(format!("official-bind-trigger-{}", acc.id)),
                                    current_binding_label,
                                )
                                .open(dropdown_open)
                                .on_click(move |_, _, cx| {
                                    let id = acc_id_dropdown.clone();
                                    store_dropdown_toggle.update(cx, |s, cx| {
                                        if s.open_binding_dropdown_for.as_deref()
                                            == Some(id.as_str())
                                        {
                                            s.open_binding_dropdown_for = None;
                                        } else {
                                            s.open_binding_dropdown_for = Some(id);
                                        }
                                        cx.notify();
                                    });
                                }),
                            )
                            .when_some(dropdown_options, |d, menu| d.child(menu)),
                    ),
            )
            .when_some(inline_edit_box, |d, edit| d.child(edit))
            .into_any_element()
    }
}

impl Render for OfficialAccountsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let data = self.store.read(cx);
        let snap = data.official_snapshot.as_ref();
        let oauth_waiting = data.oauth_waiting;
        let importing = data.importing_official;
        let loading = data.official_loading;
        let has_accounts = snap.is_some_and(|s| !s.accounts.is_empty());
        let refreshing_any_quota = data.refreshing_quota_id.is_some();
        let query = data.official_search_query.trim().to_lowercase();

        let mode = resolve_official_account_mode(
            snap,
            &data.settings.relay_profiles,
            &data.settings.active_relay_id,
        );
        let live_email_display = mode
            .live_email()
            .map(str::to_string)
            .unwrap_or_else(|| t("未检测到官方登录"));
        let saved_count_display = snap.map(|s| s.accounts.len()).unwrap_or(0).to_string();
        let active_mode_summary = mode.summary_text();

        let filtered: Vec<OfficialAccountView> = snap
            .map(|s| {
                s.accounts
                    .iter()
                    .filter(|acc| {
                        if query.is_empty() {
                            return true;
                        }
                        acc.email.to_lowercase().contains(&query)
                            || acc
                                .label
                                .as_ref()
                                .is_some_and(|l| l.to_lowercase().contains(&query))
                            || acc
                                .account_name
                                .as_ref()
                                .is_some_and(|n| n.to_lowercase().contains(&query))
                            || acc
                                .plan_type
                                .as_ref()
                                .is_some_and(|p| p.to_lowercase().contains(&query))
                    })
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();

        let total_filtered = filtered.len();
        let total_pages = total_filtered.div_ceil(PAGE_SIZE).max(1);
        let current_page = data.official_page.min(total_pages).max(1);
        let start = (current_page - 1) * PAGE_SIZE;
        let paged_accounts: Vec<OfficialAccountView> = filtered
            .into_iter()
            .skip(start)
            .take(PAGE_SIZE)
            .collect();
        let has_search_query = !data.official_search_query.is_empty();

        // 顶部动作按钮句柄
        let store_oauth = self.store.clone();
        let oauth_lbl_input = self.oauth_label_input.clone();
        let store_local = self.store.clone();
        let store_import_toggle = self.store.clone();
        let store_export = self.store.clone();
        let store_quota_all = self.store.clone();
        let store_clear_search = self.store.clone();
        let search_inp_clear = self.search_input.clone();
        let store_prev_page = self.store.clone();
        let store_next_page = self.store.clone();

        let oauth_box = self.render_oauth_box(cx);
        let import_box = self.render_import_box(cx);
        let export_box = self.render_export_box(cx);

        let accounts_content: AnyElement = if loading && !has_accounts {
            div()
                .py_8()
                .w_full()
                .flex()
                .items_center()
                .justify_center()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(t("正在读取官方账号…"))
                .into_any_element()
        } else if paged_accounts.is_empty() {
            let store_empty_oauth = self.store.clone();
            div()
                .h_flex()
                .w_full()
                .items_center()
                .justify_between()
                .gap_4()
                .p_5()
                .rounded(px(10.))
                .bg(cx.theme().secondary.opacity(0.28))
                .border_1()
                .border_color(cx.theme().border)
                .child(
                    div()
                        .v_flex()
                        .flex_1()
                        .gap_1()
                        .child(
                            div()
                                .text_sm()
                                .font_semibold()
                                .text_color(cx.theme().foreground)
                                .child(t("还没有保存的官方账号")),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(t("点击右上角「OAuth 登录」或「读取本机登录」即可添加官方账号，并与任意第三方 API 供应商自由绑定。")),
                        ),
                )
                .child(
                    AppButton::new("official-empty-oauth")
                        .primary()
                        .small()
                        .icon(IconName::Plus)
                        .label(t("OAuth 登录"))
                        .on_click(move |_, _, cx| {
                            store_empty_oauth
                                .update(cx, |s, cx| s.start_official_oauth(None, cx));
                        }),
                )
                .into_any_element()
        } else {
            let cards: Vec<AnyElement> = paged_accounts
                .iter()
                .map(|acc| self.render_account_card(acc, cx))
                .collect();
            div().v_flex().w_full().gap_3().children(cards).into_any_element()
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
                    .flex_wrap()
                    .gap_2()
                    .child(
                        AppButton::new("official-top-oauth")
                            .primary()
                            .small()
                            .disabled(oauth_waiting)
                            .icon(IconName::Plus)
                            .label(if oauth_waiting {
                                t("等待授权…")
                            } else {
                                t("OAuth 登录")
                            })
                            .on_click(move |_, _, cx| {
                                let lbl = oauth_lbl_input.read(cx).value().trim().to_string();
                                store_oauth.update(cx, |s, cx| {
                                    s.start_official_oauth(
                                        (!lbl.is_empty()).then_some(lbl),
                                        cx,
                                    );
                                });
                            }),
                    )
                    .child(
                        AppButton::new("official-top-import-local")
                            .secondary()
                            .small()
                            .disabled(importing)
                            .icon(IconName::Download)
                            .label(t("读取本机登录"))
                            .on_click(move |_, _, cx| {
                                store_local
                                    .update(cx, |s, cx| s.import_official_from_local(cx));
                            }),
                    )
                    .child(
                        AppButton::new("official-top-import")
                            .secondary()
                            .small()
                            .icon(IconName::Upload)
                            .label(t("导入"))
                            .on_click(move |_, _, cx| {
                                store_import_toggle.update(cx, |s, cx| {
                                    s.import_open = !s.import_open;
                                    cx.notify();
                                });
                            }),
                    )
                    .child(
                        AppButton::new("official-top-export")
                            .secondary()
                            .small()
                            .disabled(!has_accounts)
                            .icon(IconName::Copy)
                            .label(t("导出"))
                            .on_click(move |_, _, cx| {
                                store_export
                                    .update(cx, |s, cx| s.export_official_accounts(cx));
                            }),
                    )
                    .child(
                        AppButton::new("official-top-refresh-quotas")
                            .secondary()
                            .small()
                            .disabled(refreshing_any_quota || !has_accounts)
                            .icon(IconName::RefreshCw)
                            .label(t("刷新全部配额"))
                            .on_click(move |_, _, cx| {
                                store_quota_all
                                    .update(cx, |s, cx| s.refresh_official_quota(None, cx));
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
                            .gap_3()
                            .child(Self::render_metric_card(
                                cx,
                                t("本机当前登录"),
                                live_email_display,
                            ))
                            .child(Self::render_metric_card(
                                cx,
                                t("已保存账号"),
                                saved_count_display,
                            ))
                            .child(Self::render_metric_card(
                                cx,
                                t("当前生效模式"),
                                active_mode_summary,
                            )),
                    )
                    .when_some(oauth_box, |d, b| d.child(b))
                    .when_some(import_box, |d, b| d.child(b))
                    .when_some(export_box, |d, b| d.child(b))
                    .child(
                        div()
                            .h_flex()
                            .w_full()
                            .items_center()
                            .gap_2()
                            .child(
                                div().flex_1().child(
                                    Input::new(&self.search_input).prefix(
                                        Icon::new(IconName::Search)
                                            .with_size(gpui_kit::component::Size::Size(px(14.)))
                                            .text_color(cx.theme().muted_foreground),
                                    ),
                                ),
                            )
                            .when(has_search_query, |d| {
                                d.child(
                                    AppButton::new("official-clear-search")
                                        .ghost()
                                        .small_icon()
                                        .icon(IconName::X)
                                        .tooltip(t("清除搜索"))
                                        .on_click(move |_, window, cx| {
                                            search_inp_clear.update(cx, |inp, cx| {
                                                inp.set_value("", window, cx);
                                            });
                                            store_clear_search.update(cx, |s, cx| {
                                                s.official_search_query.clear();
                                                s.official_page = 1;
                                                cx.notify();
                                            });
                                        }),
                                )
                            }),
                    )
                    .child(accounts_content)
                    .when(total_filtered > 0, |d| {
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
                                            "{} {} {} · {} {} / {} {}",
                                            t("共"),
                                            total_filtered,
                                            t("个账号"),
                                            t("第"),
                                            current_page,
                                            total_pages,
                                            t("页")
                                        )),
                                )
                                .child(
                                    div()
                                        .h_flex()
                                        .items_center()
                                        .gap_2()
                                        .child(
                                            AppButton::new("official-prev-page")
                                                .secondary()
                                                .small()
                                                .disabled(current_page <= 1)
                                                .label(t("上一页"))
                                                .on_click(move |_, _, cx| {
                                                    store_prev_page.update(cx, |s, cx| {
                                                        s.official_page =
                                                            current_page.saturating_sub(1).max(1);
                                                        cx.notify();
                                                    });
                                                }),
                                        )
                                        .child(
                                            AppButton::new("official-next-page")
                                                .secondary()
                                                .small()
                                                .disabled(current_page >= total_pages)
                                                .label(t("下一页"))
                                                .on_click(move |_, _, cx| {
                                                    store_next_page.update(cx, |s, cx| {
                                                        s.official_page =
                                                            (current_page + 1).min(total_pages);
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
