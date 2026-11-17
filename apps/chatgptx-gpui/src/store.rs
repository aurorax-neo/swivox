use std::collections::HashSet;
use std::future::Future;
use std::time::{Duration, Instant};

use chatgptx_backend::auto_takeover::AutoTakeoverPayload;
use chatgptx_backend::commands::{
    self, BindRelayOfficialAccountRequest, CcsProvidersPayload, CodexMultiInstance,
    CodexMultiInstanceRequest, CodexMultiLaunchRequest, CompleteOfficialOAuthRequest,
    ContextDeleteRequest, ContextEntryRequest, ContextSettingsRequest, DeleteLocalSessionRequest,
    ImportOfficialAccountsRequest, LaunchRequest, ListLocalSessionsRequest, LocalSessionsPayload,
    LogRequest, LogsPayload, McpImportPreviewPayload, McpImportRequest, NativeBrowserDiagnostics,
    OverviewPayload, ProviderDoctorPayload, RelayFilesPayload, RelayProfileSwitchRequest,
    RemoveEnvConflictsRequest, SaveGlobalLaunchProxyRequest, SaveRelayFileRequest, SkillsPayload,
    SubmitOfficialOAuthCallbackRequest, ToolEntry, UpdateOfficialAccountRequest, WatcherPayload,
};
use chatgptx_backend::i18n::{self, Language, t};
use chatgptx_backend::phase2_rules::{
    multi_launch_instance_failure, multi_launch_instance_running, should_display_multi_instance,
};
use chatgptx_backend::phase3_rules::{
    PROVIDER_PRESETS, ProviderViewKind, create_aggregate_draft, create_custom_relay_profile,
    create_preset_relay_profile, duplicate_relay_profile, move_relay_profile_in_view,
    relay_provider_view, remove_relay_profile, upsert_relay_profile_into_settings,
};
use chatgptx_backend::phase4_rules::SessionGroupMode;
use chatgptx_backend::phase5_rules::{
    AgentCacheReportView, MarketCatalogView, MarketFilter, MarketScriptView, MarketSource,
    UpdateCheckView, UserScriptInventoryView, parse_agent_cache_scan_payload,
    parse_plugin_market_catalog, parse_script_market_list, parse_user_script_inventory,
    recommended_cache_selection,
};
use chatgptx_backend::ui_prefs::{self, UiPreferences, UiThemeMode};
use chatgptx_core::env_conflicts::EnvConflict;
use chatgptx_core::official_accounts::{
    OAuthLoginStartResponse, OfficialAccountView, OfficialAccountsSnapshot,
};
use chatgptx_core::provider_import::ProviderImportRequest;
use chatgptx_core::relay_config::CodexContextEntry;
use chatgptx_core::relay_environment::RelayEnvironmentReport;
use chatgptx_core::settings::{AggregateRelayProfile, BackendSettings, RelayMode, RelayProfile};
use chatgptx_core::skills::SkillRepo;
use chatgptx_core::tools::ToolId;
use chatgptx_data::{LocalSession, SessionIndexCleanupPreview};
use gpui_kit::component::theme::{Theme, ThemeMode};
use gpui_kit::*;

use crate::routes::{Route, StatusTone, destination_from_navigation_intent, status_tone};
use crate::widgets::apply_cjk_typography;

