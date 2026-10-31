"""Contract checks for the native premium launcher (visual QA remains manual).

These checks do not claim to verify actual WebKit/GTK pixels. They protect
against accidental loss of icons, selection visuals or safe launch semantics.
"""
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[1]
LAUNCHER = (ROOT / "crates/kira-shell/src/launcher.rs").read_text()
CSS = (ROOT / "crates/kira-shell/src/launcher.css").read_text()


class LauncherDesignContract(unittest.TestCase):
    def test_palette_uses_only_approved_base_colors(self):
        allowed = {"#0a0908", "#49111c", "#f2f4f3", "#a9927d", "#5e503f"}
        used = set(re.findall(r"#[0-9a-f]{6}\b", CSS))
        self.assertEqual(used, allowed)

    def test_launcher_is_scoped_floating_surface(self):
        for selector in [
            "window.kira-launcher-window",
            ".kira-launcher-card",
            "entry.kira-search-field",
            "row.kira-app-row:selected",
            ".kira-app-icon-shell",
            ".kira-app-detail",
            ".kira-launch-error",
            ".kira-empty-title",
            ".kira-keycap",
        ]:
            self.assertIn(selector, CSS)
        self.assertIn("border-radius: 20px", CSS)
        self.assertIn("set_exclusive_zone(0)", LAUNCHER)
        self.assertIn("set_keyboard_mode(KeyboardMode::Exclusive)", LAUNCHER)

    def test_real_app_icons_and_only_known_gio_appinfo_launch(self):
        self.assertIn(".icon()", LAUNCHER)
        self.assertIn("gtk::Image::from_gicon", LAUNCHER)
        self.assertIn("application-x-executable-symbolic", LAUNCHER)
        self.assertIn('include_str!("launcher.css")', LAUNCHER)
        self.assertIn("app.launch(&[], None::<&gtk::gio::AppLaunchContext>)", LAUNCHER)
        self.assertNotIn("sh -c", LAUNCHER.split("fn fill(", 1)[-1])

    def test_keyboard_navigation_and_scrolling_remain(self):
        self.assertIn("ensure_selected_visible(list, row, &scroll)", LAUNCHER)
        self.assertIn("gtk::gdk::Key::Down", LAUNCHER)
        self.assertIn("gtk::gdk::Key::Up", LAUNCHER)
        self.assertIn("gtk::gdk::Key::Escape", LAUNCHER)
        self.assertIn("search.grab_focus()", LAUNCHER)
        self.assertIn("window.present()", LAUNCHER)


if __name__ == "__main__":
    unittest.main()
