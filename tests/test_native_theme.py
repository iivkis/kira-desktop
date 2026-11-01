"""Single native palette and shared overlay plumbing; no GTK display required."""
import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
NATIVE = ROOT / "crates/kira-shell/src"
PALETTE = (NATIVE / "native-palette.css").read_text()
HELPERS = (NATIVE / "native_ui.rs").read_text()
LAUNCHER = (NATIVE / "launcher.rs").read_text()
CALENDAR = (NATIVE / "calendar.rs").read_text()
FRONTEND = (ROOT / "frontend/src/lib/themes/kira-default.css").read_text()
NIX = (ROOT / "nix/package.nix").read_text()
SHELL = (NATIVE / "main.rs").read_text()


class NativeThemeContractTests(unittest.TestCase):
    def test_one_canonical_palette_for_both_native_overlays(self):
        colors = dict(re.findall(
            r"@define-color (kira_[a-z_]+) (#[0-9a-fA-F]{8});", PALETTE
        ))
        expected = {
            "kira_black": ("black", "#000000ff"),
            "kira_prussian_blue": ("prussian-blue", "#14213dff"),
            "kira_orange": ("orange", "#fca311ff"),
            "kira_alabaster_grey": ("alabaster-grey", "#e5e5e5ff"),
            "kira_white": ("white", "#ffffffff"),
        }
        self.assertEqual(colors, {k: value for k, (_, value) in expected.items()})
        for key, (frontend_name, value) in expected.items():
            self.assertIn(f"--{frontend_name}: {value};", FRONTEND, key)
        for css in ("launcher.css", "calendar.css", "system_panel.css"):
            source = (NATIVE / css).read_text()
            self.assertNotIn("@define-color", source, css)
            self.assertNotRegex(source, r"#[0-9a-fA-F]{8}\b")

    def test_shared_style_provider_is_loaded_before_component_css(self):
        self.assertIn('include_str!("native-palette.css")', HELPERS)
        self.assertIn('format!("{}\\n{}", include_str!("native-palette.css"), styles)', HELPERS)
        for component in (LAUNCHER, CALENDAR, (NATIVE / "system_panel.rs").read_text()):
            self.assertIn("native_ui::install_styles(", component)
            self.assertIn("native_ui::dismiss_overlay(", component)
            self.assertNotIn("gtk::CssProvider::new()", component)
        self.assertIn("native_ui::find_overlay(", LAUNCHER)
        self.assertIn("mod native_ui;", SHELL)

    def test_nix_keeps_shared_css_in_native_rust_source(self):
        for asset in ("launcher.css", "calendar.css", "native-palette.css", "system_panel.css"):
            self.assertIn(f'/crates/kira-shell/src/{asset}', NIX)


if __name__ == "__main__":
    unittest.main()
