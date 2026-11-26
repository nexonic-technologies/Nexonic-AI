# Presence engine (M15)

Makes Sharee and Nexa talk with real lip sync, expressions and head motion, **on the Mac**,
with free, commercially usable models:

| Stage | Model | Licence | Runs on |
|---|---|---|---|
| Voice | Kokoro-82M via mlx-audio | Apache-2.0 | Apple GPU (MLX) |
| Audio → head pose and expression | JoyVASA (MLX weights) | MIT | Apple GPU |
| Portrait animation, eyes, lips | LivePortrait / FasterLivePortrait-MLX | MIT | Apple GPU |
| Precise mouth (next) | MuseTalk 1.5 MLX | MIT | Apple GPU |
| Face landmarks | LivePortrait landmark model | MIT | Apple GPU |

InsightFace models are **not** used (non-commercial licence).
Optional premium mode (later): a cloud live avatar (e.g. HeyGen LiveAvatar), off by default.

- `sources/` — Sharee and Nexa source portraits (© Nexonic Technologies, not under the AGPL).
- `render_demo.py` — renders talking clips; run by the `Presence probe` workflow on Apple Silicon.

Next: a persistent sidecar that streams frames to the app in real time (idle, listening,
thinking, speaking states), bundled with its own runtime so users install nothing.

## Probe results (26 Nov 2026, GitHub Apple M1 virtual runner)

- Render: ~3–7 minutes per 7-second clip on the slow virtual M1 (no Neural Engine, shared GPU); expect far faster on an M5.
- Identity, head motion, blinks and expressions: natural, no paste-back seams.
- **Lip sync: too weak** — JoyVASA mostly drives head/expression; mouth opening is minimal. Next step: add MuseTalk 1.5 MLX on the mouth region (driven by the same audio) for real visemes.
- Pinned snapshots: FasterLivePortrait-MLX-weights `2cc2ac9…`, Kokoro-82M-bf16 `a71e4d3…`.
