"""MuseTalk 1.5 MLX mouth and viseme refinement stage for Nexonic AI.

Takes the LivePortrait-animated video (which already carries identity, head
motion, blinks, and upper-face expressions) plus the synthesized speech audio,
detects per-frame face boxes using FasterLivePortrait's MIT-licensed MLX
landmark model (no InsightFace, no PyTorch), runs MuseTalk 1.5 in MLX, and
feathers only the lower-face / mouth region back onto each frame.
"""

from __future__ import annotations

import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any, Sequence

import cv2
import numpy as np

Box = tuple[int, int, int, int]


def _nose_bridge_y(landmarks: np.ndarray) -> float:
    """Estimate the vertical midpoint (nose bridge) for MuseTalk's 256x256 crop.

    MuseTalk masks the lower half (y >= 128) of the 256x256 face crop and keeps
    the upper half as identity reference, so the midpoint must sit on the nose
    bridge between the eyes and mouth.
    """
    n = landmarks.shape[0]
    if n == 203:
        left_eye = np.mean(landmarks[[0, 6, 12, 18]], axis=0)
        right_eye = np.mean(landmarks[[24, 30, 36, 42]], axis=0)
        eye_y = float(0.5 * (left_eye[1] + right_eye[1]))
        lip_y = float(0.5 * (landmarks[48, 1] + landmarks[66, 1]))
        return 0.62 * eye_y + 0.38 * lip_y
    if n == 106:
        left_eye = np.mean(landmarks[[33, 35, 40, 39]], axis=0)
        right_eye = np.mean(landmarks[[87, 89, 94, 93]], axis=0)
        eye_y = float(0.5 * (left_eye[1] + right_eye[1]))
        lip_y = float(0.5 * (landmarks[52, 1] + landmarks[61, 1]))
        return 0.62 * eye_y + 0.38 * lip_y
    if n == 68:
        return float(landmarks[29, 1])

    min_y = float(np.min(landmarks[:, 1]))
    max_y = float(np.max(landmarks[:, 1]))
    return min_y + 0.44 * (max_y - min_y)


def compute_musetalk_box(
    landmarks: np.ndarray,
    frame_shape: tuple[int, ...],
    extra_margin: int = 10,
) -> Box | None:
    """Compute a near-square (x1, y1, x2, y2) crop aligned for MuseTalk 1.5."""
    if landmarks is None or landmarks.ndim != 2 or landmarks.shape[0] < 5:
        return None
    h, w = int(frame_shape[0]), int(frame_shape[1])
    if h <= 0 or w <= 0:
        return None

    min_x = float(np.min(landmarks[:, 0]))
    max_x = float(np.max(landmarks[:, 0]))
    chin_y = float(np.max(landmarks[:, 1]))
    half_y = _nose_bridge_y(landmarks)
    half_dist = max(12.0, chin_y - half_y)

    y1 = max(0, int(round(half_y - half_dist)))
    y2 = min(h, int(round(chin_y + extra_margin)))
    if y2 - y1 < 32:
        return None

    cx = 0.5 * (min_x + max_x)
    face_w = max(16.0, max_x - min_x)
    half_w = max(0.5 * face_w * 1.12, 0.5 * (y2 - y1))
    x1 = max(0, int(round(cx - half_w)))
    x2 = min(w, int(round(cx + half_w)))
    if x2 - x1 < 32:
        return None
    return (x1, y1, x2, y2)


def smooth_boxes(boxes: Sequence[Box | None], alpha: float = 0.35) -> list[Box | None]:
    """Fill missing detections and apply EMA smoothing to stop crop jitter."""
    valid = [b for b in boxes if b is not None]
    if not valid:
        return list(boxes)

    filled: list[Box] = []
    last = valid[0]
    for b in boxes:
        if b is not None:
            last = b
        filled.append(last)

    smoothed: list[Box | None] = []
    cur = np.array(filled[0], dtype=np.float32)
    for b in filled:
        cur = alpha * np.array(b, dtype=np.float32) + (1.0 - alpha) * cur
        smoothed.append((int(round(cur[0])), int(round(cur[1])), int(round(cur[2])), int(round(cur[3]))))
    return smoothed


