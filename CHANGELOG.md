# Changelog

## 0.2.4 - 2026-11-24

- **修复 Antigravity（反重力）`2.34.0+` 上下文监控圆环展示异常**：
  - 适配 Antigravity `2.34.0+` 移除 `language_server` 命令行 `--csrf_token` 参数的变化，Hub 页面改为同源携带 Session Cookie 与 `agy_session_csrf_token` 直连 Connect-RPC，并支持已打开窗口免重启原地热升级组件；
  - 切换至轻量增量接口 `GetCascadeTrajectoryGeneratorMetadata`（带游标增量拉取，空闲轮询零重算，旧版自动回退 `GetCascadeTrajectory`），彻底消除长对话拉取数十 MB 轨迹载荷导致的卡顿；
  - 修正 `.agy-composer-footer` 工具栏插入锚点使其紧贴模型选择器右侧，升级为等高胶囊进度环与百分比标签，补齐亮/暗主题自适应卡片样式、顶部水位条、`M`/`K` 单位格式化以及点击外部 / `Escape` 关闭。
- **大幅提速 Lamplighter（点灯人）手动点火与多账号配额刷新**：
  - 重构候选模型低延迟分档排序（优先选用 `flash-lite`、`haiku`、`sonnet-*-low` 等低首包延迟模型，规避慢速 `tiered` / `thinking` 模型）；
  - 重构 SSE 流式截断器（跨包行缓冲，命中首个生成分片或结束标记即刻断开连接，移除 2 秒硬等待）与渐进式快速确认（`350ms` 起步，Codex 支持响应头零等待确认），并移除手动点火末尾冗余的全客户端串行重轮询；
  - Antigravity 三端点配额查询（`loadCodeAssist` / `retrieveUserQuotaSummary` / `fetchAvailableModels`）与多账号轮询全面改为并发执行。

## 0.2.3 - 2026-11-23

- **修复 Lamplighter（点灯人）休眠唤醒后自动点火报 `HTTP 401: {` 误触发熔断问题**：
  - 修复配额轮询通过 `refresh_account_quota` 刷新落盘新 `access_token` 后未同步回写内存凭据的问题，避免同轮自动点火仍携带旧过期 Token 触发 `HTTP 401`；
  - 点火前新增主动 Token 有效期校验（`ensure_fresh_account_token` / `is_access_token_expired_or_near_expiry`），并在流式点火遇到偶发 `HTTP 401` 时自动强制刷新 Token 重试一次，同时补充 Cloud Code 备用端点回退；
  - 新增 `summarize_upstream_error_body` 自动提取多行 JSON 响应中的 `error.message`（杜绝首行截断为 `HTTP 401: {`），优化熔断策略（仅永久权限/吊销错误立即熔断，偶发错误走阶梯退避），并在配额查询恢复正常时自动自愈解除历史 `HTTP 401` 误暂停。

## 0.2.2 - 2026-11-20

- **修复 Antigravity（反重力）切换账号失效（macOS & Windows）**：对齐新版 `language_server` 的 `StoredToken` 结构（省略已弃用的超长 `LegacyIDToken`，避免超限截断），补齐 Windows Credential Manager 的 `go-keyring-base64:` 编解码与 `CredReadW` / `CredWriteW` / `CredDeleteW` 完整支持，优先落盘 `~/.gemini/` 文件级回退凭据，并在切号时透传账号专属代理、补齐 Windows 停止进程锁释放等待及新增「切换并重启桌面版」选项。
- **根治 macOS 钥匙串频繁弹出 `security` 密码授权窗口**：移除全仓生产代码中的 `/usr/bin/security` 子进程调用，重构为纯原生 `Security.framework` 进程内静默读写（全程包裹 `SecKeychainSetUserInteractionAllowed(false)`，在内存中直接构造 `-A` 免确认 ACL 且解除 4096 字节行长上限），并仅在 `config.toml` 显式启用 `keychain` 存储时同步 `Codex Auth`。
- **开机自启迁移至全局「设置」并新增「启动到托盘」**：将开机自启开关从「安装维护」迁移至全局「设置 -> 启动与系统托盘」，提供「登录时自动启动」与「启动到托盘」两个独立开关（支持中英双语切换与静默驻留托盘启动）。

## 0.2.1 - 2026-11-19

- **修复 Windows 点击关闭按钮 (`X`) 导致程序全部退出与托盘消失**：主程序显式启用 `QuitMode::Explicit` 并在窗口关闭后自动清理句柄，点击关闭按钮仅隐藏到系统托盘、保留后台守护与托盘图标常驻；同时优化 Windows 托盘为左键点击直接恢复主窗口、右键弹出「显示主窗口 / 退出程序」菜单。
- **修复 Windows 下官方 ChatGPT / Codex 任务栏图标被替换问题**：移除启动官方打包版 Codex 后通过 `WM_SETICON` 与 `PKEY_AppUserModel_*` 覆盖其窗口及任务栏图标/名称的遗留行为，完整保留官方应用的原生任务栏图标与分组。
- **macOS 分架构独立打包与仓库精简**：macOS 发布产物拆分为 `arm64`（Apple Silicon）与 `x64`（Intel）独立 DMG，单包体积减半并由应用内更新器按本机架构自动匹配；同步清理仓库内未编译孤立模块、历史废弃皮肤资产与零调用后端封装。

