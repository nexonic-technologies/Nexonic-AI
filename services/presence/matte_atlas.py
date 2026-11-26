"""Cut Sharee and Nexa out of their background in every atlas frame.

Runs BiRefNet (MIT, ZhengPeng7/BiRefNet-portrait) on the Apple Silicon GPU
(MPS) when available, otherwise on the CPU and rewrites each frame as a WebP with an alpha channel, so the
characters float on the presence stage instead of sitting in a dark box.
Each head pose and expression gets its own matte, so the cut-out follows every
head turn; the half-closed and closed-lid frames reuse the matte of their
open-eye frame (blinks do not change the silhouette).

Usage: python matte_atlas.py --atlas /tmp/atlas [--revision <sha>]
"""

from __future__ import annotations

import argparse
import time
from pathlib import Path

REPO_ID = "ZhengPeng7/BiRefNet-portrait"


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--atlas", required=True)
    ap.add_argument("--revision", default=None)
    ap.add_argument("--size", type=int, default=1024)
    ap.add_argument("--quality", type=int, default=82)
    args = ap.parse_args()

    import numpy as np
    import torch
    from huggingface_hub import snapshot_download
    from PIL import Image, ImageFilter
    from transformers import AutoModelForImageSegmentation

    path = snapshot_download(REPO_ID, revision=args.revision)
    print(f"[matte] model snapshot: {path}", flush=True)
    device = "mps" if torch.backends.mps.is_available() else "cpu"
    model = AutoModelForImageSegmentation.from_pretrained(path, trust_remote_code=True)
    model.to(device).eval()
    if device == "mps":
        model.half()
    dtype = torch.float16 if device == "mps" else torch.float32
    mean = torch.tensor([0.485, 0.456, 0.406]).view(1, 3, 1, 1)
    std = torch.tensor([0.229, 0.224, 0.225]).view(1, 3, 1, 1)

    frames = sorted(Path(args.atlas).glob("*/*.webp"))
    open_frames = [f for f in frames if f.stem.endswith("_o")]
    print(f"[matte] {len(open_frames)} poses to matte on {device}", flush=True)
    t0 = time.time()
    for n, f in enumerate(open_frames, 1):
        img = Image.open(f).convert("RGB")
        x = torch.from_numpy(np.asarray(img.resize((args.size, args.size), Image.BICUBIC), dtype=np.float32) / 255.0)
        x = ((x.permute(2, 0, 1)[None] - mean) / std).to(device, dtype)
        with torch.no_grad():
            pred = model(x)[-1].sigmoid().float().cpu()[0, 0].numpy()
        alpha = Image.fromarray((pred * 255).clip(0, 255).astype(np.uint8)).resize(img.size, Image.BICUBIC)
        # soften the very edge a touch so hair blends into the stage glow
        alpha = alpha.filter(ImageFilter.GaussianBlur(0.6))
        for variant in (f, f.with_name(f.stem[:-2] + "_h.webp"), f.with_name(f.stem[:-2] + "_c.webp")):
            if not variant.exists():
                continue
            rgba = Image.open(variant).convert("RGB") if variant != f else img.copy()
            rgba.putalpha(alpha)
            rgba.save(variant, quality=args.quality, alpha_quality=90, method=6)
        if n % 5 == 0:
            print(f"[matte] {n}/{len(open_frames)} ({time.time() - t0:.0f}s)", flush=True)
    print(f"[matte] {len(frames)} frames in {time.time() - t0:.1f}s on {device}", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
