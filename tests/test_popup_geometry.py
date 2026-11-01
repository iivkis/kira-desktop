"""Shared geometry contract for calendar and system popup."""
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
HELPERS = (ROOT / "crates/kira-shell/src/native_ui.rs").read_text()
CALENDAR = (ROOT / "crates/kira-shell/src/calendar.rs").read_text()
MONITOR = (ROOT / "crates/kira-shell/src/system_panel.rs").read_text()


class PopupGeometryTests(unittest.TestCase):
    def test_launcher_is_centered_without_affecting_other_widgets(self):
        self.assertIn("fn position_for(&self, kind: Surface)", CALENDAR)
        self.assertIn("let centered = kind == Surface::Launcher;", CALENDAR)
        self.assertIn("self.position_for(kind);", CALENDAR)
        self.assertIn("self.window.set_anchor(Edge::Top, !centered);", CALENDAR)
        self.assertIn("self.window.set_anchor(Edge::Right, !centered);", CALENDAR)
        self.assertIn("self.window.set_anchor(Edge::Bottom, false);", CALENDAR)
        self.assertIn("self.window.set_anchor(Edge::Left, false);", CALENDAR)
        self.assertIn("self.window.set_exclusive_zone(-1);", CALENDAR)
        self.assertIn("self.window.set_exclusive_zone(0);", CALENDAR)
        self.assertIn("self.window.set_margin(Edge::Top, 0);", CALENDAR)
        self.assertIn("self.window.set_margin(Edge::Right, 0);", CALENDAR)
        self.assertIn("self.window.set_margin(Edge::Right, 12);", CALENDAR)
        self.assertIn(
            "set_margin(Edge::Top, crate::native_ui::POPUP_TOP_MARGIN)",
            CALENDAR,
        )
        # The shared layer-shell window and WebKit renderer stay in place.
        self.assertIn("static CALENDAR: RefCell<Option<Rc<CalendarOverlay>>>", CALENDAR)
        self.assertNotIn("window.close()", CALENDAR)

    def test_half_margin_and_monitor_relative_height(self):
        self.assertIn("pub const POPUP_TOP_MARGIN: i32 = 25;", HELPERS)
        self.assertIn("i64::from(output_height.max(1)) * 7 / 10", HELPERS)
        self.assertIn("monitor_at_surface(&surface)", HELPERS)
        self.assertIn("connect_map(move |window|", HELPERS)
        self.assertIn("window.set_default_size(width, height)", HELPERS)
        self.assertNotIn("gtk::ScrolledWindow", HELPERS)
        self.assertNotIn("gtk::ScrolledWindow", MONITOR)
        self.assertNotIn("gtk::ApplicationWindow::new", MONITOR)
        self.assertIn("native_ui::configure_webview_popup", CALENDAR)
        self.assertIn("output_width.saturating_sub(24)", HELPERS)
        self.assertIn("height = preferred_height.min(height_limit)", HELPERS)
        for source in (CALENDAR,):
            self.assertNotIn("408, 600", source)
            self.assertNotIn("378, 490", source)
            self.assertIn("native_ui::POPUP_TOP_MARGIN", source)
            self.assertNotIn("set_margin(Edge::Top, 50)", source)
            self.assertIn("set_exclusive_zone(0)", source)


if __name__ == "__main__":
    unittest.main()
