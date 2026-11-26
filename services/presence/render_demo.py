"""Render talking test clips of Sharee and Nexa entirely on Apple Silicon.

Pipeline (all MIT/Apache, on-device, no cloud):
  text --Kokoro (mlx-audio)--> speech audio
       --JoyVASA (MLX)-------> head pose + expression motion from the audio
       --LivePortrait (MLX)--> animated portrait (eyes, brows, lips, head)

Usage (from a FasterLivePortrait-MLX checkout with `uv sync` done):
  uv run python /path/to/render_demo.py --flp . --out /tmp/presence-out

This is the M15 feasibility probe: it proves quality and measures speed on
real Apple Silicon before the presence engine is wired into the app.
"""

from __future__ import annotations

import argparse
import json
import os
import platform
import shutil
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent

AGENTS = {
    "sharee": {
        "image": HERE / "sources" / "sharee.png",
        "voice": "am_michael",
        "text": (
            "Hi, I'm Sharee. Nexa and I have been looking at your week. "
            "There's one thing I'd change first, and I think you'll like it."
        ),
    },
    "nexa": {
        "image": HERE / "sources" / "nexa.png",
        "voice": "af_heart",
        "text": (
            "And I'm Nexa. Sharee's right, mostly. "
            "Let's keep the mornings clear for deep work, and I'll handle the rest."
        ),
    },
}


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--flp", required=True, help="path to the FasterLivePortrait-MLX checkout")
    ap.add_argument("--out", required=True)
    ap.add_argument("--agents", default="sharee,nexa")
    args = ap.parse_args()

    flp = Path(args.flp).resolve()
    out = Path(args.out).resolve()
    out.mkdir(parents=True, exist_ok=True)
    os.chdir(flp)  # FLP resolves checkpoints and configs relative to its root
    sys.path.insert(0, str(flp))

    import mlx.core as mx  # noqa: E402
    from omegaconf import OmegaConf  # noqa: E402

    from src.pipelines.gradio_live_portrait_pipeline import GradioLivePortraitPipeline  # noqa: E402
    from src.runtime_assets import ensure_runtime_assets  # noqa: E402
    from src.utils.mlx_profiles import apply_mlx_profile  # noqa: E402

    report: dict = {
        "machine": platform.platform(),
        "chip": platform.processor() or platform.machine(),
        "mlx_device": str(mx.default_device()),
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
        spec = AGENTS[name]
        save_dir = out / name
        save_dir.mkdir(exist_ok=True)
        t = time.time()
        org, crop, _ = pipe.run_text_driving(spec["text"], spec["voice"], str(spec["image"]), save_dir=str(save_dir))
        elapsed = time.time() - t
        final = out / f"{name}-talking.mp4"
        shutil.copy(org, final)
        shutil.copy(crop, out / f"{name}-talking-face.mp4")
        report["clips"][name] = {"voice": spec["voice"], "seconds": round(elapsed, 1), "file": final.name}
        print(f"[presence] {name}: {elapsed:.1f}s -> {final}", flush=True)

    (out / "report.json").write_text(json.dumps(report, indent=2))
    print(json.dumps(report, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
