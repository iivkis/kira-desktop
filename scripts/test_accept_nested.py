"""Fail-closed private D-Bus acceptance launcher, no graphics required."""
from pathlib import Path
import socket
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
from dev_preview import PreviewError
from accept_nested import (
    PARENT_BUS, TOKEN, acceptance_command, child_environment, verify_private_bus,
)
from private_bus import PRIVATE_BUS_DIR, private_bus_config, run_session_arguments


class PrivateBusAcceptanceTests(unittest.TestCase):
    def setUp(self):
        self.parent = {
            "WAYLAND_DISPLAY": "wayland-host",
            "XDG_RUNTIME_DIR": "/run/user/1000",
            "DBUS_SESSION_BUS_ADDRESS": "unix:path=/run/user/1000/bus",
            "KIRA_PREVIEW": "1",
            "KIRA_NOTIFICATIONS_PREVIEW": "1",
            "KIRA_SNI_PREVIEW": "1",
            "KIRA_ISOLATED_SESSION_BUS": "1",
            "NIRI_SOCKET": "/real/niri.sock",
            "KIRA_CORE_SOCKET": "/real/core.sock",
            PRIVATE_BUS_DIR: "/tmp/forged-private-dbus",
        }

    def test_parent_context_is_scrubbed_and_preserved(self):
        child = child_environment(self.parent, "1234567890abcdef" * 2)
        self.assertEqual(child[PARENT_BUS], self.parent["DBUS_SESSION_BUS_ADDRESS"])
        self.assertEqual(child[TOKEN], "1234567890abcdef" * 2)
        self.assertEqual(child["WAYLAND_DISPLAY"], self.parent["WAYLAND_DISPLAY"])
        for key in (
            "KIRA_PREVIEW", "KIRA_SNI_PREVIEW", "KIRA_NOTIFICATIONS_PREVIEW",
            "KIRA_ISOLATED_SESSION_BUS", "NIRI_SOCKET", "KIRA_CORE_SOCKET",
            PRIVATE_BUS_DIR,
        ):
            self.assertNotIn(key, child)
            self.assertIn(key, self.parent)

    def test_valid_fresh_bus_and_all_denied_shared_bus_cases(self):
        with private_bus_config() as config, socket.socket(socket.AF_UNIX) as server:
            sock = config.parent / "dbus-local-test"
            server.bind(str(sock))
            candidate = child_environment(self.parent, "a" * 32)
            candidate[PRIVATE_BUS_DIR] = str(config.parent)
            candidate["DBUS_SESSION_BUS_ADDRESS"] = (
                f"unix:path={sock},guid=" + "a" * 32
            )
            verify_private_bus(candidate)
            for changed in (
                {"DBUS_SESSION_BUS_ADDRESS": self.parent["DBUS_SESSION_BUS_ADDRESS"]},
                {"DBUS_SESSION_BUS_ADDRESS": ""},
                {"DBUS_SESSION_BUS_ADDRESS": "unix:path=/tmp/other-bus"},
                {"DBUS_SESSION_BUS_ADDRESS": candidate["DBUS_SESSION_BUS_ADDRESS"]
                 + ";unix:path=/tmp/fallback"},
                {TOKEN: "not-a-token"},
                {PRIVATE_BUS_DIR: ""},
                {PRIVATE_BUS_DIR: str(config.parent.parent)},
                {"KIRA_SNI_PREVIEW": "1"},
                {"KIRA_NOTIFICATIONS_PREVIEW": "1"},
            ):
                with self.subTest(changed=changed), self.assertRaises(PreviewError):
                    verify_private_bus({**candidate, **changed})

    def test_rejects_symlinked_socket_and_relaxed_directory_permissions(self):
        with private_bus_config() as config, socket.socket(socket.AF_UNIX) as server:
            sock = config.parent / "dbus-local-test"
            server.bind(str(sock))
            alias = config.parent / "dbus-alias"
            alias.symlink_to(sock)
            candidate = child_environment(self.parent, "b" * 32)
            candidate[PRIVATE_BUS_DIR] = str(config.parent)
            candidate["DBUS_SESSION_BUS_ADDRESS"] = f"unix:path={alias}"
            with self.assertRaises(PreviewError):
                verify_private_bus(candidate)

            candidate["DBUS_SESSION_BUS_ADDRESS"] = f"unix:path={sock}"
            config.parent.chmod(0o755)
            try:
                with self.assertRaises(PreviewError):
                    verify_private_bus(candidate)
            finally:
                config.parent.chmod(0o700)
            verify_private_bus(candidate)

    def test_missing_parent_wayland_or_invalid_nonce_fails(self):
        for env in (
            {"XDG_RUNTIME_DIR": "/run/user/1000"},
            {"WAYLAND_DISPLAY": "wayland-0"},
        ):
            with self.assertRaises(PreviewError):
                child_environment(env, "a" * 32)
        with self.assertRaises(PreviewError):
            child_environment(self.parent, "x" * 32)

    def test_child_uses_separate_bus_runner_and_explicit_paths(self):
        with private_bus_config() as config:
            command = acceptance_command(
                Path("/repo/scripts/dev_preview.py"), Path("/repo/result"),
                Path("/tmp/new-evidence"), "/bin/dbus-run-session", config,
                Path("/repo/target/debug"),
            )
            self.assertEqual(
                command[:3],
                ["/bin/dbus-run-session", "--config-file=" + str(config), "--"],
            )
            self.assertIn("--acceptance-notifications", command)
            self.assertEqual(command[command.index("--acceptance-profile") + 1], "full")
            self.assertEqual(command[command.index("--evidence-dir") + 1], "/tmp/new-evidence")
            self.assertEqual(command[command.index("--package-dir") + 1], "/repo/result")
            self.assertEqual(command[command.index("--bin-dir") + 1], "/repo/target/debug")
            self.assertNotIn("sh", command)

    def test_bus_config_is_0700_temporary_and_never_activates_user_services(self):
        with private_bus_config() as config:
            self.assertTrue(config.is_file())
            self.assertEqual(config.parent.stat().st_mode & 0o777, 0o700)
            self.assertEqual(config.stat().st_mode & 0o777, 0o600)
            contents = config.read_text(encoding="utf-8")
            self.assertIn("<auth>EXTERNAL</auth>", contents)
            self.assertIn("unix:tmpdir=" + str(config.parent), contents)
            self.assertNotIn("standard_session_servicedirs", contents)
            self.assertEqual(run_session_arguments("/dbus/run", config),
                             ["/dbus/run", "--config-file=" + str(config), "--"])
        self.assertFalse(config.exists())


if __name__ == "__main__":
    unittest.main()
