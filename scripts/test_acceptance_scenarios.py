"""Private D-Bus acceptance protocol tests without Niri or a user session."""
from pathlib import Path
import json
import sys
import tempfile
from types import SimpleNamespace
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
from dev_preview import PreviewError
from smoke_sni_fixture_bus import menu_call
from acceptance_scenarios import (
    await_native_server, await_real_sni_fixture, close_notification,
    gdbus_args, notification_id, notify, sni_property_get_all,
    verify_notification_sender_probe,
)


class RealProtocolAcceptanceTests(unittest.TestCase):
    def setUp(self):
        self.env = {
            "DBUS_SESSION_BUS_ADDRESS": "unix:path=/tmp/dbus-isolated",
            "WAYLAND_DISPLAY": "nested",
        }

    @staticmethod
    def reply(stdout, returncode=0, stderr=""):
        return SimpleNamespace(stdout=stdout, stderr=stderr, returncode=returncode)

    def test_only_fixed_freedesktop_methods_can_be_sent(self):
        expected = [
            "gdbus", "call", "--session",
            "--dest", "org.freedesktop.Notifications",
            "--object-path", "/org/freedesktop/Notifications",
            "--method", "org.freedesktop.Notifications.Notify",
        ]
        self.assertEqual(gdbus_args("Notify")[:9], expected)
        for forbidden in ("org.freedesktop.DBus.StopServiceByName", "Exec", "Remove"):
            with self.assertRaises(PreviewError):
                gdbus_args(forbidden, "evil")

    def test_real_notify_and_close_use_only_returned_id(self):
        calls = []

        def runner(argv, *, env, text, capture_output, check, timeout):
            self.assertIs(env, self.env)
            self.assertEqual((text, capture_output, check, timeout), (True, True, False, 8))
            calls.append(argv)
            if argv[argv.index("--method") + 1].endswith(".Notify"):
                self.assertEqual(argv[-3:], ["[]", "{}", "5000"])
                return self.reply("(uint32 92,)\n")
            return self.reply("()\n")

        identifier = notify("entry_real", 0, self.env, runner=runner)
        self.assertEqual(identifier, 92)
        close_notification(identifier, self.env, runner=runner)
        self.assertEqual(len(calls), 2)
        self.assertEqual(calls[-1][-1], "92")

    def test_bad_ids_and_signatures_fail_closed(self):
        for answer in ("(uint32 0,)", "(uint32 4294967296,)", "()", "(42,)", "error"):
            with self.subTest(answer=answer), self.assertRaises(PreviewError):
                notification_id(answer)
        for bad in (0, -1, 2**32):
            with self.assertRaises(PreviewError):
                close_notification(bad, self.env)
        with self.assertRaises(PreviewError):
            notify("../hello", 0, self.env)

    def test_server_identity_cannot_be_satisfied_by_a_foreign_daemon(self):
        def foreign(argv, **kwargs):
            return self.reply("('Not Kira', 'Another vendor', '1', '1.3')")

        with self.assertRaisesRegex(PreviewError, "foreign"):
            await_native_server(self.env, runner=foreign, clock=lambda: 0)

        def native(argv, **kwargs):
            return self.reply("('Kira Desktop', 'Kira', '0.1', '1.3')")

        self.assertIn("Kira Desktop", await_native_server(self.env, runner=native, clock=lambda: 0))

    def test_real_sni_menu_cli_separates_negative_depth_from_options(self):
        from unittest.mock import patch

        calls = []

        def runner(argv, *, env, text, capture_output, timeout, check):
            self.assertIs(env, self.env)
            self.assertEqual((text, capture_output, timeout, check), (True, True, 8, False))
            calls.append(argv)
            return self.reply("(uint32 1, (0, {}, []))")

        with patch("smoke_sni_fixture_bus.subprocess.run", side_effect=runner):
            menu_call("GetLayout", self.env)
            menu_call("AboutToShow", self.env)
            menu_call("ClickDisabled", self.env)
            menu_call("ClickUnknown", self.env)
            menu_call("ClickCheck", self.env)
        self.assertEqual(calls[0][-4:], ["--", "0", "-1", "[]"])
        self.assertEqual(calls[0][calls[0].index("--dest") + 1], "org.kira.AcceptanceSNI")
        self.assertEqual(calls[0][calls[0].index("--object-path") + 1], "/KiraAcceptanceMenu")
        self.assertEqual(calls[1][-2:], ["--", "20"])
        self.assertEqual(calls[2][-5:], ["--", "30", "clicked", "<int32 0>", "0"])
        self.assertEqual(calls[3][-5:], ["--", "999", "clicked", "<int32 0>", "0"])
        self.assertEqual(calls[4][-5:], ["--", "40", "clicked", "<int32 0>", "0"])
        self.assertTrue(all(argv[argv.index("--dest") + 1] == "org.kira.AcceptanceSNI" for argv in calls))
        with self.assertRaises(PreviewError):
            menu_call("org.freedesktop.DBus.StopServiceByName", self.env)

    def test_notification_sender_probe_needs_private_bus_and_full_proof(self):
        env = {
            **self.env,
            "KIRA_PREVIEW": "1",
            "KIRA_ISOLATED_SESSION_BUS": "1",
            "KIRA_NOTIFICATIONS_PREVIEW": "1",
            "KIRA_ACCEPTANCE_TOKEN": "a" * 32,
            "KIRA_ACCEPTANCE_PARENT_BUS": "unix:path=/tmp/parent-dbus",
        }
        with tempfile.TemporaryDirectory() as tmp:
            probe = Path(tmp) / "kira-notification-acceptance"
            probe.write_text("test fixture binary", encoding="utf-8")
            probe.chmod(0o700)
            called = []

            def runner(argv, *, env, text, capture_output, check, timeout):
                self.assertEqual(argv, [str(probe)])
                self.assertEqual((text, capture_output, check, timeout), (True, True, False, 15))
                called.append(env)
                return self.reply(json.dumps({
                    "same_sender_replaced": True, "foreign_sender_isolated": True,
                    "stale_id_rejected": True, "private_bus": True,
                }))

            success = verify_notification_sender_probe(probe, env, runner=runner)
            self.assertEqual(len(called), 1)
            self.assertEqual(success, {
                "same_sender_replaced": True, "foreign_sender_isolated": True,
                "stale_id_rejected": True, "private_bus": True,
            })

            for missing in (
                "KIRA_PREVIEW", "KIRA_ISOLATED_SESSION_BUS",
                "KIRA_NOTIFICATIONS_PREVIEW", "KIRA_ACCEPTANCE_TOKEN",
            ):
                with self.subTest(missing=missing), self.assertRaises(PreviewError):
                    verify_notification_sender_probe(probe, {k: v for k, v in env.items() if k != missing}, runner=runner)
            with self.assertRaises(PreviewError):
                verify_notification_sender_probe(
                    probe, {**env, "DBUS_SESSION_BUS_ADDRESS": env["KIRA_ACCEPTANCE_PARENT_BUS"]},
                    runner=runner,
                )
            with self.assertRaises(PreviewError):
                verify_notification_sender_probe(probe.with_name("missing"), env, runner=runner)

            def incomplete(argv, **kwargs):
                return self.reply('{"same_sender_replaced":true,"private_bus":true}')

            with self.assertRaises(PreviewError):
                verify_notification_sender_probe(probe, env, runner=incomplete)

            def invalid(argv, **kwargs):
                return self.reply("not json")

            with self.assertRaises(PreviewError):
                verify_notification_sender_probe(probe, env, runner=invalid)

            def failed(argv, **kwargs):
                return self.reply("", returncode=1, stderr="fixture failure")

            with self.assertRaisesRegex(PreviewError, "fixture failure"):
                verify_notification_sender_probe(probe, env, runner=failed)

    def test_fixture_must_register_through_real_watcher_and_publish_pixmap(self):
        def fixture(argv, **kwargs):
            dest = argv[argv.index("--dest") + 1]
            if dest == "org.kde.StatusNotifierWatcher":
                return self.reply(
                    "({'RegisteredStatusNotifierItems': <[':1.43/StatusNotifierItem']>, "
                    "'ProtocolVersion': <0>},)"
                )
            self.assertEqual(dest, "org.kira.AcceptanceSNI")
            return self.reply(
                "({'Id': <'kira-sni-acceptance'>, 'IconPixmap': <[(24, 24, [])]>, "
                "'Status': <'NeedsAttention'>},)"
            )

        found = await_real_sni_fixture(self.env, runner=fixture, clock=lambda: 0)
        self.assertIn("RegisteredStatusNotifierItems", found["watcher"])
        self.assertIn("IconPixmap", found["item"])
        with self.assertRaises(PreviewError):
            sni_property_get_all("org.kde.NotAllowed", "/", "bad", self.env)


if __name__ == "__main__":
    unittest.main()
