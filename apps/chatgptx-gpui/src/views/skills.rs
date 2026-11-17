use chatgptx_backend::i18n::t;
use chatgptx_backend::phase4_rules::matches_management_query;
use chatgptx_core::skills::SkillRepo;
use gpui_kit::assets::IconName;
use gpui_kit::component::Icon;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::theme::ActiveTheme as _;
use gpui_kit::component::{Sizable as _, StyledExt as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::store::AppStore;
use crate::views::{avatar_badge, panel_card};
use crate::widgets::{AppBadge, AppButton};

pub struct SkillsView {
    store: Entity<AppStore>,
    search_input: Entity<InputState>,
    repo_owner_input: Entity<InputState>,
    repo_name_input: Entity<InputState>,
    repo_branch_input: Entity<InputState>,
    repo_subdir_input: Entity<InputState>,
}

impl SkillsView {
    pub fn new(store: Entity<AppStore>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search_input =
            cx.new(|cx| InputState::new(window, cx).placeholder(t("搜索 Skill 名称、ID、描述或仓库")));
        let repo_owner_input =
            cx.new(|cx| InputState::new(window, cx).placeholder(t("owner (如 openai)")));
        let repo_name_input =
            cx.new(|cx| InputState::new(window, cx).placeholder(t("repo (如 skills)")));
        let repo_branch_input = cx.new(|cx| {
            let mut s = InputState::new(window, cx).placeholder("main");
            s.set_value("main", window, cx);
            s
        });
        let repo_subdir_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder(t("可选子目录，如 skills/.curated"))
        });

        let store_search = store.clone();
        cx.subscribe(&search_input, move |_, inp, ev: &InputEvent, cx| {
            if matches!(ev, InputEvent::Change) {
                let q = inp.read(cx).value().to_string();
                store_search.update(cx, |s, cx| {
                    s.skills_search_query = q;
                    cx.notify();
                });
            }
        })
        .detach();

        cx.observe(&store, |_, _, cx| cx.notify()).detach();

        Self {
            store,
            search_input,
            repo_owner_input,
            repo_name_input,
            repo_branch_input,
            repo_subdir_input,
        }
    }

    fn render_repos_drawer(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let data = self.store.read(cx);
        if !data.skills_repos_open {
            return None;
        }
        let repos = data
            .skills_payload
            .as_ref()
            .map(|p| p.repos.clone())
            .unwrap_or_default();

        let owner_inp = self.repo_owner_input.clone();
        let name_inp = self.repo_name_input.clone();
        let branch_inp = self.repo_branch_input.clone();
        let subdir_inp = self.repo_subdir_input.clone();
        let store_add = self.store.clone();
        let store_close = self.store.clone();

        let repo_rows: Vec<AnyElement> = repos
            .into_iter()
            .map(|repo| {
                let key = repo.key();
                let repo_toggle = repo.clone();
                let store_toggle = self.store.clone();
                let key_del = key.clone();
                let store_del = self.store.clone();

                div()
                    .h_flex()
                    .w_full()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .p_3()
                    .rounded(px(8.))
                    .bg(cx.theme().secondary.opacity(0.26))
                    .border_1()
                    .border_color(cx.theme().border)
                    .child(
                        div()
                            .text_xs()
                            .font_medium()
                            .text_color(cx.theme().foreground)
                            .child(key.clone()),
                    )
                    .child(
                        div()
                            .h_flex()
                            .flex_shrink_0()
                            .items_center()
                            .gap_2()
                            .child(
                                Switch::new(SharedString::from(format!("skill-repo-sw-{key}")))
                                    .checked(repo.enabled)
                                    .on_change(move |checked, _, cx| {
                                        let mut next = repo_toggle.clone();
                                        next.enabled = *checked;
                                        store_toggle.update(cx, |s, cx| {
                                            s.upsert_skill_repo_entry(next, cx)
                                        });
                                    }),
                            )
                            .child(
                                AppButton::new(SharedString::from(format!(
                                    "skill-repo-del-{key}"
                                )))
                                .ghost()
                                .small_icon()
                                .icon(IconName::Trash)
                                .on_click(move |_, _, cx| {
                                    let k = key_del.clone();
                                    store_del.update(cx, |s, cx| s.delete_skill_repo_by_key(k, cx));
                                }),
                            ),
                    )
                    .into_any_element()
            })
            .collect();

        Some(
            panel_card(
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
                                    .text_sm()
                                    .font_semibold()
                                    .text_color(cx.theme().foreground)
                                    .child(t("GitHub Skill 仓库源管理")),
                            )
                            .child(
                                AppButton::new("skill-repos-close")
                                    .ghost()
                                    .small_icon()
                                    .icon(IconName::X)
                                    .on_click(move |_, _, cx| {
                                        store_close.update(cx, |s, cx| {
                                            s.skills_repos_open = false;
                                            cx.notify();
                                        });
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .h_flex()
                            .w_full()
                            .gap_2()
                            .child(div().flex_1().min_w_0().child(Input::new(&self.repo_owner_input)))
                            .child(div().flex_1().min_w_0().child(Input::new(&self.repo_name_input)))
                            .child(div().w(px(110.)).child(Input::new(&self.repo_branch_input)))
                            .child(div().flex_1().min_w_0().child(Input::new(&self.repo_subdir_input)))
                            .child(
                                AppButton::new("skill-repo-add-btn")
                                    .primary()
                                    .small()
                                    .icon(IconName::Plus)
                                    .label(t("添加源"))
                                    .on_click(move |_, window, cx| {
                                        let owner = owner_inp.read(cx).value().trim().to_string();
                                        let name = name_inp.read(cx).value().trim().to_string();
                                        let branch = branch_inp.read(cx).value().trim().to_string();
                                        let subdir = subdir_inp.read(cx).value().trim().to_string();
                                        if !owner.is_empty() && !name.is_empty() {
                                            owner_inp.update(cx, |i, cx| i.set_value("", window, cx));
                                            name_inp.update(cx, |i, cx| i.set_value("", window, cx));
                                            subdir_inp
                                                .update(cx, |i, cx| i.set_value("", window, cx));
                                        }
                                        store_add.update(cx, |s, cx| {
                                            s.upsert_skill_repo_entry(
                                                SkillRepo {
                                                    owner,
                                                    name,
                                                    branch: if branch.is_empty() {
                                                        "main".to_string()
                                                    } else {
                                                        branch
                                                    },
                                                    subdir,
                                                    enabled: true,
                                                },
                                                cx,
                                            );
                                        });
                                    }),
                            ),
                    )
                    .children(repo_rows),
            )
            .into_any_element(),
        )
    }
}

