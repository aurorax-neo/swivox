use swivox_backend::commands;
use swivox_backend::i18n::t;
use swivox_backend::phase4_rules::{MCP_PRESETS, matches_management_query};
use swivox_core::mcp_config::{McpServerForm, McpTransport};
use gpui_kit::assets::IconName;
use gpui_kit::component::Icon;
use gpui_kit::component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::theme::ActiveTheme as _;
use gpui_kit::component::{Sizable as _, StyledExt as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::store::AppStore;
use crate::views::{avatar_badge, panel_card};
use crate::widgets::{AppBadge, AppButton};

pub struct McpView {
    store: Entity<AppStore>,
    search_input: Entity<InputState>,
    json_import_input: Entity<TextareaState>,

    applied_seed_seq: u64,
    editor_is_new: bool,
    transport: McpTransport,
    enabled: bool,
    id_input: Entity<InputState>,
    command_input: Entity<InputState>,
    args_input: Entity<InputState>,
    cwd_input: Entity<InputState>,
    url_input: Entity<InputState>,
    bearer_input: Entity<InputState>,
    toml_textarea: Entity<TextareaState>,
}

impl McpView {
    pub fn new(store: Entity<AppStore>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search_input =
            cx.new(|cx| InputState::new(window, cx).placeholder(t("搜索 MCP 标识、命令或 URL")));
        let json_import_input = cx.new(|cx| {
            TextareaState::new(window, cx).placeholder(
                "{\n  \"mcpServers\": {\n    \"fetch\": { \"command\": \"uvx\", \"args\": [\"mcp-server-fetch\"] }\n  }\n}",
            )
        });

        let id_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("context7 / fetch / my-mcp"));
        let command_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("npx / uvx / node"));
        let args_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(t("空格或换行分隔参数，例如：-y @upstash/context7-mcp"))
        });
        let cwd_input = cx.new(|cx| InputState::new(window, cx).placeholder(t("可选工作目录")));
        let url_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("https://example.com/mcp"));
        let bearer_input = cx.new(|cx| {
            InputState::new(window, cx)
                .masked(true)
                .placeholder(t("可选 Bearer Token"))
        });
        let toml_textarea = cx.new(|cx| {
            TextareaState::new(window, cx).placeholder("command = \"uvx\"\nargs = [\"mcp-server-fetch\"]")
        });

        let store_search = store.clone();
        cx.subscribe(&search_input, move |_, inp, ev: &InputEvent, cx| {
            if matches!(ev, InputEvent::Change) {
                let q = inp.read(cx).value().to_string();
                store_search.update(cx, |s, cx| {
                    s.mcp_search_query = q;
                    cx.notify();
                });
            }
        })
        .detach();

        cx.observe(&store, |_, _, cx| cx.notify()).detach();

        Self {
            store,
            search_input,
            json_import_input,
            applied_seed_seq: 0,
            editor_is_new: true,
            transport: McpTransport::Stdio,
            enabled: true,
            id_input,
            command_input,
            args_input,
            cwd_input,
            url_input,
            bearer_input,
            toml_textarea,
        }
    }

    fn sync_editor_seed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let seed_opt = self.store.read(cx).mcp_editor_seed.clone();
        if let Some((seq, id, toml_body, is_new)) = seed_opt
            && seq != self.applied_seed_seq
        {
            self.applied_seed_seq = seq;
            self.editor_is_new = is_new;
            let parsed = commands::parse_mcp_entry(toml_body.clone()).payload.form;
            self.transport = parsed.transport;
            self.enabled = parsed.enabled;

            self.id_input
                .update(cx, |inp, cx| inp.set_value(id, window, cx));
            self.command_input
                .update(cx, |inp, cx| inp.set_value(parsed.command, window, cx));
            self.args_input.update(cx, |inp, cx| {
                inp.set_value(parsed.args.join(" "), window, cx)
            });
            self.cwd_input
                .update(cx, |inp, cx| inp.set_value(parsed.cwd, window, cx));
            self.url_input
                .update(cx, |inp, cx| inp.set_value(parsed.url, window, cx));
            self.bearer_input.update(cx, |inp, cx| {
                inp.set_value(parsed.bearer_token, window, cx)
            });
            self.toml_textarea
                .update(cx, |inp, cx| inp.set_value(toml_body, window, cx));
        }
    }

    fn build_toml_from_fields(&self, cx: &App) -> Result<String, String> {
        let existing_toml = self.toml_textarea.read(cx).value().to_string();
        let mut base_form: McpServerForm = commands::parse_mcp_entry(existing_toml).payload.form;
        base_form.transport = self.transport;
        base_form.enabled = self.enabled;
        match self.transport {
            McpTransport::Stdio => {
                base_form.command = self.command_input.read(cx).value().trim().to_string();
                base_form.args = self
                    .args_input
                    .read(cx)
                    .value()
                    .split_whitespace()
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect();
                base_form.cwd = self.cwd_input.read(cx).value().trim().to_string();
                base_form.url.clear();
            }
            McpTransport::Http => {
                base_form.url = self.url_input.read(cx).value().trim().to_string();
                base_form.bearer_token = self.bearer_input.read(cx).value().trim().to_string();
                base_form.command.clear();
                base_form.args.clear();
            }
        }
        let built = commands::build_mcp_entry(base_form);
        if built.status == "ok" {
            Ok(built.payload.toml_body)
        } else {
            Err(built.message)
        }
    }

    fn render_editor_drawer(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.store.read(cx).mcp_editor_open {
            return None;
        }
        let is_stdio = self.transport == McpTransport::Stdio;
        let enabled = self.enabled;
        let is_new = self.editor_is_new;

        let ent_stdio = cx.entity();
        let ent_http = cx.entity();
        let ent_en = cx.entity();
        let ent_sync_toml = cx.entity();
        let ent_save = cx.entity();
        let store_close = self.store.clone();

        Some(
            panel_card(
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
                                    .child(if is_new {
                                        t("新建 MCP 服务器")
                                    } else {
                                        t("编辑 MCP 服务器")
                                    }),
                            )
                            .child(
                                div()
                                    .h_flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        Switch::new("mcp-ed-enabled")
                                            .checked(enabled)
                                            .label(t("启用"))
                                            .on_change(move |val, _, cx| {
                                                let v = *val;
                                                ent_en.update(cx, |this, cx| {
                                                    this.enabled = v;
                                                    cx.notify();
                                                });
                                            }),
                                    )
                                    .child(
                                        AppButton::new("mcp-ed-close")
                                            .ghost()
                                            .small_icon()
                                            .icon(IconName::X)
                                            .on_click(move |_, _, cx| {
                                                store_close.update(cx, |s, cx| {
                                                    s.mcp_editor_open = false;
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
                            .gap_4()
                            .child(
                                div()
                                    .v_flex()
                                    .flex_1()
                                    .min_w_0()
                                    .gap_1p5()
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child("MCP ID ([mcp_servers.<id>])"),
                                    )
                                    .child(Input::new(&self.id_input).disabled(!is_new)),
                            )
                            .child(
                                div()
                                    .v_flex()
                                    .flex_1()
                                    .min_w_0()
                                    .gap_1p5()
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(t("传输方式")),
                                    )
                                    .child(
                                        div()
                                            .h_flex()
                                            .gap_2()
                                            .child(
                                                AppButton::new("mcp-tr-stdio")
                                                    .outline()
                                                    .small()
                                                    .selected(is_stdio)
                                                    .label(t("Stdio (本地进程)"))
                                                    .on_click(move |_, _, cx| {
                                                        ent_stdio.update(cx, |this, cx| {
                                                            this.transport = McpTransport::Stdio;
                                                            cx.notify();
                                                        });
                                                    }),
                                            )
                                            .child(
                                                AppButton::new("mcp-tr-http")
                                                    .outline()
                                                    .small()
                                                    .selected(!is_stdio)
                                                    .label(t("HTTP / SSE (远程 URL)"))
                                                    .on_click(move |_, _, cx| {
                                                        ent_http.update(cx, |this, cx| {
                                                            this.transport = McpTransport::Http;
                                                            cx.notify();
                                                        });
                                                    }),
                                            ),
                                    ),
                            ),
                    )
                    .when(is_stdio, |d| {
                        d.child(
                            div()
                                .h_flex()
                                .w_full()
                                .gap_4()
                                .child(
                                    div()
                                        .v_flex()
                                        .w(px(220.))
                                        .gap_1p5()
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(cx.theme().muted_foreground)
                                                .child(t("启动命令 (command)")),
                                        )
                                        .child(Input::new(&self.command_input)),
                                )
                                .child(
                                    div()
                                        .v_flex()
                                        .flex_1()
                                        .min_w_0()
                                        .gap_1p5()
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(cx.theme().muted_foreground)
                                                .child(t("命令参数 (args)")),
                                        )
                                        .child(Input::new(&self.args_input)),
                                )
                                .child(
                                    div()
                                        .v_flex()
                                        .w(px(210.))
                                        .gap_1p5()
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(cx.theme().muted_foreground)
                                                .child(t("工作目录 (cwd，可选)")),
                                        )
                                        .child(Input::new(&self.cwd_input)),
                                ),
                        )
                    })
                    .when(!is_stdio, |d| {
                        d.child(
                            div()
                                .h_flex()
                                .w_full()
                                .gap_4()
                                .child(
                                    div()
                                        .v_flex()
                                        .flex_1()
                                        .min_w_0()
                                        .gap_1p5()
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(cx.theme().muted_foreground)
                                                .child("Endpoint URL"),
                                        )
                                        .child(Input::new(&self.url_input)),
                                )
                                .child(
                                    div()
                                        .v_flex()
                                        .flex_1()
                                        .min_w_0()
                                        .gap_1p5()
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(cx.theme().muted_foreground)
                                                .child("Bearer Token"),
                                        )
                                        .child(Input::new(&self.bearer_input).mask_toggle()),
                                ),
                        )
                    })
                    .child(
                        div()
                            .v_flex()
                            .w_full()
                            .gap_1p5()
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
                                            .child(t("TOML 表体（支持追加 env / http_headers 等高级字段）")),
                                    )
                                    .child(
                                        AppButton::new("mcp-ed-refresh-toml")
                                            .ghost()
                                            .small()
                                            .icon(IconName::RefreshCw)
                                            .label(t("从表单生成 TOML"))
                                            .on_click(move |_, window, cx| {
                                                ent_sync_toml.update(cx, |this, cx| {
                                                    if let Ok(toml) = this.build_toml_from_fields(cx)
                                                    {
                                                        this.toml_textarea.update(
                                                            cx,
                                                            |inp, cx| inp.set_value(toml, window, cx),
                                                        );
                                                    }
                                                });
                                            }),
                                    ),
                            )
                            .child(Textarea::new(&self.toml_textarea).h(px(110.))),
                    )
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .justify_end()
                            .gap_2()
                            .child(
                                AppButton::new("mcp-ed-save")
                                    .primary()
                                    .small()
                                    .icon(IconName::Save)
                                    .label(t("保存并同步 MCP"))
                                    .on_click(move |_, window, cx| {
                                        ent_save.update(cx, |this, cx| {
                                            let id = this.id_input.read(cx).value().to_string();
                                            let toml = match this.build_toml_from_fields(cx) {
                                                Ok(built) => {
                                                    this.toml_textarea.update(cx, |inp, cx| {
                                                        inp.set_value(built.clone(), window, cx)
                                                    });
                                                    built
                                                }
                                                Err(_) => {
                                                    this.toml_textarea.read(cx).value().to_string()
                                                }
                                            };
                                            this.store.update(cx, |s, cx| {
                                                s.upsert_mcp_entry(id, toml, cx)
                                            });
                                        });
                                    }),
                            ),
                    ),
            )
            .into_any_element(),
        )
    }

    fn render_json_import_drawer(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let data = self.store.read(cx);
        if !data.mcp_import_open {
            return None;
        }
        let preview = data.mcp_import_preview.clone();
        let json_inp_preview = self.json_import_input.clone();
        let json_inp_apply = self.json_import_input.clone();
        let store_preview = self.store.clone();
        let store_apply = self.store.clone();
        let store_close = self.store.clone();

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
                                    .child(t("从 JSON 批量导入 MCP 服务器（兼容 Claude Desktop 格式）")),
                            )
                            .child(
                                AppButton::new("mcp-import-close")
                                    .ghost()
                                    .small_icon()
                                    .icon(IconName::X)
                                    .on_click(move |_, _, cx| {
                                        store_close.update(cx, |s, cx| {
                                            s.mcp_import_open = false;
                                            cx.notify();
                                        });
                                    }),
                            ),
                    )
                    .child(Textarea::new(&self.json_import_input).h(px(130.)))
                    .when_some(preview, |d, pv| {
                        let summary_ids: Vec<String> =
                            pv.entries.iter().map(|e| e.id.clone()).collect();
                        d.child(
                            div()
                                .v_flex()
                                .gap_1()
                                .p_3()
                                .rounded(px(8.))
                                .bg(cx.theme().secondary.opacity(0.32))
                                .border_1()
                                .border_color(cx.theme().border)
                                .child(
                                    div()
                                        .text_xs()
                                        .font_semibold()
                                        .text_color(cx.theme().foreground)
                                        .child(format!(
                                            "{} ({}): {}",
                                            t("待导入条目"),
                                            summary_ids.len(),
                                            summary_ids.join(", ")
                                        )),
                                )
                                .when(!pv.warnings.is_empty(), |box_el| {
                                    box_el.child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().warning)
                                            .child(pv.warnings.join("；")),
                                    )
                                }),
                        )
                    })
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .gap_2()
                            .child(
                                AppButton::new("mcp-json-preview-btn")
                                    .secondary()
                                    .small()
                                    .label(t("解析预览"))
                                    .on_click(move |_, _, cx| {
                                        let txt = json_inp_preview.read(cx).value().to_string();
                                        store_preview.update(cx, |s, cx| s.preview_mcp_json(txt, cx));
                                    }),
                            )
                            .child(
                                AppButton::new("mcp-json-import-btn")
                                    .primary()
                                    .small()
                                    .icon(IconName::Download)
                                    .label(t("确认批量导入"))
                                    .on_click(move |_, _, cx| {
                                        let txt = json_inp_apply.read(cx).value().to_string();
                                        store_apply.update(cx, |s, cx| s.import_mcp_json(txt, cx));
                                    }),
                            ),
                    ),
            )
            .into_any_element(),
        )
    }
}

