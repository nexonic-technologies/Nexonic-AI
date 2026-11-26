"""Render the live-portrait frame atlas for Sharee and Nexa on Apple Silicon.

Every frame is a real LivePortrait re-animation of the character's own
neutral portrait (FasterLivePortrait-MLX): the head turns, the eyes move and
close, and the face takes each expression from the personality datasheets.
The desktop app plays these frames back to follow the cursor, blink, breathe
and change mood until the real-time presence sidecar (M15) replaces them.

Usage (inside the FasterLivePortrait-MLX checkout):
    python render_atlas.py --flp . --out /tmp/atlas
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parent.parent
PORTRAITS = REPO / "apps" / "desktop" / "src" / "assets" / "characters"

# Head-pose grid for cursor following (degrees). The eyes lead the head.
YAWS = (-14, -7, 0, 7, 14)
PITCHES = (-8, 0, 8)
# Expressions share the head-turn grid so Sharee and Nexa can smile at each other.
EXPR_YAWS = YAWS
# Blinks use LivePortrait's eye-retargeting network (target eye-open ratio).
EYE_RATIO = {"h": 0.17, "c": 0.0}


def load_spec() -> dict:
    return json.loads((HERE / "atlas_spec.json").read_text())


def apply_controls(delta, c: dict) -> tuple[float, float, float]:
    """Map facial controls onto implicit-keypoint offsets (21 keypoints x 3).

    Keypoint roles follow the LivePortrait implicit keypoints: 1/2 brows,
    11/15 eyes, 13/16 lids and cheeks, 14/17/19/20 mouth and lips, 3/7 cheeks.
    Returns extra (pitch, yaw, roll) in degrees.
    """
    smile = c.get("smile", 0.0)
    mouth = c.get("mouth", 0.0)
    eyes = c.get("eyes", 0.0)
    brow = c.get("brow", 0.0)
    px = c.get("pupil_x", 0.0)
    py = c.get("pupil_y", 0.0)
    pitch, yaw, roll = c.get("pitch", 0.0), c.get("yaw", 0.0), c.get("roll", 0.0)

    d = delta[0]
    # AU6 + AU12: cheek raise and lip-corner pull
    d[20, 1] += smile * -0.01
    d[14, 1] += smile * -0.02
    d[17, 1] += smile * 0.0065
    d[17, 2] += smile * 0.003
    d[13, 1] += smile * -0.00275
    d[16, 1] += smile * -0.00275
    d[3, 1] += smile * -0.0035
    d[7, 1] += smile * -0.0035
    # AU25/26: lips part, jaw drop
    d[19, 1] += mouth * 0.001
    d[19, 2] += mouth * 0.0001
    d[17, 1] += mouth * -0.0001
    pitch -= mouth * 0.05
    # gaze (pupils)
    if px > 0:
        d[11, 0] += px * 0.0007
        d[15, 0] += px * 0.001
    else:
        d[11, 0] += px * 0.001
        d[15, 0] += px * 0.0007
    d[11, 1] += py * -0.001
    d[15, 1] += py * -0.001
    eyes -= py / 2.0
    # AU5 / AU7 / AU45: lids (negative closes)
    d[11, 1] += eyes * -0.001
    d[13, 1] += eyes * 0.0003
    d[15, 1] += eyes * -0.001
    d[16, 1] += eyes * 0.0003
    d[1, 1] += eyes * -0.00025
    d[2, 1] += eyes * 0.00025
    # AU1/AU2 (raise, positive) and AU4 (lower, negative)
    if brow > 0:
        d[1, 1] += brow * 0.001
        d[2, 1] += brow * -0.001
    else:
        d[1, 0] += brow * -0.001
        d[2, 0] += brow * 0.001
        d[1, 1] += brow * 0.0003
        d[2, 1] += brow * -0.0003
    return pitch, yaw, roll


def frame_plan(spec: dict, persona: str) -> list[tuple[str, dict]]:
    p = spec["personas"][persona]
    head = p["headRange"]  # fraction of the grid angle this persona turns
    gaze = spec["gazeGain"]
    plan: list[tuple[str, dict]] = []

    def pose(yaw: float, pitch: float) -> dict:
        return {
            "yaw": yaw * head,
            "pitch": pitch * head,
            "pupil_x": max(-15.0, min(15.0, yaw * gaze)),
            "pupil_y": max(-15.0, min(15.0, -pitch * gaze)),
        }

    for pitch in PITCHES:
        for yaw in YAWS:
            for eye in ("o", "h", "c"):
                c = pose(yaw, pitch)
                if eye in EYE_RATIO:
                    c["eye_ratio"] = EYE_RATIO[eye]
                plan.append((f"g_{yaw}_{pitch}_{eye}", c))

    intensity = p["intensity"]
    for name, base in spec["expressions"].items():
        scale = intensity.get(name, 1.0)
        for yaw in EXPR_YAWS:
            for eye in ("o", "c"):
                c = pose(yaw, 0)
                for k, v in base.items():
                    if not k.startswith("_"):
                        c[k] = c.get(k, 0.0) + v * scale
                if eye in EYE_RATIO:
                    c["eye_ratio"] = EYE_RATIO[eye]
                plan.append((f"e_{name}_{yaw}_{eye}", c))
    return plan


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--flp", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--agents", default="sharee,nexa")
    ap.add_argument("--width", type=int, default=560)
    ap.add_argument("--quality", type=int, default=80)
    args = ap.parse_args()

    spec = load_spec()
    flp = Path(args.flp).resolve()
    out = Path(args.out).resolve()
    out.mkdir(parents=True, exist_ok=True)
    os.chdir(flp)
    sys.path.insert(0, str(flp))

    import numpy as np  # noqa: E402
    from omegaconf import OmegaConf  # noqa: E402
    from PIL import Image  # noqa: E402

    from src.pipelines.gradio_live_portrait_pipeline import GradioLivePortraitPipeline  # noqa: E402
    from src.runtime_assets import ensure_runtime_assets  # noqa: E402
    from src.utils.crop import paste_back_numpy  # noqa: E402
    from src.utils.mlx_profiles import apply_mlx_profile  # noqa: E402
    from src.utils.utils import get_rotation_matrix  # noqa: E402

    cfg = OmegaConf.load("configs/mlx_infer.yaml")
    ensure_runtime_assets(cfg)
    apply_mlx_profile("quality")
    pipe = GradioLivePortraitPipeline(cfg)
    pipe.set_mlx_profile("quality")
    pipe.init_models(is_animal=False)
    warp = pipe.model_dict["warping_spade"]

    manifest: dict = {"version": 1, "yaws": YAWS, "pitches": PITCHES, "exprYaws": EXPR_YAWS, "personas": {}}
    for name in [a.strip() for a in args.agents.split(",") if a.strip()]:
        src_png = out / f"_{name}-source.png"
        Image.open(PORTRAITS / f"{name}.webp").convert("RGB").save(src_png)
        if not pipe.prepare_source(str(src_png)):
            raise SystemExit(f"{name}: {pipe.prepare_source_error}")
        info = pipe.src_infos[0][0]
        x_s_info, src_lmk, _R, f_s, x_s, _xcs = info[:6]
        mask_ori, m_c2o = info[-2], info[-1]
        img = pipe.src_imgs[0]
        kp = x_s_info["kp"].reshape(1, -1, 3)
        exp = x_s_info["exp"].reshape(1, -1, 3)
        scale = np.asarray(x_s_info["scale"]).reshape(1, 1, 1)
        t = np.asarray(x_s_info["t"]).reshape(1, -1)

        folder = out / name
        folder.mkdir(exist_ok=True)
        h = round(img.shape[0] * args.width / img.shape[1])
        plan = frame_plan(spec, name)
        t0 = time.time()
        for key, c in plan:
            delta = np.zeros_like(exp)
            dp, dy, dr = apply_controls(delta, c)
            R = get_rotation_matrix(x_s_info["pitch"] + dp, x_s_info["yaw"] + dy, x_s_info["roll"] + dr)
            x_d = (kp @ R + exp + delta) * scale
            x_d[:, :, 0:2] += t[:, None, 0:2]
            if "eye_ratio" in c:
                ratio = pipe.calc_combined_eye_ratio([[c["eye_ratio"]]], src_lmk)
                x_d = x_d + pipe.retarget_eye(x_s, ratio).reshape(x_d.shape)
            x_d = pipe.stitching(x_s, x_d.astype(np.float32))
            if hasattr(warp, "reset_temporal_cache"):
                warp.reset_temporal_cache()  # every atlas frame is independent
            if getattr(warp, "predict_type", None) == "mlx":
                crop = warp.predict(f_s, x_s, x_d, return_numpy=True, return_uint8=True)
            else:
                crop = warp.predict(f_s, x_s, x_d)
            full = paste_back_numpy(crop, m_c2o, img.copy(), mask_ori)
            frame = Image.fromarray(np.asarray(full, dtype=np.uint8)).resize((args.width, h), Image.LANCZOS)
            frame.save(folder / f"{key}.webp", quality=args.quality, method=6)
        secs = round(time.time() - t0, 1)
        manifest["personas"][name] = {"width": args.width, "height": h, "frames": [k for k, _ in plan], "seconds": secs}
        print(f"[atlas] {name}: {len(plan)} frames in {secs}s", flush=True)
        src_png.unlink()

    (out / "manifest.json").write_text(json.dumps(manifest, indent=2))
    print(json.dumps({k: {"frames": len(v["frames"]), "seconds": v["seconds"]} for k, v in manifest["personas"].items()}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
