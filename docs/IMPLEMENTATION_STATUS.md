# Implementation status

Classification: **Implemented** · **Tested** · **Experimental** · **Blocked** · **Pending**

| Area | Status | Evidence |
|---|---|---|
| Owner-only commit policy (hooks + CI) | Tested | `scripts/owner-policy.sh`, `Guard` workflow on every push/PR |
| Secret scanning (full history) | Tested | `Guard` → gitleaks 8.30.1, checksum-verified |
| Private-file blocking | Tested | `.gitignore`, pre-commit hook, `Guard` job |
| Permission model with emergency stop | Tested | `crates/permissions`, `cargo test` in `Rust` workflow |
| Three-domain memory isolation (storage layer) | Tested | `crates/memory-engine`, `cargo test` |
| Encrypted persistence (SQLite + XChaCha20-Poly1305) | Tested | `crates/memory-engine/src/persist.rs` wired into `AppState` (`keychain.rs`, `memory.sqlite`); consolidation pending in M23 |
| Design system tokens and dashboard spec | Implemented | `docs/DESIGN_SYSTEM.md`, `packages/design-system` |
| macOS app shell (Tauri 2 + React) | Implemented | `apps/desktop`; `Desktop` workflow builds the arm64 .app/.dmg |
| Main dashboard layout (matches owner reference) | Implemented | header, sidebar, presence, core, modes, 4 panels, dock |
| Collective Intelligence core v2 (woven torus of streamlines, flowing energy pulses, orbiting Sharee/Nexa bands with rim spikes, cursor lean, node-hover steering, motion budgets, background suspension) | Implemented | `NeuralCore.tsx`; idle drift is decorative and labelled |
| Interface motion (staged entrance, portrait parallax, cursor panel light, sliding mode ink, node connector glow, dock halo; reduced-motion = off) | Implemented | `styles/app.css` motion section |
| Live system detection (chip, memory, OS) | Tested | `system_info` command + Rust test |
| Permissions UI wired to Rust (toggle, emergency stop, resume) | Implemented | Awareness panel |
| Today's Focus (encrypted task list) | Implemented | stored in encrypted SQLite `kv` inside Tauri (`localStorage` fallback in browser preview) |
| Encrypted persistent memory (SQLite + XChaCha20-Poly1305, Keychain key) | Tested | `apps/desktop/src-tauri/src/lib.rs` + `keychain.rs` + `crates/memory-engine/src/persist.rs` |
| Typography system + motion typography | Implemented | Sora / Geist / Geist Mono, `MotionText.tsx` |
| Real video Sharee/Nexa (front-facing filmed-style idle clips, partner chat clips where they turn to each other, transparent background; no hover reactions; LiveFace atlas as fallback) | Implemented | `VideoFace.tsx`, `presenceMood.ts`, clips from `services/presence/process_clip.py` + `face_boxes.py`; lip sync to real replies waits for M15–M17 |
| Interactive core (magnetic cursor field, needles, click shockwave, drag-to-spin with momentum) | Implemented | `NeuralCore.tsx`; cursor play is never reported as agent activity |
| Talking Sharee/Nexa (on-device: Kokoro → JoyVASA → LivePortrait MLX → MuseTalk 1.5 MLX) | Experimental | `services/presence`, `Presence probe` workflow; persona/expression specs in `personas.json` |
| Audit fixes (26 Nov) | Tested | owner-policy handles rewritten branches; graceful encrypted-storage fallback; update check async, semver, opt-in; waveform moves only for real audio; Today's Focus save race fixed; 1280×800 Awareness panel fit |
| On-device brain: Qwen3.5 2B (Apache-2.0, Q4_K_M 1.28 GB) shipped inside the app, run by llama.cpp with Metal; offline, nothing leaves the Mac | Implemented | `crates/model-router` (`local` feature); pinned by SHA-256, downloaded and verified on the macOS build runner; real-inference test (GPU and CPU) in CI; context sized to the Mac's memory |
| Own-key cloud brains: OpenAI, Anthropic, DeepSeek, plus OpenRouter streaming (respects the free-only guard) | Implemented | `crates/model-router/src/cloud.rs`, `src-tauri/src/brain.rs`, Intelligence panel |
| Conversation: Sharee and Nexa answer in turn (or only the one named), streamed; Nexa sees Sharee's reply; real states drive presence and the core | Implemented | `lib/conversation.ts`, Conversation panel; not saved to memory yet (M16) |
| Agent memory, voice | Pending | M16/M28 |
| OpenRouter router (catalogue, Keychain key, free-only default, 429 cooldown fallback, test completion) | Tested | `crates/model-router/src/openrouter.rs`, `src-tauri/src/keychain.rs`; opened from Intelligence → OpenRouter |
| Character art | Interim | owner concept art; real-time digital humans in M15 |
| Agents, model routing, voice, awareness, automation | Pending | Phases C–E |
| Packaging and releases | Implemented (preview) | pushing a `v*` tag builds the DMG with the brain inside and publishes a GitHub release (`Desktop` → Publish release); ad-hoc signed, not notarized; first release v0.1.0 |

## Next actionable task

Wire Sharee and Nexa agent runtimes (M16) to `memory-engine` (remember conversations with consent), then voice and lip sync on the video faces (M15/M28). Router follow-ups: use the OpenRouter cooldown fallback for streamed replies, automatic fallback between brains, optional larger on-device models for 16 GB+ Macs.