impl Render for McpView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_editor_seed(window, cx);

        let data = self.store.read(cx);
        let query = data.mcp_search_query.clone();
        let managed = data.mcp_entries.clone();
        let live_count = data.mcp_live_entries.len();

        let filtered: Vec<_> = managed
            .iter()
            .filter(|e| matches_management_query(&[&e.id, &e.title, &e.summary], &query))
            .cloned()
            .collect();

        let store_sync = self.store.clone();
        let store_json_toggle = self.store.clone();
        let store_new = self.store.clone();

        let preset_cards: Vec<AnyElement> = MCP_PRESETS
            .iter()
            .map(|preset| {
                let already_added = managed.iter().any(|e| e.id == preset.id);
                let store_add = self.store.clone();
                let store_web = self.store.clone();
                let pid = preset.id.to_string();
                let body = preset.current_platform_toml_body();
                let url = preset.homepage.to_string();

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
                                    .h_flex()
                                    .flex_wrap()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_semibold()
                                            .text_color(cx.theme().foreground)
                                            .child(preset.id),
                                    )
                                    .child(AppBadge::outline(preset.name)),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(t(preset.description)),
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
                                    "mcp-preset-web-{}",
                                    preset.id
                                )))
                                .ghost()
                                .small_icon()
                                .icon(IconName::Globe)
                                .tooltip(preset.homepage)
                                .on_click(move |_, _, cx| {
                                    let u = url.clone();
                                    store_web.update(cx, |s, cx| s.open_external_url(u, cx));
                                }),
                            )
                            .child(
                                AppButton::new(SharedString::from(format!(
                                    "mcp-preset-add-{}",
                                    preset.id
                                )))
                                .secondary()
                                .small()
                                .disabled(already_added)
                                .icon(if already_added {
                                    IconName::CircleCheck
                                } else {
                                    IconName::Plus
                                })
                                .label(if already_added {
                                    t("已添加")
                                } else {
                                    t("一键添加")
                                })
                                .on_click(move |_, _, cx| {
                                    let id = pid.clone();
                                    let toml = body.clone();
                                    store_add.update(cx, |s, cx| s.upsert_mcp_entry(id, toml, cx));
                                }),
                            ),
                    )
                    .into_any_element()
            })
            .collect();

        let entry_cards: AnyElement = if filtered.is_empty() {
            div()
                .p_5()
                .w_full()
                .rounded(px(10.))
                .bg(cx.theme().secondary.opacity(0.24))
                .border_1()
                .border_color(cx.theme().border)
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(t("当前还没有托管的 MCP 服务器。可点击右上角「新建 MCP」或从下方官方预设一键添加。"))
                .into_any_element()
        } else {
            let rows: Vec<AnyElement> = filtered
                .into_iter()
                .map(|entry| {
                    let e_toggle = entry.clone();
                    let store_toggle = self.store.clone();
                    let id_edit = entry.id.clone();
                    let toml_edit = entry.toml_body.clone();
                    let store_edit = self.store.clone();
                    let id_del = entry.id.clone();
                    let store_del = self.store.clone();
                    let initial = entry
                        .id
                        .chars()
                        .next()
                        .map(|c| c.to_uppercase().to_string())
                        .unwrap_or_else(|| "M".to_string());

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
                                        .gap_0p5()
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
                                                        .child(entry.id.clone()),
                                                )
                                                .child(if entry.enabled {
                                                    AppBadge::good(t("已启用"))
                                                } else {
                                                    AppBadge::secondary(t("已停用"))
                                                }),
                                        )
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(cx.theme().muted_foreground)
                                                .whitespace_nowrap()
                                                .overflow_hidden()
                                                .child(entry.summary.clone()),
                                        ),
                                ),
                        )
                        .child(
                            div()
                                .h_flex()
                                .flex_shrink_0()
                                .items_center()
                                .gap_2()
                                .child(
                                    Switch::new(SharedString::from(format!(
                                        "mcp-row-sw-{}",
                                        entry.id
                                    )))
                                    .checked(entry.enabled)
                                    .on_change(move |checked, _, cx| {
                                        let target = e_toggle.clone();
                                        let next = *checked;
                                        store_toggle.update(cx, |s, cx| {
                                            s.toggle_mcp_entry_enabled(target, next, cx)
                                        });
                                    }),
                                )
                                .child(
                                    AppButton::new(SharedString::from(format!(
                                        "mcp-row-edit-{}",
                                        entry.id
                                    )))
                                    .ghost()
                                    .small_icon()
                                    .icon(IconName::SquarePen)
                                    .tooltip(t("编辑"))
                                    .on_click(move |_, _, cx| {
                                        let id = id_edit.clone();
                                        let body = toml_edit.clone();
                                        store_edit.update(cx, |s, cx| {
                                            s.open_mcp_editor(id, body, false, cx)
                                        });
                                    }),
                                )
                                .child(
                                    AppButton::new(SharedString::from(format!(
                                        "mcp-row-del-{}",
                                        entry.id
                                    )))
                                    .ghost()
                                    .small_icon()
                                    .icon(IconName::Trash)
                                    .tooltip(t("删除"))
                                    .on_click(move |_, _, cx| {
                                        let id = id_del.clone();
                                        store_del.update(cx, |s, cx| s.delete_mcp_entry(id, cx));
                                    }),
                                ),
                        )
                        .into_any_element()
                })
                .collect();
            div().v_flex().w_full().gap_2p5().children(rows).into_any_element()
        };

        let editor_drawer = self.render_editor_drawer(cx);
        let json_drawer = self.render_json_import_drawer(cx);

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
                                "{}：{}",
                                t("托管 MCP"),
                                managed.len()
                            )))
                            .child(AppBadge::info(format!(
                                "{}：{live_count}",
                                t("本机 config.toml 生效数")
                            ))),
                    )
                    .child(
                        div()
                            .h_flex()
                            .flex_shrink_0()
                            .items_center()
                            .gap_2()
                            .child(
                                AppButton::new("mcp-top-sync-live")
                                    .secondary()
                                    .small()
                                    .icon(IconName::RefreshCw)
                                    .label(t("同步至 config.toml"))
                                    .on_click(move |_, _, cx| {
                                        store_sync.update(cx, |s, cx| s.sync_mcp_to_live(cx));
                                    }),
                            )
                            .child(
                                AppButton::new("mcp-top-import-json")
                                    .secondary()
                                    .small()
                                    .icon(IconName::Upload)
                                    .label(t("JSON 导入"))
                                    .on_click(move |_, _, cx| {
                                        store_json_toggle.update(cx, |s, cx| {
                                            s.mcp_import_open = !s.mcp_import_open;
                                            s.mcp_editor_open = false;
                                            cx.notify();
                                        });
                                    }),
                            )
                            .child(
                                AppButton::new("mcp-top-new")
                                    .primary()
                                    .small()
                                    .icon(IconName::Plus)
                                    .label(t("新建 MCP"))
                                    .on_click(move |_, _, cx| {
                                        store_new.update(cx, |s, cx| {
                                            s.open_mcp_editor(
                                                String::new(),
                                                "command = \"npx\"\nargs = [\"-y\", \"\"]\n"
                                                    .to_string(),
                                                true,
                                                cx,
                                            );
                                        });
                                    }),
                            ),
                    ),
            )
            .when_some(editor_drawer, |d, b| d.child(b))
            .when_some(json_drawer, |d, b| d.child(b))
            .child(panel_card(
                cx,
                div()
                    .v_flex()
                    .w_full()
                    .gap_3p5()
                    .child(
                        Input::new(&self.search_input).prefix(
                            Icon::new(IconName::Search)
                                .with_size(gpui_kit::component::Size::Size(px(14.)))
                                .text_color(cx.theme().muted_foreground),
                        ),
                    )
                    .child(entry_cards),
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
                            .child(t("常用 MCP 预设库")),
                    )
                    .children(preset_cards),
            ))
    }
}
