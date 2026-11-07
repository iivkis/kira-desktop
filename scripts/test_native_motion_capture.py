"""Safety and timestamp regressions for opt-in nested motion captures."""
import json
from pathlib import Path
import socket
import sys
import tempfile
from types import SimpleNamespace
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
from dev_preview import PreviewError
from capture_native_motion import (
    capture_frames,
    validate_capture_args,
    validate_capture_environment,
)


class NestedMotionCaptureTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="kira-motion-evidence-test-")
        self.addCleanup(self.temp.cleanup)
        self.runtime = Path(self.temp.name)
        self.parent = self.runtime / "wayland-parent"
        self.nested = self.runtime / "wayland-nested"
        self.sockets = []
        for path in (self.parent, self.nested):
            sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            sock.bind(str(path))
            sock.listen(8)
            self.sockets.append(sock)
            self.addCleanup(sock.close)
        self.environment = {
            "XDG_RUNTIME_DIR": str(self.runtime),
            "WAYLAND_DISPLAY": self.parent.name,
            "NIRI_SOCKET": "/parent/niri.sock",
            "NIRI_CONFIG": "/parent/config.kdl",
        }

    def test_requires_distinct_live_nested_wayland_socket(self):
        with self.assertRaises(PreviewError):
            validate_capture_environment(self.parent.name, self.environment)
        with self.assertRaises(PreviewError):
            validate_capture_environment("wayland-missing", self.environment)
        alias = self.runtime / "wayland-host-alias"
        alias.symlink_to(self.parent)
        with self.assertRaises(PreviewError):
            validate_capture_environment(alias.name, self.environment)

    def test_strips_parent_niri_ipc_and_preserves_host_environment(self):
        isolated = validate_capture_environment(self.nested.name, self.environment)
        self.assertEqual(isolated["WAYLAND_DISPLAY"], self.nested.name)
        self.assertNotIn("NIRI_SOCKET", isolated)
        self.assertNotIn("NIRI_CONFIG", isolated)
        self.assertEqual(self.environment["WAYLAND_DISPLAY"], self.parent.name)
        self.assertEqual(self.environment["NIRI_SOCKET"], "/parent/niri.sock")

    def test_bounded_capture_and_label_arguments(self):
        for args in [("../escape", 1, 33), ("", 1, 33), ("go", 0, 33),
                     ("go", 61, 33), ("go", 1, 9), ("go", 1, 501)]:
            with self.subTest(args=args), self.assertRaises(PreviewError):
                validate_capture_args(*args)
        validate_capture_args("launcher-down", 60, 33)

    def test_manifest_contains_real_sample_timestamps_not_fabricated_fps(self):
        calls = []

        def fake_grim(argv, *, env, capture_output, text, check, timeout):
            self.assertEqual(env["WAYLAND_DISPLAY"], self.nested.name)
            self.assertEqual(argv[:3], ["/mock/grim", "-t", "png"])
            self.assertTrue(capture_output and text)
            self.assertFalse(check)
            self.assertEqual(timeout, 10)
            Path(argv[-1]).write_bytes(b"mock png, no actual compositor content")
            calls.append(argv[-1])
            return SimpleNamespace(returncode=0, stderr="")

        output = self.runtime / "evidence"
        manifest = capture_frames(
            self.nested.name, output, "rapid-filter", 2, 10,
            self.environment, grim="/mock/grim", runner=fake_grim,
        )
        metadata = json.loads(manifest.read_text(encoding="utf-8"))
        self.assertEqual(metadata["status"], "unreviewed")
        self.assertEqual(metadata["label"], "rapid-filter")
        self.assertEqual(len(metadata["frames"]), 2)
        self.assertEqual(len(calls), 2)
        elapsed = [frame["elapsed_ms"] for frame in metadata["frames"]]
        self.assertGreaterEqual(elapsed[1], elapsed[0])
        self.assertTrue(all((output / frame["file"]).is_file() for frame in metadata["frames"]))


if __name__ == "__main__":
    unittest.main()
