"""Local evidence integrity tests; deliberately no Niri or false parity claims."""
import json
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
from dev_preview import PreviewError
from validate_nested_evidence import MAIN_SHA, validate_bundle, write_index

# Genuine complete 1x1 PNG with IHDR/IDAT/IEND and all three CRCs.
PIXEL = bytes.fromhex(
    "89504e470d0a1a0a0000000d4948445200000001000000010806000000"
    "1f15c4890000000d49444154789c63606060f80f00010401005fe5c34b"
    "0000000049454e44ae426082"
)


class EvidenceValidationTests(unittest.TestCase):
    def setUp(self):
        tmp = tempfile.TemporaryDirectory(prefix="kira-evidence-test-")
        self.addCleanup(tmp.cleanup)
        self.root = Path(tmp.name)
        self.names = [
            ("entry", "toast-entry"),
            ("exit", "toast-exit"),
            ("tray", "tray-live-sni"),
        ]
        manifests = []
        for directory, label in self.names:
            path = self.root / directory
            path.mkdir()
            for index in range(2):
                (path / f"{label}-{index:03}.png").write_bytes(PIXEL)
            manifest = path / f"{label}-manifest.json"
            manifest.write_text(json.dumps({
                "status": "unreviewed", "source_baseline": MAIN_SHA,
                "label": label, "frames": [
                    {"file": f"{label}-000.png", "elapsed_ms": 0.1},
                    {"file": f"{label}-001.png", "elapsed_ms": 16.3},
                ]
            }), encoding="utf-8")
            manifests.append(f"{directory}/{label}-manifest.json")
        (self.root / "scenario-manifest.json").write_text(json.dumps({
            "status": "unreviewed", "source_baseline": MAIN_SHA,
            "captures": manifests,
            "sni_fixture": {
                "registered_on_private_bus": True,
                "sent_real_status_changed": True,
            },
        }), encoding="utf-8")

    def test_valid_png_crc_and_offline_review_stays_unreviewed(self):
        sections = validate_bundle(self.root)
        self.assertEqual(len(sections), 3)
        self.assertTrue(all(item["dimensions"] == (1, 1) for item in sections))
        index = write_index(self.root, sections)
        content = index.read_text(encoding="utf-8")
        self.assertIn("Unreviewed, native-only capture", content)
        self.assertIn("toast-entry-000.png", content)
        self.assertNotIn("<script", content)
        with self.assertRaises(PreviewError):
            write_index(self.root, sections)

    def test_rejects_corrupted_png_crc_and_missing_pixel_data(self):
        screenshot = self.root / "entry/toast-entry-001.png"
        original = screenshot.read_bytes()
        screenshot.write_bytes(original[:-8] + b"not-png!")
        with self.assertRaisesRegex(PreviewError, "checksum|PNG"):
            validate_bundle(self.root)
        screenshot.write_bytes(PIXEL[:40])
        with self.assertRaises(PreviewError):
            validate_bundle(self.root)

    def test_rejects_symlink_and_cross_directory_frames(self):
        frame = self.root / "entry/toast-entry-001.png"
        frame.unlink()
        frame.symlink_to(self.root / "exit/toast-exit-000.png")
        with self.assertRaises(PreviewError):
            validate_bundle(self.root)

    def test_rejects_nonmonotonic_times_and_verified_claims(self):
        path = self.root / "entry/toast-entry-manifest.json"
        manifest = json.loads(path.read_text(encoding="utf-8"))
        manifest["frames"][1]["elapsed_ms"] = -5
        path.write_text(json.dumps(manifest), encoding="utf-8")
        with self.assertRaises(PreviewError):
            validate_bundle(self.root)
        manifest["frames"][1]["elapsed_ms"] = 16.3
        manifest["status"] = "verified"
        path.write_text(json.dumps(manifest), encoding="utf-8")
        with self.assertRaises(PreviewError):
            validate_bundle(self.root)

    def test_rejects_missing_sni_or_unexpected_capture_path(self):
        path = self.root / "scenario-manifest.json"
        manifest = json.loads(path.read_text(encoding="utf-8"))
        manifest["sni_fixture"]["registered_on_private_bus"] = False
        path.write_text(json.dumps(manifest), encoding="utf-8")
        with self.assertRaises(PreviewError):
            validate_bundle(self.root)
        manifest["sni_fixture"]["registered_on_private_bus"] = True
        manifest["captures"][0] = "../parent/unsafe-manifest.json"
        path.write_text(json.dumps(manifest), encoding="utf-8")
        with self.assertRaises(PreviewError):
            validate_bundle(self.root)


if __name__ == "__main__":
    unittest.main()
