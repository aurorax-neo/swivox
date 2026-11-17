use swivox_backend::i18n::{t, tf};
use swivox_backend::phase5_rules::{
    DictationPreset, apply_dictation_preset, detect_dictation_preset, validate_dictation_settings,
};
use swivox_core::settings::{BackendSettings, DictationProvider};
use gpui_kit::assets::IconName;
use gpui_kit::component::input::{Input, InputState, Textarea, TextareaState};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::theme::ActiveTheme as _;
use gpui_kit::component::StyledExt as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::store::AppStore;
use crate::views::{health_item_row, panel_card};
use crate::widgets::{AppBadge, AppButton};

pub struct EnhanceView {
    store: Entity<AppStore>,
    hydrated_once: bool,

    // Stepwise inputs
    stepwise_url_input: Entity<InputState>,
    stepwise_key_input: Entity<InputState>,
    stepwise_model_input: Entity<InputState>,

    // Dictation inputs
    dict_url_input: Entity<InputState>,
    dict_key_input: Entity<InputState>,
    dict_env_input: Entity<InputState>,
    dict_model_input: Entity<InputState>,
    dict_lang_input: Entity<InputState>,

    // Image overlay inputs
    overlay_path_input: Entity<InputState>,
    overlay_opacity_input: Entity<InputState>,

    // Codex extra CLI args
    extra_args_textarea: Entity<TextareaState>,
}

