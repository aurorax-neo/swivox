//! Phase 4 下沉纯业务规则：MCP 内置预设库、管理页多词模糊搜索、会话按项目/时间分组及时间戳格式化。

use swivox_data::LocalSession;

use crate::i18n::t;

// ============================================================================
// 1. MCP 服务器内置预设（100% 对应 mcp-presets.ts）
// ============================================================================

#[derive(Debug, Clone, Copy)]
pub struct McpPreset {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub tags: &'static [&'static str],
    pub homepage: &'static str,
    pub uses_uvx: bool,
    pub package_name: &'static str,
}

impl McpPreset {
    pub fn toml_body(self, windows: bool) -> String {
        if self.uses_uvx {
            return format!("command = \"uvx\"\nargs = [\"{}\"]\n", self.package_name);
        }
        if windows {
            format!(
                "command = \"cmd\"\nargs = [\"/c\", \"npx\", \"-y\", \"{}\"]\n",
                self.package_name
            )
        } else {
            format!(
                "command = \"npx\"\nargs = [\"-y\", \"{}\"]\n",
                self.package_name
            )
        }
    }

    pub fn current_platform_toml_body(self) -> String {
        self.toml_body(cfg!(windows))
    }
}

pub const MCP_PRESETS: &[McpPreset] = &[
    McpPreset {
        id: "fetch",
        name: "mcp-server-fetch",
        description: "抓取网页并转成适合模型阅读的文本。需要本机有 uvx（uv）。",
        tags: &["stdio", "web"],
        homepage: "https://github.com/modelcontextprotocol/servers/tree/main/src/fetch",
        uses_uvx: true,
        package_name: "mcp-server-fetch",
    },
    McpPreset {
        id: "time",
        name: "@modelcontextprotocol/server-time",
        description: "查询当前时间并做时区换算。",
        tags: &["stdio", "utility"],
        homepage: "https://github.com/modelcontextprotocol/servers/tree/main/src/time",
        uses_uvx: false,
        package_name: "@modelcontextprotocol/server-time",
    },
    McpPreset {
        id: "memory",
        name: "@modelcontextprotocol/server-memory",
        description: "基于知识图谱的长期记忆，跨会话保留事实。",
        tags: &["stdio", "memory"],
        homepage: "https://github.com/modelcontextprotocol/servers/tree/main/src/memory",
        uses_uvx: false,
        package_name: "@modelcontextprotocol/server-memory",
    },
    McpPreset {
        id: "sequential-thinking",
        name: "@modelcontextprotocol/server-sequential-thinking",
        description: "把复杂问题拆成可回溯的思考步骤。",
        tags: &["stdio", "reasoning"],
        homepage: "https://github.com/modelcontextprotocol/servers/tree/main/src/sequentialthinking",
        uses_uvx: false,
        package_name: "@modelcontextprotocol/server-sequential-thinking",
    },
    McpPreset {
        id: "context7",
        name: "@upstash/context7-mcp",
        description: "按需拉取库和框架的最新文档，减少版本过时的回答。",
        tags: &["stdio", "docs"],
        homepage: "https://context7.com",
        uses_uvx: false,
        package_name: "@upstash/context7-mcp",
    },
];

pub fn mcp_preset_by_id(id: &str) -> Option<&'static McpPreset> {
    MCP_PRESETS.iter().find(|p| p.id == id)
}

// ============================================================================
// 2. 管理页多词搜索与会话分组规则（100% 对应 management-model.ts）
// ============================================================================

