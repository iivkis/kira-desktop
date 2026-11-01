"""Static GTK integration checks; real compiler + Wayland checks run in Nix CI."""
import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RUST = (ROOT / "crates/kira-shell/src/calendar.rs").read_text()
MODEL = (ROOT / "crates/kira-shell/src/calendar_model.rs").read_text()
CSS = (ROOT / "crates/kira-shell/src/calendar.css").read_text()
SHELL = (ROOT / "crates/kira-shell/src/main.rs").read_text()
SDK = (ROOT / "frontend/src/lib/sdk/core.ts").read_text()
FRONTEND = (ROOT / "frontend/src/App.svelte").read_text()
NIX = (ROOT / "nix/package.nix").read_text()


class CalendarDesignTests(unittest.TestCase):
    def test_only_custom_calendar_grid(self):
        self.assertIn("gtk::Grid::new()", RUST)
        self.assertIn("crate::native_ui::configure_popup(&window, &scroll, 378, 490)", RUST)
        self.assertIn("crate::native_ui::POPUP_TOP_MARGIN", RUST)
        self.assertIn("gtk::Button::new()", RUST)
        self.assertIn("month_grid(state.year, state.month)", RUST)
        self.assertNotIn("gtk::Calendar", RUST)
        self.assertNotIn("calendar.kira-calendar-grid", CSS)
        for selector in (
            "button.kira-day", "button.kira-day.kira-day-selected",
            "button.kira-calendar-nav", "button.kira-calendar-copy",
            ".kira-calendar-today-button", ".kira-grid-week",
            ".kira-calendar-month", ".kira-calendar-year",
        ):
            self.assertIn(selector, CSS)

    def test_copy_date_is_plain_orange_text_not_a_tab_or_chip(self):
        block = CSS.split("button.kira-calendar-copy {", 1)[1].split("}", 1)[0]
        self.assertIn("min-height: 29px;", block)
        self.assertIn("padding: 0 4px;", block)
        self.assertIn("border: 0;", block)
        self.assertIn("border-radius: 0;", block)
        self.assertIn("background: transparent;", block)
        self.assertIn("color: @kira_orange;", block)
        self.assertNotIn("border-bottom:", block)
        for state in ("hover", "active"):
            rule = CSS.split(f"button.kira-calendar-copy:{state} {{", 1)[1].split("}", 1)[0]
            self.assertIn("background: transparent;", rule)
            self.assertNotIn("border:", rule)
        self.assertIn("button.kira-calendar-copy:focus-visible", CSS)
        self.assertIn('copy.set_label("COPIED ✓")', RUST)
        today = CSS.split("button.kira-calendar-today-button {", 1)[1].split("}", 1)[0]
        self.assertIn("background: transparent;", today)

    def test_selected_keyboard_focus_contrasts_with_selected_fill(self):
        # Orange selection cannot also use an orange focus ring.
        rule = CSS.split("button.kira-day.kira-day-selected:focus-visible {", 1)[1].split("}", 1)[0]
        self.assertIn("outline: 2px solid @kira_black;", rule)
        self.assertIn("outline-offset: -3px;", rule)

    def test_interactive_adjacent_days_pass_normal_text_contrast(self):
        match = re.search(
            r"button\.kira-day\.kira-day-adjacent\s*\{\s*"
            r"color:\s*rgba\((\d+),\s*(\d+),\s*(\d+),\s*([\d.]+)\);",
            CSS,
        )
        self.assertIsNotNone(match)
        r, g, b = map(int, match.groups()[:3])
        alpha = float(match.group(4))
        background = (20, 33, 61)
        foreground = tuple(alpha * c + (1 - alpha) * bg
                           for c, bg in zip((r, g, b), background))

        def luminance(channels):
            def linear(v):
                v /= 255.0
                return v / 12.92 if v <= 0.04045 else ((v + 0.055) / 1.055) ** 2.4
            return sum(weight * linear(channel) for weight, channel in
                       zip((0.2126, 0.7152, 0.0722), channels))

        lighter, darker = sorted((luminance(foreground), luminance(background)), reverse=True)
        self.assertGreaterEqual((lighter + 0.05) / (darker + 0.05), 4.5)

    def test_date_model_and_keyboard(self):
        for token in (
            "pub fn month_grid", "pub fn shift_month", "pub fn iso_week",
            "fn leap_years_and_century_rules",
            "fn dates_round_trip_across_gregorian_boundaries",
            "fn iso_week_year_crossing_and_day_of_year",
        ):
            self.assertIn(token, MODEL)
        for token in (
            "set_exclusive_zone(0)", "set_keyboard_mode(KeyboardMode::OnDemand)",
            "gtk::gdk::Key::Escape", "gtk::gdk::Key::Left",
            "gtk::gdk::Key::Page_Up", "gtk::gdk::Key::Page_Down",
            "shift_days(7)", "shift_days(-7)", "clipboard().set_text",
            "timeout_add_seconds_local(1", "picker.render(false)",
            "copy.set_tooltip_text", "Rc::downgrade(&picker)",
        ):
            self.assertIn(token, RUST)

    def test_webkit_bridge_and_packaged_css(self):
        self.assertIn("mod calendar;", SHELL)
        self.assertIn("mod calendar_model;", SHELL)
        self.assertIn('register_script_message_handler("kiraCalendar", None)', SHELL)
        self.assertIn('if value.to_string() == "toggle"', SHELL)
        self.assertIn("postNativeCommand('kiraCalendar', 'toggle')", SDK)
        self.assertIn("channel.postMessage(command)", SDK)
        self.assertIn("onclick={activateCalendar}", FRONTEND)
        self.assertIn('aria-haspopup="dialog"', FRONTEND)
        self.assertIn("calendar.css", NIX)
        self.assertIn('include_str!("calendar.css")', RUST)
        self.assertIn("crate::native_ui::install_styles(", RUST)


if __name__ == "__main__":
    unittest.main()
