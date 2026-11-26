# Design system

Source: the owner's two reference designs (main dashboard, UI template sheet), received
26 Nov 2026. The references are kept locally and are not published in this repository.
Product branding is **Nexonic AI** everywhere (the references still say "Personal AI").

These are starting tokens. Contrast and real rendering on the target display decide the
final values; every change is recorded here.

## Color — dark theme

| Token | Value | Use |
|---|---|---|
| `bg.primary` | `#0B0F14` | App background |
| `surface` | `#111827` | Sidebar, panels |
| `surface.elevated` | `#1E293B` | Raised panels, popovers |
| `surface.card` | `#2A3342` | Cards, list rows |
| `accent.sharee` | `#00DEC6` | Sharee: cyan / turquoise light, selection, waveform |
| `accent.nexa` | `#FFB568` | Nexa: amber / copper light, waveform |
| `info` | `#3B82F6` | Informational |
| `error` | `#EF4444` | Errors |
| `success` | `#10B981` | Active / online / completed |
| `intelligence` | `#A78BFA` | Collective Intelligence, learning, AI brand |

## Color — light theme (designed separately, not an inversion)

| Token | Value |
|---|---|
| `bg.primary` | `#F8FAFC` |
| `surface` | `#FFFFFF` |
| `surface.card` | `#EEF2F7` |
| `border` | `#E2E8F0` |
| `accent.sharee` | `#0EA5E9` |
| `accent.nexa` | `#F59E0B` |
| `info` | `#3B82F6` |
| `error` | `#EF4444` |
| `success` | `#22C55E` |
| `intelligence` | `#8B5CF6` |

## Typography

Inter with SF Pro as the native fallback; one family, variable weights, tabular numbers
for figures. Urdu/Arabic-script text uses a script-appropriate fallback with right-to-left
layout.

| Style | Size / weight |
|---|---|
| H1 | 48 / Bold |
| H2 | 36 / Semibold |
| H3 | 28 / Semibold |
| H4 | 20 / Medium |
| Body Large | 16 / Regular |
| Body | 14 / Regular |
| Body Small | 12 / Medium |
| Caption | 11 / Medium |

## Spacing, grid, radius

- 8-point system: 4, 8, 12, 16, 24, 32, 48, 64.
- 12-column layout grid, max content width 1920 px.
- Radii: 8 (controls), 12 (cards), 16 (panels), 24 (dock, hero surfaces).

## Components (from the template sheet)

- Buttons: primary (accent outline + glow on focus), secondary, ghost, icon (mic, play).
- Inputs: search with ⌘K hint, conversation input with mic, model selector.
- Controls: toggle, checkbox, radio, slider, waveform.
- Status tags: Active, Analyzing, Learning, Online, Offline, Beta.
- Cards and panels: card with title/description, progress card, stat tiles (Tasks, Memory).
- Lists: file item with menu, agent item with presence.
- Visual/data: AI core orb, voice waveform, thinking indicator, loading ring, line/bar charts,
  radial progress, radar chart, memory knowledge graph, task-flow diagram.
- Avatars with presence: Online, Thinking, Speaking, Idle, Offline.

## Interaction states

Listening · Speaking · Thinking · Executing · Learning · Completed · Attention · Error —
each driven by a real application event (see the state machine planned for M31), never
by a timer.

## Main dashboard composition (reference 1)

- **Header:** window controls, Nexonic AI wordmark, "My Intelligence Environment" with the
  tagline, global search (⌘K), notifications, profile.
- **Left navigation:** Presence, Visual Hub, Memory, Evolution, Activity, Computer Control,
  Intelligence, Settings; local-system card (chip, core version, running state —
  **detected at runtime, never hardcoded**); agent availability switcher ("Both Online").
- **Centre stage:** Sharee left (cyan light), Nexa right (amber light), Collective
  Intelligence core in the middle with six nodes: Reasoning, Memory, Learning, Perception,
  Planning, Action. Each agent shows name, presence state, mic control and live waveform.
- **Mode bar:** Your World, Collaborate, Analyze, Create, Automate.
- **Panels:** Today's Focus, Agent Collaboration, Recent Memory Events, Awareness & System.
- **Dock:** "Talk to Sharee, Nexa, or both…" input with voice, plus Open File, Search,
  Create, Automate.

## Key workflow (blueprint)

User Interaction → Perception → Reasoning → Collaboration → Action → Learning →
Personalized Outcome.