def build_mouth_blend_mask(height: int, width: int) -> np.ndarray:
    """Create a soft (H, W, 1) float32 mask targeting the lower face and jaw."""
    mask = np.zeros((height, width), dtype=np.float32)
    cy = int(round(height * 0.72))
    cx = int(round(width * 0.50))
    ry = max(4, int(round(height * 0.22)))
    rx = max(4, int(round(width * 0.33)))
    cv2.ellipse(mask, (cx, cy), (rx, ry), 0, 0, 360, 1.0, -1)
    # Keep upper face / nose bridge untouched
    mask[: int(round(height * 0.46)), :] = 0.0
    k = max(5, (min(height, width) // 8) | 1)
    mask = cv2.GaussianBlur(mask, (k, k), 0)
    peak = float(mask.max())
    if peak > 1e-6:
        mask /= peak
    return mask[..., None]


def match_skin_tone(generated_bgr: np.ndarray, target_bgr: np.ndarray, mask: np.ndarray) -> np.ndarray:
    """Align subtle luminance/color offset inside the border ring to avoid seams."""
    ring = (mask[..., 0] > 0.05) & (mask[..., 0] < 0.45)
    if int(np.count_nonzero(ring)) < 16:
        return generated_bgr
    gen_f = generated_bgr.astype(np.float32)
    tgt_f = target_bgr.astype(np.float32)
    delta = np.mean(tgt_f[ring] - gen_f[ring], axis=0)
    delta = np.clip(delta, -14.0, 14.0)
    return np.clip(gen_f + delta, 0, 255).astype(np.uint8)


def apply_musetalk_to_video(
    video_path: Path,
    audio_path: Path,
    out_path: Path,
    musetalk_repo: Path,
    weights_dir: Path,
    face_analyzer: Any,
    *,
    extra_margin: int = 10,
    batch_size: int = 8,
) -> dict[str, Any]:
    """Refine mouth visemes on an animated talking video using MuseTalk 1.5 MLX."""
    repo_str = str(musetalk_repo.resolve())
    if repo_str not in sys.path:
        sys.path.insert(0, repo_str)

    import mlx.core as mx
    from musetalk_mlx.pipeline_mlx import MuseTalkPipeline

    mx.set_default_device(mx.gpu)

    cap = cv2.VideoCapture(str(video_path))
    if not cap.isOpened():
        raise RuntimeError(f"Could not open video for MuseTalk pass: {video_path}")
    fps = cap.get(cv2.CAP_PROP_FPS) or 25.0
    frames: list[np.ndarray] = []
    while True:
        ok, frame = cap.read()
        if not ok:
            break
        frames.append(frame)
    cap.release()
    if not frames:
        raise RuntimeError(f"Video contained 0 frames: {video_path}")

    raw_boxes: list[Box | None] = []
    for frame in frames:
        faces = face_analyzer.predict(frame)
        if faces:
            raw_boxes.append(compute_musetalk_box(faces[0], frame.shape, extra_margin=extra_margin))
        else:
            raw_boxes.append(None)
    boxes = smooth_boxes(raw_boxes)

    pipe = MuseTalkPipeline.from_pretrained_mlx(weights_dir)
    chunks = pipe.encode_audio_from_wav(str(audio_path), fps=int(round(fps)))
    n_audio = int(chunks.shape[0])
    n_frames = min(len(frames), n_audio)
    if n_frames == 0:
        raise RuntimeError("Audio produced 0 MuseTalk chunks")

    latents = []
    valid_indices: list[int] = []
    for i in range(n_frames):
        box = boxes[i]
        if box is None:
            continue
        x1, y1, x2, y2 = box
        crop = cv2.resize(frames[i][y1:y2, x1:x2], (256, 256), interpolation=cv2.INTER_LANCZOS4)
        latents.append(pipe.get_latents_for_unet(crop))
        valid_indices.append(i)

    if not valid_indices:
        raise RuntimeError(f"No faces detected across {n_frames} frames in {video_path}")

    lat_stack = mx.concatenate(latents, axis=0)
    idx_mx = mx.array(valid_indices)
    recon = pipe.run_batched(lat_stack, chunks[idx_mx], batch_size=batch_size)

    recon_by_frame = {frame_idx: recon[pos] for pos, frame_idx in enumerate(valid_indices)}
    h, w = frames[0].shape[:2]

    with tempfile.TemporaryDirectory(prefix="musetalk-out-") as tmp:
        silent_path = Path(tmp) / "silent.mp4"
        fourcc = cv2.VideoWriter_fourcc(*"mp4v")
        writer = cv2.VideoWriter(str(silent_path), fourcc, float(fps), (w, h))
        for i, frame in enumerate(frames):
            box = boxes[i] if i < len(boxes) else None
            gen = recon_by_frame.get(i)
            if box is None or gen is None:
                writer.write(frame)
                continue
            x1, y1, x2, y2 = box
            patch = cv2.resize(gen, (x2 - x1, y2 - y1), interpolation=cv2.INTER_LANCZOS4)
            target = frame[y1:y2, x1:x2]
            mask = build_mouth_blend_mask(y2 - y1, x2 - x1)
            matched = match_skin_tone(patch, target, mask)
            blended = (matched.astype(np.float32) * mask + target.astype(np.float32) * (1.0 - mask))
            out_frame = frame.copy()
            out_frame[y1:y2, x1:x2] = np.clip(blended, 0, 255).astype(np.uint8)
            writer.write(out_frame)
        writer.release()

        out_path.parent.mkdir(parents=True, exist_ok=True)
        subprocess.run(
            [
                "ffmpeg",
                "-y",
                "-v",
                "error",
                "-i",
                str(silent_path),
                "-i",
                str(audio_path),
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                "-crf",
                "18",
                "-c:a",
                "aac",
                "-b:a",
                "192k",
                "-shortest",
                str(out_path),
            ],
            check=True,
        )

    return {
        "frames": len(frames),
        "audioChunks": n_audio,
        "syncedFrames": len(valid_indices),
        "fps": round(float(fps), 2),
    }
