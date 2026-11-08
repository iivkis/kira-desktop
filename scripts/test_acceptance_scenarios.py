"""Private D-Bus acceptance protocol tests without Niri or a user session."""
from pathlib import Path
import sys
from types import SimpleNamespace
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
from dev_preview import PreviewError
from acceptance_scenarios import (
    await_native_server, await_real_sni_fixture, close_notification,
    gdbus_args, notification_id, notify, sni_property_get_all,
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
