"""Fixtures for 1.0/1.5/2.0 real Niri logical-scale acceptance.

Mocks verify the harness contract, not graphical success or source/main parity.
"""
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))

from dev_preview import PreviewError, acceptance_output_scale, preview_config
from graphical_scale_matrix import (
    CAPTURES_PER_SCALE, PHYSICAL_VIEWPORT, SCALES, run_matrix,
)


def fake_captures(size: tuple[int, int], frame_count: int) -> list[dict]:
    return [
        {"dimensions": size, "frames": list(range(n))}
        for n in (14, 14, frame_count - 28)
    ]


class GraphicalScaleMatrixTests(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory(prefix="kira-scale-fixtures-")
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name)
        self.output = self.root / "captures"
        self.package = self.root / "package"

    def test_niri_kdl_config_sets_scale_on_private_winit_output_only(self):
        args = (
            Path("/python"), Path("/repo/dev_preview.py"),
            Path("/private/ready.json"), "validnonce",
        )
        default = preview_config(*args)
        scaled = preview_config(*args, acceptance_scale=1.5)
        self.assertNotIn('output "winit"', default)
        self.assertIn('output "winit"', scaled)
        self.assertIn("scale 1.5", scaled)
        self.assertIn("spawn-at-startup", scaled)
        with self.assertRaises(PreviewError):
            preview_config(*args, acceptance_scale=0.75)

    def test_niri_ipc_scale_attestation_is_not_config_guessing(self):
        def actual(scale):
            return {"winit": {"logical": {"scale": scale}}}

        for scale in SCALES:
            self.assertEqual(acceptance_output_scale(actual(scale), scale), scale)
        for data, expected in (
            (actual(1.0), 1.5),
            ({}, 1.0),
            ({"winit": {"logical": None}}, 1.0),
            ({"winit": {"logical": {"scale": "1.5"}}}, 1.5),
            ({"winit": {"logical": {"scale": True}}}, 1.0),
        ):
            with self.subTest(data=data), self.assertRaises(PreviewError):
                acceptance_output_scale(data, expected)

    def test_three_independent_private_scale_runs(self):
        with patch("graphical_scale_matrix.run_virtual") as runner:
            with patch(
                "graphical_scale_matrix.validate_bundle",
                return_value=fake_captures(PHYSICAL_VIEWPORT, CAPTURES_PER_SCALE),
            ):
                manifest = run_matrix(self.package, self.output)
        self.assertEqual(runner.call_count, len(SCALES))
        for index, scale in enumerate(SCALES):
            call = runner.call_args_list[index]
            self.assertEqual(
                call.args,
                (self.package, self.output / f"scale-{scale:.1f}", *PHYSICAL_VIEWPORT),
            )
            self.assertEqual(call.kwargs, {"profile": "smoke", "scale": scale})
        report = json.loads(manifest.read_text(encoding="utf-8"))
        self.assertEqual(report["status"], "unreviewed")
        self.assertEqual([row["niri_scale"] for row in report["scales"]], list(SCALES))
        self.assertEqual(sum(row["frames"] for row in report["scales"]), 102)
        self.assertEqual(report["graphical_input_and_motion_parity"], "not verified")

    def test_old_capture_and_fake_viewport_fail_closed(self):
        self.output.mkdir()
        sentinel = self.output / "sentinel"
        sentinel.write_text("untouched", encoding="utf-8")
        with patch("graphical_scale_matrix.run_virtual") as runner:
            with self.assertRaises(PreviewError):
                run_matrix(self.package, self.output)
            runner.assert_not_called()
        self.assertEqual(sentinel.read_text(encoding="utf-8"), "untouched")
        for wrong, count in (((640, 400), 34), ((1280, 800), 33)):
            target = self.root / f"case-{count}-{wrong[0]}"
            with patch("graphical_scale_matrix.run_virtual"):
                with patch(
                    "graphical_scale_matrix.validate_bundle",
                    return_value=fake_captures(wrong, count),
                ):
                    with self.assertRaises(PreviewError):
                        run_matrix(self.package, target)
            self.assertFalse((target / "scale-matrix-manifest.json").exists())


if __name__ == "__main__":
    unittest.main()
