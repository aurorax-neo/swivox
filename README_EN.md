<p align="center">
  <img src="docs/images/readme-preview/hero.svg" alt="Swivox — Native Rust + GPUI hub for the Codex desktop app" width="100%">
</p>

<p align="center">
  <a href="README.md">中文</a> | English
</p>

<p align="center">
  <a href="https://github.com/aurorax-neo/swivox/releases"><img alt="Release" src="https://img.shields.io/github/v/release/aurorax-neo/swivox?style=flat-square&amp;color=6366f1&amp;label=release"></a>
  <a href="https://github.com/aurorax-neo/swivox/stargazers"><img alt="Stars" src="https://img.shields.io/github/stars/aurorax-neo/swivox?style=flat-square&amp;color=e8b86d"></a>
  <a href="LICENSE"><img alt="License" src="https://img.shields.io/badge/license-AGPL--3.0-838da8?style=flat-square"></a>
  <img alt="Platform" src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-838da8?style=flat-square">
  <img alt="Stack" src="https://img.shields.io/badge/built%20with-Rust%20%2B%20GPUI-4cc9f0?style=flat-square">
</p>

**Swivox** is a GPU-accelerated native desktop control center for OpenAI Codex / ChatGPT and Google Antigravity, built end-to-end with **Rust + GPUI** (~50 MB resident memory, zero WebView overhead). Through CDP, a local protocol conversion proxy, and background scheduling engines, Swivox provides multi-account Codex OAuth management, first-class **Antigravity** seamless hot-switching & proxy-free launch, **Lamplighter** multi-provider quota monitoring & 5-hour window auto-ignition, 25+ API provider presets & aggregate routing, per-model context window catalogs, isolated multi-instance launching, MCP / Skills / session management, and runtime UI enhancements—without modifying `app.asar` or breaking `.app` code signatures. All Swivox state (`~/.swivox`) and local ports (`58318–58321`) are cleanly namespaced and isolated.

## Key Features

| Area | Capabilities |
| --- | --- |
| **Codex Official Accounts** | Multi-account OAuth PKCE browser sign-in, manual callback parsing, import from local `auth.json` / macOS Keychain / JSON, automatic token refresh, live 5h / weekly / monthly quota checks, one-click switching, and per-account third-party API provider binding |
| **Native Antigravity Support** | Personal & Enterprise GCP OAuth login, Cloud Code Private 3-endpoint quota inspection (`Gemini` / `Claude & GPT` / `Gemini 4 Argon` 5h + weekly windows & authoritative tier detection), **Desktop Hot-Switch** (reloads `language_server` without closing windows), per-account Machine ID isolation & baseline restore, **Proxy-Free Launch** without TUN, reversible 3,376-entry CDP Chinese localization, and live Context Usage Ring |
| **Lamplighter (Quota & Auto-Ignition)** | Zero-config aggregation of Swivox's own Codex & Antigravity accounts plus Claude / Kimi / Grok & optional CLIProxyAPI (CPA); **5-hour window reset auto-ignition** (`07:00–22:30` window, rolling-reset detection, 7d exhaustion guard, backoff circuit breaker & manual Ignite Now), weighted trend curves & depletion forecast, Bark / Feishu / DingTalk / Telegram / ntfy alerts, and local redacted `127.0.0.1:58318/mcp` server |
| **API & Aggregate Providers** | 25 built-in provider presets, custom Responses / Chat Completions providers, Failover pools, Model aggregation, cross-provider encrypted reasoning sanitization, connectivity testing, and Provider Doctor diagnostics |
| **Codex Multi-Launch & Model Catalogs** | Launch isolated Codex instances for official accounts or API providers; configure per-model context windows (`deepseek-v4-pro[1M]`), auto-compact percentages, and image policies with automatic native `model_catalog_json` generation |
| **Enhancements, MCP & Cache Cleaner** | Zero-layout-shift floating toast notifications, `config.toml` MCP & GitHub Skills sync, session backup & repair, cloud Whisper & offline SenseVoiceSmall dictation, Stepwise follow-up suggestions, custom image overlays, `window.swivox` user scripts, and whitelisted AI Agent cache cleanup |

## Quick Start