impl Render for SkillsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let data = self.store.read(cx);
        let loading = data.skills_loading;
        let busy_id = data.skills_busy_id.clone();
        let filter_tab = data.skills_filter_tab;
        let query = data.skills_search_query.clone();
        let payload = data.skills_payload.clone();

        let all_skills = payload.as_ref().map(|p| p.skills.clone()).unwrap_or_default();
        let backups = payload.as_ref().map(|p| p.backups.clone()).unwrap_or_default();
        let repo_errors = payload
            .as_ref()
            .map(|p| p.repo_errors.clone())
            .unwrap_or_default();

        let installed_cnt = all_skills.iter().filter(|s| s.installed).count();
        let enabled_cnt = all_skills.iter().filter(|s| s.enabled).count();
        let update_cnt = all_skills.iter().filter(|s| s.update_available).count();

        let filtered: Vec<_> = all_skills
            .into_iter()
            .filter(|s| match filter_tab {
                "installed" => s.installed,
                "updates" => s.update_available,
                _ => true,
            })
            .filter(|s| {
                matches_management_query(&[&s.id, &s.name, &s.description, &s.repo_key], &query)
            })
            .collect();

        let store_repos_btn = self.store.clone();
        let store_refresh_btn = self.store.clone();
        let repos_drawer = self.render_repos_drawer(cx);

        let filter_buttons: Vec<AnyElement> = [
            ("all", t("全部")),
            ("installed", format!("{} ({installed_cnt})", t("已安装"))),
            ("updates", format!("{} ({update_cnt})", t("可更新"))),
        ]
        .into_iter()
        .map(|(tab_key, lbl)| {
            let st = self.store.clone();
            AppButton::new(SharedString::from(format!("skills-tab-{tab_key}")))
                .outline()
                .small()
                .selected(filter_tab == tab_key)
                .label(lbl)
                .on_click(move |_, _, cx| {
                    st.update(cx, |s, cx| {
                        s.skills_filter_tab = tab_key;
                        cx.notify();
                    });
                })
                .into_any_element()
        })
        .collect();

        let skills_body: AnyElement = if filtered.is_empty() {
            div()
                .p_5()
                .w_full()
                .rounded(px(10.))
                .bg(cx.theme().secondary.opacity(0.24))
                .border_1()
                .border_color(cx.theme().border)
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(if loading {
                    t("正在拉取远端 GitHub Skill 目录…")
                } else {
                    t("当前筛选下没有匹配的 Skill；点击右上角「刷新远端目录」拉取最新官方与社区技能库。")
                })
                .into_any_element()
        } else {
            let rows: Vec<AnyElement> = filtered
                .into_iter()
                .map(|skill| {
                    let is_busy = busy_id.as_deref() == Some(skill.id.as_str());
                    let id_sw = skill.id.clone();
                    let store_sw = self.store.clone();
                    let rk_inst = skill.repo_key.clone();
                    let id_inst = skill.id.clone();
                    let upd = skill.update_available;
                    let store_inst = self.store.clone();
                    let id_uninst = skill.id.clone();
                    let store_uninst = self.store.clone();

                    let title = if skill.name.trim().is_empty() {
                        skill.id.clone()
                    } else {
                        skill.name.clone()
                    };
                    let initial = title
                        .chars()
                        .next()
                        .map(|c| c.to_uppercase().to_string())
                        .unwrap_or_else(|| "S".to_string());

                    div()
                        .h_flex()
                        .w_full()
                        .items_center()
                        .justify_between()
                        .gap_3()
                        .p_3p5()
                        .rounded(px(10.))
                        .bg(cx.theme().secondary.opacity(0.28))
                        .border_1()
                        .border_color(cx.theme().border)
                        .child(
                            div()
                                .h_flex()
                                .items_center()
                                .gap_3()
                                .flex_1()
                                .min_w_0()
                                .child(avatar_badge(cx, initial))
                                .child(
                                    div()
                                        .v_flex()
                                        .flex_1()
                                        .min_w_0()
                                        .gap_1()
                                        .child(
                                            div()
                                                .h_flex()
                                                .flex_wrap()
                                                .items_center()
                                                .gap_2()
                                                .child(
                                                    div()
                                                        .text_sm()
                                                        .font_semibold()
                                                        .text_color(cx.theme().foreground)
                                                        .child(title),
                                                )
                                                .when(skill.bundled, |d| {
                                                    d.child(AppBadge::info(t("系统内置")))
                                                })
                                                .when(skill.installed && !skill.bundled, |d| {
                                                    d.child(AppBadge::good(t("已安装")))
                                                })
                                                .when(skill.update_available, |d| {
                                                    d.child(AppBadge::warn(t("有更新")))
                                                })
                                                .when(!skill.repo_key.is_empty(), |d| {
                                                    d.child(AppBadge::outline(
                                                        skill.repo_key.clone(),
                                                    ))
                                                }),
                                        )
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(cx.theme().muted_foreground)
                                                .child(if skill.description.trim().is_empty() {
                                                    skill.id.clone()
                                                } else {
                                                    skill.description.clone()
                                                }),
                                        ),
                                ),
                        )
                        .child(
                            div()
                                .h_flex()
                                .flex_shrink_0()
                                .items_center()
                                .gap_2()
                                .when(skill.installed && !skill.bundled, |d| {
                                    d.child(
                                        Switch::new(SharedString::from(format!(
                                            "skill-en-{}",
                                            skill.id
                                        )))
                                        .checked(skill.enabled)
                                        .on_change(move |checked, _, cx| {
                                            let id = id_sw.clone();
                                            let next = *checked;
                                            store_sw.update(cx, |s, cx| {
                                                s.toggle_skill_enabled(id, next, cx)
                                            });
                                        }),
                                    )
                                })
                                .when(
                                    !skill.bundled && (!skill.installed || skill.update_available),
                                    |d| {
                                        d.child(
                                            AppButton::new(SharedString::from(format!(
                                                "skill-inst-{}",
                                                skill.id
                                            )))
                                            .primary()
                                            .small()
                                            .disabled(is_busy || skill.repo_key.is_empty())
                                            .icon(IconName::Download)
                                            .label(if is_busy {
                                                t("处理中…")
                                            } else if skill.update_available {
                                                t("更新")
                                            } else {
                                                t("安装")
                                            })
                                            .on_click(move |_, _, cx| {
                                                let rk = rk_inst.clone();
                                                let id = id_inst.clone();
                                                store_inst.update(cx, |s, cx| {
                                                    s.install_or_update_skill(rk, id, upd, cx)
                                                });
                                            }),
                                        )
                                    },
                                )
                                .when(skill.installed && !skill.bundled, |d| {
                                    d.child(
                                        AppButton::new(SharedString::from(format!(
                                            "skill-uninst-{}",
                                            skill.id
                                        )))
                                        .ghost()
                                        .small_icon()
                                        .icon(IconName::Trash)
                                        .tooltip(t("卸载（自动移入备份区）"))
                                        .on_click(move |_, _, cx| {
                                            let id = id_uninst.clone();
                                            store_uninst.update(cx, |s, cx| {
                                                s.uninstall_skill_by_id(id, cx)
                                            });
                                        }),
                                    )
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
                    .justify_between()
                    .gap_3()
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .gap_2()
                            .child(AppBadge::secondary(format!(
                                "{}：{installed_cnt}",
                                t("已安装")
                            )))
                            .child(AppBadge::good(format!("{}：{enabled_cnt}", t("启用中")))),
                    )
                    .child(
                        div()
                            .h_flex()
                            .flex_shrink_0()
                            .items_center()
                            .gap_2()
                            .child(
                                AppButton::new("skills-top-repos")
                                    .secondary()
                                    .small()
                                    .icon(IconName::FolderOpen)
                                    .label(t("仓库源管理"))
                                    .on_click(move |_, _, cx| {
                                        store_repos_btn.update(cx, |s, cx| {
                                            s.skills_repos_open = !s.skills_repos_open;
                                            cx.notify();
                                        });
                                    }),
                            )
                            .child(
                                AppButton::new("skills-top-refresh")
                                    .primary()
                                    .small()
                                    .disabled(loading)
                                    .icon(IconName::RefreshCw)
                                    .label(if loading {
                                        t("刷新中…")
                                    } else {
                                        t("刷新远端目录")
                                    })
                                    .on_click(move |_, _, cx| {
                                        store_refresh_btn
                                            .update(cx, |s, cx| s.refresh_skills_remote(cx));
                                    }),
                            ),
                    ),
            )
            .when_some(repos_drawer, |d, b| d.child(b))
            .when(!repo_errors.is_empty(), |d| {
                d.child(
                    div()
                        .p_3()
                        .rounded(px(9.))
                        .bg(cx.theme().warning.opacity(0.12))
                        .border_1()
                        .border_color(cx.theme().warning.opacity(0.45))
                        .text_xs()
                        .text_color(cx.theme().warning)
                        .child(repo_errors.join("；")),
                )
            })
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
                            .gap_3()
                            .child(
                                div().flex_1().min_w_0().child(
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
                                    .children(filter_buttons),
                            ),
                    )
                    .child(skills_body),
            ))
            .when(!backups.is_empty(), |root| {
                let b_rows: Vec<AnyElement> = backups
                    .into_iter()
                    .map(|bk| {
                        let bid_res = bk.id.clone();
                        let store_res = self.store.clone();
                        let bid_del = bk.id.clone();
                        let store_del = self.store.clone();
                        div()
                            .h_flex()
                            .w_full()
                            .items_center()
                            .justify_between()
                            .gap_3()
                            .p_3()
                            .rounded(px(9.))
                            .bg(cx.theme().secondary.opacity(0.24))
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
                                            .text_xs()
                                            .font_semibold()
                                            .text_color(cx.theme().foreground)
                                            .child(format!("{} ({})", bk.name, bk.skill_id)),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(bk.backed_up_at),
                                    ),
                            )
                            .child(
                                div()
                                    .h_flex()
                                    .flex_shrink_0()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        AppButton::new(SharedString::from(format!(
                                            "skill-bk-res-{}",
                                            bk.id
                                        )))
                                        .secondary()
                                        .small()
                                        .label(t("恢复"))
                                        .on_click(move |_, _, cx| {
                                            let id = bid_res.clone();
                                            store_res.update(cx, |s, cx| {
                                                s.restore_skill_from_backup(id, cx)
                                            });
                                        }),
                                    )
                                    .child(
                                        AppButton::new(SharedString::from(format!(
                                            "skill-bk-del-{}",
                                            bk.id
                                        )))
                                        .ghost()
                                        .small_icon()
                                        .icon(IconName::Trash)
                                        .on_click(move |_, _, cx| {
                                            let id = bid_del.clone();
                                            store_del.update(cx, |s, cx| {
                                                s.delete_skill_backup_by_id(id, cx)
                                            });
                                        }),
                                    ),
                            )
                            .into_any_element()
                    })
                    .collect();

                root.child(panel_card(
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
                                .child(t("已卸载 Skill 备份恢复区")),
                        )
                        .children(b_rows),
                ))
            })
    }
}
