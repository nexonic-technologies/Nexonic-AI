"""Measure each character's face in their packed idle clip so the app can put
Sharee and Nexa on the same eye line at the same size.

    python face_boxes.py apps/desktop/src/assets/clips

Reads <dir>/<name>/idle.mp4 (colour | alpha packed side by side) and writes
<dir>/manifest.json with {"faces": {name: {cx, cy, w}}} in 0-1 units of the
colour half. The head is measured from the alpha matte of the first frame:
its top, and its width just above the ears, so it needs no face detector.
"""

import json
import sys
from pathlib import Path

import cv2
import numpy as np


def face_box(clip: Path) -> dict:
    cap = cv2.VideoCapture(str(clip))
    ok, frame = cap.read()
    cap.release()
    if not ok:
        raise SystemExit(f"cannot read {clip}")
    alpha = frame[:, frame.shape[1] // 2 :, 0] > 128
    h, w = alpha.shape
    rows = np.where(alpha.any(axis=1))[0]
    if len(rows) == 0:
        raise SystemExit(f"empty matte in {clip}")
    top = int(rows[0])
    widths, centres = [], []
    for y in range(top + int(0.06 * h), top + int(0.12 * h)):
        xs = np.where(alpha[y])[0]
        if len(xs):
            widths.append(xs[-1] - xs[0])
            centres.append((xs[-1] + xs[0]) / 2)
    hw = float(np.median(widths))
    return {"cx": round(float(np.median(centres)) / w, 4), "cy": round((top + 0.62 * hw) / h, 4), "w": round(hw / w, 4)}


def main() -> None:
    root = Path(sys.argv[1])
    faces = {d.name: face_box(d / "idle.mp4") for d in sorted(root.iterdir()) if (d / "idle.mp4").exists()}
    (root / "manifest.json").write_text(json.dumps({"faces": faces}, indent=2) + "\n")
    print(json.dumps(faces))


if __name__ == "__main__":
    main()
