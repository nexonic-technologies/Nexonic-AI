"""Unit tests for the presence persona specification and MuseTalk crop/blend math."""

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
        build_mouth_blend_mask,
        compute_musetalk_box,
        match_skin_tone,
        smooth_boxes,
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
    def test_compute_box_203_points_stays_in_bounds(self) -> None:
        lmk = np.zeros((203, 2), dtype=np.float32)
        lmk[:, 0] = np.linspace(120, 220, 203)
        lmk[:, 1] = np.linspace(100, 230, 203)
        lmk[[0, 6, 12, 18], 1] = 130.0
        lmk[[24, 30, 36, 42], 1] = 130.0
        lmk[48, 1] = 190.0
        lmk[66, 1] = 196.0
        box = compute_musetalk_box(lmk, (320, 340, 3), extra_margin=10)
        self.assertIsNotNone(box)
        x1, y1, x2, y2 = box  # type: ignore[misc]
        self.assertGreaterEqual(x1, 0)
        self.assertGreaterEqual(y1, 0)
        self.assertLessEqual(x2, 340)
        self.assertLessEqual(y2, 320)
        self.assertGreater(x2 - x1, 40)
        self.assertGreater(y2 - y1, 40)

    def test_smooth_boxes_fills_gaps(self) -> None:
        boxes = [(10, 10, 110, 110), None, (14, 14, 114, 114)]
        out = smooth_boxes(boxes, alpha=0.5)
        self.assertEqual(len(out), 3)
        self.assertTrue(all(b is not None for b in out))

    def test_mouth_mask_keeps_upper_face_zero(self) -> None:
        mask = build_mouth_blend_mask(256, 256)
        self.assertEqual(mask.shape, (256, 256, 1))
        self.assertLess(float(mask[:90, :, 0].max()), 0.05)
        self.assertGreater(float(mask[175:205, 110:145, 0].mean()), 0.5)

    def test_match_skin_tone_preserves_shape_and_dtype(self) -> None:
        gen = np.full((64, 64, 3), 100, dtype=np.uint8)
        tgt = np.full((64, 64, 3), 110, dtype=np.uint8)
        mask = build_mouth_blend_mask(64, 64)
        out = match_skin_tone(gen, tgt, mask)
        self.assertEqual(out.shape, gen.shape)
        self.assertEqual(out.dtype, np.uint8)


if __name__ == "__main__":
    unittest.main()
