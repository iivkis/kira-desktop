"""Pure fixtures for the real three-viewport Niri capture orchestrator.

Mocks never manufacture parity evidence: they only check that the harness
rejects wrong screenshot dimensions and missing real capture frame counts.
"""
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))

from dev_preview import PreviewError
from graphical_viewport_matrix import (
    CAPTURES_PER_VIEWPORT, VIEWPORTS, run_matrix,
)


def sample_capture(dimensions: tuple[int, int], frames: int) -> list[dict]:
    return [
        {"dimensions": dimensions, "frames": list(range(n))}
        for n in (14, 14, frames - 28)
    ]


class GraphicalViewportMatrixTests(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory(prefix="kira-viewport-tests-")
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name)
        self.output = self.root / "evidence"
        self.package = self.root / "package"

    def test_uses_three_distinct_isolated_viewport_runs(self):
        def stub_validation(scene: Path) -> list[dict]:
            width, height = (int(value) for value in scene.name.split("x"))
            return sample_capture((width, height), CAPTURES_PER_VIEWPORT)

        with patch("graphical_viewport_matrix.run_virtual") as runner:
            with patch(
                "graphical_viewport_matrix.validate_bundle",
                side_effect=stub_validation,
            ):
                path = run_matrix(self.package, self.output)
        self.assertEqual(len(runner.call_args_list), len(VIEWPORTS))
        for index, (width, height) in enumerate(VIEWPORTS):
            args = runner.call_args_list[index]
            self.assertEqual(args.args, (
                self.package,
                self.output / f"{width}x{height}",
                width,
                height,
            ))
            self.assertEqual(args.kwargs, {"profile": "smoke"})
        report = json.loads(path.read_text(encoding="utf-8"))
        self.assertEqual(report["status"], "unreviewed")
        self.assertEqual(len(report["viewports"]), 3)
        self.assertEqual(sum(item["frames"] for item in report["viewports"]), 102)
        self.assertEqual(report["graphical_input_and_motion_parity"], "not verified")

    def test_refuses_preexisting_evidence_without_touching_it(self):
        self.output.mkdir()
        sentinel = self.output / "sentinel"
        sentinel.write_text("untouched", encoding="utf-8")
        with patch("graphical_viewport_matrix.run_virtual") as runner:
            with self.assertRaises(PreviewError):
                run_matrix(self.package, self.output)
        runner.assert_not_called()
        self.assertEqual(sentinel.read_text(encoding="utf-8"), "untouched")

    def test_refuses_wrong_actual_nested_viewport_or_missing_frames(self):
        width, height = VIEWPORTS[0]
        for dimensions, frames in (
            ((width + 1, height), CAPTURES_PER_VIEWPORT),
            ((width, height), CAPTURES_PER_VIEWPORT - 1),
        ):
            with self.subTest(dimensions=dimensions, frames=frames):
                output = self.root / f"case-{frames}-{dimensions[0]}"
                with patch("graphical_viewport_matrix.run_virtual"):
                    with patch(
                        "graphical_viewport_matrix.validate_bundle",
                        return_value=sample_capture(dimensions, frames),
                    ):
                        with self.assertRaises(PreviewError):
                            run_matrix(self.package, output)
                self.assertFalse((output / "matrix-manifest.json").exists())


if __name__ == "__main__":
    unittest.main()
