"""Fail-closed virtual compositor tests: no invented frames or Niri mocks."""
from pathlib import Path
import socket
import stat
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))

from dev_preview import PreviewError
from headless_wayland_acceptance import (
    accept_command, isolated_virtual_environment, verify_virtual_socket,
    weston_command, run_virtual, validate_virtual_viewport,
)


class HeadlessAcceptanceTests(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory(prefix="kira-headless-tests-")
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name).resolve()
        self.socket = "kira-virtual-abcdef012345"
        self.ambient = {
            "DBUS_SESSION_BUS_ADDRESS": "unix:path=/run/user/1000/bus",
            "XDG_RUNTIME_DIR": "/run/user/1000",
            "HOME": "/home/user", "XDG_CONFIG_HOME": "/home/user/.config",
            "KIRA_PREVIEW": "1", "KIRA_ACCEPTANCE_TOKEN": "a" * 32,
            "NIRI_CONFIG": "/home/user/niri.kdl",
            "KIRA_CORE_SOCKET": "/home/user/core.sock",
        }

    def test_environment_replaces_real_home_xdg_and_bus(self):
        env = isolated_virtual_environment(self.ambient, self.root, self.socket)
        self.assertEqual(env["HOME"], str(self.root / "home"))
        self.assertEqual(env["WAYLAND_DISPLAY"], self.socket)
        for key in (
            "HOME", "XDG_RUNTIME_DIR", "XDG_CONFIG_HOME",
            "XDG_STATE_HOME", "XDG_CACHE_HOME", "XDG_DATA_HOME",
        ):
            self.assertEqual(Path(env[key]).parent, self.root)
            self.assertEqual(stat.S_IMODE(Path(env[key]).stat().st_mode), 0o700)
        for key in ("NIRI_CONFIG", "KIRA_PREVIEW", "KIRA_CORE_SOCKET",
                    "DBUS_SESSION_BUS_ADDRESS", "KIRA_ACCEPTANCE_TOKEN"):
            self.assertNotIn(key, env)
        self.assertEqual(self.ambient["HOME"], "/home/user")

    def test_refuse_inherited_live_wayland_or_niri(self):
        for extra in (
            {"WAYLAND_DISPLAY": "wayland-0"},
            {"NIRI_SOCKET": "/run/user/1000/niri"},
        ):
            with self.subTest(extra=extra), self.assertRaises(PreviewError):
                isolated_virtual_environment(
                    {**self.ambient, **extra}, self.root, self.socket
                )
            with self.assertRaises(PreviewError):
                run_virtual(
                    self.root / "missing", self.root / "evidence", 1280, 800,
                    ambient={**self.ambient, **extra},
                )

    def test_real_png_viewport_must_match_the_claimed_capture_resolution(self):
        captures = [
            {"dimensions": (800, 600), "label": name}
            for name in ("toast-entry", "toast-exit", "tray-live-sni")
        ]
        validate_virtual_viewport(captures, 800, 600)
        for changed in (
            captures[:-1],
            [{**captures[0], "dimensions": (1280, 800)}, *captures[1:]],
            [{**captures[0], "dimensions": (800, 599)}, *captures[1:]],
        ):
            with self.subTest(changed=changed), self.assertRaises(PreviewError):
                validate_virtual_viewport(changed, 800, 600)

    def test_weston_command_is_headless_bounded_and_software_only(self):
        command = weston_command(
            "weston", self.socket, self.root / "weston.log", 1280, 800,
        )
        self.assertIn("--backend=headless-backend.so", command)
        self.assertIn("--renderer=gl", command)
        self.assertIn("--no-config", command)
        self.assertNotIn("--backend=drm-backend.so", command)
        for width, height in ((10, 600), (800, 10), (99999, 800), (800, 99999)):
            with self.assertRaises(PreviewError):
                weston_command("weston", self.socket, self.root / "log", width, height)
        with self.assertRaises(PreviewError):
            weston_command("weston", "../host", self.root / "log", 800, 600)

    def test_socket_verification_requires_private_real_unix_endpoint(self):
        env = isolated_virtual_environment(self.ambient, self.root, self.socket)
        with self.assertRaises(PreviewError):
            verify_virtual_socket(env, self.socket)
        endpoint = self.root / "runtime" / self.socket
        endpoint.write_text("ordinary file")
        with self.assertRaises(PreviewError):
            verify_virtual_socket(env, self.socket)
        endpoint.unlink()
        with socket.socket(socket.AF_UNIX) as server:
            server.bind(str(endpoint))
            server.listen(1)
            self.assertEqual(verify_virtual_socket(env, self.socket), endpoint)
            runtime = self.root / "runtime"
            runtime.chmod(0o755)
            try:
                with self.assertRaises(PreviewError):
                    verify_virtual_socket(env, self.socket)
            finally:
                runtime.chmod(0o700)
        endpoint.unlink()
        endpoint.symlink_to(self.root / "anything")
        with self.assertRaises(PreviewError):
            verify_virtual_socket(env, self.socket)

    def test_coordinator_argv_cannot_be_shell_interpreted(self):
        cmd = accept_command(
            Path("/repo/scripts/accept_nested.py"),
            Path("/repo/result"), Path("/out/capture"),
        )
        self.assertEqual(cmd[0], sys.executable)
        self.assertIn("--package-dir", cmd)
        self.assertIn("--output", cmd)
        self.assertEqual(cmd[cmd.index("--profile") + 1], "smoke")
        full = accept_command(
            Path("/repo/scripts/accept_nested.py"),
            Path("/repo/result"), Path("/out/capture"), profile="full",
        )
        self.assertEqual(full[full.index("--profile") + 1], "full")
        self.assertEqual(full[full.index("--scale") + 1], "1.0")
        scaled = accept_command(
            Path("/repo/accept.py"), Path("/repo/result"),
            Path("/out/capture"), scale=2.0,
        )
        self.assertEqual(scaled[scaled.index("--scale") + 1], "2.0")
        with self.assertRaises(PreviewError):
            accept_command(Path("/repo/accept.py"), Path("/repo/result"),
                           Path("/out/capture"), scale=0.8)
        with self.assertRaises(PreviewError):
            accept_command(Path("/repo/accept.py"), Path("/result"),
                           Path("/out"), profile="unknown")
        self.assertNotIn("sh", cmd)


if __name__ == "__main__":
    unittest.main()
