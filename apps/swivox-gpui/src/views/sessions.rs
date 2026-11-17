use std::time::{SystemTime, UNIX_EPOCH};

use swivox_backend::i18n::{t, tf};
use swivox_backend::phase4_rules::{
    SessionGroupMode, format_relative_time_ms, group_local_sessions, matches_management_query,
};
use gpui_kit::assets::IconName;
use gpui_kit::component::Icon;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::theme::ActiveTheme as _;
use gpui_kit::component::{Sizable as _, StyledExt as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::store::AppStore;
use crate::views::panel_card;
use crate::widgets::{AppBadge, AppButton};

pub struct SessionsView {
    store: Entity<AppStore>,
    search_input: Entity<InputState>,
}

impl SessionsView {
    pub fn new(store: Entity<AppStore>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder(t("搜索会话标题、项目目录、供应商或线程 ID"))
        });
        let store_search = store.clone();
        cx.subscribe(&search_input, move |_, inp, ev: &InputEvent, cx| {
            if matches!(ev, InputEvent::Change) {
                let q = inp.read(cx).value().to_string();
                store_search.update(cx, |s, cx| {
                    s.sessions_search_query = q;
                    cx.notify();
                });
            }
        })
        .detach();

        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        Self {
            store,
            search_input,
        }
    }
}

