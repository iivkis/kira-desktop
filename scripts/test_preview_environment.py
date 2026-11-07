"""Preview environment isolation regression tests. No GTK/Niri daemon required."""
import socket
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from dev_preview import PreviewError, isolated_shell_environment


class PreviewIsolationTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="kira-preview-test-")
        self.addCleanup(self.tmp.cleanup)
        self.path = Path(self.tmp.name)
        self.s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.addCleanup(self.s.close)
        self.sock = self.path / "nested.sock"
        self.s.bind(str(self.sock))
        self.base = {
            "HOME": "/home/test",
            "XDG_CONFIG_HOME": "/home/test/.config",
            "NIRI_CONFIG": "/home/test/.config/niri/config.kdl",
        }

    def environment(self, socket_path=None, parent_socket=None):
        return isolated_shell_environment(
            self.base,
            "wayland-nested",
            str(socket_path) if socket_path is not None else None,
            parent_socket,
            self.path,
            self.path / "niri.kdl",
            self.path / "core.sock",
            self.path / "kira.kdl",
        )

    def test_save_paths_and_niri_socket_are_isolated(self):
        env = self.environment(self.sock)
        self.assertEqual(env["NIRI_SOCKET"], str(self.sock))
        self.assertEqual(env["NIRI_CONFIG"], str(self.path / "niri.kdl"))
        self.assertEqual(env["XDG_CONFIG_HOME"], str(self.path / "xdg-config"))
        self.assertEqual(env["KIRA_CORE_SOCKET"], str(self.path / "core.sock"))
        self.assertEqual(env["KIRA_PREVIEW"], "1")
        self.assertEqual(self.base["NIRI_CONFIG"], "/home/test/.config/niri/config.kdl")
        self.assertTrue((self.path / "xdg-config").is_dir())

    def test_missing_nested_ipc_socket_is_rejected(self):
        with self.assertRaises(PreviewError):
            self.environment(None)
        with self.assertRaises(PreviewError):
            self.environment(self.path / "missing.sock")

    def test_parent_compositor_ipc_socket_is_rejected(self):
        with self.assertRaises(PreviewError):
            self.environment(self.sock, str(self.sock))


if __name__ == "__main__":
    unittest.main()