impl EnhanceView {
    pub fn new(store: Entity<AppStore>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let s = store.read(cx).settings.clone();

        let stepwise_url_input = cx.new(|cx| {
            let mut st = InputState::new(window, cx).placeholder("https://api.openai.com/v1");
            st.set_value(s.codex_app_stepwise_base_url.clone(), window, cx);
            st
        });
        let stepwise_key_input = cx.new(|cx| {
            let mut st = InputState::new(window, cx)
                .masked(true)
                .placeholder(t("sk-...（留空读取环境变量）"));
            st.set_value(s.codex_app_stepwise_api_key.clone(), window, cx);
            st
        });
        let stepwise_model_input = cx.new(|cx| {
            let mut st = InputState::new(window, cx).placeholder("gpt-4o-mini / deepseek-v4-flash");
            st.set_value(s.codex_app_stepwise_model.clone(), window, cx);
            st
        });

        let dict_url_input = cx.new(|cx| {
            let mut st = InputState::new(window, cx).placeholder("https://api.groq.com/openai/v1");
            st.set_value(s.dictation.base_url.clone(), window, cx);
            st
        });
        let dict_key_input = cx.new(|cx| {
            let mut st = InputState::new(window, cx).masked(true).placeholder("gsk_... / sk-...");
            st.set_value(s.dictation.api_key.clone(), window, cx);
            st
        });
        let dict_env_input = cx.new(|cx| {
            let mut st = InputState::new(window, cx).placeholder("GROQ_API_KEY / OPENAI_API_KEY");
            st.set_value(s.dictation.api_key_env.clone(), window, cx);
            st
        });
        let dict_model_input = cx.new(|cx| {
            let mut st = InputState::new(window, cx).placeholder("whisper-large-v3-turbo");
            st.set_value(s.dictation.model.clone(), window, cx);
            st
        });
        let dict_lang_input = cx.new(|cx| {
            let mut st = InputState::new(window, cx).placeholder(t("zh / en（留空自动识别）"));
            st.set_value(s.dictation.language.clone(), window, cx);
            st
        });

        let overlay_path_input = cx.new(|cx| {
            let mut st = InputState::new(window, cx).placeholder(t("本地壁纸路径或图片 URL"));
            st.set_value(s.codex_app_image_overlay_path.clone(), window, cx);
            st
        });
        let overlay_opacity_input = cx.new(|cx| {
            let mut st = InputState::new(window, cx).placeholder("15");
            st.set_value(
                s.codex_app_image_overlay_opacity.to_string(),
                window,
                cx,
            );
            st
        });
        let init_args = s.codex_extra_args.join("\n");
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
            hydrated_once: true,
            stepwise_url_input,
            stepwise_key_input,
            stepwise_model_input,
            dict_url_input,
            dict_key_input,
            dict_env_input,
            dict_model_input,
            dict_lang_input,
            overlay_path_input,
            overlay_opacity_input,
            extra_args_textarea,
        }
    }

    fn switch_row(
        &self,
        cx: &App,
        id: &'static str,
        title: String,
        desc: String,
        checked: bool,
        mutate: impl Fn(&mut BackendSettings, bool) + 'static,
    ) -> AnyElement {
        let store = self.store.clone();
        let title_for_toast = title.clone();
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
                    .gap_0p5()
                    .child(
                        div()
                            .text_sm()
                            .font_medium()
                            .text_color(cx.theme().foreground)
                            .child(title),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(desc),
                    ),
            )
            .child(
                Switch::new(id)
                    .checked(checked)
                    .on_change(move |val, _, cx| {
                        let next_val = *val;
                        let title_cp = title_for_toast.clone();
                        store.update(cx, |s, cx| {
                            let mut next = s.settings.clone();
                            mutate(&mut next, next_val);
                            s.persist_settings_with_notice(next, title_cp, cx);
                        });
                    }),
            )
            .into_any_element()
    }

    fn render_general_tab(&self, cx: &mut Context<Self>) -> AnyElement {
        let (s, px_snap) = {
            let data = self.store.read(cx);
            (data.settings.clone(), data.network_proxy_snapshot.clone())
        };
        let st_goto_settings = self.store.clone();
        let cur_codex_sel = px_snap.config.clients.codex.clone();
        let mut codex_px_opts = vec![
            ("system".to_string(), t("跟随系统代理")),
            ("app".to_string(), t("跟随本程序")),
            ("direct".to_string(), t("强制直连")),
        ];
        for n in &px_snap.config.nodes {
            codex_px_opts.push((format!("node:{}", n.id), n.name.clone()));
        }
        let codex_px_btns: Vec<AnyElement> = codex_px_opts
            .into_iter()
            .map(|(sel, lbl)| {
                let selected = cur_codex_sel == sel;
                let st = self.store.clone();
                let b = AppButton::new(SharedString::from(format!("codex-enh-px-{sel}")))
                    .small()
                    .label(lbl);
                let b = if selected { b.selected(true) } else { b.outline() };
                b.on_click(move |_, _, cx| {
                    let next_sel = sel.clone();
                    st.update(cx, |s, cx| {
                        s.update_self_or_client_proxy_assignment("codex", next_sel, cx);
                    });
                })
                .into_any_element()
            })
            .collect();

        let rows = vec![
            self.switch_row(
                cx,
                "enh-plugin-market",
                t("解锁 Codex 插件市场"),
                t("开放 Codex 客户端内的插件浏览与安装入口"),
                s.codex_app_plugin_marketplace_unlock,
                |st, v| st.codex_app_plugin_marketplace_unlock = v,
            ),
            self.switch_row(
                cx,
                "enh-model-whitelist",
                t("解锁全量自定义模型白名单"),
                t("允许在 Codex 模型选择器中自由展示并选择第三方自定义模型"),
                s.codex_app_model_whitelist_unlock,
                |st, v| st.codex_app_model_whitelist_unlock = v,
            ),
            self.switch_row(
                cx,
                "enh-usage-line",
                t("底部实时配额与用量状态栏"),
                t("在对话输入框下方展示上下文占比与配额消耗概览"),
                s.codex_app_usage_status_line,
                |st, v| st.codex_app_usage_status_line = v,
            ),
            self.switch_row(
                cx,
                "enh-session-del",
                t("客户端侧边栏快捷删除会话"),
                t("在会话列表项提供一键删除本地会话入口"),
                s.codex_app_session_delete,
                |st, v| st.codex_app_session_delete = v,
            ),
            self.switch_row(
                cx,
                "enh-md-export",
                t("会话导出为 Markdown"),
                t("支持将当前完整对话历史一键导出为结构化 Markdown 文档"),
                s.codex_app_markdown_export,
                |st, v| st.codex_app_markdown_export = v,
            ),
            self.switch_row(
                cx,
                "enh-answer-outline",
                t("长回复结构化目录大纲"),
                t("自动提取模型长回复的标题层级并提供侧边跳转导航"),
                s.codex_app_answer_outline_enabled,
                |st, v| st.codex_app_answer_outline_enabled = v,
            ),
            self.switch_row(
                cx,
                "enh-scroll-restore",
                t("切换会话保持滚动位置"),
                t("在不同对话线程间往返切换时恢复上次阅读偏移"),
                s.codex_app_thread_scroll_restore,
                |st, v| st.codex_app_thread_scroll_restore = v,
            ),
            self.switch_row(
                cx,
                "enh-goals",
                t("启用 Codex Goals 长程目标特性"),
                t("向 Codex 配置同步开启实验性多轮自治目标能力"),
                s.codex_goals_enabled,
                |st, v| st.codex_goals_enabled = v,
            ),
            self.switch_row(
                cx,
                "enh-capacity-retry",
                t("上游容量繁忙自动重试"),
                t("遇到临时过载或限流错误时自动退避重试"),
                s.codex_app_capacity_auto_retry,
                |st, v| st.codex_app_capacity_auto_retry = v,
            ),
            self.switch_row(
                cx,
                "enh-zh-locale",
                t("强制 Codex 界面中文化"),
                t("为官方客户端界面注入简体中文翻译增强层"),
                s.codex_app_force_chinese_locale,
                |st, v| st.codex_app_force_chinese_locale = v,
            ),
        ];

        div()
            .v_flex()
            .w_full()
            .gap_4()
            .child(panel_card(
                cx,
                div()
                    .v_flex()
                    .w_full()
                    .gap_3()
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
                                    .gap_0p5()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_semibold()
                                            .text_color(cx.theme().foreground)
                                            .child(t("Codex 客户端与 Codex 点火隔离代理")),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(t("用于 Codex 桌面启动、OpenAI OAuth 登录、官方账号配额刷新及 Lamplighter Codex 点火（与本程序及 Antigravity 完全隔离）")),
                                    ),
                            )
                            .child(
                                AppButton::new("codex-enh-manage-proxy-pool")
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
                                            .child(t("Codex 客户端与 Codex 点火代理")),
                                    )
                                    .child(AppBadge::info(format!(
                                        "{}: {}",
                                        t("当前生效"),
                                        px_snap.effective_codex
                                    ))),
                            )
                            .child(div().h_flex().flex_wrap().gap_1p5().children(codex_px_btns)),
                    ),
            ))
            .child(panel_card(cx, div().v_flex().w_full().gap_2p5().children(rows)))
            .child({
                let ent_args_save = cx.entity();
                panel_card(
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
                                .child(t("Codex 附加启动参数")),
                        )
                        .child(Textarea::new(&self.extra_args_textarea).h(px(88.)))
                        .child(
                            div().h_flex().w_full().justify_end().child(
                                AppButton::new("enh-save-extra-args")
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
                )
            })
            .into_any_element()
    }

    fn render_stepwise_tab(&self, cx: &mut Context<Self>) -> AnyElement {
        let data = self.store.read(cx);
        let s = data.settings.clone();
        let testing = data.stepwise_testing;

        let ent_save = cx.entity();
        let ent_test = cx.entity();

        panel_card(
            cx,
            div()
                .v_flex()
                .w_full()
                .gap_3p5()
                .child(self.switch_row(
                    cx,
                    "stepwise-enable",
                    t("启用下一步追问建议 (Stepwise)"),
                    t("每轮回复结束后自动生成可一键点击发送的后续追问候选"),
                    s.codex_app_stepwise_enabled,
                    |st, v| st.codex_app_stepwise_enabled = v,
                ))
                .child(self.switch_row(
                    cx,
                    "stepwise-direct-send",
                    t("点击建议直接发送"),
                    t("关闭时点击建议仅填入输入框，开启时点击即刻提交"),
                    s.codex_app_stepwise_direct_send,
                    |st, v| st.codex_app_stepwise_direct_send = v,
                ))
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
                                        .child("Stepwise Base URL"),
                                )
                                .child(Input::new(&self.stepwise_url_input)),
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
                                        .child(t("建议生成模型")),
                                )
                                .child(Input::new(&self.stepwise_model_input)),
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
                                .child("Stepwise API Key"),
                        )
                        .child(Input::new(&self.stepwise_key_input).mask_toggle()),
                )
                .child(
                    div()
                        .h_flex()
                        .justify_end()
                        .gap_2()
                        .child(
                            AppButton::new("stepwise-test-btn")
                                .secondary()
                                .small()
                                .disabled(testing)
                                .icon(IconName::FlaskConical)
                                .label(if testing {
                                    t("测试中…")
                                } else {
                                    t("测试连接")
                                })
                                .on_click(move |_, _, cx| {
                                    ent_test.update(cx, |this, cx| {
                                        let mut draft = this.store.read(cx).settings.clone();
                                        draft.codex_app_stepwise_base_url = this
                                            .stepwise_url_input
                                            .read(cx)
                                            .value()
                                            .trim()
                                            .to_string();
                                        draft.codex_app_stepwise_api_key = this
                                            .stepwise_key_input
                                            .read(cx)
                                            .value()
                                            .trim()
                                            .to_string();
                                        draft.codex_app_stepwise_model = this
                                            .stepwise_model_input
                                            .read(cx)
                                            .value()
                                            .trim()
                                            .to_string();
                                        this.store
                                            .update(cx, |s, cx| s.test_stepwise_now(draft, cx));
                                    });
                                }),
                        )
                        .child(
                            AppButton::new("stepwise-save-btn")
                                .primary()
                                .small()
                                .icon(IconName::Save)
                                .label(t("保存下一步建议设置"))
                                .on_click(move |_, _, cx| {
                                    ent_save.update(cx, |this, cx| {
                                        let mut next = this.store.read(cx).settings.clone();
                                        next.codex_app_stepwise_base_url = this
                                            .stepwise_url_input
                                            .read(cx)
                                            .value()
                                            .trim()
                                            .to_string();
                                        next.codex_app_stepwise_api_key = this
                                            .stepwise_key_input
                                            .read(cx)
                                            .value()
                                            .trim()
                                            .to_string();
                                        next.codex_app_stepwise_model = this
                                            .stepwise_model_input
                                            .read(cx)
                                            .value()
                                            .trim()
                                            .to_string();
                                        this.store.update(cx, |s, cx| {
                                            s.persist_settings_with_notice(
                                                next,
                                                t("下一步建议"),
                                                cx,
                                            )
                                        });
                                    });
                                }),
                        ),
                ),
        )
        .into_any_element()
    }

    fn render_dictation_tab(&self, cx: &mut Context<Self>) -> AnyElement {
        let data = self.store.read(cx);
        let dict = data.settings.dictation.clone();
        let current_preset = detect_dictation_preset(&dict);
        let is_sensevoice = dict.provider == DictationProvider::SenseVoice;
        let sv_ready = data
            .sensevoice_status
            .get("ready")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let sv_downloading = data.sensevoice_downloading;
        let store_dl = self.store.clone();
        let ent_save = cx.entity();

        let preset_btns: Vec<AnyElement> = [
            DictationPreset::Groq,
            DictationPreset::OpenAi,
            DictationPreset::Local,
            DictationPreset::SenseVoice,
            DictationPreset::Custom,
        ]
        .into_iter()
        .map(|pr| {
            let ent = cx.entity();
            AppButton::new(SharedString::from(format!("dict-preset-{pr:?}")))
                .outline()
                .small()
                .selected(current_preset == pr)
                .label(t(pr.label()))
                .on_click(move |_, window, cx| {
                    ent.update(cx, |this, cx| {
                        let mut next = this.store.read(cx).settings.clone();
                        apply_dictation_preset(&mut next.dictation, pr);
                        let u = next.dictation.base_url.clone();
                        let m = next.dictation.model.clone();
                        this.dict_url_input
                            .update(cx, |i, cx| i.set_value(u, window, cx));
                        this.dict_model_input
                            .update(cx, |i, cx| i.set_value(m, window, cx));
                        this.store.update(cx, |s, cx| {
                            s.persist_settings_with_notice(next, t("语音听写"), cx);
                        });
                    });
                })
                .into_any_element()
        })
        .collect();

        panel_card(
            cx,
            div()
                .v_flex()
                .w_full()
                .gap_3p5()
                .child(self.switch_row(
                    cx,
                    "dictation-enable",
                    t("启用语音听写输入"),
                    t("支持云端 Whisper 兼容接口或完全离线的本机 SenseVoiceSmall ONNX 模型"),
                    dict.enabled,
                    |st, v| st.dictation.enabled = v,
                ))
                .child(
                    div()
                        .h_flex()
                        .flex_wrap()
                        .items_center()
                        .gap_2()
                        .children(preset_btns),
                )
                .when(is_sensevoice, |d| {
                    d.child(
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
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_medium()
                                            .text_color(cx.theme().foreground)
                                            .child(t("SenseVoiceSmall (INT8) 本机离线模型")),
                                    )
                                    .child(if sv_ready {
                                        AppBadge::good(t("已就绪"))
                                    } else {
                                        AppBadge::warn(t("未下载"))
                                    }),
                            )
                            .child(
                                AppButton::new("sensevoice-dl-btn")
                                    .primary()
                                    .small()
                                    .disabled(sv_downloading)
                                    .icon(IconName::Download)
                                    .label(if sv_downloading {
                                        t("正在下载模型…")
                                    } else if sv_ready {
                                        t("重新校验模型")
                                    } else {
                                        t("下载离线模型")
                                    })
                                    .on_click(move |_, _, cx| {
                                        store_dl
                                            .update(cx, |s, cx| s.download_sensevoice_model(cx));
                                    }),
                            ),
                    )
                })
                .when(!is_sensevoice, |d| {
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
                                            .child("Base URL"),
                                    )
                                    .child(Input::new(&self.dict_url_input)),
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
                                            .child(t("转写模型")),
                                    )
                                    .child(Input::new(&self.dict_model_input)),
                            ),
                    )
                    .child(
                        div()
                            .h_flex()
                            .flex_wrap()
                            .w_full()
                            .gap_4()
                            .child(
                                div()
                                    .v_flex()
                                    .flex_1()
                                    .min_w(px(200.))
                                    .gap_1p5()
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child("API Key"),
                                    )
                                    .child(Input::new(&self.dict_key_input).mask_toggle()),
                            )
                            .child(
                                div()
                                    .v_flex()
                                    .flex_1()
                                    .min_w(px(200.))
                                    .gap_1p5()
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(t("环境变量名（可选）")),
                                    )
                                    .child(Input::new(&self.dict_env_input)),
                            )
                            .child(
                                div()
                                    .v_flex()
                                    .w(px(235.))
                                    .gap_1p5()
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(t("首选语言代码")),
                                    )
                                    .child(Input::new(&self.dict_lang_input)),
                            ),
                    )
                })
                .child(
                    div().h_flex().justify_end().child(
                        AppButton::new("dict-save-btn")
                            .primary()
                            .small()
                            .icon(IconName::Save)
                            .label(t("保存语音听写设置"))
                            .on_click(move |_, _, cx| {
                                ent_save.update(cx, |this, cx| {
                                    let mut next = this.store.read(cx).settings.clone();
                                    if next.dictation.provider != DictationProvider::SenseVoice {
                                        next.dictation.base_url = this
                                            .dict_url_input
                                            .read(cx)
                                            .value()
                                            .trim()
                                            .to_string();
                                        next.dictation.model = this
                                            .dict_model_input
                                            .read(cx)
                                            .value()
                                            .trim()
                                            .to_string();
                                        next.dictation.api_key = this
                                            .dict_key_input
                                            .read(cx)
                                            .value()
                                            .trim()
                                            .to_string();
                                        next.dictation.api_key_env = this
                                            .dict_env_input
                                            .read(cx)
                                            .value()
                                            .trim()
                                            .to_string();
                                    }
                                    next.dictation.language = this
                                        .dict_lang_input
                                        .read(cx)
                                        .value()
                                        .trim()
                                        .to_string();
                                    if let Some(issue) =
                                        validate_dictation_settings(&next.dictation)
                                    {
                                        this.store.update(cx, |s, cx| {
                                            s.set_notice(t("语音听写"), issue, "failed", cx)
                                        });
                                        return;
                                    }
                                    this.store.update(cx, |s, cx| {
                                        s.persist_settings_with_notice(next, t("语音听写"), cx)
                                    });
                                });
                            }),
                    ),
                ),
        )
        .into_any_element()
    }

    fn render_overlay_tab(&self, cx: &mut Context<Self>) -> AnyElement {
        let s = self.store.read(cx).settings.clone();
        let fit = s.codex_app_image_overlay_fit_mode.clone();
        let ent_pick = cx.entity();
        let ent_save = cx.entity();
        let store_reset = self.store.clone();

        let fit_btns: Vec<AnyElement> = [
            ("cover", t("Cover (铺满裁剪)")),
            ("contain", t("Contain (完整适应)")),
        ]
            .into_iter()
            .map(|(mode_key, lbl)| {
                let st = self.store.clone();
                AppButton::new(SharedString::from(format!("ov-fit-{mode_key}")))
                    .outline()
                    .small()
                    .selected(fit == mode_key)
                    .label(lbl)
                    .on_click(move |_, _, cx| {
                        st.update(cx, |s, cx| {
                            let mut next = s.settings.clone();
                            next.codex_app_image_overlay_fit_mode = mode_key.to_string();
                            s.persist_settings_with_notice(next, t("背景图层"), cx);
                        });
                    })
                    .into_any_element()
            })
            .collect();

        panel_card(
            cx,
            div()
                .v_flex()
                .w_full()
                .gap_3p5()
                .child(self.switch_row(
                    cx,
                    "overlay-enable",
                    t("启用客户端自定义背景图层"),
                    t("为 Codex 窗口叠加自定义壁纸与半透明雾化效果"),
                    s.codex_app_image_overlay_enabled,
                    |st, v| st.codex_app_image_overlay_enabled = v,
                ))
                .child(
                    div()
                        .h_flex()
                        .flex_wrap()
                        .w_full()
                        .items_end()
                        .gap_3()
                        .child(
                            div()
                                .v_flex()
                                .flex_1()
                                .min_w(px(220.))
                                .gap_1p5()
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(t("背景图片路径")),
                                )
                                .child(Input::new(&self.overlay_path_input)),
                        )
                        .child(
                            AppButton::new("overlay-pick-file")
                                .secondary()
                                .small()
                                .icon(IconName::FolderOpen)
                                .label(t("选择图片"))
                                .on_click(move |_, window, cx| {
                                    let win_h = window.window_handle();
                                    let ent = ent_pick.clone();
                                    cx.spawn(async move |cx| {
                                        if let Some(f) = rfd::AsyncFileDialog::new()
                                            .add_filter(
                                                "Image",
                                                &["png", "jpg", "jpeg", "webp", "gif"],
                                            )
                                            .pick_file()
                                            .await
                                        {
                                            let p = f.path().to_string_lossy().to_string();
                                            cx.update(|app| {
                                                let _ = win_h.update(app, |_, win, cx| {
                                                    ent.update(cx, |v, cx| {
                                                        v.overlay_path_input.update(
                                                            cx,
                                                            |inp, cx| inp.set_value(p, win, cx),
                                                        );
                                                    });
                                                });
                                            });
                                        }
                                    })
                                    .detach();
                                }),
                        )
                        .child(
                            div()
                                .v_flex()
                                .w(px(120.))
                                .gap_1p5()
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(t("不透明度 (1-60%)")),
                                )
                                .child(Input::new(&self.overlay_opacity_input)),
                        ),
                )
                .child(div().h_flex().items_center().gap_2().children(fit_btns))
                .child(
                    div()
                        .h_flex()
                        .justify_end()
                        .gap_2()
                        .child(
                            AppButton::new("overlay-reset-btn")
                                .outline()
                                .small()
                                .label(t("恢复默认"))
                                .on_click(move |_, _, cx| {
                                    store_reset.update(cx, |s, cx| s.reset_image_overlay(cx));
                                }),
                        )
                        .child(
                            AppButton::new("overlay-save-btn")
                                .primary()
                                .small()
                                .icon(IconName::Save)
                                .label(t("保存图层设置"))
                                .on_click(move |_, _, cx| {
                                    ent_save.update(cx, |this, cx| {
                                        let mut next = this.store.read(cx).settings.clone();
                                        next.codex_app_image_overlay_path = this
                                            .overlay_path_input
                                            .read(cx)
                                            .value()
                                            .trim()
                                            .to_string();
                                        if let Ok(op) = this
                                            .overlay_opacity_input
                                            .read(cx)
                                            .value()
                                            .trim()
                                            .parse::<u8>()
                                        {
                                            next.codex_app_image_overlay_opacity = op.clamp(1, 80);
                                        }
                                        this.store.update(cx, |s, cx| {
                                            s.persist_settings_with_notice(next, t("背景图层"), cx)
                                        });
                                    });
                                }),
                        ),
                ),
        )
        .into_any_element()
    }

    fn render_browser_tab(&self, cx: &mut Context<Self>) -> AnyElement {
        let data = self.store.read(cx);
        let s = data.settings.clone();
        let diag = data.native_browser_diag.clone();
        let store_ref = self.store.clone();

        let (compat_ok, compat_state, compat_detail) = match &diag {
            Some(d) => (
                d.compatibility.state == "compatible" || d.compatibility.state == "ready",
                d.compatibility.state.clone(),
                if d.compatibility.detail.is_empty() {
                    t("原生浏览器补丁状态已读取")
                } else {
                    d.compatibility.detail.clone()
                },
            ),
            None => (false, "not_checked".to_string(), t("正在检测…")),
        };

        let (conn_ok, conn_state, conn_detail) = match &diag {
            Some(d) => (
                !d.connection.browsers.is_empty() || d.connection.state == "connected",
                d.connection.state.clone(),
                tf(
                    "已发现连接浏览器：{0} 个",
                    &[&d.connection.browsers.len().to_string()],
                ),
            ),
            None => (false, "not_checked".to_string(), t("正在检测…")),
        };

        panel_card(
            cx,
            div()
                .v_flex()
                .w_full()
                .gap_3p5()
                .child(self.switch_row(
                    cx,
                    "enh-browser-ident",
                    t("原生浏览器连接要求显式身份确认"),
                    t("控制 Codex 连接本机浏览器桥接时的握手校验策略"),
                    s.codex_app_native_browser_require_identification,
                    |st, v| st.codex_app_native_browser_require_identification = v,
                ))
                .child(health_item_row(
                    cx,
                    compat_ok,
                    t("客户端运行时兼容状态"),
                    compat_detail,
                    &compat_state,
                ))
                .child(health_item_row(
                    cx,
                    conn_ok,
                    t("本机浏览器桥接连接"),
                    conn_detail,
                    &conn_state,
                ))
                .child(
                    div().h_flex().justify_end().child(
                        AppButton::new("enh-browser-refresh")
                            .secondary()
                            .small()
                            .icon(IconName::RefreshCw)
                            .label(t("重新检测状态"))
                            .on_click(move |_, _, cx| {
                                store_ref.update(cx, |s, cx| s.refresh_enhance_page(cx));
                            }),
                    ),
                ),
        )
        .into_any_element()
    }
}