impl Render for SessionsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let data = self.store.read(cx);
        let loading = data.sessions_loading;
        let repair_busy = data.session_repair_busy;
        let group_mode = data.sessions_group_mode;
        let query = data.sessions_search_query.clone();
        let payload = data.sessions_payload.clone();
        let cleanup_preview = data.session_cleanup_preview.clone();

        let total_count = payload.as_ref().map(|p| p.total_count).unwrap_or(0);
        let has_more = payload.as_ref().is_some_and(|p| p.has_more);
        let all_sessions = payload
            .as_ref()
            .map(|p| p.sessions.clone())
            .unwrap_or_default();

        let filtered: Vec<_> = all_sessions
            .into_iter()
            .filter(|s| {
                matches_management_query(&[&s.id, &s.title, &s.cwd, &s.model_provider], &query)
            })
            .collect();

        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let groups = group_local_sessions(&filtered, group_mode, now_ms);

        let store_repair = self.store.clone();
        let store_cleanup_prev = self.store.clone();
        let store_refresh = self.store.clone();
        let store_by_proj = self.store.clone();
        let store_by_time = self.store.clone();
        let store_more = self.store.clone();

        let cleanup_banner: Option<AnyElement> = cleanup_preview.map(|pv| {
            let cnt = pv.candidates.len();
            let store_close = self.store.clone();
            let store_apply = self.store.clone();
            div()
                .h_flex()
                .flex_wrap()
                .w_full()
                .items_center()
                .justify_between()
                .gap_3()
                .p_4()
                .rounded(px(10.))
                .bg(cx.theme().info.opacity(0.1))
                .border_1()
                .border_color(cx.theme().info.opacity(0.45))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(220.))
                        .text_xs()
                        .font_medium()
                        .text_color(cx.theme().foreground)
                        .child(tf(
                            "检测到 {0} 条仅存在于索引中的孤立会话记录（执行前会自动完整备份原索引）。",
                            &[&cnt.to_string()],
                        )),
                )
                .child(
                    div()
                        .h_flex()
                        .flex_shrink_0()
                        .gap_2()
                        .child(
                            AppButton::new("sess-clean-close")
                                .outline()
                                .small()
                                .label(t("关闭"))
                                .on_click(move |_, _, cx| {
                                    store_close.update(cx, |s, cx| {
                                        s.session_cleanup_preview = None;
                                        cx.notify();
                                    });
                                }),
                        )
                        .when(cnt > 0, |d| {
                            d.child(
                                AppButton::new("sess-clean-apply")
                                    .primary()
                                    .small()
                                    .label(t("立即备份并清理"))
                                    .on_click(move |_, _, cx| {
                                        store_apply.update(cx, |s, cx| {
                                            s.request_action_confirm(
                                                crate::store::PendingActionConfirm::ApplyStaleSessionCleanup {
                                                    count: cnt,
                                                },
                                                cx,
                                            )
                                        });
                                    }),
                            )
                        }),
                )
                .into_any_element()
        });

        let groups_body: AnyElement = if groups.is_empty() {
            div()
                .p_6()
                .w_full()
                .rounded(px(10.))
                .bg(cx.theme().secondary.opacity(0.24))
                .border_1()
                .border_color(cx.theme().border)
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(if loading {
                    t("正在读取本地 Codex 会话记录…")
                } else {
                    t("没有找到匹配的本地 Codex 会话。")
                })
                .into_any_element()
        } else {
            let blocks: Vec<AnyElement> = groups
                .into_iter()
                .map(|grp| {
                    let rows: Vec<AnyElement> = grp
                        .sessions
                        .iter()
                        .map(|sess| {
                            let sid_copy = sess.id.clone();
                            let store_copy = self.store.clone();
                            let sess_del = sess.clone();
                            let store_del = self.store.clone();
                            let collapsed_title = sess
                                .title
                                .lines()
                                .map(str::trim)
                                .filter(|l| !l.is_empty())
                                .collect::<Vec<_>>()
                                .join(" ");
                            let title = if collapsed_title.is_empty() {
                                sess.id.clone()
                            } else {
                                collapsed_title
                            };
                            let rel_time = format_relative_time_ms(sess.updated_at_ms, now_ms);

                            div()
                                .h_flex()
                                .w_full()
                                .items_center()
                                .justify_between()
                                .gap_3()
                                .p_3()
                                .rounded(px(9.))
                                .bg(cx.theme().secondary.opacity(0.26))
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
                                                .h_flex()
                                                .items_center()
                                                .gap_2()
                                                .child(
                                                    div()
                                                        .flex_1()
                                                        .min_w_0()
                                                        .text_sm()
                                                        .font_medium()
                                                        .text_color(cx.theme().foreground)
                                                        .truncate()
                                                        .child(title),
                                                )
                                                .when(!sess.model_provider.is_empty(), |d| {
                                                    d.child(AppBadge::secondary(
                                                        sess.model_provider.clone(),
                                                    ))
                                                })
                                                .when(sess.archived, |d| {
                                                    d.child(AppBadge::outline(t("已归档")))
                                                }),
                                        )
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(cx.theme().muted_foreground)
                                                .whitespace_nowrap()
                                                .overflow_hidden()
                                                .child(format!(
                                                    "{rel_time} · {} · {}",
                                                    if sess.cwd.is_empty() {
                                                        "-"
                                                    } else {
                                                        &sess.cwd
                                                    },
                                                    sess.id
                                                )),
                                        ),
                                )
                                .child(
                                    div()
                                        .h_flex()
                                        .flex_shrink_0()
                                        .items_center()
                                        .gap_1p5()
                                        .child(
                                            AppButton::new(SharedString::from(format!(
                                                "sess-copy-{}",
                                                sess.id
                                            )))
                                            .ghost()
                                            .small_icon()
                                            .icon(IconName::Copy)
                                            .tooltip(t("复制会话 ID"))
                                            .on_click(move |_, _, cx| {
                                                cx.write_to_clipboard(ClipboardItem::new_string(
                                                    sid_copy.clone(),
                                                ));
                                                store_copy.update(cx, |s, cx| {
                                                    s.set_notice(
                                                        t("会话管理"),
                                                        t("已复制会话 ID 到剪贴板。"),
                                                        "ok",
                                                        cx,
                                                    );
                                                });
                                            }),
                                        )
                                        .child(
                                            AppButton::new(SharedString::from(format!(
                                                "sess-del-{}",
                                                sess.id
                                            )))
                                            .ghost()
                                            .small_icon()
                                            .icon(IconName::Trash)
                                            .tooltip(t("删除本地会话"))
                                            .on_click(move |_, _, cx| {
                                                let target = sess_del.clone();
                                                store_del.update(cx, |s, cx| {
                                                    s.session_delete_confirm = Some(target);
                                                    cx.notify();
                                                });
                                            }),
                                        ),
                                )
                                .into_any_element()
                        })
                        .collect();

                    div()
                        .v_flex()
                        .w_full()
                        .gap_2()
                        .child(
                            div()
                                .h_flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    div()
                                        .text_xs()
                                        .font_semibold()
                                        .text_color(cx.theme().foreground)
                                        .child(grp.label),
                                )
                                .child(AppBadge::outline(grp.sessions.len().to_string()))
                                .when(!grp.path.is_empty(), |d| {
                                    d.child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(grp.path),
                                    )
                                }),
                        )
                        .children(rows)
                        .into_any_element()
                })
                .collect();
            div().v_flex().w_full().gap_4().children(blocks).into_any_element()
        };

        div()
            .v_flex()
            .w_full()
            .gap_4()
            .child(
                div()
                    .h_flex()
                    .flex_wrap()
                    .w_full()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(AppBadge::secondary(format!(
                        "{}：{total_count}",
                        t("本地会话总数")
                    )))
                    .child(
                        div()
                            .h_flex()
                            .flex_wrap()
                            .items_center()
                            .gap_2()
                            .child(
                                AppButton::new("sess-repair-idx")
                                    .secondary()
                                    .small()
                                    .disabled(repair_busy)
                                    .icon(IconName::Wrench)
                                    .label(t("修复会话索引"))
                                    .on_click(move |_, _, cx| {
                                        store_repair.update(cx, |s, cx| {
                                            s.request_action_confirm(
                                                crate::store::PendingActionConfirm::RepairSessionIndex,
                                                cx,
                                            )
                                        });
                                    }),
                            )
                            .child(
                                AppButton::new("sess-preview-cleanup")
                                    .secondary()
                                    .small()
                                    .disabled(repair_busy)
                                    .icon(IconName::ShieldCheck)
                                    .label(t("检查失效索引"))
                                    .on_click(move |_, _, cx| {
                                        store_cleanup_prev
                                            .update(cx, |s, cx| s.preview_stale_session_index(cx));
                                    }),
                            )
                            .child(
                                AppButton::new("sess-refresh-btn")
                                    .primary()
                                    .small()
                                    .disabled(loading)
                                    .icon(IconName::RefreshCw)
                                    .label(t("刷新会话"))
                                    .on_click(move |_, _, cx| {
                                        store_refresh
                                            .update(cx, |s, cx| s.refresh_sessions(false, true, cx));
                                    }),
                            ),
                    ),
            )
            .when_some(cleanup_banner, |d, b| d.child(b))
            .child(panel_card(
                cx,
                div()
                    .v_flex()
                    .w_full()
                    .gap_4()
                    .child(
                        div()
                            .h_flex()
                            .flex_wrap()
                            .w_full()
                            .items_center()
                            .gap_3()
                            .child(
                                div().flex_1().min_w(px(220.)).child(
                                    Input::new(&self.search_input).prefix(
                                        Icon::new(IconName::Search)
                                            .with_size(gpui_kit::component::Size::Size(px(14.)))
                                            .text_color(cx.theme().muted_foreground),
                                    ),
                                ),
                            )
                            .child(
                                div()
                                    .h_flex()
                                    .flex_shrink_0()
                                    .items_center()
                                    .gap_1p5()
                                    .child(
                                        AppButton::new("sess-grp-proj")
                                            .outline()
                                            .small()
                                            .selected(group_mode == SessionGroupMode::Project)
                                            .icon(IconName::FolderOpen)
                                            .label(t("按项目分组"))
                                            .on_click(move |_, _, cx| {
                                                store_by_proj.update(cx, |s, cx| {
                                                    s.sessions_group_mode =
                                                        SessionGroupMode::Project;
                                                    cx.notify();
                                                });
                                            }),
                                    )
                                    .child(
                                        AppButton::new("sess-grp-time")
                                            .outline()
                                            .small()
                                            .selected(group_mode == SessionGroupMode::Time)
                                            .label(t("按时间分组"))
                                            .on_click(move |_, _, cx| {
                                                store_by_time.update(cx, |s, cx| {
                                                    s.sessions_group_mode = SessionGroupMode::Time;
                                                    cx.notify();
                                                });
                                            }),
                                    ),
                            ),
                    )
                    .child(groups_body)
                    .when(has_more, |d| {
                        d.child(
                            div().h_flex().justify_center().pt_2().child(
                                AppButton::new("sess-load-more")
                                    .secondary()
                                    .small()
                                    .disabled(loading)
                                    .label(t("加载更多会话"))
                                    .on_click(move |_, _, cx| {
                                        store_more
                                            .update(cx, |s, cx| s.refresh_sessions(true, false, cx));
                                    }),
                            ),
                        )
                    }),
            ))
    }
}
