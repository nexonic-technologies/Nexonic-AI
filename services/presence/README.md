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

## Real video clips (in the app today)

Each character has short 8-second clips that start and end on the same
front-facing frame, so they loop and switch seamlessly:

- `idle`: breathing, natural blinks, small head movements, a faint smile.
- `chat`: turns toward the other character, talks and smiles, then faces the
  user again. Played now and then when nobody is talking to them.

```
python process_clip.py IN.mp4 OUT.mp4 --rvm rvm_mobilenetv3_fp32.onnx   # colour | alpha, H.264
python face_boxes.py apps/desktop/src/assets/clips                      # same eye line for both
```

Background removal uses Robust Video Matting (ONNX, CPU). The app composites
the packed clips in WebGL (`VideoFace.tsx`). Clips are not lip-synced to real
replies yet: that arrives with the agent runtimes and model router (M15–M17).
Source clips stay out of the repository.

## Live-portrait atlas (fallback)

Until the real-time sidecar streams frames, the app plays a frame atlas that is a real
re-animation of each character's own portrait:

- `render_atlas.py` drives FasterLivePortrait-MLX keypoints directly: a 5×3 head-pose grid
  (yaw ±14°, pitch ±8°) with the eyes leading the head, open / half / closed lids through
  LivePortrait's eye-retargeting network, and nine datasheet expressions (friendly, happy,
  attentive, curious, thinking, concerned, confident, calm, determined) at three head turns.
  Controls and per-persona intensity live in `atlas_spec.json` (FACS units noted per expression).
- `matte_atlas.py` cuts every frame out with BiRefNet-portrait (MIT) on the Apple Silicon GPU,
  so the characters have transparent backgrounds.
- The `Presence atlas` workflow runs both and prints the frames into its job log (base64 tar,
  with a SHA-256) because artifact downloads are blocked from the build container.
- In the app, `LiveFace.tsx` plays the atlas (gaze follow, blinks on each persona's rhythm,
  breathing, 300 ms mood cross-fades) and `presenceMood.ts` chooses moods from real events only:
  hover, click, node hover, what the user types and sends, and idle time.

## Probe results (26 Nov 2026, GitHub Apple M1 virtual runner)

- **Baseline (Kokoro + JoyVASA + LivePortrait MLX):** ~3–7 minutes per 7-second clip on the virtual M1; natural identity, head motion, blinks and expressions, but minimal mouth opening.
- **MuseTalk 1.5 MLX stage:** Wired via `mouth_sync.py` using pinned `xocialize/musetalk-mlx` (`c6eb30e…`) and `mlx-community/MuseTalk-1.5-fp16` (`ad54104…`), replacing only the feathered lower-face/jaw region so upper-face identity and expressions stay intact.
- Pinned snapshots: FasterLivePortrait-MLX-weights `2cc2ac9…`, Kokoro-82M-bf16 `a71e4d3…`, MuseTalk-1.5-fp16 `ad54104…`.
