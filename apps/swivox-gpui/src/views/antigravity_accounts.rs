//! Antigravity 多账号管理、Google OAuth 登录、5H 与周配额可视化、无感热切号与导入导出视图。

use chrono::{Local, TimeZone};
use gpui_kit::assets::IconName;
use gpui_kit::component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use gpui_kit::component::theme::ActiveTheme as _;
use gpui_kit::component::StyledExt as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use swivox_backend::commands;
use swivox_backend::i18n::{t, tf};
use swivox_core::antigravity::{AntigravityAccountView, AntigravityGroupQuota};

use crate::store::AppStore;
use crate::views::{avatar_badge, panel_card};
use crate::widgets::{AppBadge, AppButton};

pub struct AntigravityAccountsView {
    store: Entity<AppStore>,
    search_input: Entity<InputState>,
    manual_callback_input: Entity<InputState>,
    import_textarea: Entity<TextareaState>,
    edit_label_input: Entity<InputState>,
    edit_proxy_input: Entity<InputState>,
    edit_proxy_mode: String,
}

impl AntigravityAccountsView {
    pub fn new(store: Entity<AppStore>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder(t("搜索 Google 邮箱、备注或订阅等级"))
        });
        let manual_callback_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("http://localhost:xxxxx/auth/callback?code=...")
        });
        let import_textarea = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(t("粘贴导出的 JSON 数组，或每行一条 Google OAuth refresh_token"))
        });
        let edit_label_input =
            cx.new(|cx| InputState::new(window, cx).placeholder(t("例如：主力 AI Pro")));
        let edit_proxy_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("http://127.0.0.1:7890"));

        let store_search = store.clone();
        cx.subscribe(&search_input, move |_, input, ev: &InputEvent, cx| {
            if matches!(ev, InputEvent::Change) {
                let q = input.read(cx).value().to_string();
                store_search.update(cx, |s, cx| {
                    s.ag_search_query = q;
                    cx.notify();
                });
            }
        })
        .detach();

        cx.observe(&store, |_, _, cx| cx.notify()).detach();

        Self {
            store,
            search_input,
            manual_callback_input,
            import_textarea,
            edit_label_input,
            edit_proxy_input,
            edit_proxy_mode: "inherit".to_string(),
        }
    }

    fn metric_card(cx: &App, label: String, value: String) -> Div {
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
                    .child(label),
            )
            .child(
                div()
                    .text_sm()
                    .font_semibold()
                    .truncate()
                    .text_color(cx.theme().foreground)
                    .child(value),
            )
    }

    fn format_reset_time(epoch: Option<i64>) -> String {
        let Some(ts) = epoch.filter(|v| *v > 0) else {
            return "-".to_string();
        };
        let now = chrono::Utc::now().timestamp();
        let diff = ts - now;
        let clock = Local
            .timestamp_opt(ts, 0)
            .single()
            .map(|dt| dt.format("%m/%d %H:%M").to_string())
            .unwrap_or_else(|| "-".to_string());
        if diff <= 0 {
            t("可重置")
        } else if diff >= 86_400 {
            format!("{}d · {clock}", (diff + 43_200) / 86_400)
        } else if diff >= 3600 {
            format!("{}h · {clock}", (diff + 1800) / 3600)
        } else {
            format!("{}m · {clock}", ((diff + 30) / 60).max(1))
        }
    }

    fn render_quota_bar(cx: &App, label: String, pct: f64, reset: Option<i64>) -> Div {
        let clamped = pct.clamp(0.0, 100.0);
        let fill_color = if clamped <= 15.0 {
            cx.theme().danger
        } else if clamped <= 45.0 {
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
                    .gap_2()
                    .child(
                        div()
                            .text_xs()
                            .font_medium()
                            .text_color(cx.theme().foreground)
                            .child(format!("{label}  {:.0}%", clamped)),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(Self::format_reset_time(reset)),
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
                            .w(relative((clamped / 100.0) as f32))
                            .bg(fill_color),
                    ),
            )
    }

    fn render_group_row(cx: &App, grp: &AntigravityGroupQuota) -> Div {
        let mut bars = Vec::new();
        if let Some(p5) = grp.five_hour_percentage {
            bars.push(Self::render_quota_bar(
                cx,
                "5H".to_string(),
                p5,
                grp.five_hour_reset_time,
            ));
        }
        if let Some(pw) = grp.weekly_percentage {
            bars.push(Self::render_quota_bar(
                cx,
                t("周配额"),
                pw,
                grp.weekly_reset_time,
            ));
        }

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
                    .w_full()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .text_xs()
                            .font_semibold()
                            .text_color(cx.theme().foreground)
                            .child(grp.display_name.clone()),
                    )
                    .when(grp.weekly_exhausted, |row| {
                        row.child(AppBadge::bad(t("周配额已耗尽")))
                    }),
            )
            .child(div().h_flex().w_full().gap_4().children(bars))
    }

    fn render_account_card(
        &self,
        acc: &AntigravityAccountView,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let data = self.store.read(cx);
        let switching = data.ag_switching_id.as_deref() == Some(acc.id.as_str());
        let refreshing = data.ag_refreshing_id.as_deref() == Some(acc.id.as_str());
        let is_editing = data.ag_editing_id.as_deref() == Some(acc.id.as_str());
        let proxy_nodes = data.network_proxy_snapshot.config.nodes.clone();

        let initial = acc
            .email
            .chars()
            .next()
            .unwrap_or('A')
            .to_ascii_uppercase()
            .to_string();

        let tier_badge = match acc.subscription_tier.as_str() {
            "ULTRA" => AppBadge::good("Google AI Ultra"),
            "PRO" => AppBadge::good("Google AI Pro"),
            "ENTERPRISE" => AppBadge::good("GCP Enterprise"),
            _ => AppBadge::new("Free"),
        };

        let store_switch = self.store.clone();
        let store_refresh = self.store.clone();
        let store_edit = self.store.clone();
        let store_del = self.store.clone();
        let acc_for_confirm = acc.clone();
        let id_refresh = acc.id.clone();
        let id_edit = acc.id.clone();
        let id_del = acc.id.clone();
        let cur_label = acc.label.clone().unwrap_or_default();
        let cur_proxy_mode = acc.proxy_mode.clone();
        let cur_proxy_url = acc.egress_proxy_url.clone().unwrap_or_default();
        let label_inp = self.edit_label_input.clone();
        let proxy_inp = self.edit_proxy_input.clone();

        let validation_url = acc
            .quota
            .as_ref()
            .and_then(|q| q.validation_url.clone());

        div()
            .v_flex()
            .w_full()
            .gap_3()
            .p_4()
            .rounded(px(10.))
            .bg(cx.theme().background)
            .border_1()
            .border_color(if acc.is_active {
                cx.theme().primary.opacity(0.6)
            } else {
                cx.theme().border
            })
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
                            .min_w_0()
                            .flex_1()
                            .child(avatar_badge(cx, initial))
                            .child(
                                div()
                                    .v_flex()
                                    .gap_1()
                                    .min_w_0()
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
                                                    .text_color(cx.theme().foreground)
                                                    .child(acc.email.clone()),
                                            )
                                            .child(tier_badge)
                                            .when(acc.is_active, |r| {
                                                r.child(AppBadge::good(t("当前生效")))
                                            })
                                            .when(acc.is_live_desktop, |r| {
                                                r.child(AppBadge::info(t("桌面版在线")))
                                            })
                                            .when(acc.is_live_ide, |r| {
                                                r.child(AppBadge::info(t("IDE 在线")))
                                            })
                                            .when(acc.disabled, |r| {
                                                r.child(AppBadge::bad(t("令牌已失效")))
                                            }),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(
                                                acc.label
                                                    .clone()
                                                    .or_else(|| acc.name.clone())
                                                    .unwrap_or_else(|| t("未填写备注")),
                                            ),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .h_flex()
                            .flex_shrink_0()
                            .items_center()
                            .gap_2()
                            .when_some(validation_url, |row, url| {
                                row.child(
                                    AppButton::new(SharedString::from(format!(
                                        "ag-verify-{}",
                                        acc.id
                                    )))
                                    .outline()
                                    .small()
                                    .icon(IconName::ExternalLink)
                                    .label(t("完成安全验证"))
                                    .on_click(move |_, _, _| {
                                        let _ = commands::open_external_url(url.clone());
                                    }),
                                )
                            })
                            .child(
                                if acc.is_active {
                                    AppButton::new(SharedString::from(format!(
                                        "ag-switch-{}",
                                        acc.id
                                    )))
                                    .secondary()
                                    .small()
                                    .disabled(switching)
                                    .icon(IconName::RefreshCw)
                                    .label(if switching {
                                        t("切换中...")
                                    } else {
                                        t("重新应用")
                                    })
                                } else {
                                    AppButton::new(SharedString::from(format!(
                                        "ag-switch-{}",
                                        acc.id
                                    )))
                                    .primary()
                                    .small()
                                    .disabled(switching)
                                    .icon(IconName::ArrowLeftRight)
                                    .label(if switching {
                                        t("切换中...")
                                    } else {
                                        t("切换使用")
                                    })
                                }
                                .on_click(move |_, _, cx| {
                                    let target = acc_for_confirm.clone();
                                    store_switch.update(cx, |s, cx| {
                                        s.ag_use_confirm_account = Some(target);
                                        cx.notify();
                                    });
                                }),
                            )
                            .child(
                                AppButton::new(SharedString::from(format!(
                                    "ag-refresh-{}",
                                    acc.id
                                )))
                                .outline()
                                .small_icon()
                                .disabled(refreshing)
                                .icon(IconName::RefreshCw)
                                .tooltip(t("刷新配额"))
                                .on_click(move |_, _, cx| {
                                    let id = id_refresh.clone();
                                    store_refresh.update(cx, |s, cx| {
                                        s.refresh_single_antigravity_quota(id, cx)
                                    });
                                }),
                            )
                            .child(
                                AppButton::new(SharedString::from(format!("ag-edit-{}", acc.id)))
                                    .outline()
                                    .small_icon()
                                    .icon(IconName::Settings)
                                    .tooltip(t("备注与专属代理"))
                                    .on_click(cx.listener(move |view, _, window, cx| {
                                        view.edit_proxy_mode = cur_proxy_mode.clone();
                                        let lbl = cur_label.clone();
                                        let purl = cur_proxy_url.clone();
                                        label_inp.update(cx, |inp, cx| inp.set_value(lbl, window, cx));
                                        proxy_inp.update(cx, |inp, cx| inp.set_value(purl, window, cx));
                                        let id = id_edit.clone();
                                        store_edit.update(cx, |s, cx| {
                                            s.ag_editing_id = if s.ag_editing_id.as_deref() == Some(&id) {
                                                None
                                            } else {
                                                Some(id)
                                            };
                                            cx.notify();
                                        });
                                    })),
                            )
                            .child(
                                AppButton::new(SharedString::from(format!("ag-del-{}", acc.id)))
                                    .ghost()
                                    .small_icon()
                                    .icon(IconName::Trash)
                                    .tooltip(t("删除账号"))
                                    .on_click(move |_, _, cx| {
                                        let id = id_del.clone();
                                        store_del.update(cx, |s, cx| {
                                            s.ag_delete_confirm_id = Some(id);
                                            cx.notify();
                                        });
                                    }),
                            ),
                    ),
            )
            .when_some(acc.quota.as_ref(), |card, q| {
                if !q.groups.is_empty() {
                    let rows: Vec<AnyElement> = q
                        .groups
                        .iter()
                        .map(|g| Self::render_group_row(cx, g).into_any_element())
                        .collect();
                    card.child(div().v_flex().w_full().gap_2().children(rows))
                } else if let Some(ref err) = q.error {
                    card.child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().danger)
                            .child(err.clone()),
                    )
                } else {
                    card
                }
            })
            .when(is_editing, |card| {
                let id_save = acc.id.clone();
                let store_save = self.store.clone();
                let store_cancel = self.store.clone();
                let li = self.edit_label_input.clone();
                let pi = self.edit_proxy_input.clone();
                let mode_now = self.edit_proxy_mode.clone();

                let mut proxy_choices: Vec<(String, String)> = vec![
                    ("inherit".to_string(), t("跟随客户端代理")),
                    ("direct".to_string(), t("强制直连")),
                ];
                for n in &proxy_nodes {
                    proxy_choices.push((format!("node:{}", n.id), n.name.clone()));
                }
                proxy_choices.push(("custom".to_string(), t("自定义地址")));

                card.child(
                    div()
                        .v_flex()
                        .w_full()
                        .gap_3()
                        .p_3()
                        .rounded(px(8.))
                        .bg(cx.theme().secondary.opacity(0.28))
                        .border_1()
                        .border_color(cx.theme().border)
                        .child(Input::new(&self.edit_label_input))
                        .child(
                            div()
                                .h_flex()
                                .flex_wrap()
                                .gap_1p5()
                                .children(proxy_choices.into_iter().map(|(m, lbl)| {
                                    let active = self.edit_proxy_mode == m;
                                    let btn = AppButton::new(SharedString::from(format!(
                                        "ag-pmode-{}-{m}",
                                        acc.id
                                    )))
                                    .small()
                                    .label(lbl);
                                    let btn = if active {
                                        btn.selected(true)
                                    } else {
                                        btn.outline()
                                    };
                                    btn.on_click(cx.listener(move |v, _, _, cx| {
                                        v.edit_proxy_mode = m.clone();
                                        cx.notify();
                                    }))
                                })),
                        )
                        .when(self.edit_proxy_mode == "custom", |box_el| {
                            box_el.child(Input::new(&self.edit_proxy_input))
                        })
                        .child(
                            div()
                                .h_flex()
                                .w_full()
                                .justify_end()
                                .gap_2()
                                .child(
                                    AppButton::new("ag-edit-cancel")
                                        .outline()
                                        .small()
                                        .label(t("取消"))
                                        .on_click(move |_, _, cx| {
                                            store_cancel.update(cx, |s, cx| {
                                                s.ag_editing_id = None;
                                                cx.notify();
                                            });
                                        }),
                                )
                                .child(
                                    AppButton::new("ag-edit-save")
                                        .primary()
                                        .small()
                                        .label(t("保存设置"))
                                        .on_click(move |_, _, cx| {
                                            let lbl = li.read(cx).value().to_string();
                                            let purl = pi.read(cx).value().to_string();
                                            let id = id_save.clone();
                                            let m = mode_now.clone();
                                            store_save.update(cx, |s, cx| {
                                                s.save_antigravity_account_meta(
                                                    id, lbl, m, purl, cx,
                                                );
                                            });
                                        }),
                                ),
                        ),
                )
            })
    }
}