impl Render for EnhanceView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let _ = self.hydrated_once;
        let data = self.store.read(cx);
        let master = data.settings.enhancements_enabled;
        let tab = data.enhance_tab;
        let store_master = self.store.clone();

        let tabs = [
            ("general", t("常规增强")),
            ("stepwise", t("下一步建议")),
            ("dictation", t("语音听写")),
            ("overlay", t("背景图层")),
            ("browser", t("原生浏览器")),
        ];
        let tab_btns: Vec<AnyElement> = tabs
            .into_iter()
            .map(|(k, lbl)| {
                let st = self.store.clone();
                AppButton::new(SharedString::from(format!("enh-tab-{k}")))
                    .outline()
                    .small()
                    .selected(tab == k)
                    .label(lbl)
                    .on_click(move |_, _, cx| {
                        st.update(cx, |s, cx| {
                            s.enhance_tab = k;
                            cx.notify();
                        });
                    })
                    .into_any_element()
            })
            .collect();

        let body = match tab {
            "stepwise" => self.render_stepwise_tab(cx),
            "dictation" => self.render_dictation_tab(cx),
            "overlay" => self.render_overlay_tab(cx),
            "browser" => self.render_browser_tab(cx),
            _ => self.render_general_tab(cx),
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
                    .gap_4()
                    .child(
                        div()
                            .v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_1()
                            .child(
                                div()
                                    .text_sm()
                                    .font_semibold()
                                    .text_color(cx.theme().foreground)
                                    .child(t("启用 Codex 客户端增强注入")),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(t("控制启动 Codex 时是否挂载 Swivox 渲染层增强能力（修改后对运行中实例可重启生效）。")),
                            ),
                    )
                    .child(
                        Switch::new("enh-master-switch")
                            .checked(master)
                            .on_change(move |val, _, cx| {
                                let v = *val;
                                store_master.update(cx, |s, cx| {
                                    let mut next = s.settings.clone();
                                    next.enhancements_enabled = v;
                                    s.persist_settings_with_notice(next, t("Codex增强"), cx);
                                });
                            }),
                    ),
            ))
            .child(div().h_flex().flex_wrap().items_center().gap_2().children(tab_btns))
            .child(body)
    }
}
