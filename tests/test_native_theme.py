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
        # One native CSS provider is needed solely to remove the GTK theme's
        # gray window backing. It must not introduce a native color palette
        # or affect the permanent Top Bar: visual styling stays in Svelte.
        self.assertEqual(HELPERS.count("gtk::CssProvider::new()"), 1)
        self.assertIn("window.kira-popup-transparent", HELPERS)
        self.assertIn("background-color: transparent;", HELPERS)
        self.assertIn("box-shadow: none;", HELPERS)
        self.assertNotIn("gtk::CssProvider", SHELL)
        self.assertNotIn("gtk::CssProvider", OVERLAY)

    def test_nixos_shell_uses_scoped_cyrillic_font_config(self):
        style = (ROOT / "frontend/src/style.css").read_text()
        self.assertIn('--font-sans: "Noto Sans", "DejaVu Sans"', style)
        self.assertIn("writeText,", NIX)
        self.assertIn("noto-fonts,", NIX)
        self.assertIn("noto-fonts-color-emoji,", NIX)
        self.assertIn('<include ignore_missing="yes">/etc/fonts/fonts.conf</include>', NIX)
        self.assertIn("<dir>${noto-fonts}/share/fonts</dir>", NIX)
        self.assertIn("<dir>${noto-fonts-color-emoji}/share/fonts</dir>", NIX)
        self.assertIn('gappsWrapperArgs+=(--set-default FONTCONFIG_FILE "${uiFontsConf}")', NIX)
        # Keep system/host font settings untouched; this is a package wrapper.
        self.assertNotIn("fonts.packages", NIX)

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