#[derive(Debug, Clone)]
pub struct InlineNotice {
    pub title: String,
    pub message: String,
    pub tone: StatusTone,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MultiPollOutcome {
    Ready,
    Failed,
    Pending,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum RelaySubscreen {
    #[default]
    List,
    Catalog,
    Editor {
        profile_id: String,
        is_new: bool,
    },
}

#[derive(Debug, Clone)]
pub struct RelayEditorSeed {
    pub seq: u64,
    pub profile: RelayProfile,
    pub aggregate: Option<AggregateRelayProfile>,
    pub is_new: bool,
}

pub struct AppStore {
    pub tokio_handle: tokio::runtime::Handle,
    pub prefs: UiPreferences,
    pub route: Route,
    pub active_tool: String,
    pub tool_entries: Vec<ToolEntry>,
    pub overview: Option<OverviewPayload>,
    pub settings: BackendSettings,
    pub relay_environment: Option<RelayEnvironmentReport>,
    pub launch_pending: bool,
    pub env_refreshing: bool,
    pub notice: Option<InlineNotice>,

    // 官方账号管理状态
    pub official_snapshot: Option<OfficialAccountsSnapshot>,
    pub official_loading: bool,
    pub official_search_query: String,
    pub official_page: usize,
    pub oauth_session: Option<OAuthLoginStartResponse>,
    pub oauth_waiting: bool,
    pub import_open: bool,
    pub importing_official: bool,
    pub export_json_text: String,
    pub switching_account_id: Option<String>,
    pub refreshing_quota_id: Option<String>,
    pub editing_account_id: Option<String>,
    pub open_binding_dropdown_for: Option<String>,
    pub use_confirm_account: Option<OfficialAccountView>,
    pub delete_confirm_account: Option<OfficialAccountView>,

    // Codex 多开状态
    pub multi_instances: Vec<CodexMultiInstance>,
    pub multi_instances_loading: bool,
    pub multi_launch_pending: Option<String>,
    pub multi_action_pending: Option<String>,

    // 供应商切换与 Profile 详情编辑状态 (Phase 3)
    pub relay_view_tab: ProviderViewKind,
    pub relay_subscreen: RelaySubscreen,
    pub relay_catalog_query: String,
    pub relay_editor_seed: Option<RelayEditorSeed>,
    pub relay_editor_seq: u64,
    pub use_confirm_relay_id: Option<String>,
    pub delete_confirm_relay_id: Option<String>,
    pub env_conflicts: Vec<EnvConflict>,
    pub ccs_providers: Option<CcsProvidersPayload>,
    pub ccs_import_open: bool,
    pub pending_provider_import: Option<ProviderImportRequest>,
    pub relay_switching: bool,
    pub testing_profile_id: Option<String>,
    pub doctor_running: bool,
    pub doctor_result: Option<ProviderDoctorPayload>,
    pub fetching_models: bool,
    pub fetched_models_seq: u64,
    pub last_fetched_models: Option<(u64, Vec<String>)>,
    pub relay_files: Option<RelayFilesPayload>,

    // Phase 4：全局管理 (MCP / Skills / 会话管理)
    pub mcp_entries: Vec<CodexContextEntry>,
    pub mcp_live_entries: Vec<CodexContextEntry>,
    pub mcp_search_query: String,
    pub mcp_import_open: bool,
    pub mcp_import_preview: Option<McpImportPreviewPayload>,
    pub mcp_editor_open: bool,
    pub mcp_editor_seq: u64,
    pub mcp_editor_seed: Option<(u64, String, String, bool)>, // (seq, id, toml_body, is_new)

    pub skills_payload: Option<SkillsPayload>,
    pub skills_loading: bool,
    pub skills_busy_id: Option<String>,
    pub skills_search_query: String,
    pub skills_filter_tab: &'static str, // "all" | "installed" | "updates"
    pub skills_repos_open: bool,

    pub sessions_payload: Option<LocalSessionsPayload>,
    pub sessions_loading: bool,
    pub sessions_search_query: String,
    pub sessions_group_mode: SessionGroupMode,
    pub session_delete_confirm: Option<LocalSession>,
    pub session_repair_busy: bool,
    pub session_cleanup_preview: Option<SessionIndexCleanupPreview>,

    // Phase 5：扩展与系统页 (Enhance / UserScripts / PluginMarket / AgentCache / Maintenance / About / Settings)
    pub enhance_tab: &'static str, // "general" | "stepwise" | "dictation" | "overlay" | "browser"
    pub sensevoice_status: serde_json::Value,
    pub sensevoice_downloading: bool,
    pub stepwise_testing: bool,
    pub native_browser_diag: Option<NativeBrowserDiagnostics>,

    pub user_scripts_inventory: UserScriptInventoryView,
    pub market_scripts: Vec<MarketScriptView>,
    pub script_market_loading: bool,
    pub script_installing_id: Option<String>,

    pub plugin_source: MarketSource,
    pub plugin_filter: MarketFilter,
    pub plugin_query: String,
    pub plugin_page: usize,
    pub plugin_catalog_public: Option<MarketCatalogView>,
    pub plugin_catalog_full: Option<MarketCatalogView>,
    pub plugin_loading: bool,
    pub plugin_installing_id: Option<String>,

    pub cache_report: Option<AgentCacheReportView>,
    pub cache_selected_ids: HashSet<String>,
    pub cache_scanning: bool,
    pub cache_cleaning: bool,
    pub cache_confirm_open: bool,

    pub watcher_state: Option<WatcherPayload>,
    pub auto_takeover_state: Option<AutoTakeoverPayload>,
    pub shortcuts_repairing: bool,
    pub provider_sync_busy: bool,

    pub logs_payload: Option<LogsPayload>,
    pub update_info: Option<UpdateCheckView>,
    pub checking_update: bool,
    pub installing_update: bool,

    pub proxy_testing: bool,
    pub reset_confirm_open: bool,
}

impl AppStore {
    pub fn new(tokio_handle: tokio::runtime::Handle, prefs: UiPreferences) -> Self {
        let tools_res = commands::list_tools();
        let active_tool = tools_res.payload.active_tool;
        let tool_entries = tools_res.payload.tools;
        let loaded_settings_res = commands::load_settings();
        let settings = loaded_settings_res.payload.settings;
        let user_scripts_inventory =
            parse_user_script_inventory(&loaded_settings_res.payload.user_scripts);
        let official_snapshot = Some(commands::load_official_accounts().payload);
        let env_conflicts = commands::check_env_conflicts().payload.conflicts;
        let pending_provider_import = commands::load_pending_provider_import().payload.pending;

        // 默认定位到当前激活供应商所在的分区标签页
        let agg_map: std::collections::HashMap<String, AggregateRelayProfile> = settings
            .aggregate_relay_profiles
            .iter()
            .map(|a| (a.id.clone(), a.clone()))
            .collect();
        let initial_tab = settings
            .relay_profiles
            .iter()
            .find(|p| p.id == settings.active_relay_id)
            .map(|p| relay_provider_view(p, &agg_map))
            .unwrap_or(ProviderViewKind::Direct);

        Self {
            tokio_handle,
            prefs,
            route: Route::Overview,
            active_tool,
            tool_entries,
            overview: None,
            settings,
            relay_environment: None,
            launch_pending: false,
            env_refreshing: false,
            notice: None,

            official_snapshot,
            official_loading: false,
            official_search_query: String::new(),
            official_page: 1,
            oauth_session: None,
            oauth_waiting: false,
            import_open: false,
            importing_official: false,
            export_json_text: String::new(),
            switching_account_id: None,
            refreshing_quota_id: None,
            editing_account_id: None,
            open_binding_dropdown_for: None,
            use_confirm_account: None,
            delete_confirm_account: None,

            multi_instances: Vec::new(),
            multi_instances_loading: false,
            multi_launch_pending: None,
            multi_action_pending: None,

            relay_view_tab: initial_tab,
            relay_subscreen: RelaySubscreen::List,
            relay_catalog_query: String::new(),
            relay_editor_seed: None,
            relay_editor_seq: 0,
            use_confirm_relay_id: None,
            delete_confirm_relay_id: None,
            env_conflicts,
            ccs_providers: None,
            ccs_import_open: false,
            pending_provider_import,
            relay_switching: false,
            testing_profile_id: None,
            doctor_running: false,
            doctor_result: None,
            fetching_models: false,
            fetched_models_seq: 0,
            last_fetched_models: None,
            relay_files: None,

            mcp_entries: Vec::new(),
            mcp_live_entries: Vec::new(),
            mcp_search_query: String::new(),
            mcp_import_open: false,
            mcp_import_preview: None,
            mcp_editor_open: false,
            mcp_editor_seq: 0,
            mcp_editor_seed: None,

            skills_payload: None,
            skills_loading: false,
            skills_busy_id: None,
            skills_search_query: String::new(),
            skills_filter_tab: "all",
            skills_repos_open: false,

            sessions_payload: None,
            sessions_loading: false,
            sessions_search_query: String::new(),
            sessions_group_mode: SessionGroupMode::Project,
            session_delete_confirm: None,
            session_repair_busy: false,
            session_cleanup_preview: None,

            enhance_tab: "general",
            sensevoice_status: commands::dictation_local_model_status(),
            sensevoice_downloading: false,
            stepwise_testing: false,
            native_browser_diag: None,

            user_scripts_inventory,
            market_scripts: Vec::new(),
            script_market_loading: false,
            script_installing_id: None,

            plugin_source: MarketSource::Public,
            plugin_filter: MarketFilter::All,
            plugin_query: String::new(),
            plugin_page: 0,
            plugin_catalog_public: None,
            plugin_catalog_full: None,
            plugin_loading: false,
            plugin_installing_id: None,

            cache_report: None,
            cache_selected_ids: HashSet::new(),
            cache_scanning: false,
            cache_cleaning: false,
            cache_confirm_open: false,

            watcher_state: Some(commands::load_watcher_state().payload),
            auto_takeover_state: Some(commands::load_auto_takeover_state().payload),
            shortcuts_repairing: false,
            provider_sync_busy: false,

            logs_payload: None,
            update_info: None,
            checking_update: false,
            installing_update: false,

            proxy_testing: false,
            reset_confirm_open: false,
        }
    }

    pub fn codex_app_running(&self) -> bool {
        self.overview
            .as_ref()
            .is_some_and(|ov| ov.runtime_health.codex_app.status == "running")
    }

    pub fn spawn_tokio<T, Fut, F>(&self, cx: &mut Context<Self>, fut: Fut, on_done: F)
    where
        T: Send + 'static,
        Fut: Future<Output = T> + Send + 'static,
        F: FnOnce(&mut Self, T, &mut Context<Self>) + 'static,
    {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.tokio_handle.spawn(async move {
            let output = fut.await;
            let _ = tx.send(output);
        });
        cx.spawn(async move |this, cx| {
            if let Ok(result) = rx.await {
                let _ = this.update(cx, |store, cx| {
                    on_done(store, result, cx);
                });
            }
        })
        .detach();
    }

    pub fn set_notice(
        &mut self,
        title: impl Into<String>,
        message: impl Into<String>,
        status: &str,
        cx: &mut Context<Self>,
    ) {
        self.notice = Some(InlineNotice {
            title: title.into(),
            message: t(&message.into()),
            tone: status_tone(status),
        });
        cx.notify();
    }

    pub fn clear_notice(&mut self, cx: &mut Context<Self>) {
        self.notice = None;
        cx.notify();
    }

    pub fn initialize(&mut self, cx: &mut Context<Self>) {
        self.consume_pending_navigation(cx);
        self.refresh_overview(false, cx);
        self.refresh_relay_environment(false, cx);
    }

    pub fn consume_pending_navigation(&mut self, cx: &mut Context<Self>) {
        let pending_url = commands::load_pending_provider_import().payload.pending;
        if pending_url.is_some() {
            self.pending_provider_import = pending_url;
            self.navigate(Route::Relay, cx);
            return;
        }
        if let Ok(Some(intent)) = commands::consume_pending_manager_navigation() {
            let target = destination_from_navigation_intent(&intent);
            self.navigate(target, cx);
        }
    }

    pub fn navigate(&mut self, next: Route, cx: &mut Context<Self>) {
        if self.route != next {
            self.route = next;
            self.open_binding_dropdown_for = None;
            cx.notify();
        }
        match next {
            Route::Overview => self.refresh_overview(false, cx),
            Route::RelayEnvironment => self.refresh_relay_environment(false, cx),
            Route::OfficialAccounts => {
                self.refresh_settings_silent(cx);
                self.refresh_official_accounts(false, cx);
            }
            Route::Relay => {
                self.refresh_relay_page(false, cx);
            }
            Route::MultiLaunch => {
                self.refresh_settings_silent(cx);
                self.refresh_official_accounts(false, cx);
                self.refresh_multi_instances(false, cx);
            }
            Route::Context => self.refresh_mcp_page(false, cx),
            Route::Skills => self.load_local_skills(cx),
            Route::Sessions => self.refresh_sessions(false, false, cx),
            Route::Enhance => self.refresh_enhance_page(cx),
            Route::UserScripts => self.refresh_user_scripts_page(false, cx),
            Route::PluginMarket => {
                if self.plugin_catalog_public.is_none() {
                    self.load_plugin_market(self.plugin_source, false, cx);
                }
            }
            Route::AgentCache => {
                if self.cache_report.is_none() {
                    self.scan_agent_cache_now(cx);
                }
            }
            Route::Maintenance => self.refresh_maintenance_page(false, cx),
            Route::About => self.refresh_about_page(false, cx),
            Route::Settings => {
                self.refresh_settings_silent(cx);
                self.official_snapshot = Some(commands::load_official_accounts().payload);
                cx.notify();
            }
        }
    }

    pub fn refresh_settings_silent(&mut self, cx: &mut Context<Self>) {
        let loaded = commands::load_settings();
        if loaded.status == "ok" {
            self.settings = loaded.payload.settings;
            cx.notify();
        }
    }

    pub fn switch_tool(&mut self, tool_id: String, cx: &mut Context<Self>) {
        let Some(entry) = self.tool_entries.iter().find(|e| e.id == tool_id) else {
            return;
        };
        if !entry.switchable {
            return;
        }
        self.active_tool = tool_id.clone();
        if let Ok(parsed) =
            serde_json::from_value::<ToolId>(serde_json::Value::String(tool_id.clone()))
        {
            self.settings.active_tool = parsed;
            let saved = commands::save_settings(self.settings.clone());
            if saved.status == "ok" {
                self.settings = saved.payload.settings;
            }
        }
        let tools_res = commands::list_tools();
        self.tool_entries = tools_res.payload.tools;
        if !self.route.visible_for_tool(&self.active_tool) {
            self.route = Route::Overview;
        }
        cx.notify();
    }

    pub fn toggle_theme(&mut self, cx: &mut Context<Self>) {
        self.prefs.theme = self.prefs.theme.toggled();
        let _ = ui_prefs::save_ui_preferences(&self.prefs);
        let mode = match self.prefs.theme {
            UiThemeMode::Dark => ThemeMode::Dark,
            UiThemeMode::Light => ThemeMode::Light,
        };
        Theme::change(mode, None, cx);
        apply_cjk_typography(cx);
        cx.refresh_windows();
        cx.notify();
    }

    pub fn toggle_language(&mut self, cx: &mut Context<Self>) {
        let next: Language = self.prefs.language.toggled();
        self.prefs.language = next;
        i18n::set_language(next);
        let _ = ui_prefs::save_ui_preferences(&self.prefs);
        cx.notify();
        cx.refresh_windows();
    }

    pub fn refresh_current(&mut self, cx: &mut Context<Self>) {
        match self.route {
            Route::Overview => self.refresh_overview(true, cx),
            Route::RelayEnvironment => self.refresh_relay_environment(true, cx),
            Route::OfficialAccounts => self.refresh_official_accounts(true, cx),
            Route::Relay => self.refresh_relay_page(true, cx),
            Route::MultiLaunch => {
                self.refresh_settings_silent(cx);
                self.refresh_official_accounts(false, cx);
                self.refresh_multi_instances(true, cx);
            }
            Route::Context => self.refresh_mcp_page(true, cx),
            Route::Skills => self.refresh_skills_remote(cx),
            Route::Sessions => self.refresh_sessions(false, true, cx),
            Route::Enhance => self.refresh_enhance_page(cx),
            Route::UserScripts => self.refresh_user_scripts_page(true, cx),
            Route::PluginMarket => self.load_plugin_market(self.plugin_source, true, cx),
            Route::AgentCache => self.scan_agent_cache_now(cx),
            Route::Maintenance => self.refresh_maintenance_page(true, cx),
            Route::About => self.refresh_about_page(true, cx),
            Route::Settings => {
                self.refresh_settings_silent(cx);
                self.official_snapshot = Some(commands::load_official_accounts().payload);
                cx.notify();
            }
        }
    }

    pub fn refresh_overview(&mut self, show_toast: bool, cx: &mut Context<Self>) {
        let tools_res = commands::list_tools();
        self.tool_entries = tools_res.payload.tools;
        self.spawn_tokio(
            cx,
            async move { commands::load_overview().await },
            move |store, result, cx| {
                store.overview = Some(result.payload);
                if show_toast {
                    store.set_notice(t("健康检查"), result.message, &result.status, cx);
                } else {
                    cx.notify();
                }
            },
        );
    }

    pub fn refresh_relay_environment(&mut self, show_toast: bool, cx: &mut Context<Self>) {
        self.env_refreshing = true;
        cx.notify();
        self.spawn_tokio(
            cx,
            async move { tokio::task::spawn_blocking(commands::check_relay_environment).await },
            move |store, join_res, cx| {
                store.env_refreshing = false;
                if let Ok(result) = join_res {
                    store.relay_environment = Some(result.payload);
                    if show_toast {
                        store.set_notice(
                            t("中转站环境配置检测"),
                            result.message,
                            &result.status,
                            cx,
                        );
                    }
                }
                cx.notify();
            },
        );
    }

    pub fn launch_codex(&mut self, cx: &mut Context<Self>) {
        if self.launch_pending {
            return;
        }
        self.launch_pending = true;
        let app_path = self.settings.codex_app_path.clone();
        cx.notify();
        self.spawn_tokio(
            cx,
            async move {
                let req = LaunchRequest {
                    app_path,
                    debug_port: 9229,
                    helper_port: 57321,
                    sync_active_relay: true,
                };
                let launch_res = commands::launch_chatgptx(req).await;
                let overview_res = commands::load_overview().await;
                (launch_res, overview_res)
            },
            |store, (launch_res, overview_res), cx| {
                store.launch_pending = false;
                store.overview = Some(overview_res.payload);
                store.set_notice(t("启动 Codex"), launch_res.message, &launch_res.status, cx);
            },
        );
    }

    pub fn restart_codex(&mut self, cx: &mut Context<Self>) {
        if self.launch_pending {
            return;
        }
        self.launch_pending = true;
        let app_path = self.settings.codex_app_path.clone();
        cx.notify();
        self.spawn_tokio(
            cx,
            async move {
                let req = LaunchRequest {
                    app_path,
                    debug_port: 9229,
                    helper_port: 57321,
                    sync_active_relay: true,
                };
                let restart_res = commands::restart_chatgptx(req).await;
                let overview_res = commands::load_overview().await;
                (restart_res, overview_res)
            },
            |store, (restart_res, overview_res), cx| {
                store.launch_pending = false;
                store.overview = Some(overview_res.payload);
                store.set_notice(t("重启 Codex"), restart_res.message, &restart_res.status, cx);
            },
        );
    }

    pub fn stop_codex(&mut self, cx: &mut Context<Self>) {
        if self.launch_pending {
            return;
        }
        self.launch_pending = true;
        cx.notify();
        self.spawn_tokio(
            cx,
            async move {
                let stop_res = commands::stop_chatgptx(None).await;
                let overview_res = commands::load_overview().await;
                (stop_res, overview_res)
            },
            |store, (stop_res, overview_res), cx| {
                store.launch_pending = false;
                store.overview = Some(overview_res.payload);
                store.set_notice(t("停止 Codex"), stop_res.message, &stop_res.status, cx);
            },
        );
    }

    // ========================================================================
    // 官方账号管理动作
    // ========================================================================

    pub fn open_external_url(&mut self, url: String, cx: &mut Context<Self>) {
        let res = commands::open_external_url(url);
        if res.status != "ok" {
            self.set_notice(t("打开浏览器"), res.message, &res.status, cx);
        }
    }

    pub fn refresh_official_accounts(&mut self, show_toast: bool, cx: &mut Context<Self>) {
        self.official_loading = true;
        cx.notify();
        self.spawn_tokio(
            cx,
            async move { tokio::task::spawn_blocking(commands::load_official_accounts).await },
            move |store, join_res, cx| {
                store.official_loading = false;
                if let Ok(res) = join_res {
                    store.official_snapshot = Some(res.payload);
                    if show_toast {
                        store.set_notice(t("官方账号"), res.message, &res.status, cx);
                    }
                }
                cx.notify();
            },
        );
    }

    pub fn start_official_oauth(&mut self, label: Option<String>, cx: &mut Context<Self>) {
        if self.oauth_waiting {
            return;
        }
        self.oauth_waiting = true;
        cx.notify();

        self.spawn_tokio(
            cx,
            async move { commands::start_official_oauth_login().await },
            move |store, start_res, cx| {
                if start_res.status != "ok" || start_res.payload.is_none() {
                    store.oauth_waiting = false;
                    store.set_notice(t("OAuth 登录"), start_res.message, "failed", cx);
                    return;
                }
                let session = start_res.payload.unwrap();
                let _ = commands::open_external_url(session.auth_url.clone());
                let login_id = session.login_id.clone();
                store.oauth_session = Some(session);
                cx.notify();

                store.spawn_tokio(
                    cx,
                    async move {
                        commands::complete_official_oauth_login(CompleteOfficialOAuthRequest {
                            login_id,
                            label,
                            set_active: true,
                        })
                        .await
                    },
                    |store, complete_res, cx| {
                        store.oauth_waiting = false;
                        if complete_res.status == "ok" {
                            store.oauth_session = None;
                            store.official_snapshot = Some(complete_res.payload);
                            store.refresh_settings_silent(cx);
                            store.set_notice(t("官方账号"), complete_res.message, "ok", cx);
                        } else {
                            cx.notify();
                        }
                    },
                );
            },
        );
    }

    pub fn submit_manual_oauth_callback(
        &mut self,
        callback_url: String,
        label: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let trimmed = callback_url.trim().to_string();
        if trimmed.is_empty() {
            self.set_notice(
                t("手动回调验证"),
                t("请粘贴浏览器回调 URL 或授权 code"),
                "failed",
                cx,
            );
            return;
        }
        self.spawn_tokio(
            cx,
            async move {
                commands::submit_official_oauth_callback(SubmitOfficialOAuthCallbackRequest {
                    callback_url: trimmed,
                    label,
                    set_active: true,
                })
                .await
            },
            |store, res, cx| {
                if res.status == "ok" {
                    store.oauth_session = None;
                    store.oauth_waiting = false;
                    store.official_snapshot = Some(res.payload);
                    store.refresh_settings_silent(cx);
                    store.set_notice(t("官方账号"), res.message, "ok", cx);
                } else {
                    store.set_notice(t("手动回调验证"), res.message, "failed", cx);
                }
            },
        );
    }

    pub fn cancel_official_oauth(&mut self, cx: &mut Context<Self>) {
        let _ = commands::cancel_official_oauth_login();
        self.oauth_session = None;
        self.oauth_waiting = false;
        cx.notify();
    }

    pub fn import_official_from_local(&mut self, cx: &mut Context<Self>) {
        if self.importing_official {
            return;
        }
        self.importing_official = true;
        cx.notify();
        self.spawn_tokio(
            cx,
            async move { commands::import_official_account_from_local(None).await },
            |store, res, cx| {
                store.importing_official = false;
                store.official_snapshot = Some(res.payload);
                store.set_notice(t("导入本机账号"), res.message, &res.status, cx);
            },
        );
    }

    pub fn import_official_from_text(
        &mut self,
        input: String,
        label: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let trimmed = input.trim().to_string();
        if trimmed.is_empty() {
            self.set_notice(
                t("导入官方账号"),
                t("请粘贴 JSON、auth.json 内容或 Refresh Token"),
                "failed",
                cx,
            );
            return;
        }
        if self.importing_official {
            return;
        }
        self.importing_official = true;
        cx.notify();
        self.spawn_tokio(
            cx,
            async move {
                commands::import_official_accounts(ImportOfficialAccountsRequest {
                    input: trimmed,
                    label,
                })
                .await
            },
            |store, res, cx| {
                store.importing_official = false;
                store.official_snapshot = Some(res.payload);
                if res.status == "ok" {
                    store.import_open = false;
                }
                store.set_notice(t("导入官方账号"), res.message, &res.status, cx);
            },
        );
    }

    pub fn export_official_accounts(&mut self, cx: &mut Context<Self>) {
        let res = commands::export_official_accounts(None);
        if res.status == "ok" {
            self.export_json_text = res.payload.clone();
            cx.write_to_clipboard(ClipboardItem::new_string(res.payload));
            self.set_notice(
                t("导出官方账号"),
                t("已导出全部官方账号 JSON 并复制到剪贴板。"),
                "ok",
                cx,
            );
        } else {
            self.set_notice(t("导出官方账号"), res.message, "failed", cx);
        }
    }

    pub fn switch_official_account(
        &mut self,
        account_id: String,
        restart_codex: bool,
        cx: &mut Context<Self>,
    ) {
        self.switching_account_id = Some(account_id.clone());
        self.use_confirm_account = None;
        cx.notify();
        self.spawn_tokio(
            cx,
            async move {
                let switch_res =
                    commands::switch_official_account(account_id, Some(restart_codex)).await;
                let overview_res = commands::load_overview().await;
                (switch_res, overview_res)
            },
            |store, (res, overview_res), cx| {
                store.switching_account_id = None;
                store.overview = Some(overview_res.payload);
                if res.status == "ok" {
                    if let Some(payload) = res.payload {
                        store.official_snapshot = Some(payload.accounts);
                        store.settings = payload.settings;
                    } else {
                        store.refresh_settings_silent(cx);
                    }
                    store.set_notice(t("切换官方账号"), res.message, "ok", cx);
                } else {
                    store.set_notice(t("切换官方账号"), res.message, "failed", cx);
                }
            },
        );
    }

    pub fn bind_relay_for_account(
        &mut self,
        account: OfficialAccountView,
        next_relay_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.open_binding_dropdown_for = None;
        let is_active = account.is_active;
        let acc_id = account.id.clone();
        let req = UpdateOfficialAccountRequest {
            account_id: account.id,
            label: account.label,
            bound_relay_profile_id: next_relay_id.filter(|s| !s.trim().is_empty()),
            proxy_mode: Some(account.proxy_mode),
            egress_proxy_url: account.egress_proxy_url,
        };
        let res = commands::update_official_account(req);
        self.official_snapshot = Some(res.payload);
        if res.status == "ok" {
            if is_active {
                self.switch_official_account(acc_id, false, cx);
            } else {
                self.set_notice(
                    t("绑定供应商"),
                    t("已保存该官方账号绑定的供应商。"),
                    "ok",
                    cx,
                );
            }
        } else {
            self.set_notice(t("绑定供应商"), res.message, "failed", cx);
        }
    }

    pub fn save_official_account_label(
        &mut self,
        account: OfficialAccountView,
        next_label: String,
        cx: &mut Context<Self>,
    ) {
        let trimmed = next_label.trim().to_string();
        let req = UpdateOfficialAccountRequest {
            account_id: account.id,
            label: (!trimmed.is_empty()).then_some(trimmed),
            bound_relay_profile_id: account.bound_relay_profile_id,
            proxy_mode: Some(account.proxy_mode),
            egress_proxy_url: account.egress_proxy_url,
        };
        let res = commands::update_official_account(req);
        self.official_snapshot = Some(res.payload);
        if res.status == "ok" {
            self.editing_account_id = None;
            self.set_notice(t("账号备注"), t("已更新官方账号备注。"), "ok", cx);
        } else {
            self.set_notice(t("账号备注"), res.message, "failed", cx);
        }
    }

    pub fn refresh_official_quota(&mut self, account_id: Option<String>, cx: &mut Context<Self>) {
        if self.refreshing_quota_id.is_some() {
            return;
        }
        self.refreshing_quota_id = Some(
            account_id
                .clone()
                .unwrap_or_else(|| "__all__".to_string()),
        );
        cx.notify();
        self.spawn_tokio(
            cx,
            async move { commands::refresh_official_account_quota(account_id).await },
            |store, res, cx| {
                store.refreshing_quota_id = None;
                store.official_snapshot = Some(res.payload);
                store.set_notice(t("刷新配额"), res.message, &res.status, cx);
            },
        );
    }

    pub fn delete_official_account(
        &mut self,
        account: OfficialAccountView,
        cx: &mut Context<Self>,
    ) {
        self.delete_confirm_account = None;
        let res = commands::delete_official_account(account.id);
        let empty = res.payload.accounts.is_empty();
        self.official_snapshot = Some(res.payload);
        if empty {
            self.official_page = 1;
        }
        self.refresh_settings_silent(cx);
        self.set_notice(t("删除官方账号"), res.message, &res.status, cx);
    }

    // ========================================================================
    // Codex 多开管理动作
    // ========================================================================

    pub fn refresh_multi_instances(&mut self, show_error_toast: bool, cx: &mut Context<Self>) {
        if self.multi_instances_loading {
            return;
        }
        self.multi_instances_loading = true;
        cx.notify();
        self.spawn_tokio(
            cx,
            async move { commands::list_codex_multi_instances().await },
            move |store, res, cx| {
                store.multi_instances_loading = false;
                store.multi_instances = res
                    .payload
                    .instances
                    .into_iter()
                    .filter(should_display_multi_instance)
                    .collect();
                if show_error_toast && res.status != "ok" {
                    store.set_notice(t("当前运行实例"), res.message, &res.status, cx);
                } else {
                    cx.notify();
                }
            },
        );
    }

    pub fn launch_multi_profile(&mut self, profile_id: String, cx: &mut Context<Self>) {
        if self.multi_launch_pending.is_some() {
            return;
        }
        self.multi_launch_pending = Some(profile_id.clone());
        cx.notify();

        let instance_id = profile_id.clone();
        self.spawn_tokio(
            cx,
            async move {
                let launch_res = commands::launch_codex_multi_instance(CodexMultiLaunchRequest {
                    settings: None,
                    profile_id,
                    instance_id: String::new(),
                    app_path: String::new(),
                    debug_port: 0,
                })
                .await;
                let (outcome, latest) = poll_multi_instance_in_tokio(&instance_id).await;
                (launch_res, outcome, latest)
            },
            |store, (launch_res, outcome, latest), cx| {
                store.multi_launch_pending = None;
                store.multi_instances = latest;
                match outcome {
                    MultiPollOutcome::Ready => {
                        store.set_notice(
                            t("Codex多开"),
                            t("Codex 多开实例已就绪。"),
                            "ok",
                            cx,
                        );
                    }
                    MultiPollOutcome::Failed => {
                        store.set_notice(
                            t("Codex多开"),
                            t("Codex 实例启动失败，请查看实例列表中的原因。"),
                            "failed",
                            cx,
                        );
                    }
                    MultiPollOutcome::Pending => {
                        store.set_notice(t("Codex多开"), launch_res.message, &launch_res.status, cx);
                    }
                }
            },
        );
    }

    pub fn launch_multi_official_account(
        &mut self,
        account_id: String,
        cx: &mut Context<Self>,
    ) {
        if self.multi_launch_pending.is_some() {
            return;
        }
        let profile_key = format!("official:{account_id}");
        let instance_key = format!("official-{account_id}");
        self.multi_launch_pending = Some(profile_key.clone());
        cx.notify();

        self.spawn_tokio(
            cx,
            async move {
                let launch_res = commands::launch_codex_multi_instance(CodexMultiLaunchRequest {
                    settings: None,
                    profile_id: profile_key,
                    instance_id: instance_key.clone(),
                    app_path: String::new(),
                    debug_port: 0,
                })
                .await;
                let (outcome, latest) = poll_multi_instance_in_tokio(&instance_key).await;
                (launch_res, outcome, latest)
            },
            |store, (launch_res, outcome, latest), cx| {
                store.multi_launch_pending = None;
                store.multi_instances = latest;
                match outcome {
                    MultiPollOutcome::Ready => {
                        store.set_notice(
                            t("Codex多开"),
                            t("官方账号多开实例已就绪。"),
                            "ok",
                            cx,
                        );
                    }
                    MultiPollOutcome::Failed => {
                        store.set_notice(
                            t("Codex多开"),
                            t("Codex 实例启动失败，请查看实例列表中的原因。"),
                            "failed",
                            cx,
                        );
                    }
                    MultiPollOutcome::Pending => {
                        store.set_notice(t("Codex多开"), launch_res.message, &launch_res.status, cx);
                    }
                }
            },
        );
    }

    pub fn restart_multi_instance(&mut self, instance_id: String, cx: &mut Context<Self>) {
        if self.multi_action_pending.is_some() {
            return;
        }
        self.multi_action_pending = Some(instance_id.clone());
        cx.notify();

        self.spawn_tokio(
            cx,
            async move {
                let res = commands::restart_codex_multi_instance(CodexMultiInstanceRequest {
                    instance_id,
                })
                .await;
                let list_res = commands::list_codex_multi_instances().await;
                (res, list_res)
            },
            |store, (res, list_res), cx| {
                store.multi_action_pending = None;
                store.multi_instances = list_res
                    .payload
                    .instances
                    .into_iter()
                    .filter(should_display_multi_instance)
                    .collect();
                store.set_notice(t("重启 Codex 实例"), res.message, &res.status, cx);
            },
        );
    }

    pub fn stop_multi_instance(&mut self, instance_id: String, cx: &mut Context<Self>) {
        if self.multi_action_pending.is_some() {
            return;
        }
        self.multi_action_pending = Some(instance_id.clone());
        cx.notify();

        self.spawn_tokio(
            cx,
            async move {
                let res = commands::stop_codex_multi_instance(CodexMultiInstanceRequest {
                    instance_id,
                })
                .await;
                let list_res = commands::list_codex_multi_instances().await;
                (res, list_res)
            },
            |store, (res, list_res), cx| {
                store.multi_action_pending = None;
                store.multi_instances = list_res
                    .payload
                    .instances
                    .into_iter()
                    .filter(should_display_multi_instance)
                    .collect();
                store.set_notice(t("结束 Codex 实例"), res.message, &res.status, cx);
            },
        );
    }

    // ========================================================================
    // 供应商切换与 Profile 详情编辑动作 (Phase 3)
    // ========================================================================

    pub fn refresh_relay_page(&mut self, show_toast: bool, cx: &mut Context<Self>) {
        self.refresh_settings_silent(cx);
        self.official_snapshot = Some(commands::load_official_accounts().payload);
        self.env_conflicts = commands::check_env_conflicts().payload.conflicts;
        self.pending_provider_import = commands::load_pending_provider_import().payload.pending;
        let files_res = commands::read_relay_files();
        if files_res.status == "ok" {
            self.relay_files = Some(files_res.payload);
        }
        if show_toast {
            self.set_notice(t("供应商列表"), t("已同步最新供应商配置状态。"), "ok", cx);
        } else {
            cx.notify();
        }
    }

    pub fn set_relay_master_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.settings.relay_profiles_enabled = enabled;
        let saved = commands::save_settings(self.settings.clone());
        if saved.status == "ok" {
            self.settings = saved.payload.settings;
            let msg = if enabled {
                t("已开启供应商配置切换。")
            } else {
                t("已关闭供应商配置切换（仅保存设置，不再改写 Codex live 文件）。")
            };
            self.set_notice(t("启用供应商配置切换"), msg, "ok", cx);
        } else {
            self.set_notice(t("启用供应商配置切换"), saved.message, "failed", cx);
        }
    }

    pub fn remove_all_env_conflicts(&mut self, cx: &mut Context<Self>) {
        let names: Vec<String> = self
            .env_conflicts
            .iter()
            .map(|c| c.name.clone())
            .collect();
        if names.is_empty() {
            return;
        }
        let res = commands::remove_env_conflicts(RemoveEnvConflictsRequest { names });
        self.env_conflicts = res.payload.remaining;
        self.set_notice(t("环境变量冲突"), res.message, &res.status, cx);
    }

    pub fn toggle_ccs_import(&mut self, cx: &mut Context<Self>) {
        self.ccs_import_open = !self.ccs_import_open;
        if self.ccs_import_open {
            let res = commands::load_ccs_providers();
            self.ccs_providers = Some(res.payload);
        }
        cx.notify();
    }

    pub fn refresh_ccs_providers(&mut self, cx: &mut Context<Self>) {
        let res = commands::load_ccs_providers();
        let status = res.status.clone();
        let msg = res.message.clone();
        self.ccs_providers = Some(res.payload);
        self.set_notice("cc-switch", msg, &status, cx);
    }

    pub fn save_ccs_db_path(&mut self, next_path: String, cx: &mut Context<Self>) {
        self.settings.ccs_db_path = next_path.trim().to_string();
        let saved = commands::save_settings(self.settings.clone());
        if saved.status == "ok" {
            self.settings = saved.payload.settings;
            let res = commands::load_ccs_providers();
            let status = res.status.clone();
            let msg = res.message.clone();
            self.ccs_providers = Some(res.payload);
            self.set_notice("cc-switch", msg, &status, cx);
        } else {
            self.set_notice("cc-switch", saved.message, "failed", cx);
        }
    }

    pub fn import_ccs_providers_now(&mut self, cx: &mut Context<Self>) {
        let res = commands::import_ccs_providers();
        if res.status == "ok" {
            self.settings = res.payload.settings;
            self.ccs_import_open = false;
            self.tool_entries = commands::list_tools().payload.tools;
            self.set_notice(t("从第三方导入"), res.message, "ok", cx);
        } else {
            self.set_notice(t("从第三方导入"), res.message, "failed", cx);
        }
    }

    pub fn confirm_pending_url_import(&mut self, cx: &mut Context<Self>) {
        let res = commands::confirm_pending_provider_import();
        self.pending_provider_import = None;
        if res.status == "ok" {
            self.settings = res.payload.settings;
            self.tool_entries = commands::list_tools().payload.tools;
            self.set_notice(t("导入供应商"), res.message, "ok", cx);
        } else {
            self.set_notice(t("导入供应商"), res.message, "failed", cx);
        }
    }

    pub fn dismiss_pending_url_import(&mut self, cx: &mut Context<Self>) {
        let _ = commands::dismiss_pending_provider_import();
        self.pending_provider_import = None;
        cx.notify();
    }

    pub fn open_relay_catalog(&mut self, cx: &mut Context<Self>) {
        self.relay_catalog_query.clear();
        self.relay_subscreen = RelaySubscreen::Catalog;
        self.doctor_result = None;
        cx.notify();
    }

    pub fn back_to_relay_list(&mut self, cx: &mut Context<Self>) {
        self.relay_subscreen = RelaySubscreen::List;
        self.doctor_result = None;
        cx.notify();
    }

    fn seed_editor(
        &mut self,
        profile: RelayProfile,
        aggregate: Option<AggregateRelayProfile>,
        is_new: bool,
        cx: &mut Context<Self>,
    ) {
        self.relay_editor_seq = self.relay_editor_seq.wrapping_add(1);
        let id = profile.id.clone();
        self.relay_editor_seed = Some(RelayEditorSeed {
            seq: self.relay_editor_seq,
            profile,
            aggregate,
            is_new,
        });
        self.doctor_result = None;
        self.relay_subscreen = RelaySubscreen::Editor {
            profile_id: id,
            is_new,
        };
        let files_res = commands::read_relay_files();
        if files_res.status == "ok" {
            self.relay_files = Some(files_res.payload);
        }
        cx.notify();
    }

    pub fn open_new_custom_relay(&mut self, cx: &mut Context<Self>) {
        let draft = create_custom_relay_profile(&self.settings);
        self.seed_editor(draft, None, true, cx);
    }

    pub fn open_new_preset_relay(&mut self, preset_index: usize, cx: &mut Context<Self>) {
        let Some(preset) = PROVIDER_PRESETS.get(preset_index) else {
            return;
        };
        let draft = create_preset_relay_profile(&self.settings, preset);
        self.seed_editor(draft, None, true, cx);
    }

    pub fn open_new_aggregate_relay(&mut self, model_aggregation: bool, cx: &mut Context<Self>) {
        let (profile, agg) = create_aggregate_draft(&self.settings, model_aggregation);
        self.seed_editor(profile, Some(agg), true, cx);
    }

    pub fn open_edit_relay(&mut self, profile_id: String, cx: &mut Context<Self>) {
        let Some(mut profile) = self
            .settings
            .relay_profiles
            .iter()
            .find(|p| p.id == profile_id)
            .cloned()
        else {
            return;
        };
        let agg = self
            .settings
            .aggregate_relay_profiles
            .iter()
            .find(|a| a.id == profile_id)
            .cloned();

        // 若正在编辑当前生效的非聚合供应商，优先同步 live config.toml 内容以便预览一致
        if profile.id == self.settings.active_relay_id && profile.relay_mode != RelayMode::Aggregate
        {
            let files_res = commands::read_relay_files();
            if files_res.status == "ok" {
                if !files_res.payload.config_contents.trim().is_empty() {
                    profile.config_contents = files_res.payload.config_contents.clone();
                }
                self.relay_files = Some(files_res.payload);
            }
        }
        self.seed_editor(profile, agg, false, cx);
    }

    pub fn move_relay_profile(
        &mut self,
        profile_id: String,
        move_up: bool,
        cx: &mut Context<Self>,
    ) {
        let mut next = self.settings.clone();
        if !move_relay_profile_in_view(&mut next, &profile_id, move_up, self.relay_view_tab) {
            return;
        }
        let saved = commands::save_settings(next);
        if saved.status == "ok" {
            self.settings = saved.payload.settings;
            cx.notify();
        }
    }

    pub fn duplicate_relay(&mut self, profile_id: String, cx: &mut Context<Self>) {
        let mut next = self.settings.clone();
        if duplicate_relay_profile(&mut next, &profile_id).is_none() {
            return;
        }
        let saved = commands::save_settings(next);
        if saved.status == "ok" {
            self.settings = saved.payload.settings;
            self.tool_entries = commands::list_tools().payload.tools;
            self.set_notice(t("复制"), t("已创建供应商配置副本。"), "ok", cx);
        } else {
            self.set_notice(t("复制"), saved.message, "failed", cx);
        }
    }

    pub fn delete_relay(&mut self, profile_id: String, cx: &mut Context<Self>) {
        self.delete_confirm_relay_id = None;
        let mut next = self.settings.clone();
        if !remove_relay_profile(&mut next, &profile_id) {
            self.set_notice(
                t("删除供应商"),
                t("至少需要保留一个供应商配置。"),
                "failed",
                cx,
            );
            return;
        }
        let saved = commands::save_settings(next);
        if saved.status == "ok" {
            self.settings = saved.payload.settings;
            self.tool_entries = commands::list_tools().payload.tools;
            self.set_notice(t("删除供应商"), t("供应商已删除。"), "ok", cx);
        } else {
            self.set_notice(t("删除供应商"), saved.message, "failed", cx);
        }
    }

    pub fn switch_relay_profile(
        &mut self,
        target_id: String,
        restart_codex: bool,
        cx: &mut Context<Self>,
    ) {
        if self.relay_switching {
            return;
        }
        self.use_confirm_relay_id = None;
        self.relay_switching = true;

        let prev_id = self.settings.active_relay_id.clone();
        let mut next_settings = self.settings.clone();
        next_settings.relay_profiles_enabled = true;
        next_settings.active_relay_id = target_id;
        let app_path = next_settings.codex_app_path.clone();
        cx.notify();

        self.spawn_tokio(
            cx,
            async move {
                let switch_res = tokio::task::spawn_blocking(move || {
                    commands::switch_relay_profile(RelayProfileSwitchRequest {
                        settings: next_settings,
                        previous_active_relay_id: prev_id,
                    })
                })
                .await;

                if restart_codex
                    && let Ok(ref res) = switch_res
                    && res.status == "ok"
                {
                    let _ = commands::restart_chatgptx(LaunchRequest {
                        app_path,
                        debug_port: 9229,
                        helper_port: 57321,
                        sync_active_relay: true,
                    })
                    .await;
                }
                let overview_res = commands::load_overview().await;
                (switch_res, overview_res)
            },
            |store, (join_res, overview_res), cx| {
                store.relay_switching = false;
                store.overview = Some(overview_res.payload);
                store.official_snapshot = Some(commands::load_official_accounts().payload);
                let files_res = commands::read_relay_files();
                if files_res.status == "ok" {
                    store.relay_files = Some(files_res.payload);
                }
                if let Ok(res) = join_res {
                    if res.status == "ok" {
                        store.settings = res.payload.settings;
                    }
                    store.set_notice(t("切换供应商配置"), res.message, &res.status, cx);
                } else {
                    store.set_notice(
                        t("切换供应商配置"),
                        t("后台供应商切换任务异常中断"),
                        "failed",
                        cx,
                    );
                }
            },
        );
    }

    pub fn save_edited_relay_profile(
        &mut self,
        draft: RelayProfile,
        aggregate: Option<AggregateRelayProfile>,
        is_new: bool,
        switch_after_save: bool,
        cx: &mut Context<Self>,
    ) {
        let mut next_settings = self.settings.clone();
        let target_id = draft.id.clone();
        upsert_relay_profile_into_settings(&mut next_settings, draft, aggregate.clone());

        let agg_map: std::collections::HashMap<String, AggregateRelayProfile> = next_settings
            .aggregate_relay_profiles
            .iter()
            .map(|a| (a.id.clone(), a.clone()))
            .collect();
        if let Some(saved_p) = next_settings
            .relay_profiles
            .iter()
            .find(|p| p.id == target_id)
        {
            self.relay_view_tab = relay_provider_view(saved_p, &agg_map);
        }

        let saved = commands::save_settings(next_settings);
        if saved.status != "ok" {
            self.set_notice(t("保存供应商"), saved.message, "failed", cx);
            return;
        }
        self.settings = saved.payload.settings;
        self.tool_entries = commands::list_tools().payload.tools;

        let should_apply_live = switch_after_save
            || (!is_new
                && self.settings.active_relay_id == target_id
                && self.settings.relay_profiles_enabled);

        self.relay_subscreen = RelaySubscreen::List;
        self.doctor_result = None;

        if should_apply_live {
            self.switch_relay_profile(target_id, false, cx);
        } else {
            self.set_notice(t("保存供应商"), t("供应商配置已保存。"), "ok", cx);
        }
    }

    pub fn test_relay_profile(&mut self, profile: RelayProfile, cx: &mut Context<Self>) {
        if self.testing_profile_id.is_some() {
            return;
        }
        self.testing_profile_id = Some(profile.id.clone());
        let name = if profile.name.trim().is_empty() {
            t("未命名供应商")
        } else {
            profile.name.clone()
        };
        cx.notify();

        self.spawn_tokio(
            cx,
            async move { commands::test_relay_profile(profile).await },
            move |store, res, cx| {
                store.testing_profile_id = None;
                let detail = if res.payload.response_preview.trim().is_empty() {
                    res.message
                } else {
                    let preview: String = res.payload.response_preview.chars().take(120).collect();
                    format!("{} · {}", res.message, preview)
                };
                store.set_notice(
                    format!("{} ({name})", t("发送 hi 测试")),
                    detail,
                    &res.status,
                    cx,
                );
            },
        );
    }

    pub fn diagnose_relay_profile(&mut self, profile: RelayProfile, cx: &mut Context<Self>) {
        if self.doctor_running {
            return;
        }
        self.doctor_running = true;
        self.doctor_result = None;
        cx.notify();

        self.spawn_tokio(
            cx,
            async move { commands::diagnose_relay_profile(profile).await },
            |store, res, cx| {
                store.doctor_running = false;
                store.doctor_result = Some(res.payload);
                if res.status != "ok" {
                    store.set_notice(t("诊断供应商"), res.message, &res.status, cx);
                } else {
                    cx.notify();
                }
            },
        );
    }

    pub fn fetch_upstream_models(&mut self, profile: RelayProfile, cx: &mut Context<Self>) {
        if self.fetching_models {
            return;
        }
        self.fetching_models = true;
        cx.notify();

        self.spawn_tokio(
            cx,
            async move { commands::fetch_relay_profile_models(profile).await },
            |store, res, cx| {
                store.fetching_models = false;
                if res.status == "ok" {
                    store.fetched_models_seq = store.fetched_models_seq.wrapping_add(1);
                    store.last_fetched_models =
                        Some((store.fetched_models_seq, res.payload.models));
                }
                store.set_notice(t("获取上游模型"), res.message, &res.status, cx);
            },
        );
    }

    pub fn bind_official_to_relay(
        &mut self,
        relay_profile_id: String,
        official_account_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let res = commands::bind_relay_official_account(BindRelayOfficialAccountRequest {
            relay_profile_id,
            official_account_id: official_account_id.filter(|s| !s.trim().is_empty()),
        });
        self.official_snapshot = Some(res.payload);
        self.set_notice(t("绑定官方账号"), res.message, &res.status, cx);
    }

    pub fn save_live_config_file(&mut self, contents: String, cx: &mut Context<Self>) {
        let res = commands::save_relay_file(SaveRelayFileRequest {
            kind: "config".to_string(),
            contents,
        });
        let status = res.status.clone();
        let msg = res.message.clone();
        if status == "ok" {
            self.relay_files = Some(res.payload);
            self.refresh_settings_silent(cx);
        }
        self.set_notice("config.toml", msg, &status, cx);
    }

    // ========================================================================
    // Phase 4A：MCP 管理动作
    // ========================================================================

    pub fn refresh_mcp_page(&mut self, show_toast: bool, cx: &mut Context<Self>) {
        self.refresh_settings_silent(cx);
        let managed = commands::list_context_entries(ContextSettingsRequest {
            settings: self.settings.clone(),
        });
        self.mcp_entries = managed.payload.entries.mcp_servers;
        let live = commands::read_live_context_entries();
        self.mcp_live_entries = live.payload.entries.mcp_servers;
        if show_toast {
            self.set_notice("MCP", managed.message, &managed.status, cx);
        } else {
            cx.notify();
        }
    }

    pub fn open_mcp_editor(
        &mut self,
        id: String,
        toml_body: String,
        is_new: bool,
        cx: &mut Context<Self>,
    ) {
        self.mcp_editor_seq = self.mcp_editor_seq.wrapping_add(1);
        self.mcp_editor_seed = Some((self.mcp_editor_seq, id, toml_body, is_new));
        self.mcp_editor_open = true;
        self.mcp_import_open = false;
        cx.notify();
    }

    pub fn upsert_mcp_entry(&mut self, id: String, toml_body: String, cx: &mut Context<Self>) {
        let trimmed_id = id.trim().to_string();
        if trimmed_id.is_empty() {
            self.set_notice("MCP", t("请填写 MCP 服务器标识 (ID)。"), "failed", cx);
            return;
        }
        let res = commands::upsert_context_entry(ContextEntryRequest {
            settings: self.settings.clone(),
            kind: "mcp".to_string(),
            id: trimmed_id,
            toml_body,
        });
        if res.status != "ok" {
            self.set_notice("MCP", res.message, "failed", cx);
            return;
        }
        let saved = commands::save_settings(res.payload.settings);
        if saved.status == "ok" {
            self.settings = saved.payload.settings;
            self.mcp_entries = res.payload.entries.mcp_servers;
            let sync_res = commands::sync_live_context_entries(ContextSettingsRequest {
                settings: self.settings.clone(),
            });
            self.mcp_live_entries = sync_res.payload.entries.mcp_servers;
            self.mcp_editor_open = false;
            self.set_notice("MCP", t("MCP 服务器已保存并同步至 config.toml。"), "ok", cx);
        } else {
            self.set_notice("MCP", saved.message, "failed", cx);
        }
    }

    pub fn toggle_mcp_entry_enabled(
        &mut self,
        entry: CodexContextEntry,
        enabled: bool,
        cx: &mut Context<Self>,
    ) {
        let mut form = commands::parse_mcp_entry(entry.toml_body.clone())
            .payload
            .form;
        form.enabled = enabled;
        let built = commands::build_mcp_entry(form);
        if built.status != "ok" {
            self.set_notice("MCP", built.message, "failed", cx);
            return;
        }
        self.upsert_mcp_entry(entry.id, built.payload.toml_body, cx);
    }

    pub fn delete_mcp_entry(&mut self, id: String, cx: &mut Context<Self>) {
        let res = commands::delete_context_entry(ContextDeleteRequest {
            settings: self.settings.clone(),
            kind: "mcp".to_string(),
            id,
        });
        if res.status != "ok" {
            self.set_notice("MCP", res.message, "failed", cx);
            return;
        }
        let saved = commands::save_settings(res.payload.settings);
        if saved.status == "ok" {
            self.settings = saved.payload.settings;
            self.mcp_entries = res.payload.entries.mcp_servers;
            let sync_res = commands::sync_live_context_entries(ContextSettingsRequest {
                settings: self.settings.clone(),
            });
            self.mcp_live_entries = sync_res.payload.entries.mcp_servers;
            self.set_notice("MCP", t("已删除 MCP 服务器并同步。"), "ok", cx);
        } else {
            self.set_notice("MCP", saved.message, "failed", cx);
        }
    }

    pub fn sync_mcp_to_live(&mut self, cx: &mut Context<Self>) {
        let res = commands::sync_live_context_entries(ContextSettingsRequest {
            settings: self.settings.clone(),
        });
        let status = res.status.clone();
        let msg = res.message.clone();
        self.mcp_live_entries = res.payload.entries.mcp_servers;
        self.set_notice("MCP", msg, &status, cx);
    }

    pub fn preview_mcp_json(&mut self, json_text: String, cx: &mut Context<Self>) {
        let res = commands::preview_mcp_servers_json(json_text);
        let status = res.status.clone();
        let msg = res.message.clone();
        self.mcp_import_preview = Some(res.payload);
        self.set_notice("MCP JSON", msg, &status, cx);
    }

    pub fn import_mcp_json(&mut self, json_text: String, cx: &mut Context<Self>) {
        let res = commands::import_mcp_servers_json(McpImportRequest {
            settings: self.settings.clone(),
            json: json_text,
        });
        if res.status != "ok" {
            self.set_notice("MCP JSON", res.message, "failed", cx);
            return;
        }
        let msg = res.message.clone();
        let saved = commands::save_settings(res.payload.settings);
        if saved.status == "ok" {
            self.settings = saved.payload.settings;
            self.mcp_entries = res.payload.entries.mcp_servers;
            let sync_res = commands::sync_live_context_entries(ContextSettingsRequest {
                settings: self.settings.clone(),
            });
            self.mcp_live_entries = sync_res.payload.entries.mcp_servers;
            self.mcp_import_open = false;
            self.mcp_import_preview = None;
            self.set_notice("MCP JSON", msg, "ok", cx);
        } else {
            self.set_notice("MCP JSON", saved.message, "failed", cx);
        }
    }

    // ========================================================================
    // Phase 4B：Skills 技能管理动作
    // ========================================================================

    pub fn load_local_skills(&mut self, cx: &mut Context<Self>) {
        let res = commands::list_installed_skills();
        self.skills_payload = Some(res.payload);
        cx.notify();
    }

    pub fn refresh_skills_remote(&mut self, cx: &mut Context<Self>) {
        if self.skills_loading {
            return;
        }
        self.skills_loading = true;
        cx.notify();
        self.spawn_tokio(
            cx,
            async move { commands::refresh_skill_catalog().await },
            |store, res, cx| {
                store.skills_loading = false;
                let status = res.status.clone();
                let msg = res.message.clone();
                store.skills_payload = Some(res.payload);
                store.set_notice("Skills", msg, &status, cx);
            },
        );
    }

    pub fn install_or_update_skill(
        &mut self,
        repo_key: String,
        id: String,
        is_update: bool,
        cx: &mut Context<Self>,
    ) {
        if self.skills_busy_id.is_some() {
            return;
        }
        self.skills_busy_id = Some(id.clone());
        cx.notify();
        self.spawn_tokio(
            cx,
            async move {
                if is_update {
                    commands::update_skill(repo_key, id).await
                } else {
                    commands::install_skill(repo_key, id).await
                }
            },
            |store, res, cx| {
                store.skills_busy_id = None;
                let status = res.status.clone();
                let msg = res.message.clone();
                store.skills_payload = Some(res.payload);
                store.set_notice("Skills", msg, &status, cx);
            },
        );
    }

    pub fn toggle_skill_enabled(&mut self, id: String, enabled: bool, cx: &mut Context<Self>) {
        let res = commands::set_skill_enabled(id, enabled);
        let status = res.status.clone();
        let msg = res.message.clone();
        self.skills_payload = Some(res.payload);
        self.set_notice("Skills", msg, &status, cx);
    }

    pub fn uninstall_skill_by_id(&mut self, id: String, cx: &mut Context<Self>) {
        let res = commands::uninstall_skill(id);
        let status = res.status.clone();
        let msg = res.message.clone();
        self.skills_payload = Some(res.payload);
        self.set_notice("Skills", msg, &status, cx);
    }

    pub fn restore_skill_from_backup(&mut self, backup_id: String, cx: &mut Context<Self>) {
        let res = commands::restore_skill_backup(backup_id);
        let status = res.status.clone();
        let msg = res.message.clone();
        self.skills_payload = Some(res.payload);
        self.set_notice("Skills", msg, &status, cx);
    }

    pub fn delete_skill_backup_by_id(&mut self, backup_id: String, cx: &mut Context<Self>) {
        let res = commands::delete_skill_backup(backup_id);
        let status = res.status.clone();
        let msg = res.message.clone();
        self.skills_payload = Some(res.payload);
        self.set_notice("Skills", msg, &status, cx);
    }

    pub fn upsert_skill_repo_entry(&mut self, repo: SkillRepo, cx: &mut Context<Self>) {
        let res = commands::upsert_skill_repo(repo);
        let status = res.status.clone();
        let msg = res.message.clone();
        self.skills_payload = Some(res.payload);
        self.set_notice("Skills", msg, &status, cx);
    }

    pub fn delete_skill_repo_by_key(&mut self, key: String, cx: &mut Context<Self>) {
        let res = commands::delete_skill_repo(key);
        let status = res.status.clone();
        let msg = res.message.clone();
        self.skills_payload = Some(res.payload);
        self.set_notice("Skills", msg, &status, cx);
    }

    // ========================================================================
    // Phase 4C：本地会话管理动作
    // ========================================================================

    pub fn refresh_sessions(
        &mut self,
        append_more: bool,
        show_toast: bool,
        cx: &mut Context<Self>,
    ) {
        if self.sessions_loading {
            return;
        }
        self.sessions_loading = true;
        let offset = if append_more {
            self.sessions_payload
                .as_ref()
                .map(|p| p.sessions.len())
                .unwrap_or(0)
        } else {
            0
        };
        cx.notify();

        self.spawn_tokio(
            cx,
            async move {
                tokio::task::spawn_blocking(move || {
                    commands::list_local_sessions(Some(ListLocalSessionsRequest {
                        offset,
                        limit: 100,
                    }))
                })
                .await
            },
            move |store, join_res, cx| {
                store.sessions_loading = false;
                if let Ok(res) = join_res {
                    let status = res.status.clone();
                    let msg = res.message.clone();
                    if append_more
                        && let Some(prev) = &mut store.sessions_payload
                    {
                        for item in res.payload.sessions {
                            if !prev.sessions.iter().any(|s| s.id == item.id) {
                                prev.sessions.push(item);
                            }
                        }
                        prev.has_more = res.payload.has_more;
                        prev.total_count = res.payload.total_count;
                    } else {
                        store.sessions_payload = Some(res.payload);
                    }
                    if show_toast {
                        store.set_notice(t("会话管理"), msg, &status, cx);
                    } else {
                        cx.notify();
                    }
                } else {
                    cx.notify();
                }
            },
        );
    }

    pub fn confirm_delete_session(&mut self, session: LocalSession, cx: &mut Context<Self>) {
        self.session_delete_confirm = None;
        let target_id = session.id.clone();
        let req = DeleteLocalSessionRequest {
            session_id: session.id,
            title: session.title,
            db_path: (!session.db_path.trim().is_empty()).then_some(session.db_path),
        };
        let res = commands::delete_local_session(req);
        if res.status == "ok" {
            if let Some(payload) = &mut self.sessions_payload {
                payload.sessions.retain(|s| s.id != target_id);
                payload.total_count = payload.total_count.saturating_sub(1);
            }
        }
        self.set_notice(t("删除会话"), res.message, &res.status, cx);
    }

    pub fn run_repair_session_index(&mut self, cx: &mut Context<Self>) {
        if self.session_repair_busy {
            return;
        }
        self.session_repair_busy = true;
        cx.notify();
        self.spawn_tokio(
            cx,
            async move { commands::repair_session_index().await },
            |store, res, cx| {
                store.session_repair_busy = false;
                store.refresh_sessions(false, false, cx);
                store.set_notice(t("修复会话索引"), res.message, &res.status, cx);
            },
        );
    }

    pub fn preview_stale_session_index(&mut self, cx: &mut Context<Self>) {
        if self.session_repair_busy {
            return;
        }
        self.session_repair_busy = true;
        cx.notify();
        self.spawn_tokio(
            cx,
            async move { commands::preview_session_index_cleanup().await },
            |store, res, cx| {
                store.session_repair_busy = false;
                if res.status == "ok"
                    && let Ok(parsed) =
                        serde_json::from_value::<SessionIndexCleanupPreview>(res.payload)
                {
                    store.session_cleanup_preview = Some(parsed);
                }
                store.set_notice(t("检查失效索引"), res.message, &res.status, cx);
            },
        );
    }

    pub fn apply_stale_session_cleanup(&mut self, cx: &mut Context<Self>) {
        let Some(preview) = self.session_cleanup_preview.take() else {
            return;
        };
        if preview.candidates.is_empty() {
            cx.notify();
            return;
        }
        self.session_repair_busy = true;
        let sha = preview.snapshot_sha256;
        let ids: Vec<String> = preview.candidates.into_iter().map(|c| c.id).collect();
        cx.notify();
        self.spawn_tokio(
            cx,
            async move { commands::apply_session_index_cleanup(sha, ids).await },
            |store, res, cx| {
                store.session_repair_busy = false;
                store.refresh_sessions(false, false, cx);
                store.set_notice(t("清理失效索引"), res.message, &res.status, cx);
            },
        );
    }

    // ========================================================================
    // Phase 5A：Codex 增强动作 (Enhance)
    // ========================================================================

    pub fn refresh_enhance_page(&mut self, cx: &mut Context<Self>) {
        self.refresh_settings_silent(cx);
        self.sensevoice_status = commands::dictation_local_model_status();
        cx.notify();
        self.spawn_tokio(
            cx,
            async move { commands::native_browser_status().await },
            |store, diag, cx| {
                store.native_browser_diag = Some(diag);
                cx.notify();
            },
        );
    }

    pub fn persist_settings_with_notice(
        &mut self,
        next: BackendSettings,
        title: impl Into<String>,
        cx: &mut Context<Self>,
    ) {
        let saved = commands::save_settings(next);
        if saved.status == "ok" {
            self.settings = saved.payload.settings;
            self.user_scripts_inventory = parse_user_script_inventory(&saved.payload.user_scripts);
            self.set_notice(title, saved.message, "ok", cx);
        } else {
            self.set_notice(title, saved.message, "failed", cx);
        }
    }

    pub fn download_sensevoice_model(&mut self, cx: &mut Context<Self>) {
        if self.sensevoice_downloading {
            return;
        }
        self.sensevoice_downloading = true;
        cx.notify();
        self.spawn_tokio(
            cx,
            async move { commands::download_dictation_local_model().await },
            |store, res, cx| {
                store.sensevoice_downloading = false;
                store.sensevoice_status = commands::dictation_local_model_status();
                store.set_notice(t("离线语音模型"), res.message, &res.status, cx);
            },
        );
    }

    pub fn test_stepwise_now(&mut self, draft: BackendSettings, cx: &mut Context<Self>) {
        if self.stepwise_testing {
            return;
        }
        self.stepwise_testing = true;
        cx.notify();
        self.spawn_tokio(
            cx,
            async move { commands::test_stepwise_settings(draft).await },
            |store, res, cx| {
                store.stepwise_testing = false;
                store.set_notice(t("下一步建议测试"), res.message, &res.status, cx);
            },
        );
    }

    pub fn reset_image_overlay(&mut self, cx: &mut Context<Self>) {
        let res = commands::reset_image_overlay_settings();
        if res.status == "ok" {
            self.settings = res.payload.settings;
        }
        self.set_notice(t("背景图层"), res.message, &res.status, cx);
    }

    // ========================================================================
    // Phase 5B：用户脚本与脚本市场动作 (UserScripts)
    // ========================================================================

    pub fn refresh_user_scripts_page(&mut self, show_toast: bool, cx: &mut Context<Self>) {
        let load_market = self.market_scripts.is_empty() || show_toast;
        if load_market {
            self.script_market_loading = true;
        }
        cx.notify();

        self.spawn_tokio(
            cx,
            async move {
                let inv_res = commands::refresh_user_script_inventory().await;
                let mkt_res = if load_market {
                    Some(commands::refresh_script_market().await)
                } else {
                    None
                };
                (inv_res, mkt_res)
            },
            move |store, (inv_res, mkt_res), cx| {
                store.script_market_loading = false;
                store.settings = inv_res.payload.settings;
                store.user_scripts_inventory =
                    parse_user_script_inventory(&inv_res.payload.user_scripts);
                if let Some(m) = mkt_res {
                    let parsed = parse_script_market_list(&m.payload.market);
                    if !parsed.is_empty() {
                        store.market_scripts = parsed;
                    }
                }
                if show_toast {
                    store.set_notice(t("拓展"), inv_res.message, &inv_res.status, cx);
                } else {
                    cx.notify();
                }
            },
        );
    }

    pub fn set_user_scripts_master(&mut self, enabled: bool, cx: &mut Context<Self>) {
        let res = commands::set_user_scripts_enabled(enabled);
        if res.status == "ok" {
            self.settings = res.payload.settings;
            self.user_scripts_inventory = parse_user_script_inventory(&res.payload.user_scripts);
        }
        self.set_notice(t("拓展"), res.message, &res.status, cx);
    }

    pub fn set_single_user_script(&mut self, key: String, enabled: bool, cx: &mut Context<Self>) {
        let res = commands::set_user_script_enabled(key, enabled);
        if res.status == "ok" {
            self.settings = res.payload.settings;
            self.user_scripts_inventory = parse_user_script_inventory(&res.payload.user_scripts);
        }
        self.set_notice(t("拓展"), res.message, &res.status, cx);
    }

    pub fn reload_live_user_scripts(&mut self, cx: &mut Context<Self>) {
        self.spawn_tokio(
            cx,
            async move { commands::reload_user_scripts().await },
            |store, res, cx| {
                store.settings = res.payload.settings;
                store.user_scripts_inventory =
                    parse_user_script_inventory(&res.payload.user_scripts);
                store.set_notice(t("重载脚本"), res.message, &res.status, cx);
            },
        );
    }

    pub fn install_market_user_script(&mut self, id: String, cx: &mut Context<Self>) {
        if self.script_installing_id.is_some() {
            return;
        }
        self.script_installing_id = Some(id.clone());
        cx.notify();
        self.spawn_tokio(
            cx,
            async move { commands::install_market_script(id).await },
            |store, res, cx| {
                store.script_installing_id = None;
                store.user_scripts_inventory =
                    parse_user_script_inventory(&res.payload.user_scripts);
                let parsed = parse_script_market_list(&res.payload.market);
                if !parsed.is_empty() {
                    store.market_scripts = parsed;
                }
                store.set_notice(t("脚本市场"), res.message, &res.status, cx);
            },
        );
    }

    pub fn delete_local_user_script(&mut self, key: String, cx: &mut Context<Self>) {
        let res = commands::delete_user_script(key);
        if res.status == "ok" {
            self.settings = res.payload.settings;
            self.user_scripts_inventory = parse_user_script_inventory(&res.payload.user_scripts);
        }
        self.set_notice(t("卸载脚本"), res.message, &res.status, cx);
    }

    // ========================================================================
    // Phase 5C：Codex 插件市场动作 (PluginMarket)
    // ========================================================================

    pub fn load_plugin_market(
        &mut self,
        source: MarketSource,
        force_refresh: bool,
        cx: &mut Context<Self>,
    ) {
        if self.plugin_loading || self.plugin_installing_id.is_some() {
            return;
        }
        self.plugin_source = source;
        self.plugin_loading = true;
        let src_str = source.as_str().to_string();
        cx.notify();

        self.spawn_tokio(
            cx,
            async move { commands::refresh_plugin_market(src_str, force_refresh).await },
            move |store, raw, cx| {
                store.plugin_loading = false;
                let catalog = parse_plugin_market_catalog(&raw);
                let status = if catalog.status.is_empty() {
                    "ok".to_string()
                } else {
                    catalog.status.clone()
                };
                let msg = catalog.message.clone();
                match source {
                    MarketSource::Public => store.plugin_catalog_public = Some(catalog),
                    MarketSource::Full => store.plugin_catalog_full = Some(catalog),
                }
                if force_refresh && !msg.is_empty() {
                    store.set_notice(t("Codex 插件市场"), msg, &status, cx);
                } else {
                    cx.notify();
                }
            },
        );
    }

    pub fn install_plugin_item(&mut self, id: String, cx: &mut Context<Self>) {
        if self.plugin_installing_id.is_some() || self.plugin_loading {
            return;
        }
        let source = self.plugin_source;
        let src_str = source.as_str().to_string();
        self.plugin_installing_id = Some(id.clone());
        cx.notify();

        self.spawn_tokio(
            cx,
            async move {
                let inst_res = commands::install_plugin_market_item(src_str.clone(), id).await;
                let refreshed = commands::refresh_plugin_market(src_str, false).await;
                (inst_res, refreshed)
            },
            move |store, (inst_res, refreshed), cx| {
                store.plugin_installing_id = None;
                let catalog = parse_plugin_market_catalog(&refreshed);
                match source {
                    MarketSource::Public => store.plugin_catalog_public = Some(catalog),
                    MarketSource::Full => store.plugin_catalog_full = Some(catalog),
                }
                let status = inst_res
                    .get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("ok")
                    .to_string();
                let default_msg = t("插件安装流程已完成。");
                let msg = inst_res
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&default_msg)
                    .to_string();
                store.set_notice(t("Codex 插件市场"), msg, &status, cx);
            },
        );
    }

