use swivox_backend::i18n::{t, tf};
use swivox_core::manager_navigation::ManagerNavigationIntent;
use gpui_kit::assets::IconName;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Route {
    Overview,
    OfficialAccounts,
    Relay,
    MultiLaunch,
    RelayEnvironment,
    AntigravityAccounts,
    AntigravityEnhance,
    Lamplighter,
    Sessions,
    Context,
    Skills,
    UserScripts,
    Enhance,
    PluginMarket,
    AgentCache,
    Maintenance,
    About,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectionPlacement {
    Top,
    Bottom,
}

pub struct RouteSpec {
    pub id: Route,
    pub tool: Option<&'static str>,
}

pub const ALL_ROUTES: &[RouteSpec] = &[
    RouteSpec { id: Route::Overview, tool: None },
    RouteSpec { id: Route::OfficialAccounts, tool: Some("codex") },
    RouteSpec { id: Route::Relay, tool: Some("codex") },
    RouteSpec { id: Route::MultiLaunch, tool: Some("codex") },
    RouteSpec { id: Route::AntigravityAccounts, tool: Some("antigravity") },
    RouteSpec { id: Route::AntigravityEnhance, tool: Some("antigravity") },
    RouteSpec { id: Route::Lamplighter, tool: None },
    RouteSpec { id: Route::Sessions, tool: Some("codex") },
    RouteSpec { id: Route::Context, tool: Some("codex") },
    RouteSpec { id: Route::Skills, tool: Some("codex") },
    RouteSpec { id: Route::UserScripts, tool: Some("codex") },
    RouteSpec { id: Route::Enhance, tool: Some("codex") },
    RouteSpec { id: Route::PluginMarket, tool: Some("codex") },
    RouteSpec { id: Route::AgentCache, tool: None },
    RouteSpec { id: Route::Maintenance, tool: Some("codex") },
    RouteSpec { id: Route::About, tool: None },
    RouteSpec { id: Route::Settings, tool: None },
    RouteSpec { id: Route::RelayEnvironment, tool: Some("codex") },
];

pub const GLOBAL_NAVIGATION_ROUTES: &[Route] = &[
    Route::AgentCache,
    Route::About,
    Route::Settings,
];

pub struct NavSection {
    pub label_zh: &'static str,
    pub routes: &'static [Route],
    pub placement: SectionPlacement,
}

pub const NAVIGATION_SECTIONS: &[NavSection] = &[
    NavSection {
        label_zh: "工作区",
        routes: &[
            Route::Overview,
            Route::OfficialAccounts,
            Route::Relay,
            Route::MultiLaunch,
            Route::AntigravityAccounts,
            Route::AntigravityEnhance,
            Route::Lamplighter,
        ],
        placement: SectionPlacement::Top,
    },
    NavSection {
        label_zh: "全局管理",
        routes: &[Route::Context, Route::Skills, Route::Sessions],
        placement: SectionPlacement::Top,
    },
    NavSection {
        label_zh: "扩展",
        routes: &[Route::UserScripts, Route::Enhance, Route::PluginMarket],
        placement: SectionPlacement::Top,
    },
    NavSection {
        label_zh: "系统",
        routes: &[Route::Maintenance],
        placement: SectionPlacement::Bottom,
    },
];

impl Route {
    pub fn is_global(self) -> bool {
        GLOBAL_NAVIGATION_ROUTES.contains(&self)
    }

    pub fn required_tool(self) -> Option<&'static str> {
        ALL_ROUTES
            .iter()
            .find(|spec| spec.id == self)
            .and_then(|spec| spec.tool)
    }

    pub fn visible_for_tool(self, active_tool: &str) -> bool {
        match self.required_tool() {
            Some(tool) => tool == active_tool,
            None => true,
        }
    }

    pub fn icon(self) -> IconName {
        match self {
            Route::Overview => IconName::LayoutDashboard,
            Route::OfficialAccounts => IconName::Users,
            Route::Relay => IconName::KeyRound,
            Route::MultiLaunch => IconName::Layers,
            Route::RelayEnvironment => IconName::ShieldCheck,
            Route::AntigravityAccounts => IconName::Users,
            Route::AntigravityEnhance => IconName::Hammer,
            Route::Lamplighter => IconName::Bell,
            Route::Sessions => IconName::MessageCircle,
            Route::Context => IconName::Network,
            Route::Skills => IconName::BookOpen,
            Route::UserScripts => IconName::FileCode,
            Route::Enhance => IconName::Hammer,
            Route::PluginMarket => IconName::Store,
            Route::AgentCache => IconName::Trash,
            Route::Maintenance => IconName::Wrench,
            Route::About => IconName::Info,
            Route::Settings => IconName::Settings,
        }
    }

    pub fn title(self) -> String {
        match self {
            Route::Overview => t("概览"),
            Route::OfficialAccounts => t("官方账号"),
            Route::Relay => t("供应商配置"),
            Route::MultiLaunch => t("Codex多开"),
            Route::RelayEnvironment => t("中转站环境配置检测"),
            Route::AntigravityAccounts => t("Antigravity 账号"),
            Route::AntigravityEnhance => t("Antigravity 增强"),
            Route::Lamplighter => t("点灯人监控"),
            Route::Sessions => t("会话管理"),
            Route::Context => "MCP".to_string(),
            Route::Skills => "Skills".to_string(),
            Route::UserScripts => t("拓展"),
            Route::Enhance => t("Codex增强"),
            Route::PluginMarket => t("Codex 插件市场"),
            Route::AgentCache => t("AI Agent 缓存清理"),
            Route::Maintenance => t("安装维护"),
            Route::About => t("关于"),
            Route::Settings => t("设置"),
        }
    }

    pub fn subtitle(self) -> String {
        match self {
            Route::Overview => t("检查问题、启动与快速修复"),
            Route::Relay => t("管理 API 供应商、协议、Key 与配置文件"),
            Route::OfficialAccounts => {
                t("管理多 ChatGPT 官方账号 OAuth 登录、切换、配额查询与供应商绑定")
            }
            Route::MultiLaunch => t("为不同供应商启动相互隔离的 Codex 实例"),
            Route::RelayEnvironment => t("排查可能干扰中转站配置的本机环境"),
            Route::AntigravityAccounts => {
                t("管理 Google / Antigravity 多账号 OAuth、5H 与周配额、无感热切号及专属代理")
            }
            Route::AntigravityEnhance => {
                t("免 TUN 代理启动、可逆界面汉化、上下文监控圆环与设备指纹保护")
            }
            Route::Lamplighter => {
                t("监控已支持客户端（Codex / Antigravity）账号的 5 小时与 7 天额度、窗口重置自动点火、趋势曲线与 Bark / Webhook 推送")
            }
            Route::Sessions => t("查看、删除和修复 Codex 本地会话"),
            Route::Context => t("独立管理 MCP 服务器与插件"),
            Route::Skills => t("从 GitHub 仓库安装 Skill 到 Codex"),
            Route::Enhance => t("管理常用功能、语音输入与下一步建议。"),
            Route::UserScripts => t("内置和用户自定义拓展清单"),
            Route::PluginMarket => t("检索插件，按需下载并安装到 Codex"),
            Route::AgentCache => t("扫描 Codex 与 Swivox 的已知缓存目录，由你选择清理项目。"),
            Route::Maintenance => t("入口检查与修复、开机启动与应用路径"),
            Route::About => t("版本信息、项目链接、GitHub Release 更新、日志与诊断"),
            Route::Settings => t("界面主题、语言偏好、代理节点列表与本程序自身代理"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusTone {
    Good,
    Warn,
    Bad,
}

pub fn status_tone(status: &str) -> StatusTone {
    if matches!(
        status,
        "found" | "installed" | "ok" | "running" | "running_degraded" | "connected"
    ) {
        StatusTone::Good
    } else if matches!(status, "failed" | "missing") {
        StatusTone::Bad
    } else {
        StatusTone::Warn
    }
}

pub fn status_label(status: &str) -> String {
    match status {
        "found" => t("已找到"),
        "missing" => t("缺失"),
        "installed" => t("已安装"),
        "ok" => t("正常"),
        "running" => t("运行中"),
        "running_degraded" => t("运行中（增强等待中）"),
        "degraded" => t("已降级"),
        "starting" => t("启动中"),
        "failed" => t("失败"),
        "archived" => t("已归档"),
        "accepted" => t("已受理"),
        "not_checked" => t("未检查"),
        "not_implemented" => t("未实现"),
        "unsupported" => t("不支持"),
        "compatible" | "ready" => t("已就绪"),
        "watching" => t("监听中"),
        "paused" => t("已暂停"),
        "disabled" => t("已禁用"),
        "stopped" => t("未运行"),
        "connected" => t("已连接"),
        "disconnected" => t("未连接"),
        "unknown" => t("未知"),
        other => other.to_string(),
    }
}

pub fn destination_from_navigation_intent(intent: &ManagerNavigationIntent) -> Route {
    if intent.page == "pluginMarket" {
        return Route::PluginMarket;
    }
    if intent.section.is_some() || intent.page == "enhance" {
        return Route::Enhance;
    }
    Route::Settings
}

pub fn tool_tooltip(name: &str, home_dir: &str, switchable: bool, relay_count: usize, running: bool) -> String {
    let dir = if home_dir.is_empty() {
        t("未配置目录")
    } else {
        home_dir.to_string()
    };
    let base = if switchable {
        tf("{0}｜{1}｜{2} 个供应商", &[name, &dir, &relay_count.to_string()])
    } else {
        tf("{0}｜{1}｜供应商配置尚未接入", &[name, &dir])
    };
    if running {
        format!("{base}｜{}", t("运行中"))
    } else {
        base
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use swivox_backend::i18n::{Language, set_language};

    #[test]
    fn route_visibility_filters_codex_only_routes_for_other_tools() {
        assert_eq!(ALL_ROUTES.len(), 18);
        assert!(Route::Overview.visible_for_tool("grok"));
        assert!(Route::Lamplighter.visible_for_tool("antigravity"));
        assert!(Route::AntigravityAccounts.visible_for_tool("antigravity"));
        assert!(!Route::AntigravityAccounts.visible_for_tool("codex"));
        assert!(Route::AgentCache.visible_for_tool("grok"));
        assert!(Route::About.visible_for_tool("grok"));
        assert!(Route::Settings.visible_for_tool("grok"));
        assert!(!Route::Relay.visible_for_tool("grok"));
        assert!(!Route::OfficialAccounts.visible_for_tool("grok"));
        assert!(Route::Relay.visible_for_tool("codex"));
    }

    #[test]
    fn route_titles_and_status_labels_translate_cleanly_in_english() {
        set_language(Language::En);
        assert_eq!(Route::Overview.title(), "Overview");
        assert_eq!(status_label("running"), "Running");
        assert_eq!(status_label("stopped"), "Not running");
        set_language(Language::Zh);
        assert_eq!(Route::Overview.title(), "概览");
        assert_eq!(status_label("running"), "运行中");
    }

    #[test]
    fn manager_navigation_intent_maps_to_expected_route() {
        let market = ManagerNavigationIntent {
            page: "pluginMarket".to_string(),
            section: None,
        };
        assert_eq!(destination_from_navigation_intent(&market), Route::PluginMarket);

        let stepwise = ManagerNavigationIntent {
            page: "settings".to_string(),
            section: Some("stepwise".to_string()),
        };
        assert_eq!(destination_from_navigation_intent(&stepwise), Route::Enhance);
    }
}
