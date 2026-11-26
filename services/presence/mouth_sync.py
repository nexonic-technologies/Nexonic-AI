"""MuseTalk 1.5 MLX mouth and viseme refinement stage for Nexonic AI.

Takes the LivePortrait-animated video (which already carries identity, head
motion, blinks, and upper-face expressions) plus the synthesized speech audio,
detects per-frame face landmarks using FasterLivePortrait's MIT-licensed MLX
landmark model (no InsightFace, no PyTorch), aligns a roll-free 256x256 face
crop for MuseTalk 1.5 MLX, and warps only the feathered lower-face / jaw patch
back onto the tilted head in each original frame.
"""

from __future__ import annotations

import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any, Sequence

import cv2
import numpy as np

MUSETALK_SIZE = 256
# Canonical positions inside the 256x256 upright crop matching MuseTalk's
# S3FD/DWPose framing: y=0 sits just above eyebrows, y=128 lands at the nose
# tip (mask boundary), mouth sits at y=158, and chin clears y=224.
_CANON_EYE_XY = (128.0, 64.0)
_CANON_LIP_XY = (128.0, 158.0)
_CANON_EYE_LIP_DIST = _CANON_LIP_XY[1] - _CANON_EYE_XY[1]


def extract_eye_lip_anchors(landmarks: np.ndarray) -> np.ndarray | None:
    """Return shape (2, 2) float32 anchors: [eye_center, lip_center]."""
    if landmarks is None or landmarks.ndim != 2 or landmarks.shape[0] < 5:
        return None
    n = landmarks.shape[0]
    if n == 203:
        left_eye = np.mean(landmarks[[0, 6, 12, 18]], axis=0)
        right_eye = np.mean(landmarks[[24, 30, 36, 42]], axis=0)
        eye_center = 0.5 * (left_eye + right_eye)
        lip_center = 0.25 * (landmarks[48] + landmarks[66] + landmarks[90] + landmarks[102])
        anchors = np.stack([eye_center, lip_center], axis=0).astype(np.float32)
    elif n == 106:
        left_eye = np.mean(landmarks[[33, 35, 40, 39]], axis=0)
        right_eye = np.mean(landmarks[[87, 89, 94, 93]], axis=0)
        eye_center = 0.5 * (left_eye + right_eye)
        lip_center = 0.5 * (landmarks[52] + landmarks[61])
        anchors = np.stack([eye_center, lip_center], axis=0).astype(np.float32)
    elif n == 68:
        left_eye = np.mean(landmarks[[36, 39]], axis=0)
        right_eye = np.mean(landmarks[[42, 45]], axis=0)
        eye_center = 0.5 * (left_eye + right_eye)
        lip_center = 0.5 * (landmarks[48] + landmarks[54])
        anchors = np.stack([eye_center, lip_center], axis=0).astype(np.float32)
    else:
        min_xy = np.min(landmarks, axis=0)
        max_xy = np.max(landmarks, axis=0)
        cx = 0.5 * (min_xy[0] + max_xy[0])
        h = max_xy[1] - min_xy[1]
        anchors = np.array(
            [[cx, min_xy[1] + 0.35 * h], [cx, min_xy[1] + 0.68 * h]],
            dtype=np.float32,
        )

    if float(np.linalg.norm(anchors[1] - anchors[0])) < 8.0:
        return None
    return anchors


def smooth_anchors(anchors: Sequence[np.ndarray | None], alpha: float = 0.45) -> list[np.ndarray | None]:
    """Fill missing anchor frames and apply EMA smoothing to eliminate jitter."""
    valid = [a for a in anchors if a is not None]
    if not valid:
        return list(anchors)

    filled: list[np.ndarray] = []
    last = valid[0]
    for a in anchors:
        if a is not None:
            last = a
        filled.append(last)

    smoothed: list[np.ndarray | None] = []
    cur = filled[0].astype(np.float32).copy()
    for a in filled:
        cur = alpha * a.astype(np.float32) + (1.0 - alpha) * cur
        smoothed.append(cur.copy())
    return smoothed


def similarity_matrix_to_canonical(anchors: np.ndarray) -> np.ndarray:
    """Build 2x3 affine matrix mapping (eye_center, lip_center) -> upright 256x256."""
    eye = anchors[0].astype(np.float32)
    lip = anchors[1].astype(np.float32)
    down = lip - eye
    ortho = np.array([-down[1], down[0]], dtype=np.float32)
    src = np.stack([eye, lip, eye + ortho], axis=0)
    dst = np.array(
        [
            _CANON_EYE_XY,
            _CANON_LIP_XY,
            (_CANON_EYE_XY[0] - _CANON_EYE_LIP_DIST, _CANON_EYE_XY[1]),
        ],
        dtype=np.float32,
    )
    return cv2.getAffineTransform(src, dst)


