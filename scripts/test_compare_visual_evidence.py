"""Source/native visual pairing validation without asserting pixel parity."""
from pathlib import Path
import json
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
from dev_preview import PreviewError
from test_validate_nested_evidence import PIXEL
from validate_nested_evidence import MAIN_SHA
from compare_visual_evidence import validate_pair, write_comparison


class SideBySideTests(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory(prefix="kira-compare-test-")
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name)
        self.manifests = []
        for role in ("main", "native"):
            folder = self.root / role
            folder.mkdir()
            frames = []
            for i in range(3):
                name = f"same-action-{i:03}.png"
                (folder / name).write_bytes(PIXEL)
                frames.append({"file": name, "elapsed_ms": float(i * 33)})
            manifest = folder / "same-action-manifest.json"
            manifest.write_text(json.dumps({
                "status": "unreviewed", "source_baseline": MAIN_SHA,
                "label": "same-action", "frames": frames,
            }), encoding="utf-8")
            self.manifests.append(manifest)
        self.output = self.root / "review"

    def test_verified_png_pair_is_bundled_but_never_certified(self):
        source, native = validate_pair(*self.manifests)
        self.assertEqual(source["dimensions"], (1, 1))
        self.assertEqual(native["dimensions"], (1, 1))
        index = write_comparison(*self.manifests, self.output)
        self.assertTrue(index.is_file())
        document = index.read_text(encoding="utf-8")
        self.assertIn("UNREVIEWED", document)
        self.assertIn("main/same-action-000.png", document)
        self.assertIn("native/same-action-002.png", document)
        self.assertNotIn("<script", document)
        report = json.loads((self.output / "comparison-manifest.json").read_text())
        self.assertEqual(report["status"], "unreviewed")
        self.assertEqual(len(report["frames"]), 3)
        self.assertNotIn("verified", report)
        with self.assertRaises(PreviewError):
            write_comparison(*self.manifests, self.output)

    def test_same_capture_cannot_be_both_main_and_native(self):
        with self.assertRaises(PreviewError):
            validate_pair(self.manifests[0], self.manifests[0])

    def test_mismatched_stimulus_and_frame_count_are_denied(self):
        path = self.manifests[1]
        payload = json.loads(path.read_text())
        payload["label"] = "different"
        path.write_text(json.dumps(payload))
        with self.assertRaises(PreviewError):
            validate_pair(*self.manifests)
        payload["label"] = "same-action"
        payload["frames"].pop()
        path.write_text(json.dumps(payload))
        with self.assertRaises(PreviewError):
            validate_pair(*self.manifests)

    def test_corrupted_frames_and_symlinks_fail_before_publication(self):
        frame = self.root / "native" / "same-action-001.png"
        frame.write_bytes(b"not a PNG")
        with self.assertRaises(PreviewError):
            write_comparison(*self.manifests, self.output)
        self.assertFalse(self.output.exists())
        frame.unlink()
        frame.symlink_to(self.root / "main" / "same-action-001.png")
        with self.assertRaises(PreviewError):
            write_comparison(*self.manifests, self.output)


if __name__ == "__main__":
    unittest.main()