Download the latest installer from [GitHub Releases](https://github.com/aurorax-neo/swivox/releases):

- **macOS (Apple Silicon & Intel Universal)**: `Swivox-*-macos-universal.dmg`
- **Windows (x64)**: `Swivox-*-windows-x64-setup.exe` / `Swivox-*-windows-x64.zip`

1. Ensure the official Codex / ChatGPT desktop app is installed on your machine.
2. Open **Swivox**, sign in or import official accounts under **Official accounts**, or configure API providers under **Provider configuration**.
3. Click **Use** on any account or provider to apply immediately or apply & restart Codex.
4. Use the topbar **Launch Codex**, **Stop Codex**, or **Restart Codex** buttons for the main instance, or open **Codex multi-launch** to run multiple isolated instances side by side.

## Data Locations & Isolation

- Codex config & auth: `~/.codex/config.toml`, `~/.codex/auth.json` (overridable via `CODEX_HOME`)
- Codex local databases: `~/.codex/sqlite/*.db` (falls back to `~/.codex/state_5.sqlite`)
- **Swivox state & logs**: `~/.swivox/` (overridable via `SWIVOX_STATE_DIR`; stores `settings.json`, `official-accounts.json`, `antigravity-accounts.json`, `ui-preferences.json`, `swivox.log`)
- **Lamplighter state & history**: `~/.swivox/lamplighter/` (`config.json`, `state.json`, `accounts.json`, `history/YYYY-MM-DD.jsonl`)
- **Swivox user scripts**: `~/.swivox/user_scripts/` and `~/.swivox/user_scripts.json`
- **Swivox multi-launch instances**: `~/.swivox/codex-multi/<instance-id>/`
- **Swivox default loopback ports**: Lamplighter MCP `58318` (opt-in), Manager guard `58319`, Launcher guard `58320`, Protocol proxy `58321` (overridable via `SWIVOX_PROTOCOL_PROXY_PORT`)

## Development

```bash
# Run Rust test suites
cargo test -p swivox-backend --lib
cargo test -p swivox-gpui
cargo test -p swivox-launcher
cargo test -p swivox-data --lib

# Build release native desktop binaries
cargo build --release -p swivox-gpui --bins
```

If you edit shards under `assets/inject/renderer-inject/`, run `node scripts/assemble-renderer-inject.mjs` to reassemble `assets/inject/renderer-inject.js`.

## Credits & License Compliance

### 1. Credits
- **[BigPizzaV3/CodexPlusPlus](https://github.com/BigPizzaV3/CodexPlusPlus)** (`AGPL-3.0-only`): Upstream base repository from which the core launcher, protocol proxy, model catalog generator, and Codex renderer injection architecture originated.
- **[longbridge/gpui-kit](https://github.com/longbridge/gpui-kit)** & **[zed-industries/zed](https://github.com/zed-industries/zed)** (`Apache-2.0`): GPU-accelerated GPUI desktop framework and UI components.
- **[jlcodes99/cockpit-tools](https://github.com/jlcodes99/cockpit-tools)** (`CC-BY-NC-SA-4.0`): Reference for multi-account official OAuth PKCE sign-in, token refresh, live quota inspection, and global network proxy workflows.
- **[lbjlaq/Antigravity-Manager](https://github.com/lbjlaq/Antigravity-Manager)** & **[DSDS-CMHL/EasyAntigravity](https://github.com/DSDS-CMHL/EasyAntigravity)** (`MIT`): Reference for Antigravity OAuth & `state.vscdb` UnifiedState Protobuf credential sync, per-account Machine ID isolation, proxy-free launch, reversible CDP UI localization dictionaries, and live context ring inspection.
- **[PosvdM/cpa-plugin-lamplighter](https://github.com/PosvdM/cpa-plugin-lamplighter)** (`MIT`): Reference for Lamplighter multi-provider quota monitoring, 5-hour window reset auto-ignition, weighted trend forecasting, Bark / Webhook notifications, and redacted local `/mcp` server.
- **[MeteorNOX/DeepSeek-Balance-Whale-Widget](https://github.com/MeteorNOX/DeepSeek-Balance-Whale-Widget)** (Code `MIT`) & **[Yang-huai406/Codex-Whale-Public](https://github.com/Yang-huai406/Codex-Whale-Public)**: Optional usage widget and character assets. See [Upstream LICENSE](assets/inject/upstream/whale-widget/LICENSE) and [PROVENANCE.md](assets/inject/upstream/whale-widget/PROVENANCE.md).

### 2. License & Disclaimer
- **Open-Source License**: Licensed under the [GNU Affero General Public License v3.0](LICENSE) (`AGPL-3.0-only`). See [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) for complete third-party notices.
- **Copyright**:
  - `Copyright (C) 2026 BigPizzaV3 and contributors` (Upstream code)
  - `Copyright (C) 2026 aurorax-neo` (Swivox GPUI native rewrite, multi-account architecture, and derivative additions)
- **Disclaimer**: OpenAI, ChatGPT, and Codex are trademarks of OpenAI, L.L.C. Swivox is not affiliated with or endorsed by OpenAI.

---

<p align="center">
  <img src="docs/images/swivox.png" alt="Swivox" width="52"><br>
  <strong>Swivox</strong><br>
  <sub><a href="https://github.com/aurorax-neo/swivox">github.com/aurorax-neo/swivox</a> · <a href="LICENSE">AGPL-3.0-only</a> · <a href="THIRD_PARTY_NOTICES.md">Third-Party Notices</a></sub>
</p>