    // ========================================================================
    // Phase 5D：AI Agent 缓存清理动作 (AgentCache)
    // ========================================================================

    pub fn scan_agent_cache_now(&mut self, cx: &mut Context<Self>) {
        if self.cache_scanning || self.cache_cleaning {
            return;
        }
        self.cache_scanning = true;
        self.cache_confirm_open = false;
        cx.notify();

        self.spawn_tokio(
            cx,
            async move { commands::scan_agent_cache().await },
            |store, res, cx| {
                store.cache_scanning = false;
                if let Some(report) = parse_agent_cache_scan_payload(&res.payload) {
                    store.cache_selected_ids = recommended_cache_selection(&report.groups, false);
                    store.cache_report = Some(report);
                }
                cx.notify();
            },
        );
    }

    pub fn clean_selected_agent_cache(&mut self, cx: &mut Context<Self>) {
        let Some(report) = &self.cache_report else {
            return;
        };
        if self.cache_selected_ids.is_empty() || self.cache_cleaning {
            return;
        }
        let scan_id = report.scan_id.clone();
        let group_ids: Vec<String> = self.cache_selected_ids.iter().cloned().collect();
        self.cache_cleaning = true;
        self.cache_confirm_open = false;
        cx.notify();

        self.spawn_tokio(
            cx,
            async move {
                let clean_res = commands::clean_agent_cache(scan_id, group_ids, true).await;
                let rescan_res = commands::scan_agent_cache().await;
                (clean_res, rescan_res)
            },
            |store, (clean_res, rescan_res), cx| {
                store.cache_cleaning = false;
                if let Some(report) = parse_agent_cache_scan_payload(&rescan_res.payload) {
                    store.cache_selected_ids = recommended_cache_selection(&report.groups, false);
                    store.cache_report = Some(report);
                }
                store.set_notice(
                    t("AI Agent 缓存清理"),
                    clean_res.message,
                    &clean_res.status,
                    cx,
                );
            },
        );
    }

