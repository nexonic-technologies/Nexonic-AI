"""Render talking test clips of Sharee and Nexa entirely on Apple Silicon.

Pipeline (all MIT/Apache/BSD, on-device, no cloud, no InsightFace):
  text --Kokoro (mlx-audio)---------> speech audio
       --JoyVASA (MLX)--------------> head pose + expression motion from audio
       --LivePortrait (MLX)---------> animated portrait (eyes, brows, head)
       --MuseTalk 1.5 (MLX)---------> precise mouth & viseme lip sync

Usage (from a FasterLivePortrait-MLX checkout with `uv sync` done):
  uv run python /path/to/render_demo.py --flp . --out /tmp/presence-out \\
      --musetalk /path/to/musetalk-mlx --musetalk-weights /path/to/MuseTalk-1.5-fp16
"""

from __future__ import annotations

import argparse
import json
import os
import platform
import shutil
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
PERSONAS_PATH = HERE / "personas.json"
DEFAULT_MUSETALK_REPO_ID = "mlx-community/MuseTalk-1.5-fp16"

DEFAULT_LINES = {
    "sharee": (
        "Hi, I'm Sharee. Nexa and I have been looking at your week. "
        "There's one thing I'd change first, and I think you'll like it."
    ),
    "nexa": (
        "And I'm Nexa. Sharee's right, mostly. "
        "Let's keep the mornings clear for deep work, and I'll handle the rest."
    ),
}


def load_personas() -> dict:
    return json.loads(PERSONAS_PATH.read_text(encoding="utf-8"))


def find_synthesized_wav(save_dir: Path, org_video: Path, voice: str) -> Path:
    """Locate the Kokoro WAV written by run_text_driving, or extract from video."""
    expected = save_dir / f"mlx-audio-{voice}.wav"
    if expected.exists():
        return expected
    wavs = sorted(save_dir.glob("*.wav"))
    if wavs:
        return wavs[0]
    extracted = save_dir / f"extracted-{voice}.wav"
    subprocess.run(
        [
            "ffmpeg",
            "-y",
            "-v",
            "error",
            "-i",
            str(org_video),
            "-vn",
            "-ac",
            "1",
            "-ar",
            "16000",
            str(extracted),
        ],
        check=True,
    )
    return extracted


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--flp", required=True, help="path to the FasterLivePortrait-MLX checkout")
    ap.add_argument("--out", required=True)
    ap.add_argument("--agents", default="sharee,nexa")
    ap.add_argument("--expression", default="speaking", help="expression profile key from personas.json")
    ap.add_argument("--musetalk", help="path to xocialize/musetalk-mlx checkout")
    ap.add_argument("--musetalk-weights", help="local directory of mlx-community/MuseTalk-1.5-fp16")
    ap.add_argument("--musetalk-revision", default=None, help="HF revision if downloading weights")
    ap.add_argument("--skip-musetalk", action="store_true", help="only run Kokoro + JoyVASA + LivePortrait")
    args = ap.parse_args()

    spec_doc = load_personas()
    personas = spec_doc["personas"]

    flp = Path(args.flp).resolve()
    out = Path(args.out).resolve()
    out.mkdir(parents=True, exist_ok=True)
    os.chdir(flp)  # FLP resolves checkpoints and configs relative to its root
    if str(HERE) not in sys.path:
        sys.path.insert(0, str(HERE))
    sys.path.insert(0, str(flp))

    import mlx.core as mx  # noqa: E402
    from omegaconf import OmegaConf  # noqa: E402

    from src.pipelines.gradio_live_portrait_pipeline import GradioLivePortraitPipeline  # noqa: E402
    from src.runtime_assets import ensure_runtime_assets  # noqa: E402
    from src.utils.mlx_profiles import apply_mlx_profile  # noqa: E402

    musetalk_weights: Path | None = None
    if not args.skip_musetalk and args.musetalk:
        if args.musetalk_weights:
            musetalk_weights = Path(args.musetalk_weights).resolve()
        else:
            from huggingface_hub import snapshot_download  # noqa: E402

            dl_kwargs = {"revision": args.musetalk_revision} if args.musetalk_revision else {}
            musetalk_weights = Path(snapshot_download(DEFAULT_MUSETALK_REPO_ID, **dl_kwargs))

    report: dict = {
        "machine": platform.platform(),
        "chip": platform.processor() or platform.machine(),
        "mlx_device": str(mx.default_device()),
        "expression": args.expression,
        "musetalk_enabled": bool(not args.skip_musetalk and args.musetalk and musetalk_weights),
        "clips": {},
    }

    cfg = OmegaConf.load("configs/mlx_infer.yaml")
    t0 = time.time()
    ensure_runtime_assets(cfg)
    report["assets_seconds"] = round(time.time() - t0, 1)
    apply_mlx_profile("quality")
    pipe = GradioLivePortraitPipeline(cfg)
    pipe.set_mlx_profile("quality")
    t1 = time.time()
    pipe.init_models(is_animal=False)  # the web UI loads lazily; load explicitly here
    report["models_seconds"] = round(time.time() - t1, 1)

    for name in [a.strip() for a in args.agents.split(",") if a.strip()]:
        persona = personas[name]
        voice = persona["voice"]["kokoroId"]
        portrait = HERE / persona["sourcePortrait"]
        expr_cfg = persona["expressions"].get(args.expression, persona["expressions"]["speaking"])
        save_dir = out / name
        save_dir.mkdir(exist_ok=True)

        t = time.time()
        cfg.infer_params.cfg_scale = float(expr_cfg["cfgScale"])
        cfg.infer_params.driving_multiplier = float(expr_cfg["drivingMultiplier"])
        if getattr(pipe, "joyvasa_pipe", None) is not None:
            pipe.joyvasa_pipe.cfg_scale = float(expr_cfg["cfgScale"])
        org, crop, _ = pipe.run_text_driving(
            DEFAULT_LINES[name],
            voice,
            str(portrait),
            save_dir=str(save_dir),
        )
        lp_seconds = round(time.time() - t, 1)

        lp_out = out / f"{name}-liveportrait.mp4"
        shutil.copy(org, lp_out)
        shutil.copy(crop, out / f"{name}-talking-face.mp4")

        final = out / f"{name}-talking.mp4"
        clip_info: dict = {
            "voice": voice,
            "expression": args.expression,
            "liveportrait_seconds": lp_seconds,
            "file": final.name,
        }

        if report["musetalk_enabled"]:
            from mouth_sync import apply_musetalk_to_video  # noqa: E402

            if hasattr(pipe, "_release_mlx_memory"):
                pipe._release_mlx_memory()
            wav_path = find_synthesized_wav(save_dir, lp_out, voice)
            mt0 = time.time()
            mt_stats = apply_musetalk_to_video(
                lp_out,
                wav_path,
                final,
                Path(args.musetalk),
                musetalk_weights,
                pipe.model_dict["face_analysis"],
            )
            mt_seconds = round(time.time() - mt0, 1)
            clip_info["musetalk_seconds"] = mt_seconds
            clip_info["musetalk"] = mt_stats
            clip_info["seconds"] = round(lp_seconds + mt_seconds, 1)
        else:
            shutil.copy(lp_out, final)
            clip_info["seconds"] = lp_seconds

        report["clips"][name] = clip_info
        print(f"[presence] {name}: {clip_info['seconds']:.1f}s -> {final}", flush=True)

    (out / "report.json").write_text(json.dumps(report, indent=2))
    print(json.dumps(report, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