impl Render for AntigravityAccountsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (snap, query, oauth_session, import_open, loading) = {
            let s = self.store.read(cx);
            (
                s.ag_snapshot.clone(),
                s.ag_search_query.trim().to_ascii_lowercase(),
                s.ag_oauth_session.clone(),
                s.ag_import_open,
                s.ag_loading,
            )
        };

        let accounts = snap
            .as_ref()
            .map(|s| s.accounts.clone())
            .unwrap_or_default();
        let total = accounts.len();
        let pro_ultra = accounts
            .iter()
            .filter(|a| matches!(a.subscription_tier.as_str(), "PRO" | "ULTRA" | "ENTERPRISE"))
            .count();
        let live_email = snap
            .as_ref()
            .and_then(|s| s.live_desktop_email.clone().or_else(|| s.live_ide_email.clone()))
            .unwrap_or_else(|| t("未检测到在线登录"));

        let filtered: Vec<AntigravityAccountView> = accounts
            .into_iter()
            .filter(|a| {
                query.is_empty()
                    || a.email.to_ascii_lowercase().contains(&query)
                    || a.label
                        .as_deref()
                        .unwrap_or("")
                        .to_ascii_lowercase()
                        .contains(&query)
                    || a.subscription_tier.to_ascii_lowercase().contains(&query)
            })
            .collect();

        let st_oauth = self.store.clone();
        let st_gcp = self.store.clone();
        let st_scan = self.store.clone();
        let st_imp = self.store.clone();
        let st_exp = self.store.clone();
        let st_ref = self.store.clone();

        div()
            .v_flex()
            .w_full()
            .gap_4()
            .child(
                panel_card(
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
                                .child(Self::metric_card(
                                    cx,
                                    t("已保存账号"),
                                    total.to_string(),
                                ))
                                .child(Self::metric_card(
                                    cx,
                                    t("Pro / Ultra 账号"),
                                    pro_ultra.to_string(),
                                ))
                                .child(Self::metric_card(cx, t("本机实时登录"), live_email)),
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
                                        .gap_2()
                                        .flex_wrap()
                                        .child(
                                            AppButton::new("ag-oauth-personal")
                                                .primary()
                                                .small()
                                                .icon(IconName::UserPlus)
                                                .label(t("Google OAuth 登录"))
                                                .on_click(move |_, _, cx| {
                                                    st_oauth.update(cx, |s, cx| {
                                                        s.start_antigravity_oauth("personal", cx)
                                                    });
                                                }),
                                        )
                                        .child(
                                            AppButton::new("ag-oauth-gcp")
                                                .outline()
                                                .small()
                                                .icon(IconName::ShieldCheck)
                                                .label(t("GCP 企业登录"))
                                                .on_click(move |_, _, cx| {
                                                    st_gcp.update(cx, |s, cx| {
                                                        s.start_antigravity_oauth("gcp", cx)
                                                    });
                                                }),
                                        )
                                        .child(
                                            AppButton::new("ag-scan-local")
                                                .outline()
                                                .small()
                                                .disabled(loading)
                                                .icon(IconName::Download)
                                                .label(t("扫描导入本机账号"))
                                                .on_click(move |_, _, cx| {
                                                    st_scan.update(cx, |s, cx| {
                                                        s.import_local_antigravity_accounts(cx)
                                                    });
                                                }),
                                        )
                                        .child(
                                            AppButton::new("ag-open-import")
                                                .outline()
                                                .small()
                                                .icon(IconName::Upload)
                                                .label(t("批量导入 Token"))
                                                .on_click(move |_, _, cx| {
                                                    st_imp.update(cx, |s, cx| {
                                                        s.ag_import_open = !s.ag_import_open;
                                                        cx.notify();
                                                    });
                                                }),
                                        )
                                        .child(
                                            AppButton::new("ag-export-json")
                                                .outline()
                                                .small()
                                                .icon(IconName::Copy)
                                                .label(t("导出 JSON"))
                                                .on_click(move |_, _, cx| {
                                                    st_exp.update(cx, |s, cx| {
                                                        s.export_antigravity_accounts(cx)
                                                    });
                                                }),
                                        )
                                        .child(
                                            AppButton::new("ag-refresh-all")
                                                .outline()
                                                .small()
                                                .disabled(loading)
                                                .icon(IconName::RefreshCw)
                                                .label(t("刷新全部配额"))
                                                .on_click(move |_, _, cx| {
                                                    st_ref.update(cx, |s, cx| {
                                                        s.refresh_all_antigravity_quotas(cx)
                                                    });
                                                }),
                                        ),
                                )
                                .child(
                                    div()
                                        .w(px(240.))
                                        .child(Input::new(&self.search_input)),
                                ),
                        )
                        .when_some(oauth_session, |col, sess| {
                            let url_open = sess.verification_uri.clone();
                            let url_copy = sess.verification_uri.clone();
                            let st_cancel = self.store.clone();
                            let st_submit = self.store.clone();
                            let cb_inp = self.manual_callback_input.clone();
                            col.child(
                                div()
                                    .v_flex()
                                    .w_full()
                                    .gap_2p5()
                                    .p_3p5()
                                    .rounded(px(10.))
                                    .bg(cx.theme().info.opacity(0.08))
                                    .border_1()
                                    .border_color(cx.theme().info.opacity(0.4))
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
                                                    .flex_1()
                                                    .min_w(px(220.))
                                                    .text_xs()
                                                    .font_semibold()
                                                    .child(t("正在等待浏览器完成 Google OAuth 授权（自动回环捕获，也可手动粘贴回调链接）")),
                                            )
                                            .child(
                                                div()
                                                    .h_flex()
                                                    .flex_wrap()
                                                    .gap_2()
                                                    .child(
                                                        AppButton::new("ag-oauth-copy")
                                                            .outline()
                                                            .small()
                                                            .label(t("复制链接"))
                                                            .on_click(move |_, _, cx| {
                                                                cx.write_to_clipboard(
                                                                    ClipboardItem::new_string(
                                                                        url_copy.clone(),
                                                                    ),
                                                                );
                                                            }),
                                                    )
                                                    .child(
                                                        AppButton::new("ag-oauth-open")
                                                            .outline()
                                                            .small()
                                                            .label(t("打开浏览器"))
                                                            .on_click(move |_, _, _| {
                                                                let _ = commands::open_external_url(
                                                                    url_open.clone(),
                                                                );
                                                            }),
                                                    )
                                                    .child(
                                                        AppButton::new("ag-oauth-cancel")
                                                            .ghost()
                                                            .small()
                                                            .label(t("取消"))
                                                            .on_click(move |_, _, cx| {
                                                                st_cancel.update(cx, |s, cx| {
                                                                    s.cancel_antigravity_oauth(cx)
                                                                });
                                                            }),
                                                    ),
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
                                                    .child(Input::new(&self.manual_callback_input)),
                                            )
                                            .child(
                                                AppButton::new("ag-oauth-submit")
                                                    .primary()
                                                    .small()
                                                    .label(t("提交回调"))
                                                    .on_click(move |_, _, cx| {
                                                        let raw =
                                                            cb_inp.read(cx).value().to_string();
                                                        st_submit.update(cx, |s, cx| {
                                                            s.submit_antigravity_manual_callback(
                                                                raw, cx,
                                                            )
                                                        });
                                                    }),
                                            ),
                                    ),
                            )
                        })
                        .when(import_open, |col| {
                            let st_do_imp = self.store.clone();
                            let st_close_imp = self.store.clone();
                            let ta = self.import_textarea.clone();
                            col.child(
                                div()
                                    .v_flex()
                                    .w_full()
                                    .gap_3()
                                    .p_3p5()
                                    .rounded(px(10.))
                                    .bg(cx.theme().secondary.opacity(0.28))
                                    .border_1()
                                    .border_color(cx.theme().border)
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_semibold()
                                            .child(t("批量导入 Antigravity Refresh Token 或 JSON")),
                                    )
                                    .child(div().h(px(110.)).child(Textarea::new(&self.import_textarea)))
                                    .child(
                                        div()
                                            .h_flex()
                                            .w_full()
                                            .justify_end()
                                            .gap_2()
                                            .child(
                                                AppButton::new("ag-imp-cancel")
                                                    .outline()
                                                    .small()
                                                    .label(t("收起"))
                                                    .on_click(move |_, _, cx| {
                                                        st_close_imp.update(cx, |s, cx| {
                                                            s.ag_import_open = false;
                                                            cx.notify();
                                                        });
                                                    }),
                                            )
                                            .child(
                                                AppButton::new("ag-imp-confirm")
                                                    .primary()
                                                    .small()
                                                    .disabled(loading)
                                                    .label(t("立即导入"))
                                                    .on_click(move |_, _, cx| {
                                                        let txt = ta.read(cx).value().to_string();
                                                        st_do_imp.update(cx, |s, cx| {
                                                            s.import_antigravity_tokens_text(txt, cx)
                                                        });
                                                    }),
                                            ),
                                    ),
                            )
                        }),
                ),
            )
            .child(if filtered.is_empty() {
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
                                .text_color(cx.theme().foreground)
                                .child(t("暂无已保存的 Antigravity 账号")),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(tf(
                                    "点击上方「{0}」或「{1}」即可开始管理多账号配额与无感热切号。",
                                    &[&t("Google OAuth 登录"), &t("扫描导入本机账号")],
                                )),
                        ),
                )
                .into_any_element()
            } else {
                let cards: Vec<AnyElement> = filtered
                    .iter()
                    .map(|a| self.render_account_card(a, cx).into_any_element())
                    .collect();
                div().v_flex().w_full().gap_3().children(cards).into_any_element()
            })
    }
}
