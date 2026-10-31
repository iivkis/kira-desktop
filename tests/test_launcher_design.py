"""Contract checks for the GTK native editorial launcher, without a display."""
import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CSS = (ROOT / "crates/kira-shell/src/launcher.css").read_text()
RUST = (ROOT / "crates/kira-shell/src/launcher.rs").read_text()
HOST = (ROOT / "crates/kira-shell/src/main.rs").read_text()


class LauncherDesignTests(unittest.TestCase):
    def test_palette_is_canonical(self):
        self.assertEqual(
            set(re.findall(r"#[0-9a-fA-F]{6}\b", CSS)),
            {"#0a0908", "#49111c", "#f2f4f3", "#a9927d", "#5e503f"},
        )

    def test_visual_hierarchy_and_states(self):
        for selector in (
            ".kira-launcher-card", ".kira-eyebrow", ".kira-heading",
            "entry.kira-search-field:focus", "row.kira-app-row:hover",
            "row.kira-app-row:selected", ".kira-app-icon-shell", ".kira-app-detail",
            ".kira-keycap",
        ):
            self.assertIn(selector, CSS)

    def test_native_wiring_and_search(self):
        for required in (
            'include_str!("launcher.css")', "gio::AppInfo::all()",
            "info.should_show()", "set_filter_func", "connect_search_changed",
            "connect_key_pressed", "KeyboardMode::Exclusive", "set_exclusive_zone(0)",
            'app.info.launch(&[], None::<&gio::AppLaunchContext>)',
        ):
            self.assertIn(required, RUST)
        self.assertIn('mod launcher;', HOST)
        self.assertIn('register_script_message_handler("kiraLauncher"', HOST)
        self.assertIn('launcher::show(', HOST)


if __name__ == "__main__":
    unittest.main()
