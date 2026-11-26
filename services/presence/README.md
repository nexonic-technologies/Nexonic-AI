# Presence engine (M15)

Makes Sharee and Nexa talk with real lip sync, expressions and head motion, **on the Mac**,
with free, commercially usable models:

| Stage | Model | Licence | Runs on |
|---|---|---|---|
| Voice | Kokoro-82M via mlx-audio | Apache-2.0 | Apple GPU (MLX) |
| Audio → head pose and expression | JoyVASA (MLX weights) | MIT | Apple GPU |
| Portrait animation, eyes, head | LivePortrait / FasterLivePortrait-MLX | MIT | Apple GPU |
| Precise mouth & viseme lip sync | MuseTalk 1.5 MLX (`mlx-community/MuseTalk-1.5-fp16`) | MIT | Apple GPU |
| Face landmarks | LivePortrait landmark model (`MlxFaceAnalysisModel`) | MIT | Apple GPU |

InsightFace models are **not** used (non-commercial licence).
Optional premium mode (later): a cloud live avatar (e.g. HeyGen LiveAvatar), off by default.

- `sources/` — Sharee and Nexa source portraits (© Nexonic Technologies, not under the AGPL).
- `personas.json` — Sharee and Nexa voice, viseme, FACS, and expression state profiles matching the character datasheets.
- `mouth_sync.py` — Pure-MLX MuseTalk 1.5 mouth refinement and feathered lower-face paste-back (zero PyTorch, zero InsightFace).
- `render_demo.py` — End-to-end Kokoro → JoyVASA → LivePortrait MLX → MuseTalk 1.5 MLX probe; run by the `Presence probe` workflow on Apple Silicon.
- `test_presence.py` — Unit tests for persona config, landmark-to-MuseTalk crop geometry, smoothing, and feathered blending.

Next: a persistent sidecar that streams frames to the app in real time (idle, listening,
thinking, speaking states), bundled with its own runtime so users install nothing.

## Probe results (26 Nov 2026, GitHub Apple M1 virtual runner)

- **Baseline (Kokoro + JoyVASA + LivePortrait MLX):** ~3–7 minutes per 7-second clip on the virtual M1; natural identity, head motion, blinks and expressions, but minimal mouth opening.
- **MuseTalk 1.5 MLX stage:** Wired via `mouth_sync.py` using pinned `xocialize/musetalk-mlx` (`c6eb30e…`) and `mlx-community/MuseTalk-1.5-fp16` (`ad54104…`), replacing only the feathered lower-face/jaw region so upper-face identity and expressions stay intact.
- Pinned snapshots: FasterLivePortrait-MLX-weights `2cc2ac9…`, Kokoro-82M-bf16 `a71e4d3…`, MuseTalk-1.5-fp16 `ad54104…`.