pub fn matches_management_query(values: &[&str], query: &str) -> bool {
    let terms: Vec<String> = query
        .trim()
        .to_lowercase()
        .split_whitespace()
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    if terms.is_empty() {
        return true;
    }
    let haystack = values.join(" ").to_lowercase();
    terms.iter().all(|term| haystack.contains(term))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SessionGroupMode {
    #[default]
    Project,
    Time,
}

#[derive(Debug, Clone)]
pub struct SessionGroup {
    pub key: String,
    pub label: String,
    pub path: String,
    pub sessions: Vec<LocalSession>,
}

const DAY_MS: i64 = 86_400_000;

pub fn group_local_sessions(
    sessions: &[LocalSession],
    mode: SessionGroupMode,
    now_ms: i64,
) -> Vec<SessionGroup> {
    let mut sorted = sessions.to_vec();
    sorted.sort_by(|a, b| b.updated_at_ms.unwrap_or(0).cmp(&a.updated_at_ms.unwrap_or(0)));

    let today_start = now_ms - now_ms.rem_euclid(DAY_MS);
    let yesterday_start = today_start - DAY_MS;
    let week_start = today_start - 6 * DAY_MS;

    let mut groups: Vec<SessionGroup> = Vec::new();
    for session in sorted {
        let normalized_path = session
            .cwd
            .replace('\\', "/")
            .trim_end_matches('/')
            .to_string();
        let ts = session.updated_at_ms.unwrap_or(0);

        let (key, label, path_display) = match mode {
            SessionGroupMode::Project => {
                let leaf = normalized_path
                    .rsplit('/')
                    .next()
                    .filter(|s| !s.trim().is_empty())
                    .map(str::to_string)
                    .unwrap_or_else(|| t("未知目录"));
                (normalized_path.clone(), leaf, normalized_path)
            }
            SessionGroupMode::Time => {
                let (k, lbl) = if ts >= today_start {
                    ("today", t("今天"))
                } else if ts >= yesterday_start {
                    ("yesterday", t("昨天"))
                } else if ts >= week_start {
                    ("week", t("近 7 天"))
                } else {
                    ("earlier", t("更早"))
                };
                (k.to_string(), lbl, String::new())
            }
        };

        if let Some(existing) = groups.iter_mut().find(|g| g.key == key) {
            existing.sessions.push(session);
        } else {
            groups.push(SessionGroup {
                key,
                label,
                path: path_display,
                sessions: vec![session],
            });
        }
    }
    groups
}

pub fn format_relative_time_ms(ts_ms: Option<i64>, now_ms: i64) -> String {
    let Some(ts) = ts_ms.filter(|&v| v > 0) else {
        return t("未知时间");
    };
    let diff_secs = ((now_ms - ts) / 1000).max(0);
    if diff_secs < 60 {
        t("刚刚")
    } else if diff_secs < 3600 {
        format!("{} {}", diff_secs / 60, t("分钟前"))
    } else if diff_secs < 86_400 {
        format!("{} {}", diff_secs / 3600, t("小时前"))
    } else {
        format!("{} {}", diff_secs / 86_400, t("天前"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mcp_presets_produce_valid_codex_toml_bodies() {
        for preset in MCP_PRESETS {
            let unix_toml = preset.toml_body(false);
            let win_toml = preset.toml_body(true);
            assert!(
                swivox_core::mcp_config::parse_mcp_toml_body(&unix_toml).is_ok(),
                "unix toml invalid for {}",
                preset.id
            );
            assert!(
                swivox_core::mcp_config::parse_mcp_toml_body(&win_toml).is_ok(),
                "win toml invalid for {}",
                preset.id
            );
        }
    }

    #[test]
    fn management_query_matches_all_space_separated_terms() {
        assert!(matches_management_query(
            &["@upstash/context7-mcp", "stdio docs"],
            "context7 docs"
        ));
        assert!(!matches_management_query(
            &["@upstash/context7-mcp", "stdio docs"],
            "context7 memory"
        ));
    }

    #[test]
    fn session_grouping_sorts_newest_first_by_project_and_time() {
        let now = 1_760_000_000_000i64;
        let sessions = vec![
            LocalSession {
                id: "s1".to_string(),
                title: "Old task".to_string(),
                cwd: "/Users/dev/repo-a".to_string(),
                model_provider: "openai".to_string(),
                archived: false,
                updated_at_ms: Some(now - 10 * DAY_MS),
                rollout_path: String::new(),
                db_path: String::new(),
            },
            LocalSession {
                id: "s2".to_string(),
                title: "Today task".to_string(),
                cwd: "/Users/dev/repo-b/".to_string(),
                model_provider: "custom".to_string(),
                archived: false,
                updated_at_ms: Some(now - 60_000),
                rollout_path: String::new(),
                db_path: String::new(),
            },
        ];

        let by_proj = group_local_sessions(&sessions, SessionGroupMode::Project, now);
        assert_eq!(by_proj.len(), 2);
        assert_eq!(by_proj[0].label, "repo-b");
        assert_eq!(by_proj[1].label, "repo-a");

        let by_time = group_local_sessions(&sessions, SessionGroupMode::Time, now);
        assert_eq!(by_time.len(), 2);
        assert_eq!(by_time[0].key, "today");
        assert_eq!(by_time[1].key, "earlier");
    }
}