    // ========================================================================
    // Phase 5E：安装维护动作 (Maintenance)
    // ========================================================================

    pub fn refresh_maintenance_page(&mut self, show_toast: bool, cx: &mut Context<Self>) {
        self.refresh_settings_silent(cx);
        self.watcher_state = Some(commands::load_watcher_state().payload);
        self.auto_takeover_state = Some(commands::load_auto_takeover_state().payload);
        self.refresh_overview(show_toast, cx);
    }

    pub fn repair_shortcuts_now(&mut self, cx: &mut Context<Self>) {
        if self.shortcuts_repairing {
            return;
        }
        self.shortcuts_repairing = true;
        cx.notify();
        self.spawn_tokio(
            cx,
            async move {
                let rep = commands::repair_shortcuts().await;
                let ov = commands::load_overview().await;
                (rep, ov)
            },
            |store, (rep, ov), cx| {
                store.shortcuts_repairing = false;
                store.overview = Some(ov.payload);
                store.set_notice(t("修复快捷方式"), rep.message, &rep.status, cx);
            },
        );
    }

    pub fn set_login_startup(&mut self, enabled: bool, cx: &mut Context<Self>) {
        let res = commands::set_watcher_enabled(enabled);
        let status = res.status.clone();
        let msg = res.message.clone();
        self.watcher_state = Some(res.payload);
        self.set_notice(t("开机自启"), msg, &status, cx);
    }

