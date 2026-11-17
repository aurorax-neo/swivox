use std::collections::HashMap;

use chatgptx_backend::i18n::{t, tf};
use chatgptx_backend::phase2_rules::{
    aggregate_strategy_label, multi_launch_supported, provider_initial, relay_profile_config_brief,
    relay_profile_mode_label, relay_protocol_label,
};
use chatgptx_backend::phase3_rules::{
    CONTEXT_WINDOW_PRESETS, DEFAULT_AUTO_COMPACT_PERCENT, ModelWindowRow, PROVIDER_PRESETS,
    PresetCategory, ProviderViewKind, apply_editable_fields_to_profile,
    hydrate_profile_editable_fields, merge_fetched_models_into_rows, model_slug_from_row_name,
    relay_provider_view, toggle_aggregate_member, validate_model_window_rows,
};
use chatgptx_core::relay_config::relay_profile_model;
use chatgptx_core::settings::{
    AggregateRelayProfile, AggregateRelayStrategy, RelayMode, RelayProfile, RelayProtocol,
    RelaySessionProvider,
};
use gpui_kit::assets::IconName;
use gpui_kit::component::Icon;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::theme::ActiveTheme as _;
use gpui_kit::component::{Sizable as _, StyledExt as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::store::{AppStore, RelaySubscreen};
use crate::views::{avatar_badge, panel_card};
use crate::widgets::{AppBadge, AppButton, AppSelectTrigger};

#[derive(Clone, Copy, PartialEq, Eq)]
enum EditorOpenDropdown {
    Mode,
    Protocol,
    OfficialBind,
    Strategy,
}

struct ModelRowControls {
    row_uid: u64,
    model_input: Entity<InputState>,
    window_input: Entity<InputState>,
    compact_input: Entity<InputState>,
    image_handling: String,
}

pub struct RelayView {
    store: Entity<AppStore>,

    // 列表与预设搜索输入
    ccs_db_input: Entity<InputState>,
    catalog_search_input: Entity<InputState>,

    // 编辑器草稿状态
    applied_seed_seq: u64,
    applied_fetched_seq: u64,
    next_row_uid: u64,
    draft_profile: RelayProfile,
    draft_aggregate: Option<AggregateRelayProfile>,
    draft_is_new: bool,
    draft_mode: RelayMode,
    draft_official_mix_key: bool,
    draft_protocol: RelayProtocol,
    open_dropdown: Option<EditorOpenDropdown>,
    validation_error: Option<String>,

    name_input: Entity<InputState>,
    base_url_input: Entity<InputState>,
    api_key_input: Entity<InputState>,
    model_input: Entity<InputState>,
    config_textarea: Entity<TextareaState>,
    row_controls: Vec<ModelRowControls>,
}

impl RelayView {
    pub fn new(store: Entity<AppStore>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let initial_ccs_path = store.read(cx).settings.ccs_db_path.clone();
        let ccs_db_input = cx.new(|cx| {
            let mut s = InputState::new(window, cx).placeholder(t("留空使用默认路径"));
            if !initial_ccs_path.is_empty() {
                s.set_value(initial_ccs_path, window, cx);
            }
            s
        });
        let catalog_search_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder(t("搜索 DeepSeek、GLM、Kimi、OpenRouter…"))
        });

        let name_input = cx.new(|cx| InputState::new(window, cx).placeholder(t("例如：DeepSeek 官方")));
        let base_url_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("https://api.example.com/v1"));
        let api_key_input = cx.new(|cx| {
            InputState::new(window, cx)
                .masked(true)
                .placeholder("sk-...")
        });
        let model_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("gpt-5.5 / deepseek-v4-pro"));
        let config_textarea = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(t("# Codex config.toml 预览（保存时自动与上方字段及模型窗口表同步）"))
        });

        let store_for_catalog = store.clone();
        cx.subscribe(
            &catalog_search_input,
            move |_this, input, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    let q = input.read(cx).value().to_string();
                    store_for_catalog.update(cx, |s, cx| {
                        s.relay_catalog_query = q;
                        cx.notify();
                    });
                }
            },
        )
        .detach();

        cx.observe(&store, |_, _, cx| cx.notify()).detach();

        Self {
            store,
            ccs_db_input,
            catalog_search_input,
            applied_seed_seq: 0,
            applied_fetched_seq: 0,
            next_row_uid: 1,
            draft_profile: RelayProfile::default(),
            draft_aggregate: None,
            draft_is_new: false,
            draft_mode: RelayMode::PureApi,
            draft_official_mix_key: false,
            draft_protocol: RelayProtocol::Responses,
            open_dropdown: None,
            validation_error: None,
            name_input,
            base_url_input,
            api_key_input,
            model_input,
            config_textarea,
            row_controls: Vec::new(),
        }
    }

    fn alloc_row_control(
        &mut self,
        row: &ModelWindowRow,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> ModelRowControls {
        let uid = self.next_row_uid;
        self.next_row_uid = self.next_row_uid.wrapping_add(1);

        let model_val = row.model.clone();
        let win_val = row.window.clone();
        let compact_val = row.auto_compact.clone();

        let model_input = cx.new(|cx| {
            let mut s = InputState::new(window, cx).placeholder("deepseek-v4-pro[1M]");
            s.set_value(model_val, window, cx);
            s
        });
        let window_input = cx.new(|cx| {
            let mut s = InputState::new(window, cx).placeholder("128k / 1M");
            s.set_value(win_val, window, cx);
            s
        });
        let compact_input = cx.new(|cx| {
            let mut s = InputState::new(window, cx).placeholder("90");
            s.set_value(compact_val, window, cx);
            s
        });

        ModelRowControls {
            row_uid: uid,
            model_input,
            window_input,
            compact_input,
            image_handling: row.image_handling.clone(),
        }
    }

    fn sync_seed_if_needed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let seed_opt = self.store.read(cx).relay_editor_seed.clone();
        if let Some(seed) = seed_opt
            && seed.seq != self.applied_seed_seq
        {
            self.applied_seed_seq = seed.seq;
            self.draft_profile = seed.profile.clone();
            self.draft_aggregate = seed.aggregate;
            self.draft_is_new = seed.is_new;
            self.open_dropdown = None;
            self.validation_error = None;

            let hydrated = hydrate_profile_editable_fields(&seed.profile);
            self.draft_mode = hydrated.relay_mode;
            self.draft_official_mix_key = hydrated.official_mix_api_key;
            self.draft_protocol = hydrated.protocol;

            self.name_input
                .update(cx, |inp, cx| inp.set_value(hydrated.name, window, cx));
            self.base_url_input
                .update(cx, |inp, cx| inp.set_value(hydrated.base_url, window, cx));
            self.api_key_input
                .update(cx, |inp, cx| inp.set_value(hydrated.api_key, window, cx));
            self.model_input
                .update(cx, |inp, cx| inp.set_value(hydrated.model, window, cx));
            self.config_textarea.update(cx, |inp, cx| {
                inp.set_value(seed.profile.config_contents.clone(), window, cx);
            });

            self.row_controls.clear();
            for r in &hydrated.rows {
                let ctrl = self.alloc_row_control(r, window, cx);
                self.row_controls.push(ctrl);
            }
        }

        let fetched_opt = self.store.read(cx).last_fetched_models.clone();
        if let Some((seq, models)) = fetched_opt
            && seq != self.applied_fetched_seq
        {
            self.applied_fetched_seq = seq;
            let mut current_rows = self.collect_model_rows(cx);
            let added = merge_fetched_models_into_rows(&mut current_rows, &models);
            if added > 0 {
                let start = current_rows.len().saturating_sub(added);
                for r in &current_rows[start..] {
                    let ctrl = self.alloc_row_control(r, window, cx);
                    self.row_controls.push(ctrl);
                }
            }
        }
    }

    fn collect_model_rows(&self, cx: &App) -> Vec<ModelWindowRow> {
        self.row_controls
            .iter()
            .map(|ctrl| ModelWindowRow {
                model: ctrl.model_input.read(cx).value().to_string(),
                window: ctrl.window_input.read(cx).value().to_string(),
                auto_compact: ctrl.compact_input.read(cx).value().to_string(),
                image_handling: ctrl.image_handling.clone(),
            })
            .collect()
    }

    fn build_validated_draft(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(RelayProfile, Option<AggregateRelayProfile>), String> {
        let name = self.name_input.read(cx).value().trim().to_string();
        if name.is_empty() {
            return Err(t("请填写供应商名称。"));
        }

        if self.draft_mode == RelayMode::Aggregate {
            let mut agg = self.draft_aggregate.clone().unwrap_or(AggregateRelayProfile {
                id: self.draft_profile.id.clone(),
                name: name.clone(),
                session_provider: RelaySessionProvider::Custom,
                model_aggregation_enabled: false,
                strategy: AggregateRelayStrategy::Failover,
                members: Vec::new(),
                routes: Vec::new(),
                default_relay_id: String::new(),
                sync_default_member_login: false,
            });
            if agg.members.is_empty() {
                return Err(t("请至少勾选一个成员供应商。"));
            }
            agg.id = self.draft_profile.id.clone();
            agg.name = name.clone();
            if agg.model_aggregation_enabled
                && (agg.default_relay_id.is_empty()
                    || !agg.members.iter().any(|m| m.relay_id == agg.default_relay_id))
            {
                agg.default_relay_id = agg.members[0].relay_id.clone();
            }

            let mut profile = self.draft_profile.clone();
            profile.name = name;
            profile.relay_mode = RelayMode::Aggregate;
            return Ok((profile, Some(agg)));
        }

        let model = self.model_input.read(cx).value().trim().to_string();
        let base_url = self.base_url_input.read(cx).value().trim().to_string();
        let api_key = self.api_key_input.read(cx).value().trim().to_string();
        let rows = self.collect_model_rows(cx);

        validate_model_window_rows(&rows)?;

        let mut profile = self.draft_profile.clone();
        // 允许用户在下方 config.toml 文本框中保留自定义额外键
        let custom_toml = self.config_textarea.read(cx).value().to_string();
        if !custom_toml.trim().is_empty() {
            profile.config_contents = custom_toml;
        }

        apply_editable_fields_to_profile(
            &mut profile,
            &name,
            &model,
            &base_url,
            &api_key,
            self.draft_protocol,
            self.draft_mode,
            self.draft_official_mix_key,
            &rows,
        )?;

        self.config_textarea.update(cx, |inp, cx| {
            inp.set_value(profile.config_contents.clone(), window, cx);
        });

        Ok((profile, None))
    }

    fn execute_save_draft(
        &mut self,
        switch_after_save: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.build_validated_draft(window, cx) {
            Ok((profile, agg)) => {
                self.validation_error = None;
                let is_new = self.draft_is_new;
                self.store.update(cx, |s, cx| {
                    s.save_edited_relay_profile(profile, agg, is_new, switch_after_save, cx);
                });
            }
            Err(err) => {
                self.validation_error = Some(err.clone());
                self.store
                    .update(cx, |s, cx| s.set_notice(t("保存供应商"), err, "failed", cx));
                cx.notify();
            }
        }
    }

    // ========================================================================
    // 1. 供应商列表子视图 (List)
    // ========================================================================

    fn render_env_conflict_banner(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let conflicts = &self.store.read(cx).env_conflicts;
        if conflicts.is_empty() {
            return None;
        }
        let summary: Vec<String> = conflicts
            .iter()
            .map(|c| {
                let src = match c.source {
                    chatgptx_core::env_conflicts::EnvConflictSource::Process => t("当前进程"),
                    chatgptx_core::env_conflicts::EnvConflictSource::User => t("用户环境"),
                };
                format!("{} ({src})", c.name)
            })
            .collect();
        let store_clean = self.store.clone();

        Some(
            div()
                .h_flex()
                .w_full()
                .items_center()
                .justify_between()
                .gap_3()
                .p_4()
                .rounded(px(10.))
                .bg(cx.theme().warning.opacity(0.1))
                .border_1()
                .border_color(cx.theme().warning.opacity(0.45))
                .child(
                    div()
                        .h_flex()
                        .items_start()
                        .gap_3()
                        .flex_1()
                        .min_w_0()
                        .child(
                            div().pt_0p5().child(
                                Icon::new(IconName::TriangleAlert)
                                    .small()
                                    .text_color(cx.theme().warning),
                            ),
                        )
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
                                        .child(t("检测到可能覆盖 Codex 供应商配置的 OPENAI 环境变量")),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(summary.join(" · ")),
                                ),
                        ),
                )
                .child(
                    AppButton::new("relay-clean-env-conflicts")
                        .secondary()
                        .small()
                        .icon(IconName::Trash)
                        .label(t("备份并移除冲突变量"))
                        .on_click(move |_, _, cx| {
                            store_clean.update(cx, |s, cx| s.remove_all_env_conflicts(cx));
                        }),
                )
                .into_any_element(),
        )
    }

    fn render_pending_url_import_banner(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let req = self.store.read(cx).pending_provider_import.clone()?;
        let store_ok = self.store.clone();
        let store_dismiss = self.store.clone();

        Some(
            div()
                .h_flex()
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
                        .v_flex()
                        .gap_1()
                        .flex_1()
                        .min_w_0()
                        .child(
                            div()
                                .text_sm()
                                .font_semibold()
                                .text_color(cx.theme().foreground)
                                .child(format!("{}：{}", t("待确认导入供应商"), req.name)),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(format!("Base URL: {}", req.base_url)),
                        ),
                )
                .child(
                    div()
                        .h_flex()
                        .items_center()
                        .gap_2()
                        .child(
                            AppButton::new("relay-url-import-dismiss")
                                .outline()
                                .small()
                                .label(t("忽略"))
                                .on_click(move |_, _, cx| {
                                    store_dismiss
                                        .update(cx, |s, cx| s.dismiss_pending_url_import(cx));
                                }),
                        )
                        .child(
                            AppButton::new("relay-url-import-confirm")
                                .primary()
                                .small()
                                .icon(IconName::CircleCheck)
                                .label(t("确认导入"))
                                .on_click(move |_, _, cx| {
                                    store_ok.update(cx, |s, cx| s.confirm_pending_url_import(cx));
                                }),
                        ),
                )
                .into_any_element(),
        )
    }

    fn render_ccs_import_drawer(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let data = self.store.read(cx);
        if !data.ccs_import_open {
            return None;
        }
        let ccs = data.ccs_providers.clone();
        let count = ccs.as_ref().map(|p| p.providers.len()).unwrap_or(0);
        let db_source = ccs
            .as_ref()
            .map(|p| p.db_path.clone())
            .unwrap_or_default();
        let fallback = ccs.as_ref().and_then(|p| p.fallback_reason.clone());

        let ccs_input_for_save = self.ccs_db_input.clone();
        let ccs_input_for_pick = self.ccs_db_input.clone();
        let store_save = self.store.clone();
        let store_pick = self.store.clone();
        let store_refresh = self.store.clone();
        let store_import = self.store.clone();
        let store_close = self.store.clone();

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
                        .h_flex()
                        .w_full()
                        .items_center()
                        .justify_between()
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
                                        .child(t("从第三方导入 (cc-switch)")),
                                )
                                .child(AppBadge::info(format!("{count} {}", t("个可导入")))),
                        )
                        .child(
                            AppButton::new("ccs-drawer-close")
                                .ghost()
                                .small_icon()
                                .icon(IconName::X)
                                .on_click(move |_, _, cx| {
                                    store_close.update(cx, |s, cx| {
                                        s.ccs_import_open = false;
                                        cx.notify();
                                    });
                                }),
                        ),
                )
                .child(
                    div()
                        .h_flex()
                        .w_full()
                        .items_center()
                        .gap_2()
                        .child(div().flex_1().child(Input::new(&self.ccs_db_input)))
                        .child(
                            AppButton::new("ccs-pick-file")
                                .secondary()
                                .small()
                                .icon(IconName::FolderOpen)
                                .label(t("选择数据库"))
                                .on_click(move |_, window, cx| {
                                    let inp = ccs_input_for_pick.clone();
                                    let st = store_pick.clone();
                                    let win_handle = window.window_handle();
                                    cx.spawn(async move |cx| {
                                        if let Some(file) = rfd::AsyncFileDialog::new()
                                            .add_filter("SQLite", &["db", "sqlite", "sqlite3"])
                                            .pick_file()
                                            .await
                                        {
                                            let path_str =
                                                file.path().to_string_lossy().to_string();
                                            cx.update(|app| {
                                                let _ = win_handle.update(app, |_, win, cx| {
                                                    inp.update(cx, |state, cx| {
                                                        state.set_value(
                                                            path_str.clone(),
                                                            win,
                                                            cx,
                                                        );
                                                    });
                                                });
                                                st.update(app, |s, cx| {
                                                    s.save_ccs_db_path(path_str, cx)
                                                });
                                            });
                                        }
                                    })
                                    .detach();
                                }),
                        )
                        .child(
                            AppButton::new("ccs-save-path")
                                .secondary()
                                .small()
                                .label(t("保存路径并刷新"))
                                .on_click(move |_, _, cx| {
                                    let val = ccs_input_for_save.read(cx).value().to_string();
                                    store_save.update(cx, |s, cx| s.save_ccs_db_path(val, cx));
                                }),
                        ),
                )
                .when(!db_source.is_empty(), |d| {
                    d.child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("{}：{db_source}", t("实际来源"))),
                    )
                })
                .when_some(fallback, |d, reason| {
                    d.child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().warning)
                            .child(reason),
                    )
                })
                .child(
                    div()
                        .h_flex()
                        .items_center()
                        .gap_2()
                        .child(
                            AppButton::new("ccs-confirm-import")
                                .primary()
                                .small()
                                .disabled(count == 0)
                                .icon(IconName::Download)
                                .label(t("一键导入 cc-switch 供应商"))
                                .on_click(move |_, _, cx| {
                                    store_import.update(cx, |s, cx| s.import_ccs_providers_now(cx));
                                }),
                        )
                        .child(
                            AppButton::new("ccs-refresh-list")
                                .outline()
                                .small()
                                .icon(IconName::RefreshCw)
                                .label(t("刷新列表"))
                                .on_click(move |_, _, cx| {
                                    store_refresh.update(cx, |s, cx| s.refresh_ccs_providers(cx));
                                }),
                        ),
                )
                .into_any_element(),
        )
    }

    fn render_list_modals(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let data = self.store.read(cx);

        if let Some(target_id) = data.use_confirm_relay_id.clone() {
            let display_name = data
                .settings
                .relay_profiles
                .iter()
                .find(|p| p.id == target_id)
                .map(|p| {
                    if p.name.trim().is_empty() {
                        p.id.clone()
                    } else {
                        p.name.clone()
                    }
                })
                .unwrap_or_else(|| target_id.clone());

            let id_only = target_id.clone();
            let id_restart = target_id;
            let store_cancel = self.store.clone();
            let store_only = self.store.clone();
            let store_restart = self.store.clone();

            return Some(
                div()
                    .v_flex()
                    .w_full()
                    .gap_3()
                    .p_4()
                    .rounded(px(11.))
                    .bg(cx.theme().popover)
                    .border_1()
                    .border_color(cx.theme().primary.opacity(0.6))
                    .child(
                        div()
                            .text_base()
                            .font_semibold()
                            .text_color(cx.theme().foreground)
                            .child(t("切换供应商配置")),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!(
                                "{}「{}」{}",
                                t("请选择切换供应商"),
                                display_name,
                                t("后的生效方式：")
                            )),
                    )
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .justify_end()
                            .gap_2()
                            .child(
                                AppButton::new("relay-use-cancel")
                                    .outline()
                                    .small()
                                    .label(t("取消"))
                                    .on_click(move |_, _, cx| {
                                        store_cancel.update(cx, |s, cx| {
                                            s.use_confirm_relay_id = None;
                                            cx.notify();
                                        });
                                    }),
                            )
                            .child(
                                AppButton::new("relay-use-only")
                                    .secondary()
                                    .small()
                                    .icon(IconName::CircleCheck)
                                    .label(t("仅使用（重启生效）"))
                                    .on_click(move |_, _, cx| {
                                        let id = id_only.clone();
                                        store_only.update(cx, |s, cx| {
                                            s.switch_relay_profile(id, false, cx)
                                        });
                                    }),
                            )
                            .child(
                                AppButton::new("relay-use-restart")
                                    .primary()
                                    .small()
                                    .icon(IconName::Rocket)
                                    .label(t("使用并重启 Codex"))
                                    .on_click(move |_, _, cx| {
                                        let id = id_restart.clone();
                                        store_restart.update(cx, |s, cx| {
                                            s.switch_relay_profile(id, true, cx)
                                        });
                                    }),
                            ),
                    )
                    .into_any_element(),
            );
        }

        if let Some(del_id) = data.delete_confirm_relay_id.clone() {
            let display_name = data
                .settings
                .relay_profiles
                .iter()
                .find(|p| p.id == del_id)
                .map(|p| {
                    if p.name.trim().is_empty() {
                        p.id.clone()
                    } else {
                        p.name.clone()
                    }
                })
                .unwrap_or_else(|| del_id.clone());
            let store_cancel = self.store.clone();
            let store_del = self.store.clone();

            return Some(
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
                            .text_base()
                            .font_semibold()
                            .text_color(cx.theme().foreground)
                            .child(t("删除供应商")),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(tf("确定删除供应商「{0}」吗？", &[&display_name])),
                    )
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .justify_end()
                            .gap_2()
                            .child(
                                AppButton::new("relay-del-cancel")
                                    .secondary()
                                    .small()
                                    .label(t("取消"))
                                    .on_click(move |_, _, cx| {
                                        store_cancel.update(cx, |s, cx| {
                                            s.delete_confirm_relay_id = None;
                                            cx.notify();
                                        });
                                    }),
                            )
                            .child(
                                AppButton::new("relay-del-confirm")
                                    .danger()
                                    .small()
                                    .icon(IconName::Trash)
                                    .label(t("确认删除"))
                                    .on_click(move |_, _, cx| {
                                        let id = del_id.clone();
                                        store_del.update(cx, |s, cx| s.delete_relay(id, cx));
                                    }),
                            ),
                    )
                    .into_any_element(),
            );
        }

        None
    }

    fn render_list_subscreen(&self, cx: &mut Context<Self>) -> AnyElement {
        let data = self.store.read(cx);
        let master_enabled = data.settings.relay_profiles_enabled;
        let current_tab = data.relay_view_tab;
        let active_relay_id = data.settings.active_relay_id.clone();
        let total_profiles = data.settings.relay_profiles.len();
        let relay_switching = data.relay_switching;

        let agg_map: HashMap<String, AggregateRelayProfile> = data
            .settings
            .aggregate_relay_profiles
            .iter()
            .map(|a| (a.id.clone(), a.clone()))
            .collect();

        // 判断当前是否是纯官方账号生效（未绑定任何第三方供应商）
        let pure_official_active = data
            .official_snapshot
            .as_ref()
            .and_then(|s| s.accounts.iter().find(|a| a.is_active))
            .is_some_and(|a| {
                a.bound_relay_profile_id
                    .as_deref()
                    .unwrap_or("")
                    .trim()
                    .is_empty()
            });

        // 统计 4 个分区各自数量
        let mut count_direct = 0usize;
        let mut count_model_agg = 0usize;
        let mut count_routing = 0usize;
        let mut count_failover = 0usize;
        for p in &data.settings.relay_profiles {
            match relay_provider_view(p, &agg_map) {
                ProviderViewKind::Direct => count_direct += 1,
                ProviderViewKind::ModelAggregation => count_model_agg += 1,
                ProviderViewKind::Routing => count_routing += 1,
                ProviderViewKind::Failover => count_failover += 1,
            }
        }

        let visible_profiles: Vec<RelayProfile> = data
            .settings
            .relay_profiles
            .iter()
            .filter(|p| relay_provider_view(p, &agg_map) == current_tab)
            .cloned()
            .collect();

        let store_ccs_btn = self.store.clone();
        let store_add_btn = self.store.clone();
        let store_switch_master = self.store.clone();

        let tabs = [
            (
                ProviderViewKind::Direct,
                IconName::Cable,
                t("直连"),
                count_direct,
            ),
            (
                ProviderViewKind::ModelAggregation,
                IconName::Layers,
                t("模型聚合"),
                count_model_agg,
            ),
            (
                ProviderViewKind::Routing,
                IconName::GitBranch,
                t("需协议转换"),
                count_routing,
            ),
            (
                ProviderViewKind::Failover,
                IconName::ArrowLeftRight,
                t("故障转移"),
                count_failover,
            ),
        ];

        let tab_buttons: Vec<AnyElement> = tabs
            .into_iter()
            .map(|(kind, icon, label, cnt)| {
                let selected = current_tab == kind;
                let store_tab = self.store.clone();
                let bg = if selected {
                    cx.theme().background
                } else {
                    cx.theme().transparent
                };
                let fg = if selected {
                    cx.theme().foreground
                } else {
                    cx.theme().muted_foreground
                };

                div()
                    .id(SharedString::from(format!("relay-tab-{kind:?}")))
                    .h_flex()
                    .flex_1()
                    .h(px(34.))
                    .px_3()
                    .items_center()
                    .justify_center()
                    .gap_2()
                    .rounded(px(8.))
                    .cursor_pointer()
                    .bg(bg)
                    .when(selected, |d| {
                        d.border_1().border_color(cx.theme().border)
                    })
                    .hover(|s| s.text_color(cx.theme().foreground))
                    .on_click(move |_, _, cx| {
                        store_tab.update(cx, |s, cx| {
                            s.relay_view_tab = kind;
                            cx.notify();
                        });
                    })
                    .child(
                        Icon::new(icon)
                            .with_size(gpui_kit::component::Size::Size(px(14.)))
                            .text_color(if selected { cx.theme().primary } else { fg }),
                    )
                    .child(
                        div()
                            .pt(px(0.5))
                            .text_size(px(12.5))
                            .line_height(px(16.))
                            .when(selected, |d| d.font_semibold())
                            .text_color(fg)
                            .child(label),
                    )
                    .child(
                        div()
                            .px_1p5()
                            .h(px(18.))
                            .rounded_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .bg(cx.theme().secondary)
                            .text_size(px(10.5))
                            .line_height(px(13.))
                            .text_color(cx.theme().muted_foreground)
                            .child(cnt.to_string()),
                    )
                    .into_any_element()
            })
            .collect();

        // 绑定账号映射表：relay_id -> email
        let mut bound_emails: HashMap<String, String> = HashMap::new();
        if let Some(snap) = &data.official_snapshot {
            for acc in &snap.accounts {
                if let Some(rid) = acc.bound_relay_profile_id.as_deref()
                    && !rid.trim().is_empty()
                {
                    bound_emails
                        .entry(rid.to_string())
                        .or_insert_with(|| acc.email.clone());
                }
            }
        }

        let visible_len = visible_profiles.len();
        let list_body: AnyElement = if visible_profiles.is_empty() {
            let (empty_title, empty_desc, is_agg, is_model_agg) = match current_tab {
                ProviderViewKind::Direct => (
                    t("这个分页还没有直连供应商"),
                    t("直连分页只显示 Responses 直连且未配置模型路由的供应商；其他供应商在其余分页，配置不会丢失。"),
                    false,
                    false,
                ),
                ProviderViewKind::Routing => (
                    t("这个分页还没有需协议转换的供应商"),
                    t("Chat Completions 协议或配置了模型路由的供应商会显示在这里；其他供应商在其余分页。"),
                    false,
                    false,
                ),
                ProviderViewKind::Failover => (
                    t("还没有故障转移配置"),
                    t("在编辑页勾选成员供应商，并选择失败切换、轮转或权重策略。"),
                    true,
                    false,
                ),
                ProviderViewKind::ModelAggregation => (
                    t("还没有模型聚合配置"),
                    t("把成员模型合并到 Codex 列表，选中模型会固定发送到对应供应商。"),
                    true,
                    true,
                ),
            };
            let store_cta = self.store.clone();
            let cta_label = if !is_agg {
                t("添加供应商")
            } else if is_model_agg {
                t("创建模型聚合配置")
            } else {
                t("创建故障转移配置")
            };

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
                                .child(empty_title),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(empty_desc),
                        ),
                )
                .child(
                    AppButton::new("relay-empty-cta")
                        .primary()
                        .small()
                        .icon(IconName::Plus)
                        .label(cta_label)
                        .on_click(move |_, _, cx| {
                            store_cta.update(cx, |s, cx| {
                                if is_agg {
                                    s.open_new_aggregate_relay(is_model_agg, cx);
                                } else {
                                    s.open_relay_catalog(cx);
                                }
                            });
                        }),
                )
                .into_any_element()
        } else {
            let cards: Vec<AnyElement> = visible_profiles
                .iter()
                .enumerate()
                .map(|(idx, p)| {
                    let is_active = !pure_official_active && p.id == active_relay_id;
                    let agg = agg_map.get(&p.id);
                    let is_agg = p.relay_mode == RelayMode::Aggregate;
                    let can_multi = multi_launch_supported(p);
                    let multi_busy =
                        data.multi_launch_pending.as_deref() == Some(p.id.as_str());
                    let test_busy =
                        data.testing_profile_id.as_deref() == Some(p.id.as_str());
                    let bound_email = bound_emails.get(&p.id).cloned();

                    let title_text = if p.name.trim().is_empty() {
                        t("未命名供应商")
                    } else {
                        p.name.clone()
                    };
                    let model_str = relay_profile_model(p);
                    let mut sub_parts = vec![
                        relay_profile_mode_label(p, agg),
                        relay_protocol_label(p.protocol),
                    ];
                    if !is_agg && !model_str.trim().is_empty() {
                        sub_parts.push(model_str);
                    }
                    sub_parts.push(relay_profile_config_brief(p, agg));
                    let subtitle = sub_parts.join(" · ");

                    let pid_use = p.id.clone();
                    let store_use = self.store.clone();
                    let pid_up = p.id.clone();
                    let store_up = self.store.clone();
                    let pid_down = p.id.clone();
                    let store_down = self.store.clone();
                    let pid_multi = p.id.clone();
                    let store_multi = self.store.clone();
                    let p_test = p.clone();
                    let store_test = self.store.clone();
                    let pid_edit = p.id.clone();
                    let store_edit = self.store.clone();
                    let pid_dup = p.id.clone();
                    let store_dup = self.store.clone();
                    let pid_del = p.id.clone();
                    let store_del = self.store.clone();

                    div()
                        .h_flex()
                        .w_full()
                        .items_center()
                        .justify_between()
                        .gap_3()
                        .p_4()
                        .rounded(px(11.))
                        .bg(cx.theme().secondary.opacity(if is_active { 0.42 } else { 0.28 }))
                        .border_1()
                        .border_color(if is_active {
                            cx.theme().primary.opacity(0.65)
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
                                .child(avatar_badge(cx, provider_initial(&title_text)))
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
                                                        .child(title_text),
                                                )
                                                .when(is_active, |d| {
                                                    d.child(AppBadge::good(t("使用中")))
                                                })
                                                .when_some(bound_email, |d, em| {
                                                    d.child(AppBadge::info(format!(
                                                        "{}: {em}",
                                                        t("绑定")
                                                    )))
                                                }),
                                        )
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(cx.theme().muted_foreground)
                                                .whitespace_nowrap()
                                                .overflow_hidden()
                                                .child(subtitle),
                                        ),
                                ),
                        )
                        .child(
                            div()
                                .h_flex()
                                .items_center()
                                .gap_1()
                                .child(
                                    if is_active {
                                        AppButton::new(SharedString::from(format!(
                                            "relay-use-{}",
                                            p.id
                                        )))
                                        .secondary()
                                        .small()
                                        .selected(true)
                                        .disabled(relay_switching)
                                        .icon(IconName::CircleCheck)
                                        .label(t("使用中"))
                                    } else {
                                        AppButton::new(SharedString::from(format!(
                                            "relay-use-{}",
                                            p.id
                                        )))
                                        .outline()
                                        .small()
                                        .disabled(relay_switching)
                                        .icon(IconName::CircleCheck)
                                        .label(t("使用"))
                                    }
                                    .on_click(move |_, _, cx| {
                                        let id = pid_use.clone();
                                        store_use.update(cx, |s, cx| {
                                            s.use_confirm_relay_id = Some(id);
                                            cx.notify();
                                        });
                                    }),
                                )
                                .child(
                                    AppButton::new(SharedString::from(format!(
                                        "relay-up-{}",
                                        p.id
                                    )))
                                    .ghost()
                                    .small_icon()
                                    .disabled(idx == 0)
                                    .icon(IconName::ArrowUp)
                                    .tooltip(t("上移"))
                                    .on_click(move |_, _, cx| {
                                        let id = pid_up.clone();
                                        store_up.update(cx, |s, cx| {
                                            s.move_relay_profile(id, true, cx)
                                        });
                                    }),
                                )
                                .child(
                                    AppButton::new(SharedString::from(format!(
                                        "relay-down-{}",
                                        p.id
                                    )))
                                    .ghost()
                                    .small_icon()
                                    .disabled(idx + 1 >= visible_len)
                                    .icon(IconName::ArrowDown)
                                    .tooltip(t("下移"))
                                    .on_click(move |_, _, cx| {
                                        let id = pid_down.clone();
                                        store_down.update(cx, |s, cx| {
                                            s.move_relay_profile(id, false, cx)
                                        });
                                    }),
                                )
                                .child(
                                    AppButton::new(SharedString::from(format!(
                                        "relay-multi-{}",
                                        p.id
                                    )))
                                    .ghost()
                                    .small_icon()
                                    .disabled(!can_multi || multi_busy)
                                    .icon(IconName::Rocket)
                                    .tooltip(t("启动独立 Codex 实例"))
                                    .on_click(move |_, _, cx| {
                                        let id = pid_multi.clone();
                                        store_multi
                                            .update(cx, |s, cx| s.launch_multi_profile(id, cx));
                                    }),
                                )
                                .child(
                                    AppButton::new(SharedString::from(format!(
                                        "relay-test-{}",
                                        p.id
                                    )))
                                    .ghost()
                                    .small_icon()
                                    .disabled(is_agg || test_busy)
                                    .icon(IconName::FlaskConical)
                                    .tooltip(t("发送 hi 测试"))
                                    .on_click(move |_, _, cx| {
                                        let prof = p_test.clone();
                                        store_test
                                            .update(cx, |s, cx| s.test_relay_profile(prof, cx));
                                    }),
                                )
                                .child(
                                    AppButton::new(SharedString::from(format!(
                                        "relay-edit-{}",
                                        p.id
                                    )))
                                    .ghost()
                                    .small_icon()
                                    .icon(IconName::SquarePen)
                                    .tooltip(t("编辑"))
                                    .on_click(move |_, _, cx| {
                                        let id = pid_edit.clone();
                                        store_edit.update(cx, |s, cx| s.open_edit_relay(id, cx));
                                    }),
                                )
                                .child(
                                    AppButton::new(SharedString::from(format!(
                                        "relay-dup-{}",
                                        p.id
                                    )))
                                    .ghost()
                                    .small_icon()
                                    .icon(IconName::Copy)
                                    .tooltip(t("复制"))
                                    .on_click(move |_, _, cx| {
                                        let id = pid_dup.clone();
                                        store_dup.update(cx, |s, cx| s.duplicate_relay(id, cx));
                                    }),
                                )
                                .child(
                                    AppButton::new(SharedString::from(format!(
                                        "relay-del-{}",
                                        p.id
                                    )))
                                    .ghost()
                                    .small_icon()
                                    .disabled(total_profiles <= 1)
                                    .icon(IconName::Trash)
                                    .tooltip(t("删除供应商"))
                                    .on_click(move |_, _, cx| {
                                        let id = pid_del.clone();
                                        store_del.update(cx, |s, cx| {
                                            s.delete_confirm_relay_id = Some(id);
                                            cx.notify();
                                        });
                                    }),
                                ),
                        )
                        .into_any_element()
                })
                .collect();

            let agg_toolbar: Option<AnyElement> = matches!(
                current_tab,
                ProviderViewKind::Failover | ProviderViewKind::ModelAggregation
            )
            .then(|| {
                let is_model = current_tab == ProviderViewKind::ModelAggregation;
                let store_new_agg = self.store.clone();
                div()
                    .h_flex()
                    .w_full()
                    .justify_end()
                    .child(
                        AppButton::new("relay-new-agg-toolbar")
                            .secondary()
                            .small()
                            .icon(IconName::Plus)
                            .label(if is_model {
                                t("新建模型聚合")
                            } else {
                                t("新建故障转移")
                            })
                            .on_click(move |_, _, cx| {
                                store_new_agg.update(cx, |s, cx| {
                                    s.open_new_aggregate_relay(is_model, cx)
                                });
                            }),
                    )
                    .into_any_element()
            });

            div()
                .v_flex()
                .w_full()
                .gap_3()
                .when_some(agg_toolbar, |d, bar| d.child(bar))
                .children(cards)
                .into_any_element()
        };

        let env_banner = self.render_env_conflict_banner(cx);
        let url_banner = self.render_pending_url_import_banner(cx);
        let ccs_drawer = self.render_ccs_import_drawer(cx);
        let modals = self.render_list_modals(cx);

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
                    .gap_2()
                    .child(
                        AppButton::new("relay-top-ccs-import")
                            .secondary()
                            .small()
                            .icon(IconName::Download)
                            .label(t("从第三方导入"))
                            .on_click(move |_, _, cx| {
                                store_ccs_btn.update(cx, |s, cx| s.toggle_ccs_import(cx));
                            }),
                    )
                    .child(
                        AppButton::new("relay-top-add-provider")
                            .primary()
                            .small()
                            .icon(IconName::Plus)
                            .label(t("添加供应商"))
                            .on_click(move |_, _, cx| {
                                store_add_btn.update(cx, |s, cx| s.open_relay_catalog(cx));
                            }),
                    ),
            )
            .when_some(url_banner, |d, b| d.child(b))
            .when_some(env_banner, |d, b| d.child(b))
            .when_some(modals, |d, b| d.child(b))
            .when_some(ccs_drawer, |d, b| d.child(b))
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
                            .gap_4()
                            .p_3p5()
                            .rounded(px(10.))
                            .bg(cx.theme().secondary.opacity(0.32))
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
                                            .child(t("启用供应商配置切换")),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(t("关闭后本工具不会在手动切换时写入 Codex 的 config.toml / auth.json；启动 Codex 时始终不会自动改这些文件。")),
                                    ),
                            )
                            .child(
                                Switch::new("relay-master-switch")
                                    .checked(master_enabled)
                                    .on_change(move |checked, _, cx| {
                                        let next = *checked;
                                        store_switch_master.update(cx, |s, cx| {
                                            s.set_relay_master_enabled(next, cx);
                                        });
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .h_flex()
                            .w_full()
                            .p_1()
                            .gap_1()
                            .rounded(px(10.))
                            .bg(cx.theme().secondary.opacity(0.45))
                            .border_1()
                            .border_color(cx.theme().border.opacity(0.6))
                            .children(tab_buttons),
                    )
                    .child(list_body),
            ))
            .into_any_element()
    }

    // ========================================================================
    // 2. 预设供应商目录子视图 (Catalog)
    // ========================================================================

    fn render_catalog_subscreen(&self, cx: &mut Context<Self>) -> AnyElement {
        let query = self
            .store
            .read(cx)
            .relay_catalog_query
            .trim()
            .to_lowercase();
        let store_back = self.store.clone();
        let store_custom = self.store.clone();
        let store_failover = self.store.clone();
        let store_model_agg = self.store.clone();

        let categories = [
            PresetCategory::CnOfficial,
            PresetCategory::Aggregator,
            PresetCategory::ThirdParty,
        ];

        let mut category_blocks: Vec<AnyElement> = Vec::new();
        for cat in categories {
            let matched: Vec<(usize, &chatgptx_backend::phase3_rules::ProviderPreset)> =
                PROVIDER_PRESETS
                    .iter()
                    .enumerate()
                    .filter(|(_, p)| p.category == cat)
                    .filter(|(_, p)| {
                        query.is_empty()
                            || p.name.to_lowercase().contains(&query)
                            || p.id.to_lowercase().contains(&query)
                            || p.model.to_lowercase().contains(&query)
                            || p.base_url.to_lowercase().contains(&query)
                    })
                    .collect();
            if matched.is_empty() {
                continue;
            }

            let mut rows: Vec<AnyElement> = Vec::new();
            for pair in matched.chunks(2) {
                let mut row_cards: Vec<AnyElement> = Vec::new();
                for (preset_idx, preset) in pair.iter().copied() {
                    let store_pick = self.store.clone();
                    let store_web = self.store.clone();
                    let url = preset.website_url.to_string();
                    let proto_badge = match preset.protocol {
                        RelayProtocol::Responses => AppBadge::good("Responses"),
                        RelayProtocol::ChatCompletions => AppBadge::info("Chat Completions"),
                    };

                    row_cards.push(
                        div()
                            .h_flex()
                            .flex_1()
                            .min_w_0()
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
                                    .child(avatar_badge(cx, provider_initial(preset.name)))
                                    .child(
                                        div()
                                            .v_flex()
                                            .gap_1()
                                            .flex_1()
                                            .min_w_0()
                                            .overflow_hidden()
                                            .child(
                                                div()
                                                    .h_flex()
                                                    .flex_wrap()
                                                    .items_center()
                                                    .gap_1p5()
                                                    .child(
                                                        div()
                                                            .text_sm()
                                                            .font_semibold()
                                                            .text_color(cx.theme().foreground)
                                                            .child(preset.name),
                                                    )
                                                    .child(proto_badge),
                                            )
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(cx.theme().muted_foreground)
                                                    .whitespace_nowrap()
                                                    .overflow_hidden()
                                                    .child(format!(
                                                        "{} · {}",
                                                        preset.model, preset.base_url
                                                    )),
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
                                        AppButton::new(SharedString::from(format!(
                                            "preset-web-{}",
                                            preset.id
                                        )))
                                        .ghost()
                                        .small_icon()
                                        .icon(IconName::Globe)
                                        .tooltip(preset.website_url)
                                        .on_click(move |_, _, cx| {
                                            let u = url.clone();
                                            store_web
                                                .update(cx, |s, cx| s.open_external_url(u, cx));
                                        }),
                                    )
                                    .child(
                                        AppButton::new(SharedString::from(format!(
                                            "preset-select-{}",
                                            preset.id
                                        )))
                                        .secondary()
                                        .small()
                                        .icon(IconName::Plus)
                                        .label(t("使用预设"))
                                        .on_click(move |_, _, cx| {
                                            store_pick.update(cx, |s, cx| {
                                                s.open_new_preset_relay(preset_idx, cx)
                                            });
                                        }),
                                    ),
                            )
                            .into_any_element(),
                    );
                }
                if row_cards.len() == 1 {
                    row_cards.push(div().flex_1().into_any_element());
                }
                rows.push(div().h_flex().w_full().gap_3().children(row_cards).into_any_element());
            }

            category_blocks.push(
                div()
                    .v_flex()
                    .w_full()
                    .gap_2p5()
                    .child(
                        div()
                            .text_xs()
                            .font_semibold()
                            .text_color(cx.theme().muted_foreground)
                            .child(cat.label()),
                    )
                    .children(rows)
                    .into_any_element(),
            );
        }

        div()
            .v_flex()
            .w_full()
            .gap_4()
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
                                    .h_flex()
                                    .items_center()
                                    .gap_2p5()
                                    .child(
                                        AppButton::new("catalog-back-btn")
                                            .ghost()
                                            .small_icon()
                                            .icon(IconName::ArrowLeft)
                                            .tooltip(t("返回列表"))
                                            .on_click(move |_, _, cx| {
                                                store_back
                                                    .update(cx, |s, cx| s.back_to_relay_list(cx));
                                            }),
                                    )
                                    .child(
                                        div()
                                            .text_base()
                                            .font_semibold()
                                            .text_color(cx.theme().foreground)
                                            .child(t("添加供应商（内置预设与自定义）")),
                                    ),
                            )
                            .child(
                                div().w(px(340.)).child(
                                    Input::new(&self.catalog_search_input).prefix(
                                        Icon::new(IconName::Search)
                                            .with_size(gpui_kit::component::Size::Size(px(14.)))
                                            .text_color(cx.theme().muted_foreground),
                                    ),
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
                                    .min_w_0()
                                    .justify_between()
                                    .gap_2p5()
                                    .p_3p5()
                                    .rounded(px(10.))
                                    .bg(cx.theme().primary.opacity(0.08))
                                    .border_1()
                                    .border_color(cx.theme().primary.opacity(0.35))
                                    .child(
                                        div()
                                            .v_flex()
                                            .gap_1()
                                            .child(
                                                div()
                                                    .text_sm()
                                                    .font_semibold()
                                                    .text_color(cx.theme().foreground)
                                                    .child(t("空白自定义供应商")),
                                            )
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(cx.theme().muted_foreground)
                                                    .child(t("自由填写任意兼容 Responses 或 Chat Completions 的 Base URL 与密钥")),
                                            ),
                                    )
                                    .child(
                                        div().h_flex().child(
                                            AppButton::new("catalog-create-custom")
                                                .primary()
                                                .small()
                                                .icon(IconName::Plus)
                                                .label(t("创建自定义"))
                                                .on_click(move |_, _, cx| {
                                                    store_custom.update(cx, |s, cx| {
                                                        s.open_new_custom_relay(cx)
                                                    });
                                                }),
                                        ),
                                    ),
                            )
                            .child(
                                div()
                                    .v_flex()
                                    .flex_1()
                                    .min_w_0()
                                    .justify_between()
                                    .gap_2p5()
                                    .p_3p5()
                                    .rounded(px(10.))
                                    .bg(cx.theme().secondary.opacity(0.32))
                                    .border_1()
                                    .border_color(cx.theme().border)
                                    .child(
                                        div()
                                            .v_flex()
                                            .gap_1()
                                            .child(
                                                div()
                                                    .text_sm()
                                                    .font_semibold()
                                                    .text_color(cx.theme().foreground)
                                                    .child(t("新建故障转移池")),
                                            )
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(cx.theme().muted_foreground)
                                                    .child(t("组合多个供应商，支持顺序容灾、对话轮询、请求轮询与权重分配")),
                                            ),
                                    )
                                    .child(
                                        div().h_flex().child(
                                            AppButton::new("catalog-create-failover")
                                                .secondary()
                                                .small()
                                                .icon(IconName::ArrowLeftRight)
                                                .label(t("新建故障转移"))
                                                .on_click(move |_, _, cx| {
                                                    store_failover.update(cx, |s, cx| {
                                                        s.open_new_aggregate_relay(false, cx)
                                                    });
                                                }),
                                        ),
                                    ),
                            )
                            .child(
                                div()
                                    .v_flex()
                                    .flex_1()
                                    .min_w_0()
                                    .justify_between()
                                    .gap_2p5()
                                    .p_3p5()
                                    .rounded(px(10.))
                                    .bg(cx.theme().secondary.opacity(0.32))
                                    .border_1()
                                    .border_color(cx.theme().border)
                                    .child(
                                        div()
                                            .v_flex()
                                            .gap_1()
                                            .child(
                                                div()
                                                    .text_sm()
                                                    .font_semibold()
                                                    .text_color(cx.theme().foreground)
                                                    .child(t("新建模型聚合")),
                                            )
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(cx.theme().muted_foreground)
                                                    .child(t("合并多个成员的模型目录到单一列表，按前缀精准分发给对应供应商")),
                                            ),
                                    )
                                    .child(
                                        div().h_flex().child(
                                            AppButton::new("catalog-create-model-agg")
                                                .secondary()
                                                .small()
                                                .icon(IconName::Layers)
                                                .label(t("新建模型聚合"))
                                                .on_click(move |_, _, cx| {
                                                    store_model_agg.update(cx, |s, cx| {
                                                        s.open_new_aggregate_relay(true, cx)
                                                    });
                                                }),
                                        ),
                                    ),
                            ),
                    )
                    .children(category_blocks),
            ))
            .into_any_element()
    }

    // ========================================================================
    // 3. 供应商详情编辑器子视图 (Editor)
    // ========================================================================

    fn render_doctor_panel(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let data = self.store.read(cx);
        if !data.doctor_running && data.doctor_result.is_none() {
            return None;
        }
        let running = data.doctor_running;
        let res_opt = data.doctor_result.clone();
        let store_close = self.store.clone();

        let body: AnyElement = if running {
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(t("正在诊断供应商，请稍候。"))
                .into_any_element()
        } else if let Some(doc) = res_opt {
            let check_rows: Vec<AnyElement> = doc
                .checks
                .into_iter()
                .map(|c| {
                    let (ic, col) = match c.status.as_str() {
                        "ok" => (IconName::CircleCheck, cx.theme().success),
                        "failed" => (IconName::CircleX, cx.theme().danger),
                        _ => (IconName::TriangleAlert, cx.theme().warning),
                    };
                    div()
                        .h_flex()
                        .w_full()
                        .items_start()
                        .gap_2p5()
                        .p_2p5()
                        .rounded(px(8.))
                        .bg(cx.theme().background)
                        .border_1()
                        .border_color(cx.theme().border)
                        .child(div().pt_0p5().child(Icon::new(ic).small().text_color(col)))
                        .child(
                            div()
                                .v_flex()
                                .flex_1()
                                .gap_0p5()
                                .child(
                                    div()
                                        .text_xs()
                                        .font_semibold()
                                        .text_color(cx.theme().foreground)
                                        .child(c.title),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(c.detail),
                                ),
                        )
                        .into_any_element()
                })
                .collect();

            div()
                .v_flex()
                .w_full()
                .gap_2p5()
                .child(
                    div()
                        .text_xs()
                        .font_medium()
                        .text_color(cx.theme().foreground)
                        .child(doc.summary),
                )
                .when(!doc.recommendation.trim().is_empty(), |d| {
                    d.child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().info)
                            .child(doc.recommendation),
                    )
                })
                .children(check_rows)
                .into_any_element()
        } else {
            div().into_any_element()
        };

        Some(
            div()
                .v_flex()
                .w_full()
                .gap_3()
                .p_4()
                .rounded(px(11.))
                .bg(cx.theme().secondary.opacity(0.32))
                .border_1()
                .border_color(cx.theme().primary.opacity(0.45))
                .child(
                    div()
                        .h_flex()
                        .w_full()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .h_flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    Icon::new(IconName::Stethoscope)
                                        .small()
                                        .text_color(cx.theme().primary),
                                )
                                .child(
                                    div()
                                        .text_sm()
                                        .font_semibold()
                                        .text_color(cx.theme().foreground)
                                        .child(t("供应商连通性与协议诊断")),
                                ),
                        )
                        .child(
                            AppButton::new("doctor-close-btn")
                                .ghost()
                                .small_icon()
                                .icon(IconName::X)
                                .on_click(move |_, _, cx| {
                                    store_close.update(cx, |s, cx| {
                                        s.doctor_result = None;
                                        cx.notify();
                                    });
                                }),
                        ),
                )
                .child(body)
                .into_any_element(),
        )
    }

    fn render_dropdown_menu(
        cx: &App,
        items: Vec<(String, String, bool, Box<dyn Fn(&mut Window, &mut App)>)>,
    ) -> AnyElement {
        let rows: Vec<AnyElement> = items
            .into_iter()
            .enumerate()
            .map(|(idx, (id_key, label, selected, handler))| {
                div()
                    .id(SharedString::from(format!("ed-opt-{idx}-{id_key}")))
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
                    .on_click(move |_, window, cx| handler(window, cx))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().foreground)
                            .child(label),
                    )
                    .when(selected, |d| {
                        d.child(
                            Icon::new(IconName::Check)
                                .with_size(gpui_kit::component::Size::Size(px(13.)))
                                .text_color(cx.theme().primary),
                        )
                    })
                    .into_any_element()
            })
            .collect();

        div()
            .v_flex()
            .w_full()
            .gap_0p5()
            .p_1p5()
            .mt_1()
            .rounded(px(8.))
            .bg(cx.theme().popover)
            .border_1()
            .border_color(cx.theme().border)
            .children(rows)
            .into_any_element()
    }

    fn render_aggregate_editor_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let agg = self
            .draft_aggregate
            .clone()
            .unwrap_or_else(|| AggregateRelayProfile {
                id: self.draft_profile.id.clone(),
                name: String::new(),
                session_provider: RelaySessionProvider::Custom,
                model_aggregation_enabled: false,
                strategy: AggregateRelayStrategy::Failover,
                members: Vec::new(),
                routes: Vec::new(),
                default_relay_id: String::new(),
                sync_default_member_login: false,
            });
        let model_agg = agg.model_aggregation_enabled;
        let strategy = agg.strategy;
        let candidates: Vec<RelayProfile> = self
            .store
            .read(cx)
            .settings
            .relay_profiles
            .iter()
            .filter(|p| p.relay_mode != RelayMode::Aggregate && p.id != self.draft_profile.id)
            .cloned()
            .collect();

        let this_mode_failover = cx.entity();
        let this_mode_model = cx.entity();
        let this_strat_toggle = cx.entity();
        let strat_open = self.open_dropdown == Some(EditorOpenDropdown::Strategy);

        let strat_menu: Option<AnyElement> = strat_open.then(|| {
            let strategies = [
                AggregateRelayStrategy::Failover,
                AggregateRelayStrategy::ConversationRoundRobin,
                AggregateRelayStrategy::RequestRoundRobin,
                AggregateRelayStrategy::WeightedRoundRobin,
            ];
            let items = strategies
                .into_iter()
                .map(|st| {
                    let entity = cx.entity();
                    let label = aggregate_strategy_label(st);
                    let selected = st == strategy;
                    let cb: Box<dyn Fn(&mut Window, &mut App)> = Box::new(move |_, cx| {
                        entity.update(cx, |view, cx| {
                            if let Some(a) = &mut view.draft_aggregate {
                                a.strategy = st;
                            }
                            view.open_dropdown = None;
                            cx.notify();
                        });
                    });
                    (format!("{st:?}"), label, selected, cb)
                })
                .collect();
            Self::render_dropdown_menu(cx, items)
        });

        let member_rows: Vec<AnyElement> = candidates
            .into_iter()
            .map(|cand| {
                let member_opt = agg.members.iter().find(|m| m.relay_id == cand.id).cloned();
                let checked = member_opt.is_some();
                let weight = member_opt.as_ref().map(|m| m.weight).unwrap_or(1);
                let is_default = agg.default_relay_id == cand.id;

                let cid_toggle = cand.id.clone();
                let entity_toggle = cx.entity();
                let cid_minus = cand.id.clone();
                let entity_minus = cx.entity();
                let cid_plus = cand.id.clone();
                let entity_plus = cx.entity();
                let cid_def = cand.id.clone();
                let entity_def = cx.entity();

                div()
                    .h_flex()
                    .w_full()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .p_3()
                    .rounded(px(9.))
                    .bg(cx.theme().secondary.opacity(if checked { 0.42 } else { 0.22 }))
                    .border_1()
                    .border_color(if checked {
                        cx.theme().primary.opacity(0.5)
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
                            .child(
                                Checkbox::new(SharedString::from(format!(
                                    "agg-member-{}",
                                    cand.id
                                )))
                                .checked(checked)
                                .on_change(move |_, _, cx| {
                                    let id = cid_toggle.clone();
                                    entity_toggle.update(cx, |view, cx| {
                                        if let Some(a) = &mut view.draft_aggregate {
                                            toggle_aggregate_member(a, &id);
                                        }
                                        cx.notify();
                                    });
                                }),
                            )
                            .child(
                                div()
                                    .v_flex()
                                    .gap_0p5()
                                    .flex_1()
                                    .min_w_0()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_medium()
                                            .text_color(cx.theme().foreground)
                                            .child(if cand.name.trim().is_empty() {
                                                t("未命名供应商")
                                            } else {
                                                cand.name.clone()
                                            }),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(format!(
                                                "{} · {}",
                                                relay_protocol_label(cand.protocol),
                                                relay_profile_model(&cand)
                                            )),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .gap_2()
                            .when(
                                checked
                                    && !model_agg
                                    && strategy == AggregateRelayStrategy::WeightedRoundRobin,
                                |d| {
                                    d.child(
                                        AppButton::new(SharedString::from(format!(
                                            "agg-w-minus-{}",
                                            cand.id
                                        )))
                                        .outline()
                                        .small_icon()
                                        .label("-")
                                        .on_click(move |_, _, cx| {
                                            let id = cid_minus.clone();
                                            entity_minus.update(cx, |view, cx| {
                                                if let Some(a) = &mut view.draft_aggregate
                                                    && let Some(m) = a
                                                        .members
                                                        .iter_mut()
                                                        .find(|m| m.relay_id == id)
                                                {
                                                    m.weight = m.weight.saturating_sub(1).max(1);
                                                }
                                                cx.notify();
                                            });
                                        }),
                                    )
                                    .child(AppBadge::secondary(format!(
                                        "{}: {weight}",
                                        t("权重")
                                    )))
                                    .child(
                                        AppButton::new(SharedString::from(format!(
                                            "agg-w-plus-{}",
                                            cand.id
                                        )))
                                        .outline()
                                        .small_icon()
                                        .label("+")
                                        .on_click(move |_, _, cx| {
                                            let id = cid_plus.clone();
                                            entity_plus.update(cx, |view, cx| {
                                                if let Some(a) = &mut view.draft_aggregate
                                                    && let Some(m) = a
                                                        .members
                                                        .iter_mut()
                                                        .find(|m| m.relay_id == id)
                                                {
                                                    m.weight = (m.weight + 1).min(20);
                                                }
                                                cx.notify();
                                            });
                                        }),
                                    )
                                },
                            )
                            .when(checked && model_agg, |d| {
                                d.child(
                                    AppButton::new(SharedString::from(format!(
                                        "agg-def-{}",
                                        cand.id
                                    )))
                                    .small()
                                    .selected(is_default)
                                    .label(if is_default {
                                        t("默认兜底成员")
                                    } else {
                                        t("设为默认成员")
                                    })
                                    .on_click(move |_, _, cx| {
                                        let id = cid_def.clone();
                                        entity_def.update(cx, |view, cx| {
                                            if let Some(a) = &mut view.draft_aggregate {
                                                a.default_relay_id = id;
                                            }
                                            cx.notify();
                                        });
                                    }),
                                )
                            }),
                    )
                    .into_any_element()
            })
            .collect();

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
                        .gap_4()
                        .child(
                            div()
                                .v_flex()
                                .flex_1()
                                .gap_1p5()
                                .child(
                                    div()
                                        .text_xs()
                                        .font_medium()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(t("聚合供应商名称")),
                                )
                                .child(Input::new(&self.name_input)),
                        )
                        .child(
                            div()
                                .v_flex()
                                .flex_1()
                                .gap_1p5()
                                .child(
                                    div()
                                        .text_xs()
                                        .font_medium()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(t("聚合模式")),
                                )
                                .child(
                                    div()
                                        .h_flex()
                                        .gap_2()
                                        .child(
                                            AppButton::new("agg-mode-failover")
                                                .outline()
                                                .small()
                                                .selected(!model_agg)
                                                .icon(IconName::ArrowLeftRight)
                                                .label(t("故障转移"))
                                                .on_click(move |_, _, cx| {
                                                    this_mode_failover.update(cx, |v, cx| {
                                                        if let Some(a) = &mut v.draft_aggregate {
                                                            a.model_aggregation_enabled = false;
                                                        }
                                                        cx.notify();
                                                    });
                                                }),
                                        )
                                        .child(
                                            AppButton::new("agg-mode-model")
                                                .outline()
                                                .small()
                                                .selected(model_agg)
                                                .icon(IconName::Layers)
                                                .label(t("模型聚合"))
                                                .on_click(move |_, _, cx| {
                                                    this_mode_model.update(cx, |v, cx| {
                                                        if let Some(a) = &mut v.draft_aggregate {
                                                            a.model_aggregation_enabled = true;
                                                        }
                                                        cx.notify();
                                                    });
                                                }),
                                        ),
                                ),
                        ),
                )
                .when(!model_agg, |d| {
                    d.child(
                        div()
                            .v_flex()
                            .w(px(320.))
                            .gap_1p5()
                            .child(
                                div()
                                    .text_xs()
                                    .font_medium()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(t("请求分配与容灾策略")),
                            )
                            .child(
                                AppSelectTrigger::new(
                                    "agg-strategy-trigger",
                                    aggregate_strategy_label(strategy),
                                )
                                .open(strat_open)
                                .on_click(move |_, _, cx| {
                                    this_strat_toggle.update(cx, |v, cx| {
                                        v.open_dropdown = if v.open_dropdown
                                            == Some(EditorOpenDropdown::Strategy)
                                        {
                                            None
                                        } else {
                                            Some(EditorOpenDropdown::Strategy)
                                        };
                                        cx.notify();
                                    });
                                }),
                            )
                            .when_some(strat_menu, |col, menu| col.child(menu)),
                    )
                })
                .child(
                    div()
                        .v_flex()
                        .w_full()
                        .gap_2p5()
                        .pt_2()
                        .border_t_1()
                        .border_color(cx.theme().border)
                        .child(
                            div()
                                .text_sm()
                                .font_semibold()
                                .text_color(cx.theme().foreground)
                                .child(tf(
                                    "勾选成员供应商（已选 {0} 个）",
                                    &[&agg.members.len().to_string()],
                                )),
                        )
                        .children(member_rows),
                ),
        )
        .into_any_element()
    }

    fn render_standard_editor_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let data = self.store.read(cx);
        let fetching_models = data.fetching_models;
        let is_active =
            !self.draft_is_new && self.draft_profile.id == data.settings.active_relay_id;

        // 当前绑定的官方账号
        let official_accounts = data
            .official_snapshot
            .as_ref()
            .map(|s| s.accounts.clone())
            .unwrap_or_default();
        let current_bound_acc = official_accounts.iter().find(|a| {
            a.bound_relay_profile_id.as_deref() == Some(self.draft_profile.id.as_str())
        });
        let bound_label = current_bound_acc
            .map(|a| a.email.clone())
            .unwrap_or_else(|| t("不绑定（独立使用）"));

        let mode_label = match (self.draft_mode, self.draft_official_mix_key) {
            (RelayMode::PureApi, _) => t("纯 API Key"),
            (RelayMode::MixedApi, _) | (RelayMode::Official, true) => t("官方登录 + 第三方 API"),
            _ => t("纯官方登录"),
        };
        let proto_label = relay_protocol_label(self.draft_protocol);
        let show_api_fields =
            self.draft_mode != RelayMode::Official || self.draft_official_mix_key;

        let mode_open = self.open_dropdown == Some(EditorOpenDropdown::Mode);
        let proto_open = self.open_dropdown == Some(EditorOpenDropdown::Protocol);
        let bind_open = self.open_dropdown == Some(EditorOpenDropdown::OfficialBind);

        let ent_mode_btn = cx.entity();
        let ent_proto_btn = cx.entity();
        let ent_bind_btn = cx.entity();

        let mode_menu: Option<AnyElement> = mode_open.then(|| {
            let choices = [
                ("pure", t("纯 API Key"), RelayMode::PureApi, false),
                (
                    "mixed",
                    t("官方登录 + 第三方 API"),
                    RelayMode::MixedApi,
                    true,
                ),
                ("official", t("纯官方登录"), RelayMode::Official, false),
            ];
            let cur = (self.draft_mode, self.draft_official_mix_key);
            let items = choices
                .into_iter()
                .map(|(key, lbl, m, mix)| {
                    let ent = cx.entity();
                    let sel = (cur.0 == m)
                        || (m == RelayMode::MixedApi
                            && cur.0 == RelayMode::Official
                            && cur.1);
                    let cb: Box<dyn Fn(&mut Window, &mut App)> = Box::new(move |_, cx| {
                        ent.update(cx, |v, cx| {
                            v.draft_mode = m;
                            v.draft_official_mix_key = mix;
                            v.open_dropdown = None;
                            cx.notify();
                        });
                    });
                    (key.to_string(), lbl, sel, cb)
                })
                .collect();
            Self::render_dropdown_menu(cx, items)
        });

        let proto_menu: Option<AnyElement> = proto_open.then(|| {
            let choices = [RelayProtocol::Responses, RelayProtocol::ChatCompletions];
            let cur = self.draft_protocol;
            let items = choices
                .into_iter()
                .map(|pr| {
                    let ent = cx.entity();
                    let cb: Box<dyn Fn(&mut Window, &mut App)> = Box::new(move |_, cx| {
                        ent.update(cx, |v, cx| {
                            v.draft_protocol = pr;
                            v.open_dropdown = None;
                            cx.notify();
                        });
                    });
                    (
                        format!("{pr:?}"),
                        relay_protocol_label(pr),
                        pr == cur,
                        cb,
                    )
                })
                .collect();
            Self::render_dropdown_menu(cx, items)
        });

        let bind_menu: Option<AnyElement> = bind_open.then(|| {
            let cur_acc_id = current_bound_acc.map(|a| a.id.clone()).unwrap_or_default();
            let relay_id = self.draft_profile.id.clone();
            let mut items: Vec<(String, String, bool, Box<dyn Fn(&mut Window, &mut App)>)> =
                Vec::new();
            {
                let st = self.store.clone();
                let ent = cx.entity();
                let rid = relay_id.clone();
                items.push((
                    "none".to_string(),
                    t("不绑定（独立使用）"),
                    cur_acc_id.is_empty(),
                    Box::new(move |_, cx| {
                        ent.update(cx, |v, cx| {
                            v.open_dropdown = None;
                            cx.notify();
                        });
                        st.update(cx, |s, cx| s.bind_official_to_relay(rid.clone(), None, cx));
                    }),
                ));
            }
            for acc in &official_accounts {
                let st = self.store.clone();
                let ent = cx.entity();
                let rid = relay_id.clone();
                let aid = acc.id.clone();
                let selected = aid == cur_acc_id;
                items.push((
                    aid.clone(),
                    acc.email.clone(),
                    selected,
                    Box::new(move |_, cx| {
                        ent.update(cx, |v, cx| {
                            v.open_dropdown = None;
                            cx.notify();
                        });
                        st.update(cx, |s, cx| {
                            s.bind_official_to_relay(rid.clone(), Some(aid.clone()), cx)
                        });
                    }),
                ));
            }
            Self::render_dropdown_menu(cx, items)
        });

        // 每模型上下文窗口表格
        let configured_slug = model_slug_from_row_name(&self.model_input.read(cx).value());
        let table_rows: Vec<AnyElement> = self
            .row_controls
            .iter()
            .map(|ctrl| {
                let uid = ctrl.row_uid;
                let row_model_val = ctrl.model_input.read(cx).value().to_string();
                let row_slug = model_slug_from_row_name(&row_model_val);
                let is_default_model = !configured_slug.is_empty() && row_slug == configured_slug;

                let img_label = match ctrl.image_handling.as_str() {
                    "vlm" => t("VLM 转述"),
                    "strip" => t("剥离图片"),
                    _ => t("原样发送"),
                };
                let ent_img = cx.entity();
                let ent_set_def = cx.entity();
                let ent_del_row = cx.entity();
                let model_inp_for_def = ctrl.model_input.clone();
                let win_inp = ctrl.window_input.clone();

                let quick_presets: Vec<AnyElement> = [("128k", "128k"), ("256k", "256k"), ("1M", "1M"), ("2M", "2M")]
                    .into_iter()
                    .map(|(key, lbl)| {
                        let win_target = win_inp.clone();
                        div()
                            .id(SharedString::from(format!("win-preset-{uid}-{key}")))
                            .px_1p5()
                            .h(px(22.))
                            .rounded(px(5.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .bg(cx.theme().secondary)
                            .border_1()
                            .border_color(cx.theme().border.opacity(0.7))
                            .hover(|s| s.bg(cx.theme().accent))
                            .text_size(px(10.5))
                            .line_height(px(13.))
                            .text_color(cx.theme().muted_foreground)
                            .on_click(move |_, window, cx| {
                                win_target.update(cx, |inp, cx| inp.set_value(lbl, window, cx));
                            })
                            .child(lbl)
                            .into_any_element()
                    })
                    .collect();

                div()
                    .h_flex()
                    .w_full()
                    .items_center()
                    .gap_2()
                    .p_2p5()
                    .rounded(px(9.))
                    .bg(cx.theme().secondary.opacity(0.24))
                    .border_1()
                    .border_color(if is_default_model {
                        cx.theme().primary.opacity(0.55)
                    } else {
                        cx.theme().border
                    })
                    .child(div().flex_1().min_w(px(170.)).child(Input::new(&ctrl.model_input)))
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .gap_1()
                            .child(div().w(px(92.)).child(Input::new(&ctrl.window_input)))
                            .children(quick_presets),
                    )
                    .child(div().w(px(72.)).child(Input::new(&ctrl.compact_input)))
                    .child(
                        AppButton::new(SharedString::from(format!("row-img-{uid}")))
                            .outline()
                            .small()
                            .label(img_label)
                            .tooltip(t("点击切换图片输入策略：原样发送 / VLM 转述 / 剥离图片"))
                            .on_click(move |_, _, cx| {
                                ent_img.update(cx, |v, cx| {
                                    if let Some(c) =
                                        v.row_controls.iter_mut().find(|r| r.row_uid == uid)
                                    {
                                        c.image_handling = match c.image_handling.as_str() {
                                            "send-as-is" => "vlm".to_string(),
                                            "vlm" => "strip".to_string(),
                                            _ => "send-as-is".to_string(),
                                        };
                                    }
                                    cx.notify();
                                });
                            }),
                    )
                    .child(
                        AppButton::new(SharedString::from(format!("row-def-{uid}")))
                            .ghost()
                            .small()
                            .selected(is_default_model)
                            .label(if is_default_model {
                                t("默认")
                            } else {
                                t("设默认")
                            })
                            .on_click(move |_, window, cx| {
                                let raw = model_inp_for_def.read(cx).value().to_string();
                                let slug = model_slug_from_row_name(&raw);
                                if !slug.is_empty() {
                                    ent_set_def.update(cx, |v, cx| {
                                        v.model_input
                                            .update(cx, |inp, cx| inp.set_value(slug, window, cx));
                                        cx.notify();
                                    });
                                }
                            }),
                    )
                    .child(
                        AppButton::new(SharedString::from(format!("row-del-{uid}")))
                            .ghost()
                            .small_icon()
                            .icon(IconName::Trash)
                            .tooltip(t("删除该模型行"))
                            .on_click(move |_, _, cx| {
                                ent_del_row.update(cx, |v, cx| {
                                    v.row_controls.retain(|r| r.row_uid != uid);
                                    cx.notify();
                                });
                            }),
                    )
                    .into_any_element()
            })
            .collect();

        let ent_fetch = cx.entity();
        let ent_add_row = cx.entity();
        let ent_sync_toml = cx.entity();
        let ent_save_live_toml = cx.entity();

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
                            .text_sm()
                            .font_semibold()
                            .text_color(cx.theme().foreground)
                            .child(t("基础连接与鉴权设置")),
                    )
                    .child(
                        div()
                            .h_flex()
                            .w_full()
                            .gap_4()
                            .child(
                                div()
                                    .v_flex()
                                    .flex_1()
                                    .gap_1p5()
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(t("供应商名称")),
                                    )
                                    .child(Input::new(&self.name_input)),
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
                                            .child(t("认证与接入模式")),
                                    )
                                    .child(
                                        AppSelectTrigger::new("ed-mode-trigger", mode_label)
                                            .open(mode_open)
                                            .on_click(move |_, _, cx| {
                                                ent_mode_btn.update(cx, |v, cx| {
                                                    v.open_dropdown = if v.open_dropdown
                                                        == Some(EditorOpenDropdown::Mode)
                                                    {
                                                        None
                                                    } else {
                                                        Some(EditorOpenDropdown::Mode)
                                                    };
                                                    cx.notify();
                                                });
                                            }),
                                    )
                                    .when_some(mode_menu, |d, m| d.child(m)),
                            ),
                    )
                    .child(
                        div()
                            .h_flex()
                            .w_full()
                            .gap_4()
                            .child(
                                div()
                                    .v_flex()
                                    .flex_1()
                                    .gap_1p5()
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(t("上游接口协议")),
                                    )
                                    .child(
                                        AppSelectTrigger::new("ed-proto-trigger", proto_label)
                                            .open(proto_open)
                                            .on_click(move |_, _, cx| {
                                                ent_proto_btn.update(cx, |v, cx| {
                                                    v.open_dropdown = if v.open_dropdown
                                                        == Some(EditorOpenDropdown::Protocol)
                                                    {
                                                        None
                                                    } else {
                                                        Some(EditorOpenDropdown::Protocol)
                                                    };
                                                    cx.notify();
                                                });
                                            }),
                                    )
                                    .when_some(proto_menu, |d, m| d.child(m)),
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
                                            .child(t("绑定官方账号（可选）")),
                                    )
                                    .child(
                                        AppSelectTrigger::new("ed-bind-trigger", bound_label)
                                            .open(bind_open)
                                            .disabled(self.draft_is_new)
                                            .on_click(move |_, _, cx| {
                                                ent_bind_btn.update(cx, |v, cx| {
                                                    v.open_dropdown = if v.open_dropdown
                                                        == Some(EditorOpenDropdown::OfficialBind)
                                                    {
                                                        None
                                                    } else {
                                                        Some(EditorOpenDropdown::OfficialBind)
                                                    };
                                                    cx.notify();
                                                });
                                            }),
                                    )
                                    .when_some(bind_menu, |d, m| d.child(m)),
                            ),
                    )
                    .when(show_api_fields, |card| {
                        card.child(
                            div()
                                .h_flex()
                                .w_full()
                                .gap_4()
                                .child(
                                    div()
                                        .v_flex()
                                        .flex_1()
                                        .gap_1p5()
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(cx.theme().muted_foreground)
                                                .child("Base URL"),
                                        )
                                        .child(Input::new(&self.base_url_input)),
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
                                                .child(t("默认模型 (model)")),
                                        )
                                        .child(Input::new(&self.model_input)),
                                ),
                        )
                        .child(
                            div()
                                .v_flex()
                                .w_full()
                                .gap_1p5()
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child("API Key"),
                                )
                                .child(Input::new(&self.api_key_input).mask_toggle()),
                        )
                    }),
            ))
            .when(show_api_fields, |root| {
                root.child(panel_card(
                    cx,
                    div()
                        .v_flex()
                        .w_full()
                        .gap_3()
                        .child(
                            div()
                                .h_flex()
                                .w_full()
                                .items_start()
                                .justify_between()
                                .gap_4()
                                .child(
                                    div()
                                        .v_flex()
                                        .flex_1()
                                        .min_w_0()
                                        .gap_0p5()
                                        .child(
                                            div()
                                                .text_sm()
                                                .font_semibold()
                                                .text_color(cx.theme().foreground)
                                                .child(t("模型列表与按模型上下文窗口 / 自动压缩阈值")),
                                        )
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(cx.theme().muted_foreground)
                                                .child(format!(
                                                    "{}（{}：{}）",
                                                    t("支持 deepseek-v4-pro[1M] 后缀语法，由 ChatGPTX 生成原生 model_catalog_json"),
                                                    t("内置档位"),
                                                    CONTEXT_WINDOW_PRESETS
                                                        .iter()
                                                        .map(|(k, _)| *k)
                                                        .collect::<Vec<_>>()
                                                        .join(" / ")
                                                )),
                                        ),
                                )
                                .child(
                                    div()
                                        .h_flex()
                                        .flex_shrink_0()
                                        .items_center()
                                        .gap_2()
                                        .child(
                                            AppButton::new("ed-fetch-models")
                                                .secondary()
                                                .small()
                                                .disabled(fetching_models)
                                                .icon(IconName::RefreshCw)
                                                .label(if fetching_models {
                                                    t("获取中…")
                                                } else {
                                                    t("从上游获取模型")
                                                })
                                                .on_click(move |_, window, cx| {
                                                    ent_fetch.update(cx, |v, cx| {
                                                        if let Ok((prof, _)) =
                                                            v.build_validated_draft(window, cx)
                                                        {
                                                            v.store.update(cx, |s, cx| {
                                                                s.fetch_upstream_models(prof, cx)
                                                            });
                                                        } else {
                                                            let mut tmp = v.draft_profile.clone();
                                                            tmp.base_url = v
                                                                .base_url_input
                                                                .read(cx)
                                                                .value()
                                                                .trim()
                                                                .to_string();
                                                            tmp.upstream_base_url =
                                                                tmp.base_url.clone();
                                                            tmp.api_key = v
                                                                .api_key_input
                                                                .read(cx)
                                                                .value()
                                                                .trim()
                                                                .to_string();
                                                            v.store.update(cx, |s, cx| {
                                                                s.fetch_upstream_models(tmp, cx)
                                                            });
                                                        }
                                                    });
                                                }),
                                        )
                                        .child(
                                            AppButton::new("ed-add-model-row")
                                                .primary()
                                                .small()
                                                .icon(IconName::Plus)
                                                .label(t("添加模型"))
                                                .on_click(move |_, window, cx| {
                                                    ent_add_row.update(cx, |v, cx| {
                                                        let blank = ModelWindowRow {
                                                            model: String::new(),
                                                            window: String::new(),
                                                            auto_compact:
                                                                DEFAULT_AUTO_COMPACT_PERCENT
                                                                    .to_string(),
                                                            image_handling: "send-as-is"
                                                                .to_string(),
                                                        };
                                                        let ctrl = v.alloc_row_control(
                                                            &blank, window, cx,
                                                        );
                                                        v.row_controls.push(ctrl);
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
                                .px_2p5()
                                .text_size(px(11.))
                                .font_semibold()
                                .text_color(cx.theme().muted_foreground)
                                .child(div().flex_1().child(t("模型 ID (支持 [1M] 后缀)")))
                                .child(div().w(px(236.)).child(t("上下文窗口与快捷档位")))
                                .child(div().w(px(76.)).child(t("自动压缩 %")))
                                .child(div().w(px(195.)).child(t("图片策略与操作"))),
                        )
                        .children(table_rows),
                ))
            })
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
                            .gap_4()
                            .child(
                                div()
                                    .v_flex()
                                    .flex_1()
                                    .min_w_0()
                                    .gap_0p5()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_semibold()
                                            .text_color(cx.theme().foreground)
                                            .child(t("config.toml 预览与高级自定义")),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(t("保存供应商时会自动合并上方字段及模型窗口指针")),
                                    ),
                            )
                            .child(
                                div()
                                    .h_flex()
                                    .flex_shrink_0()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        AppButton::new("ed-sync-toml-preview")
                                            .secondary()
                                            .small()
                                            .icon(IconName::RefreshCw)
                                            .label(t("根据表单生成预览"))
                                            .on_click(move |_, window, cx| {
                                                ent_sync_toml.update(cx, |v, cx| {
                                                    if let Err(err) =
                                                        v.build_validated_draft(window, cx)
                                                    {
                                                        v.validation_error = Some(err);
                                                    } else {
                                                        v.validation_error = None;
                                                    }
                                                    cx.notify();
                                                });
                                            }),
                                    )
                                    .when(is_active, |d| {
                                        d.child(
                                            AppButton::new("ed-write-live-toml")
                                                .outline()
                                                .small()
                                                .icon(IconName::Save)
                                                .label(t("直接保存至本机 config.toml"))
                                                .on_click(move |_, _, cx| {
                                                    ent_save_live_toml.update(cx, |v, cx| {
                                                        let text = v
                                                            .config_textarea
                                                            .read(cx)
                                                            .value()
                                                            .to_string();
                                                        v.store.update(cx, |s, cx| {
                                                            s.save_live_config_file(text, cx)
                                                        });
                                                    });
                                                }),
                                        )
                                    }),
                            ),
                    )
                    .child(Textarea::new(&self.config_textarea).h(px(165.))),
            ))
            .into_any_element()
    }

    fn render_editor_subscreen(&self, cx: &mut Context<Self>) -> AnyElement {
        let data = self.store.read(cx);
        let doctor_running = data.doctor_running;
        let relay_switching = data.relay_switching;
        let is_agg = self.draft_mode == RelayMode::Aggregate;
        let show_doctor = !is_agg
            && (self.draft_mode != RelayMode::Official || self.draft_official_mix_key);

        let title_val = self.name_input.read(cx).value().trim().to_string();
        let heading_title = if title_val.is_empty() {
            if is_agg {
                t("未命名聚合供应商")
            } else {
                t("未命名供应商")
            }
        } else {
            title_val
        };

        let status_subtitle = if self.draft_is_new {
            t("新建配置草稿（点击右上角保存后加入列表）")
        } else if self.draft_profile.id == data.settings.active_relay_id {
            t("当前生效供应商（保存时将自动同步 Codex 配置）")
        } else {
            t("编辑已保存供应商")
        };

        let store_back = self.store.clone();
        let ent_doctor = cx.entity();
        let ent_save_and_switch = cx.entity();
        let ent_save = cx.entity();

        let doctor_panel = self.render_doctor_panel(cx);
        let body = if is_agg {
            self.render_aggregate_editor_body(cx)
        } else {
            self.render_standard_editor_body(cx)
        };

        div()
            .v_flex()
            .w_full()
            .gap_4()
            .child(panel_card(
                cx,
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
                            .child(
                                AppButton::new("editor-back-btn")
                                    .ghost()
                                    .small_icon()
                                    .icon(IconName::ArrowLeft)
                                    .tooltip(t("返回列表"))
                                    .on_click(move |_, _, cx| {
                                        store_back.update(cx, |s, cx| s.back_to_relay_list(cx));
                                    }),
                            )
                            .child(
                                div()
                                    .v_flex()
                                    .gap_0p5()
                                    .flex_1()
                                    .min_w_0()
                                    .child(
                                        div()
                                            .text_base()
                                            .font_semibold()
                                            .text_color(cx.theme().foreground)
                                            .child(heading_title),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(status_subtitle),
                                    )
                                    .when_some(self.validation_error.clone(), |d, err| {
                                        d.child(
                                            div()
                                                .text_xs()
                                                .font_medium()
                                                .text_color(cx.theme().danger)
                                                .child(err),
                                        )
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .gap_2()
                            .when(show_doctor, |d| {
                                d.child(
                                    AppButton::new("editor-doctor-btn")
                                        .secondary()
                                        .small()
                                        .disabled(doctor_running)
                                        .icon(IconName::Stethoscope)
                                        .label(if doctor_running {
                                            t("诊断中")
                                        } else {
                                            t("诊断供应商")
                                        })
                                        .on_click(move |_, window, cx| {
                                            ent_doctor.update(cx, |v, cx| {
                                                match v.build_validated_draft(window, cx) {
                                                    Ok((prof, _)) => {
                                                        v.validation_error = None;
                                                        v.store.update(cx, |s, cx| {
                                                            s.diagnose_relay_profile(prof, cx)
                                                        });
                                                    }
                                                    Err(err) => {
                                                        v.validation_error = Some(err);
                                                        cx.notify();
                                                    }
                                                }
                                            });
                                        }),
                                )
                            })
                            .child(
                                AppButton::new("editor-save-and-use-btn")
                                    .outline()
                                    .small()
                                    .disabled(relay_switching)
                                    .icon(IconName::CircleCheck)
                                    .label(t("保存并设为当前"))
                                    .on_click(move |_, window, cx| {
                                        ent_save_and_switch.update(cx, |v, cx| {
                                            v.execute_save_draft(true, window, cx);
                                        });
                                    }),
                            )
                            .child(
                                AppButton::new("editor-save-btn")
                                    .primary()
                                    .small()
                                    .icon(IconName::Save)
                                    .label(t("保存供应商"))
                                    .on_click(move |_, window, cx| {
                                        ent_save.update(cx, |v, cx| {
                                            v.execute_save_draft(false, window, cx);
                                        });
                                    }),
                            ),
                    ),
            ))
            .when_some(doctor_panel, |d, p| d.child(p))
            .child(body)
            .into_any_element()
    }
}

impl Render for RelayView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_seed_if_needed(window, cx);
        let sub = self.store.read(cx).relay_subscreen.clone();
        match sub {
            RelaySubscreen::List => self.render_list_subscreen(cx),
            RelaySubscreen::Catalog => self.render_catalog_subscreen(cx),
            RelaySubscreen::Editor { .. } => self.render_editor_subscreen(cx),
        }
    }
}
