"""Cross-layer calendar contract without pretending this is a Wayland smoke test."""
import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RUST = (ROOT / "crates/kira-shell/src/calendar.rs").read_text()
CSS = (ROOT / "crates/kira-shell/src/calendar.css").read_text()
SHELL = (ROOT / "crates/kira-shell/src/main.rs").read_text()
SDK = (ROOT / "frontend/src/lib/sdk/core.ts").read_text()
FRONTEND = (ROOT / "frontend/src/App.svelte").read_text()
NIX = (ROOT / "nix/package.nix").read_text()


class CalendarDesignTests(unittest.TestCase):
    def test_canonical_palette_matches_frontend(self):
        defined = dict(re.findall(
            r"@define-color (kira_[a-z_]+) (#[0-9a-fA-F]{8});", CSS
        ))
        self.assertEqual(defined, {
            "kira_black": "#000000ff",
            "kira_prussian_blue": "#14213dff",
            "kira_orange": "#fca311ff",
            "kira_alabaster_grey": "#e5e5e5ff",
            "kira_white": "#ffffffff",
        })
        self.assertEqual(set(re.findall(r"#[0-9a-f]{8}\b", CSS)), set(defined.values()))

    def test_native_layer_and_real_date_actions(self):
        for token in (
            'set_anchor(Edge::Top, true)',
            'set_anchor(Edge::Right, true)',
            'set_exclusive_zone(0)',
            'set_keyboard_mode(KeyboardMode::OnDemand)',
            'gtk::Calendar::new()', 'set_show_week_numbers(true)',
            'set_show_heading(true)', 'select_day(&today)', 'Some("date")',
            'connect_is_active_notify', 'gtk::gdk::Key::Escape',
            'timeout_add_seconds_local', 'calendar.date()',
            'clipboard().set_text', 'ISO WEEK', 'DAY {:03}',
            'date.week_of_year()', 'date.day_of_year()',
        ):
            self.assertIn(token, RUST)
        for section in (
            '.kira-calendar-hero', '.kira-calendar-workbench',
            'calendar.kira-calendar-grid', '.kira-calendar-footer',
            '.kira-calendar-copy',
        ):
            self.assertIn(section, CSS)

    def test_webkit_bridge_and_packaged_css(self):
        self.assertIn('mod calendar;', SHELL)
        self.assertIn('register_script_message_handler("kiraCalendar", None)', SHELL)
        self.assertIn('if value.to_string() == "toggle"', SHELL)
        self.assertIn("channel.postMessage('toggle')", SDK)
        self.assertIn('onclick={activateCalendar}', FRONTEND)
        self.assertIn('aria-haspopup="dialog"', FRONTEND)
        self.assertIn('calendar.css', NIX)
        self.assertIn('include_str!("calendar.css")', RUST)


if __name__ == "__main__":
    unittest.main()
