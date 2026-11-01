"""Shared geometry contract for calendar and system popup."""
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
HELPERS = (ROOT / "crates/kira-shell/src/native_ui.rs").read_text()
CALENDAR = (ROOT / "crates/kira-shell/src/calendar.rs").read_text()
MONITOR = (ROOT / "crates/kira-shell/src/system_panel.rs").read_text()


class PopupGeometryTests(unittest.TestCase):
    def test_half_margin_and_monitor_relative_height(self):
        self.assertIn("pub const POPUP_TOP_MARGIN: i32 = 25;", HELPERS)
        self.assertIn("i64::from(output_height) * 7 / 10", HELPERS)
        self.assertIn("monitor_at_surface(&surface)", HELPERS)
        self.assertIn("connect_map(move |window|", HELPERS)
        self.assertIn("scroll.set_max_content_height(height_limit)", HELPERS)
        self.assertIn("scroll.set_propagate_natural_height(true)", HELPERS)
        self.assertIn("window.set_default_size(width, -1)", HELPERS)
        self.assertNotIn("window.set_default_size(width, 490)", HELPERS)
        self.assertIn("native_ui::configure_popup", MONITOR)
        self.assertIn("native_ui::configure_webview_popup", CALENDAR)
        self.assertIn("output_width.saturating_sub(24)", HELPERS)
        self.assertIn("height = preferred_height.min(height_limit)", HELPERS)
        for source in (CALENDAR, MONITOR):
            self.assertNotIn("408, 600", source)
            self.assertNotIn("378, 490", source)
            self.assertIn("native_ui::POPUP_TOP_MARGIN", source)
            self.assertNotIn("set_margin(Edge::Top, 50)", source)
            self.assertIn("set_exclusive_zone(0)", source)


if __name__ == "__main__":
    unittest.main()