## 0.2.0 - 2026-11-18

- **品牌升级与原生 GPUI 架构重构**：全面升级为 **Swivox**（状态目录 `~/.swivox/`、本地端口 `58318–58321` 完全独立隔离），桌面管理端全量迁移至纯 Rust 原生 **GPUI** 并彻底移除旧版 Tauri 前端与构建依赖，显著降低内存占用并提升启动与渲染响应速度。
- **原生 Antigravity 一等公民支持**：
  - 支持与官方客户端 1:1 对齐的 Google OAuth PKCE `S256` 双通道登录（`personal` / `gcp`）、手动回调解析及本机凭据一键导入导出；
  - 支持三端点 5 小时 / 7 天配额查询、`state.vscdb` + 系统 Keychain/凭据库热切号与单号独立设备指纹（Machine ID）隔离及本机原始基线恢复；
  - 支持桌面版与 IDE 各自独立的免 TUN 代理启动、自动发现 `DevToolsActivePort` 回环调试端口、零文件修改可逆界面汉化（关闭瞬间还原英文并自动避开代码块与消息正文），以及实时上下文占用百分比圆环与 `tok/s` 生成速度监控。
- **原生 Lamplighter（点灯人）额度监控与自动点火**：支持多客户端隔离额度曲线记录、5 小时窗口重置后自动极小流点火计时、Codex / Antigravity 各自独立的点火隔离代理、熔断保护、Bark / 通用 Webhook（飞书 / 钉钉 / Telegram / ntfy）告警推送，以及常量时间鉴权的本地只读脱敏 `/mcp` 接口。
- **供应商模型目录与网络代理池增强**：
  - 支持统一代理节点池（Proxy Pool），本程序自身、Codex 客户端、Codex 点火、Antigravity 桌面版、Antigravity IDE 与 Antigravity 点火代理完全独立隔离与独立测速；
  - 上游模型拉取严格保留原始模型 ID，支持只读剥离渠道前缀与变体后缀匹配 `models.dev` / 内置窗口，未命中模型默认 1M 上下文窗口，并支持一键清空模型列表。
- **安全与跨平台稳定性加固**：
  - `~/.swivox` 目录与凭据文件强制 `0700`/`0600` 原子落盘，损坏 JSON 状态文件自动隔离备份防静默清空，macOS Keychain 改走 `stdin` 传参防命令行泄露；
  - 主世界 CDP 桥接启用私有令牌封存隔离，`/settings/get` 与诊断报告递归脱敏全部密钥，`/llm-proxy` 补齐防 DNS Rebinding 校验，应用内更新强制官方发布域名与安装包 SHA-256 完整性校验并复用应用内代理配置。

## 0.1.0 - 2026-10-15

- 新增「官方账号」管理页：支持多官方账号 OAuth PKCE 浏览器登录、手动回调解析、从本机 `auth.json` 与 macOS Keychain 导入，以及 JSON / Token 批量导入导出；支持实时查询 5 小时 / 周 / 月订阅配额、Token 临期自动刷新与免重登一键切换（同步写入 RFC3339 `last_refresh` 与 macOS Keychain `"Codex Auth"`）。
- 支持官方账号按账号粒度独立绑定第三方 API 供应商，并将「供应商配置」统一简化为纯 API 与聚合供应商模式；激活官方账号与激活纯 API 供应商状态严格互斥，纯官方模式下自动清理第三方模型目录覆盖以展示官方原生模型列表。
- 统一点击「使用」后的生效方式选择弹窗：在「官方账号」或「供应商配置」点击「使用」时，弹窗提供「取消 / 仅使用（重启生效） / 使用并重启 Codex」三个选项。
- 「Codex多开」支持官方账号与 API 供应商多开：支持选择任意已登录官方账号（含纯官方账号及绑定了第三方 API 供应商的官方账号）或 API 供应商启动相互隔离的 Codex 实例。
- 顶栏新增红色「停止 Codex」按钮（位于「重启 Codex」左侧），并将主实例运行状态探测、停止与重启操作和多开实例彻底隔离，主动停止不触发意外停止误报。
- 「设置」页内置全局网络代理配置（跟随系统代理 / 自定义代理 / 直连）与代理资源池，统一作用于官方账号 OAuth 登录与配额查询、插件/拓展市场拉取及 Codex 启动代理注入。
- 支持按模型配置上下文窗口（如 `deepseek-v4-pro[1M]`）与自动压缩阈值、Responses / Chat Completions 协议转换、会话管理、MCP / Skills / Codex 插件市场管理、用户脚本热重载与界面增强。
