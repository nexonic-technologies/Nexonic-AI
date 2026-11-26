# Research and architecture

Last updated: 26 Nov 2026 (M02/M04, first pass).

## 1. Starting point

- `nexonic-technologies/Nexonic-AI` was created empty on 26 Nov 2026: no commits, branches,
  workflows, issues, releases or license. It was initialized from scratch on `main`.
- Account and access verified: GitHub user `Shahrukhalid` (id 90670731), admin on the repository.
  Commits use the owner's GitHub noreply address.

## 2. Stark AI audit (read-only reference)

Repository: `Shahrukhalid/stark-ai` v2.2.0, MIT License. Nothing in that repository was changed.

| Area | What exists | Value for Nexonic AI |
|---|---|---|
| Architecture | Python FastAPI service + Next.js web app + PyInstaller desktop launcher; Docker for self-hosting | Different stack. Nexonic AI needs a native app with no bundled Python/Node/Docker requirement, so the code is **not** carried over. |
| Model access | OpenRouter free-model pool: catalogue fetch, tiered preference lists, per-model cooldown on 429/unavailable, paid models blocked unless explicitly allowed | **Design reused** for the Rust model router (M17/M19): free-only by default, cooldown-based fallback, catalogue discovery. |
| Credentials | Encrypted credential files whose key lives only in the OS vault, no plaintext fallback, verified migration with backups | **Design reused**; Nexonic AI goes further and keeps secrets in the Keychain itself. |
| Core runtime | Agent registry, typed events, messaging, permissions, tool registry, workflows | Concepts inform the agent runtime and inter-agent protocol (M16/M21). |
| Computer control | `computer/mac.py` and `computer/win.py` platform adapters | Confirms the adapter split; Nexonic AI implements it natively (Swift + Accessibility APIs). |
| Voice | Online voice plus an offline STT/TTS fallback | Requirements reference only; Nexonic AI evaluates whisper.cpp / sherpa-onnx / MLX on device. |
| Tests | Broad pytest suite (~460 API tests) | Testing discipline carried over. |

Licensing: no Stark AI source is copied. If any is copied later, its MIT notice must be kept
in `DEPENDENCY_LICENSES.md` and the file header.

## 3. Architecture decisions

| # | Decision | Reason | Rejected alternatives |
|---|---|---|---|
| A1 | Tauri 2 shell + Rust core + React/TypeScript UI + Swift native bridge | Small native bundle, WKWebView rendering, Rust for safe system services, Swift for Apple-only frameworks | Electron (bundles Chromium + Node, heavier); pure SwiftUI (slower path for the WebGL/WebGPU-heavy interface and later Windows) |
| A2 | Cargo workspace of focused crates (`permissions`, `memory-engine`, later `model-router`, `agent-runtime`, `collective-intelligence`, `automation`) | Each security boundary is testable on its own and in CI on Linux | One large crate |
| A3 | Memory isolation enforced at the storage API: a reader identity is required for every read, and a confidential record carries an explicit reader list | Confidentiality must not depend on model instructions | Prompt-level "do not share" rules |
| A4 | Permissions as an explicit, revocable grant set with an emergency stop that revokes everything and is audited | Matches macOS TCC model and the product's Full Assistance Mode | Implicit permissions per feature |
| A5 | OpenRouter first, free models only by default, paid usage requires explicit opt-in | Cost control, matches owner rule | Single fixed model |
| A6 | Release builds on GitHub-hosted macOS (Apple Silicon) runners; Developer ID signing and notarization only when credentials are provided as repository secrets | Reproducible, auditable builds | Local-only builds |

## 4. Open questions for the owner

- Project license (none chosen yet — nothing is assumed).
- Apple Developer ID certificate and notarization credentials, when distribution starts.
