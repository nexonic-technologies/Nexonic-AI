"""Unit tests for the presence persona specification and MuseTalk alignment math."""

from __future__ import annotations

import json
import sys
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

try:
    import cv2  # noqa: F401
    import numpy as np
    from mouth_sync import (
        MUSETALK_SIZE,
        build_mouth_blend_mask,
        extract_eye_lip_anchors,
        match_skin_tone,
        similarity_matrix_to_canonical,
        smooth_anchors,
    )

    HAS_CV = True
except ImportError:
    HAS_CV = False


class PersonasSpecTests(unittest.TestCase):
    def test_personas_schema_and_sources_exist(self) -> None:
        doc = json.loads((HERE / "personas.json").read_text(encoding="utf-8"))
        self.assertIn("visemes", doc)
        self.assertGreaterEqual(len(doc["visemes"]), 12)
        for key in ("sharee", "nexa"):
            p = doc["personas"][key]
            self.assertTrue((HERE / p["sourcePortrait"]).exists())
            self.assertIn("speaking", p["expressions"])
            self.assertIn("idle", p["expressions"])
            self.assertIn("listening", p["expressions"])
            self.assertIn("thinking", p["expressions"])
            for expr in p["expressions"].values():
                self.assertGreater(expr["cfgScale"], 0)
                self.assertGreater(expr["drivingMultiplier"], 0)


@unittest.skipUnless(HAS_CV, "cv2 and numpy required for mouth_sync geometry tests")
class MouthSyncGeometryTests(unittest.TestCase):
    def test_similarity_warp_uprights_tilted_face(self) -> None:
        lmk = np.zeros((203, 2), dtype=np.float32)
        # Simulate a tilted head (eye line sloped downward-right, lip shifted left)
        lmk[[0, 6, 12, 18]] = np.array([150.0, 125.0], dtype=np.float32)
        lmk[[24, 30, 36, 42]] = np.array([210.0, 145.0], dtype=np.float32)
        lmk[48] = np.array([145.0, 195.0], dtype=np.float32)
        lmk[66] = np.array([175.0, 205.0], dtype=np.float32)

        anc = extract_eye_lip_anchors(lmk)
        self.assertIsNotNone(anc)
        m = similarity_matrix_to_canonical(anc)  # type: ignore[arg-type]
        pts = np.hstack([anc, np.ones((2, 1), dtype=np.float32)]) @ m.T  # type: ignore[arg-type]
        np.testing.assert_allclose(pts[0], [128.0, 86.0], atol=1e-3)
        np.testing.assert_allclose(pts[1], [128.0, 170.5], atol=1e-3)

    def test_smooth_anchors_fills_gaps(self) -> None:
        a0 = np.array([[100.0, 100.0], [100.0, 170.0]], dtype=np.float32)
        a2 = np.array([[104.0, 102.0], [104.0, 172.0]], dtype=np.float32)
        out = smooth_anchors([a0, None, a2], alpha=0.5)
        self.assertEqual(len(out), 3)
        self.assertTrue(all(a is not None for a in out))

    def test_mouth_mask_keeps_upper_face_zero(self) -> None:
        mask = build_mouth_blend_mask(MUSETALK_SIZE, MUSETALK_SIZE)
        self.assertEqual(mask.shape, (MUSETALK_SIZE, MUSETALK_SIZE, 1))
        self.assertLess(float(mask[:105, :, 0].max()), 0.05)
        self.assertGreater(float(mask[165:195, 110:145, 0].mean()), 0.5)

    def test_match_skin_tone_preserves_shape_and_dtype(self) -> None:
        gen = np.full((MUSETALK_SIZE, MUSETALK_SIZE, 3), 100, dtype=np.uint8)
        tgt = np.full((MUSETALK_SIZE, MUSETALK_SIZE, 3), 110, dtype=np.uint8)
        mask = build_mouth_blend_mask(MUSETALK_SIZE, MUSETALK_SIZE)
        out = match_skin_tone(gen, tgt, mask)
        self.assertEqual(out.shape, gen.shape)
        self.assertEqual(out.dtype, np.uint8)


if __name__ == "__main__":
    unittest.main()
