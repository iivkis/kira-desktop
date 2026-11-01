"""Svelte Calendar overlay contract. Native GTK is just a Wayland window host."""
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RUST = (ROOT / "crates/kira-shell/src/calendar.rs").read_text()
SHELL = (ROOT / "crates/kira-shell/src/main.rs").read_text()
NATIVE = (ROOT / "crates/kira-shell/src/native_ui.rs").read_text()
SDK = (ROOT / "frontend/src/lib/sdk/core.ts").read_text()
FRONTEND = (ROOT / "frontend/src/App.svelte").read_text()
CALENDAR = (ROOT / "frontend/src/lib/components/CalendarWidget.svelte").read_text()
TS_MODEL = (ROOT / "frontend/src/lib/calendar-model.ts").read_text()
ENTRY = (ROOT / "frontend/src/main.ts").read_text()
NIX = (ROOT / "nix/package.nix").read_text()


class CalendarDesignTests(unittest.TestCase):
    def test_svelte_owns_date_grid_and_calendar_actions(self):
        for token in (
            'data-kira-ui-root', 'aria-label="Kira Calendar"',
            "monthGrid(", "onclick={() => choose(date)}",
            "onclick={goToday}", "onCopy(selectedIso)",
            "aria-pressed={isoDate(date) === selectedIso}",
            "focus-visible:outline-kira-on-accent",
        ):
            self.assertIn(token, CALENDAR)
        for token in ("monthGrid(", "shiftMonth(", "isoWeek(", "dayOfYear("):
            self.assertIn(token, TS_MODEL)
        for native_ui in ("gtk::Grid", "gtk::Button", "gtk::Label",
                          'include_str!("calendar.css")', "gtk::Calendar"):
            self.assertNotIn(native_ui, RUST)

    def test_calendar_host_is_an_isolated_webkit_overlay(self):
        self.assertIn('set_namespace(Some("kira-calendar"))', RUST)
        self.assertIn("set_exclusive_zone(0)", RUST)
        self.assertIn("set_keyboard_mode(KeyboardMode::OnDemand)", RUST)
        self.assertIn("native_ui::configure_webview_popup(&window, 378, 560)", RUST)
        self.assertIn("native_ui::POPUP_TOP_MARGIN", RUST)
        self.assertIn("restrict_webview_navigation(&view)", RUST)
        self.assertIn('register_script_message_handler("kiraCalendarUi", None)', RUST)
        self.assertIn("view.load_html(&overlay.document, None)", RUST)
        self.assertNotIn('register_script_message_handler("kira", None)', RUST)
        self.assertNotIn("gtk::CssProvider", RUST)
        self.assertIn("pub fn restrict_webview_navigation", NATIVE)
        self.assertIn("pub fn configure_webview_popup", NATIVE)

    def test_calendar_is_preloaded_and_reused_without_a_blank_surface(self):
        self.assertIn("calendar::prewarm(&calendar_app, &calendar_source)", SHELL)
        self.assertIn("static CALENDAR: RefCell<Option<Rc<CalendarOverlay>>>", RUST)
        self.assertIn("fn ensure(application: &gtk::Application, html: &str)", RUST)
        self.assertIn("self.window.set_visible(false)", RUST)
        self.assertIn("self.window.set_opacity(0.0)", RUST)
        self.assertIn("self.window.set_opacity(1.0)", RUST)
        self.assertIn("self.request_paint()", RUST)
        self.assertIn("requestAnimationFrame(() => requestAnimationFrame(", RUST)
        self.assertIn("self.generation.get() == generation", RUST)
        self.assertNotIn("window.close()", RUST)
        self.assertIn("pub fn prewarm(", RUST)
        self.assertIn("requestAnimationFrame", RUST)
        self.assertIn('sendCalendarUi(\'ready\')', SDK)
        self.assertIn("notifyCalendarMounted();", ENTRY)

    def test_surface_is_selected_by_trusted_html_marker(self):
        self.assertIn('data-kira-surface=\\"calendar\\"', RUST)
        self.assertIn("document.documentElement.dataset.kiraSurface === 'calendar'", ENTRY)
        self.assertIn("mount(CalendarWidget", ENTRY)
        self.assertIn("mount(App", ENTRY)
        self.assertIn('data-kira-ui-root', CALENDAR)
        self.assertIn("Svelte calendar DOM verified", RUST)
        self.assertIn("Svelte panel DOM verified", SHELL)
        self.assertIn("native_ui::restrict_webview_navigation(&view)", SHELL)

    def test_calendar_commands_are_allowlisted_and_dates_revalidated(self):
        self.assertIn("calendar_command(message)", RUST)
        self.assertIn("valid_date(year, month, day)", RUST)
        self.assertIn("display.clipboard().set_text(date)", RUST)
        self.assertIn("fn rejects_untrusted_calendar_commands()", RUST)
        self.assertIn('sendCalendarUi("close")'.replace('"', "'"), SDK)
        self.assertIn("sendCalendarUi('copy:' + date)", SDK)
        self.assertIn("kiraCalendarUi?: Bridge", SDK)
        self.assertIn('register_script_message_handler("kiraCalendar", None)', SHELL)
        self.assertIn('if value.to_string() == "toggle"', SHELL)
        self.assertIn("postNativeCommand('kiraCalendar', 'toggle')", SDK)
        self.assertIn("onclick={activateCalendar}", FRONTEND)

    def test_only_small_rust_date_copy_validation_survives(self):
        self.assertIn("fn valid_date(year: i32, month: u8, day: u8) -> bool", RUST)
        self.assertIn("fn rejects_untrusted_calendar_commands()", RUST)
        self.assertNotIn("mod calendar_model;", SHELL)
        self.assertNotIn("calendar_model.rs", RUST)
        self.assertNotIn("calendar.css", NIX)

if __name__ == "__main__":
    unittest.main()
