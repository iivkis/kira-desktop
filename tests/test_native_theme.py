"""The Shell hosts GTK layer surfaces; all widget presentation is Svelte."""
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
NATIVE = ROOT / "crates/kira-shell/src"
HELPERS = (NATIVE / "native_ui.rs").read_text()
LAUNCHER = (NATIVE / "launcher.rs").read_text()
OVERLAY = (NATIVE / "calendar.rs").read_text()
FRONTEND = (ROOT / "frontend/src/lib/themes/kira-default.css").read_text()
NIX = (ROOT / "nix/package.nix").read_text()
SHELL = (NATIVE / "main.rs").read_text()


class NativeThemeContractTests(unittest.TestCase):
    def test_all_five_canonical_colors_live_in_svelte_theme(self):
        for key in ("black", "prussian-blue", "orange", "alabaster-grey", "white"):
            self.assertIn("--" + key + ": ", FRONTEND)
        for file in ("launcher.css", "system_panel.css", "native-palette.css"):
            self.assertFalse((NATIVE / file).exists(), file)
            self.assertNotIn(file, NIX)
        self.assertNotIn("include_str!", HELPERS)
        self.assertNotIn("gtk::CssProvider", HELPERS)

    def test_native_code_is_only_webkit_layer_shell_and_gio(self):
        self.assertIn("webkit6::WebView::builder()", OVERLAY)
        self.assertIn("pub fn restrict_webview_navigation", HELPERS)
        self.assertIn("pub fn configure_webview_popup", HELPERS)
        self.assertIn("gio::AppInfo::all()", LAUNCHER)
        self.assertNotIn("gtk::SearchEntry", LAUNCHER)
        self.assertNotIn("gtk::Box", LAUNCHER)
        self.assertIn("mod native_ui;", SHELL)


if __name__ == "__main__":
    unittest.main()
