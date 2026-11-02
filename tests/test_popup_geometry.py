"""One stable layer-shell dock; normal widget windows are Niri-managed."""
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
HOST = (ROOT / "crates/kira-shell/src/calendar.rs").read_text()
LAYOUT = (ROOT / "crates/kira-shell/src/popup_layout.rs").read_text()
NATIVE = (ROOT / "crates/kira-shell/src/native_ui.rs").read_text()
STYLE = (ROOT / "frontend/src/style.css").read_text()
ENTRY = (ROOT / "frontend/src/main.ts").read_text()
SDK = (ROOT / "frontend/src/lib/sdk/core.ts").read_text()


class PopupGeometryTests(unittest.TestCase):
    def test_transient_widget_never_claims_detached_window_columns(self):
        self.assertIn("PopupDock::Center", LAYOUT)
        self.assertIn("PopupDock::Right", LAYOUT)
        self.assertNotIn("PopupDock::Left", LAYOUT)
        self.assertIn("popup_layout::choose_dock(kind, false)", HOST)
        self.assertIn("fn popup(&self) -> Option<Rc<CalendarOverlay>>", HOST)
        self.assertIn("view.requested.get() && !view.detached.get()", HOST)
        self.assertIn("if let Some(view) = self.popup()", HOST)
        self.assertIn("fn sync_geometry(&self)", HOST)
        self.assertIn("pub const PREFERRED_POPUP_WIDTH: i32 = 420", LAYOUT)
        self.assertIn("pub fn popup_width(", LAYOUT)
        self.assertIn("_active_count: usize", LAYOUT)
        self.assertIn("_launcher_active: bool", LAYOUT)
        self.assertNotIn("fn move_to_right", HOST)
        self.assertIn("set_anchor(Edge::Right, dock == PopupDock::Right)", HOST)
        self.assertIn("self.window.set_anchor(Edge::Left, false)", HOST)
        self.assertIn("self.window.set_exclusive_zone(-1)", HOST)
        self.assertIn("self.window.set_exclusive_zone(0)", HOST)

    def test_regular_window_keeps_existing_webview_without_new_load(self):
        self.assertIn("gtk::ApplicationWindow::new(&pool.application)", HOST)
        self.assertIn("normal.set_child(Some(&self.view))", HOST)
        self.assertIn("normal.set_decorated(true)", HOST)
        self.assertIn("normal.set_default_size(500", HOST)
        self.assertIn("self.window.set_visible(false)", HOST)
        self.assertIn("regular_window: RefCell<Option<gtk::ApplicationWindow>>", HOST)
        self.assertIn("html[data-kira-detached=", STYLE)
        self.assertIn("max-height: 100vh;", STYLE)
        self.assertEqual(HOST.count("webkit6::WebView::builder()"), 1)

    def test_svelte_size_and_generation_handshake_remain_safe(self):
        self.assertIn("observeOverlaySize(root, generation)", ENTRY)
        self.assertIn("notifyCalendarMounted(generation, window.matchMedia", ENTRY)
        self.assertIn("Some(UiCommand::ContentHeight(generation, height))", HOST)
        self.assertIn("if self.detached.get() {\n                    return;\n                }", HOST)
        self.assertIn("height.min(crate::native_ui::webview_popup_height_limit", HOST)
        self.assertIn("sendCalendarUi('size:' + generation + ':' + height)", SDK)
        self.assertIn("pub const POPUP_TOP_MARGIN: i32 = 25;", NATIVE)
        self.assertIn("pub const POPUP_RIGHT_MARGIN: i32 = 12;", NATIVE)
        self.assertIn("i64::from(output_height.max(1)) * 7 / 10", NATIVE)


if __name__ == "__main__":
    unittest.main()
