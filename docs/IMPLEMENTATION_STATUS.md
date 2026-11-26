# Implementation status

Classification: **Implemented** · **Tested** · **Experimental** · **Blocked** · **Pending**

| Area | Status | Evidence |
|---|---|---|
| Owner-only commit policy (hooks + CI) | Tested | `scripts/owner-policy.sh`, `Guard` workflow on every push/PR |
| Secret scanning (full history) | Tested | `Guard` → gitleaks 8.30.1, checksum-verified |
| Private-file blocking | Tested | `.gitignore`, pre-commit hook, `Guard` job |
| Permission model with emergency stop | Tested | `crates/permissions`, `cargo test` in `Rust` workflow |
| Three-domain memory isolation (storage layer) | Tested | `crates/memory-engine`, `cargo test` |
| Encrypted persistence, retrieval, consolidation | Pending | M20/M23 |
| Design system tokens and dashboard spec | Implemented | `docs/DESIGN_SYSTEM.md`, `packages/design-system` |
| macOS app shell (Tauri 2 + React) | Implemented | `apps/desktop`; `Desktop` workflow builds the arm64 .app/.dmg |
| Main dashboard layout (matches owner reference) | Implemented | header, sidebar, presence, core, modes, 4 panels, dock |
| Collective Intelligence core (WebGL particles + filaments, cursor-reactive, reduced-motion) | Implemented | `NeuralCore.tsx`; idle drift is decorative and labelled |
| Live system detection (chip, memory, OS) | Tested | `system_info` command + Rust test |
| Permissions UI wired to Rust (toggle, emergency stop, resume) | Implemented | Awareness panel |
| Today's Focus (local task list) | Implemented | stored locally; moves to encrypted SQLite with M20 |
| Agents, collaboration feed, memory events, voice | Pending | panels show honest empty states until M16/M17/M28 |
| Character art | Interim | owner concept art; real-time digital humans in M15 |
| Agents, model routing, voice, awareness, automation | Pending | Phases C–E |
| Packaging and releases | Pending | M47/M48 |

## Next actionable task

Add encrypted SQLite persistence (memory + focus list) and the model router (M17: OpenRouter free models first) so Sharee and Nexa come online to `memory-engine` behind the same isolation API, then start
`model-router` (OpenRouter catalogue, free-only default, cooldown fallback).
