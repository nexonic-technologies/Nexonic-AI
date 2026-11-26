"""Turn a generated portrait clip into an app-ready transparent video.

Input: an MP4 of Sharee or Nexa (for example a Veo clip made from their portrait).
Output: an H.264 MP4 twice as wide as the picture: the left half is the colour
picture, the right half is the alpha matte. The app composites the two halves
in WebGL, which plays in every WebKit build without needing HEVC-with-alpha.

Background removal: Robust Video Matting (MobileNetV3, ONNX), run as an offline
tool; its recurrent state keeps the hair edge stable from frame to frame.

Usage:
    python process_clip.py IN.mp4 OUT.mp4 --rvm rvm_mobilenetv3_fp32.onnx [--width 720 --keep 0.8]
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys

import numpy as np


def probe(path: str) -> tuple[int, int, float]:
    out = subprocess.run(
        ["ffprobe", "-v", "error", "-select_streams", "v:0", "-show_entries", "stream=width,height,r_frame_rate", "-of", "json", path],
        check=True, capture_output=True, text=True,
    ).stdout
    s = json.loads(out)["streams"][0]
    num, den = s["r_frame_rate"].split("/")
    return int(s["width"]), int(s["height"]), float(num) / float(den)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("src")
    ap.add_argument("dst")
    ap.add_argument("--rvm", required=True)
    ap.add_argument("--width", type=int, default=720)
    ap.add_argument("--keep", type=float, default=0.8, help="fraction of the height kept from the top (drops the lower torso)")
    ap.add_argument("--crf", type=int, default=19)
    args = ap.parse_args()

    import onnxruntime as ort

    w0, h0, fps = probe(args.src)
    w = args.width - args.width % 2
    h_full = round(h0 * w / w0)
    h = int(h_full * args.keep) // 2 * 2

    dec = subprocess.Popen(
        ["ffmpeg", "-v", "error", "-i", args.src, "-vf", f"scale={w}:{h_full}:flags=lanczos,crop={w}:{h}:0:0", "-f", "rawvideo", "-pix_fmt", "rgb24", "-"],
        stdout=subprocess.PIPE,
    )
    enc = subprocess.Popen(
        ["ffmpeg", "-v", "error", "-y", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s", f"{w * 2}x{h}", "-r", f"{fps}", "-i", "-",
         "-c:v", "libx264", "-preset", "slow", "-crf", str(args.crf), "-pix_fmt", "yuv420p", "-movflags", "+faststart", "-an", args.dst],
        stdin=subprocess.PIPE,
    )
    sess = ort.InferenceSession(args.rvm, providers=["CPUExecutionProvider"])
    rec = [np.zeros((1, 1, 1, 1), np.float32)] * 4
    ratio = np.array([min(1.0, 512 / max(w, h))], np.float32)
    n = 0
    frame_bytes = w * h * 3
    assert dec.stdout is not None and enc.stdin is not None
    while True:
        buf = dec.stdout.read(frame_bytes)
        if len(buf) < frame_bytes:
            break
        rgb = np.frombuffer(buf, np.uint8).reshape(h, w, 3)
        src = (rgb.astype(np.float32) / 255.0).transpose(2, 0, 1)[None]
        fgr, pha, *rec = sess.run(None, {"src": src, "r1i": rec[0], "r2i": rec[1], "r3i": rec[2], "r4i": rec[3], "downsample_ratio": ratio})
        alpha = np.clip(pha[0, 0], 0, 1)
        # keep the original colour (RVM's foreground estimate softens skin); clean the dark spill at the edge
        a3 = (alpha * 255).astype(np.uint8)
        packed = np.concatenate([rgb, np.repeat(a3[:, :, None], 3, axis=2)], axis=1)
        enc.stdin.write(packed.tobytes())
        n += 1
    enc.stdin.close()
    enc.wait()
    dec.wait()
    print(json.dumps({"frames": n, "width": w, "height": h, "fps": fps, "out": args.dst}))
    return 0 if n else 1


if __name__ == "__main__":
    sys.exit(main())
