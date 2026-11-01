"""Contract checks for the GTK native editorial launcher, without a display."""
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CSS = (ROOT / "crates/kira-shell/src/launcher.css").read_text()
RUST = (ROOT / "crates/kira-shell/src/launcher.rs").read_text()
HOST = (ROOT / "crates/kira-shell/src/main.rs").read_text()


class LauncherDesignTests(unittest.TestCase):
    def test_selection_visual_contract(self):
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