    pub fn set_auto_takeover_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        let mut next = self.settings.clone();
        next.codex_app_auto_takeover = enabled;
        let saved = commands::save_settings(next);
        if saved.status == "ok" {
            self.settings = saved.payload.settings;
            if enabled {
                chatgptx_backend::auto_takeover::start_supervisor();
            } else {
                chatgptx_backend::auto_takeover::stop_supervisor();
            }
            self.auto_takeover_state = Some(commands::load_auto_takeover_state().payload);
        }
        self.set_notice(t("自动接管官方 App"), saved.message, &saved.status, cx);
    }

    pub fn trigger_takeover_now(&mut self, cx: &mut Context<Self>) {
        let res = commands::take_over_codex_now();
        let status = res.status.clone();
        let msg = res.message.clone();
        self.auto_takeover_state = Some(res.payload);
        self.set_notice(t("立即接管"), msg, &status, cx);
    }

    pub fn pause_takeover_instance(&mut self, cx: &mut Context<Self>) {
        let res = commands::pause_auto_takeover_for_instance();
        let status = res.status.clone();
        let msg = res.message.clone();
        self.auto_takeover_state = Some(res.payload);
        self.set_notice(t("暂停接管"), msg, &status, cx);
    }

    pub fn sync_history_providers_now(&mut self, cx: &mut Context<Self>) {
        if self.provider_sync_busy {
            return;
        }
        self.provider_sync_busy = true;
        cx.notify();
        self.spawn_tokio(
            cx,
            async move { commands::sync_providers_now(None).await },
            |store, res, cx| {
                store.provider_sync_busy = false;
                store.set_notice(t("历史会话供应商对齐"), res.message, &res.status, cx);
            },
        );
    }

    // ========================================================================
    // Phase 5F：关于、日志与在线更新动作 (About)
    // ========================================================================

    pub fn refresh_about_page(&mut self, show_toast: bool, cx: &mut Context<Self>) {
        let logs_res = commands::read_latest_logs(LogRequest { lines: 200 });
        self.logs_payload = Some(logs_res.payload);
        if show_toast {
            self.set_notice(t("运行日志"), logs_res.message, &logs_res.status, cx);
        } else {
            cx.notify();
        }
    }

    pub fn clear_app_logs(&mut self, cx: &mut Context<Self>) {
        let res = commands::clear_logs();
        let status = res.status.clone();
        let msg = res.message.clone();
        self.logs_payload = Some(res.payload);
        self.set_notice(t("清空日志"), msg, &status, cx);
    }

    pub fn copy_diagnostics_report(&mut self, cx: &mut Context<Self>) {
        let res = commands::copy_diagnostics();
        if res.status == "ok" {
            cx.write_to_clipboard(ClipboardItem::new_string(res.payload.report));
        }
        self.set_notice(t("复制诊断报告"), res.message, &res.status, cx);
    }

    pub fn check_for_updates_now(&mut self, cx: &mut Context<Self>) {
        if self.checking_update {
            return;
        }
        self.checking_update = true;
        cx.notify();
        self.spawn_tokio(
            cx,
            async move { commands::check_update().await },
            |store, res, cx| {
                store.checking_update = false;
                if let Ok(info) = serde_json::from_value::<UpdateCheckView>(res.payload) {
                    store.update_info = Some(info);
                }
                store.set_notice(t("检查更新"), res.message, &res.status, cx);
            },
        );
    }

    pub fn install_available_update(&mut self, cx: &mut Context<Self>) {
        let Some(info) = &self.update_info else {
            return;
        };
        if !info.update_available || self.installing_update {
            return;
        }
        let release = chatgptx_core::update::Release {
            version: info.latest_version.clone().unwrap_or_default(),
            url: info.asset_url.clone().unwrap_or_default(),
            body: info.release_summary.clone(),
            asset_name: info.asset_name.clone(),
            asset_url: info.asset_url.clone(),
        };
        self.installing_update = true;
        cx.notify();
        self.spawn_tokio(
            cx,
            async move { commands::perform_update(Some(release)).await },
            |store, res, cx| {
                store.installing_update = false;
                let launched = res
                    .payload
                    .get("launched")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                store.set_notice(t("执行更新"), res.message, &res.status, cx);
                if res.status == "ok" && launched && cfg!(target_os = "macos") {
                    chatgptx_backend::run_graceful_shutdown();
                    cx.quit();
                }
            },
        );
    }

    // ========================================================================
    // Phase 5G：全局设置动作 (Settings)
    // ========================================================================

    pub fn save_global_proxy_settings(
        &mut self,
        mode: String,
        url: String,
        cx: &mut Context<Self>,
    ) {
        let res = commands::save_global_launch_proxy(SaveGlobalLaunchProxyRequest {
            global_proxy_mode: mode,
            global_proxy_url: url,
            proxy_resources: None,
        });
        let status = res.status.clone();
        let msg = res.message.clone();
        self.official_snapshot = Some(res.payload);
        self.set_notice(t("全局启动代理"), msg, &status, cx);
    }

    pub fn test_global_proxy_url(&mut self, url: String, cx: &mut Context<Self>) {
        if self.proxy_testing {
            return;
        }
        self.proxy_testing = true;
        cx.notify();
        self.spawn_tokio(
            cx,
            async move { commands::test_launch_proxy(url).await },
            |store, res, cx| {
                store.proxy_testing = false;
                store.set_notice(t("代理连通性测试"), res.message, &res.status, cx);
            },
        );
    }

    pub fn reset_all_settings_now(&mut self, cx: &mut Context<Self>) {
        self.reset_confirm_open = false;
        let res = commands::reset_settings();
        if res.status == "ok" {
            self.settings = res.payload.settings;
            self.user_scripts_inventory = parse_user_script_inventory(&res.payload.user_scripts);
        }
        self.set_notice(t("重置设置"), res.message, &res.status, cx);
    }
}

async fn poll_multi_instance_in_tokio(
    target_instance_id: &str,
) -> (MultiPollOutcome, Vec<CodexMultiInstance>) {
    let deadline = Instant::now() + Duration::from_secs(90);
    let mut last_visible = Vec::new();
    while Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(1800)).await;
        let list_res = commands::list_codex_multi_instances().await;
        let all = list_res.payload.instances;
        last_visible = all
            .iter()
            .filter(|i| should_display_multi_instance(i))
            .cloned()
            .collect();
        if let Some(found) = all.iter().find(|i| i.instance_id == target_instance_id) {
            if multi_launch_instance_running(found) {
                return (MultiPollOutcome::Ready, last_visible);
            }
            if multi_launch_instance_failure(found).is_some() {
                return (MultiPollOutcome::Failed, last_visible);
            }
        }
    }
    (MultiPollOutcome::Pending, last_visible)
}