def build_mouth_blend_mask(height: int = MUSETALK_SIZE, width: int = MUSETALK_SIZE) -> np.ndarray:
    """Create a soft (H, W, 1) float32 mask tightly framing the mouth and chin."""
    mask = np.zeros((height, width), dtype=np.float32)
    cy = int(round(height * 0.65))
    cx = int(round(width * 0.50))
    ry = max(4, int(round(height * 0.145)))
    rx = max(4, int(round(width * 0.205)))
    cv2.ellipse(mask, (cx, cy), (rx, ry), 0, 0, 360, 1.0, -1)
    # Keep upper face / nose tip untouched (matches MuseTalk's y=128 split)
    mask[: int(round(height * 0.50)), :] = 0.0
    k = max(5, (min(height, width) // 12) | 1)
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

    raw_anchors: list[np.ndarray | None] = []
    for frame in frames:
        faces = face_analyzer.predict(frame)
        raw_anchors.append(extract_eye_lip_anchors(faces[0]) if faces else None)
    anchors = smooth_anchors(raw_anchors)

    pipe = MuseTalkPipeline.from_pretrained_mlx(weights_dir)
    chunks = pipe.encode_audio_from_wav(str(audio_path), fps=int(round(fps)))
    n_audio = int(chunks.shape[0])
    n_frames = min(len(frames), n_audio)
    if n_frames == 0:
        raise RuntimeError("Audio produced 0 MuseTalk chunks")

    latents = []
    crops: dict[int, np.ndarray] = {}
    matrices: dict[int, np.ndarray] = {}
    valid_indices: list[int] = []
    for i in range(n_frames):
        anc = anchors[i]
        if anc is None:
            continue
        m_fwd = similarity_matrix_to_canonical(anc)
        crop = cv2.warpAffine(
            frames[i],
            m_fwd,
            (MUSETALK_SIZE, MUSETALK_SIZE),
            flags=cv2.INTER_LANCZOS4,
            borderMode=cv2.BORDER_REPLICATE,
        )
        latents.append(pipe.get_latents_for_unet(crop))
        crops[i] = crop
        matrices[i] = m_fwd
        valid_indices.append(i)

    if not valid_indices:
        raise RuntimeError(f"No faces detected across {n_frames} frames in {video_path}")

    lat_stack = mx.concatenate(latents, axis=0)
    idx_mx = mx.array(valid_indices)
    recon = pipe.run_batched(lat_stack, chunks[idx_mx], batch_size=batch_size)

    recon_by_frame = {frame_idx: recon[pos] for pos, frame_idx in enumerate(valid_indices)}
    canon_mask = build_mouth_blend_mask(MUSETALK_SIZE, MUSETALK_SIZE)
    h, w = frames[0].shape[:2]

    with tempfile.TemporaryDirectory(prefix="musetalk-out-") as tmp:
        silent_path = Path(tmp) / "silent.mp4"
        fourcc = cv2.VideoWriter_fourcc(*"mp4v")
        writer = cv2.VideoWriter(str(silent_path), fourcc, float(fps), (w, h))
        for i, frame in enumerate(frames):
            gen = recon_by_frame.get(i)
            m_fwd = matrices.get(i)
            if gen is None or m_fwd is None:
                writer.write(frame)
                continue
            matched = match_skin_tone(gen, crops[i], canon_mask)
            m_inv = cv2.invertAffineTransform(m_fwd)
            warped_patch = cv2.warpAffine(
                matched,
                m_inv,
                (w, h),
                flags=cv2.INTER_LANCZOS4,
                borderMode=cv2.BORDER_REPLICATE,
            )
            warped_mask = cv2.warpAffine(
                canon_mask[..., 0],
                m_inv,
                (w, h),
                flags=cv2.INTER_LINEAR,
                borderMode=cv2.BORDER_CONSTANT,
                borderValue=0.0,
            )[..., None]
            blended = (
                warped_patch.astype(np.float32) * warped_mask
                + frame.astype(np.float32) * (1.0 - warped_mask)
            )
            writer.write(np.clip(blended, 0, 255).astype(np.uint8))
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
