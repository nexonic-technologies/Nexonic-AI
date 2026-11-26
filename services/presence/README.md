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
