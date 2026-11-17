# Third-Party Notices & Provenance

**Swivox** is distributed under the [GNU Affero General Public License v3.0](LICENSE) (`AGPL-3.0-only`). This document records the upstream projects, adapted components, reference implementations, and third-party license notices associated with this repository.

---

## 1. Upstream Base Project — CodexPlusPlus

Swivox is a derivative work originally forked from **CodexPlusPlus**:

- **Repository**: <https://github.com/BigPizzaV3/CodexPlusPlus>
- **License**: GNU Affero General Public License v3.0 (`AGPL-3.0-only`)
- **Copyright**: Copyright (C) 2026 BigPizzaV3 and contributors
- **Derivative Modifications**: Copyright (C) 2026 aurorax-neo (<https://github.com/aurorax-neo/swivox>)

Summary of major architectural and product modifications in Swivox:
- Rebranded application package, bundle identifiers (`com.swivox.app`), extension namespace (`window.swivox`), isolated state directory (`~/.swivox`, including `user_scripts`), and isolated loopback ports (`58319–58321`) to **Swivox** with zero configuration overlap against upstream `Codex++`.
- Migrated the entire desktop management frontend from Tauri/React to a pure Rust + **GPUI** native GPU-accelerated desktop application (`apps/swivox-gpui` + `crates/swivox-backend`).
- Added multi-account official OAuth PKCE login, automatic token refresh, real-time quota inspection, macOS Keychain synchronization, per-account third-party API provider binding, official account multi-instance launch, main-instance stop control, and unified global network proxy settings.

In accordance with AGPL-3.0 Section 5 and Section 13, the complete corresponding source code of Swivox is publicly available at <https://github.com/aurorax-neo/swivox>.

---

## 2. Native Desktop UI Framework — GPUI & GPUI Kit

- **GPUI (`gpui-pre`)**: <https://github.com/zed-industries/zed> — Apache License 2.0 (`Apache-2.0`)
- **GPUI Kit (`gpui-kit`)**: <https://github.com/longbridge/gpui-kit> — Apache License 2.0 (`Apache-2.0`)

---

## 3. Design & Workflow Reference — cockpit-tools

The multi-account official OAuth management, token refresh lifecycle, quota display, and global proxy workflow in Swivox reference the design of **cockpit-tools**:

- **Repository**: <https://github.com/jlcodes99/cockpit-tools>
- **Author**: jlcodes (`jlcodes99`)
- **License**: Creative Commons Attribution-NonCommercial-ShareAlike 4.0 International (`CC-BY-NC-SA-4.0`, <https://creativecommons.org/licenses/by-nc-sa/4.0/>)
- **Notice**: Swivox is provided free of charge for personal learning, research, and non-commercial use. Respect the upstream author's attribution and non-commercial terms when referencing or reusing related designs.

---

## 4. Built-in Usage Widget — DeepSeek-Balance-Whale-Widget & Codex-Whale-Public

The optional usage widget and desktop pet engine in `assets/inject/upstream/whale-widget/` are adapted from:

- **Primary Upstream**: <https://github.com/MeteorNOX/DeepSeek-Balance-Whale-Widget>
- **Codex Desktop Adaptation Reference**: <https://github.com/Yang-huai406/Codex-Whale-Public>
- **Code License**: MIT License (see [`assets/inject/upstream/whale-widget/LICENSE`](assets/inject/upstream/whale-widget/LICENSE))
- **Media & Artwork Provenance**: Character images, animations, and audio files under `assets/inject/upstream/whale-widget/assets/` are governed by [`assets/inject/upstream/whale-widget/PROVENANCE.md`](assets/inject/upstream/whale-widget/PROVENANCE.md).

---

## 5. Local Speech Recognition Runtime — Sherpa-ONNX

- **Repository**: <https://github.com/k2-fsa/sherpa-onnx>
- **License**: Apache License 2.0 (`Apache-2.0`)
- **Notice**: Local speech recognition models (such as SenseVoiceSmall) are downloaded on demand by the user and remain subject to their respective model licenses.

---

## 6. Trademarks & Disclaimer

OpenAI, ChatGPT, and Codex are trademarks of OpenAI, L.L.C. Swivox is an independent open-source project and is not affiliated with, sponsored by, or endorsed by OpenAI.
