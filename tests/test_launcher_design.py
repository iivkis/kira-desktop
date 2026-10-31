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
        palette = {
            "kira_black": "#000000ff",
            "kira_prussian_blue": "#14213dff",
            "kira_orange": "#fca311ff",
            "kira_alabaster_grey": "#e5e5e5ff",
            "kira_white": "#ffffffff",
        }
        declared = dict(re.findall(
            r"@define-color (kira_[a-z_]+) (#[0-9a-fA-F]{8});", CSS
        ))
        self.assertEqual(declared, palette)
        self.assertEqual(set(re.findall(r"#[0-9a-fA-F]{8}\b", CSS)), set(palette.values()))
        frontend_css = (ROOT / "frontend/src/lib/themes/kira-default.css").read_text()
        for name, value in (
            ("black", "#000000ff"), ("prussian-blue", "#14213dff"),
            ("orange", "#fca311ff"), ("alabaster-grey", "#e5e5e5ff"),
            ("white", "#ffffffff"),
        ):
            self.assertIn(f"--{name}: {value};", frontend_css)
        self.assertIn("row.kira-app-row:selected .kira-app-rail", CSS)
        self.assertIn("background: @kira_orange;", CSS)
        self.assertIn("row.kira-app-row:selected .kira-app-name", CSS)
        self.assertIn("color: @kira_orange;", CSS)
        self.assertNotIn("box-shadow:", CSS)
        self.assertNotIn("border: 2px", CSS)
        self.assertIn("border: 1px solid", CSS)

    def test_visual_hierarchy_and_states(self):
        for selector in (
            ".kira-launcher-card", ".kira-launcher-intro", ".kira-workbench",
            ".kira-workbench-eyebrow", ".kira-catalog-label", ".kira-eyebrow",
            ".kira-heading", "entry.kira-search-field:focus",
            "row.kira-app-row:hover", "row.kira-app-row:selected",
            ".kira-app-rail", ".kira-app-icon-shell", ".kira-app-detail",
            ".kira-keycap",
        ):
            self.assertIn(selector, CSS)

    def test_editorial_surface_not_monolithic_highlight(self):
        self.assertIn("background: @kira_black;", CSS)
        self.assertIn("background: @kira_prussian_blue;", CSS)
        self.assertIn("row.kira-app-row:selected .kira-app-rail", CSS)
        self.assertNotIn("row.kira-app-row:selected {\n  background: @kira_orange;", CSS)
        self.assertIn('intro.add_css_class("kira-launcher-intro")', RUST)
        self.assertIn('workbench.add_css_class("kira-workbench")', RUST)
        self.assertIn('rail.add_css_class("kira-app-rail")', RUST)

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

    def test_native_row_identity_survives_filtering(self):
        self.assertIn('fn app_index(row: &gtk::ListBoxRow)', RUST)
        self.assertIn('row.set_widget_name(&format!("kira-app-{index}"))', RUST)
        self.assertNotIn('row.index()', RUST.split('fn app_index', 1)[1].split('fn visible_rows', 1)[1])
        self.assertIn('.is_child_visible()', RUST)


if __name__ == "__main__":
    unittest.main()
