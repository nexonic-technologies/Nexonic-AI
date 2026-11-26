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
| Live Sharee/Nexa portraits (cursor gaze, blinks, breathing, datasheet moods from real events, transparent cut-out) | Implemented | `LiveFace.tsx`, `presenceMood.ts`, atlas from `services/presence/render_atlas.py` + `matte_atlas.py` |
| Interactive core (magnetic cursor field, needles, click shockwave, drag-to-spin with momentum) | Implemented | `NeuralCore.tsx`; cursor play is never reported as agent activity |
| Talking Sharee/Nexa (on-device: Kokoro → JoyVASA → LivePortrait MLX → MuseTalk 1.5 MLX) | Experimental | `services/presence`, `Presence probe` workflow; persona/expression specs in `personas.json` |
| Audit fixes (26 Nov) | Tested | owner-policy handles rewritten branches; graceful encrypted-storage fallback; update check async, semver, opt-in; waveform moves only for real audio; Today's Focus save race fixed; 1280×800 Awareness panel fit |
| Agents, collaboration feed, memory events, voice | Pending | panels show honest empty states until M16/M17/M28 |
| Character art | Interim | owner concept art; real-time digital humans in M15 |
| Agents, model routing, voice, awareness, automation | Pending | Phases C–E |
| Packaging and releases | Pending | M47/M48 |

## Next actionable task

Build `crates/model-router` (M17: OpenRouter catalogue, free-only default, cooldown fallback, Keychain key) and wire Sharee and Nexa agent runtimes (M16) to `memory-engine`, alongside the M15 real-time Presence sidecar.
