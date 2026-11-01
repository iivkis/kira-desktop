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

    def test_switching_launcher_never_moves_a_mapped_webkit_buffer(self):
        # Calendar/System can switch in place. Moving to/from the centered
        # Launcher must unmap before changing layer-shell anchors, then
        # remap on a subsequent compositor frame with the new Svelte DOM.
        self.assertIn("let relocating = self.window.is_visible()", CALENDAR)
        self.assertIn("(old == Surface::Launcher) != (kind == Surface::Launcher)", CALENDAR)
        self.assertIn("let defer_present = relocating || self.reposition_pending.get();", CALENDAR)
        self.assertIn("self.window.set_visible(false);", CALENDAR)
        self.assertIn("self.reposition_pending.set(defer_present);", CALENDAR)
        show = CALENDAR.split("fn show(self: &Rc<Self>, kind: Surface)", 1)[1].split(
            "fn request_mount(", 1
        )[0]
        self.assertLess(show.index("self.window.set_visible(false)"),
                        show.index("self.position_for(kind);"))
        self.assertIn("self.was_active.set(false);", show)
        self.assertIn("self.window.set_opacity(0.0);", show)
        self.assertIn("Duration::from_millis(32)", show)
        self.assertIn("state.generation.get() == generation", show)
        self.assertIn("state.window.present();", show)
        self.assertIn("self.request_mount();", show)
        self.assertIn("reposition_pending: Cell<bool>", CALENDAR)
        # The current shared WebKit process must not be restarted on moves.
        self.assertEqual(CALENDAR.count("webkit6::WebView::builder()"), 1)
        self.assertNotIn("window.close()", CALENDAR)

    def test_svelte_intrinsic_height_and_scroll_are_bound_to_native_cap(self):
        style = (ROOT / "frontend/src/style.css").read_text()
        entry = (ROOT / "frontend/src/main.ts").read_text()
        sdk = (ROOT / "frontend/src/lib/sdk/core.ts").read_text()
        measure = (ROOT / "frontend/src/lib/overlay-size.ts").read_text()
        components = [
            (ROOT / "frontend/src/lib/components/" / name).read_text()
            for name in ("CalendarWidget.svelte", "SystemWidget.svelte",
                         "LauncherWidget.svelte")
        ]
        self.assertIn('html[data-kira-surface="calendar"] #app', style)
        self.assertIn('max-height: var(--kira-overlay-max-height, 560px)', style)
        self.assertIn('detail.maxHeight + \'px\'', entry)
        self.assertIn('observeOverlaySize(root, generation)', entry)
        self.assertIn('stopSizing?.()', entry)
        self.assertIn('notifyCalendarMounted(generation)', entry)
        self.assertLess(entry.index('observeOverlaySize(root, generation)'),
                        entry.index('notifyCalendarMounted(generation)'))
        self.assertIn('new ResizeObserver(measure)', measure)
        self.assertIn('root.getBoundingClientRect().height', measure)
        self.assertIn('observer?.disconnect()', measure)
        self.assertIn("sendCalendarUi('size:' + generation + ':' + height)", sdk)
        self.assertIn('Some(UiCommand::ContentHeight(generation, height))', CALENDAR)
        self.assertIn('self.generation.get() == generation', CALENDAR)
        self.assertIn('height.min(crate::native_ui::webview_popup_height_limit', CALENDAR)
        self.assertIn('self.content_height.get() == 0', CALENDAR)
        for source in components:
            self.assertIn('flex h-auto min-h-0 w-full flex-col', source)
            self.assertNotIn('flex h-full min-h-0 w-full flex-col', source)
        self.assertIn('overflow-y-auto', components[0])
        self.assertIn('overflow-y-auto', components[1])
        self.assertIn('overflow-y-auto', components[2])

    def test_half_margin_and_monitor_relative_height(self):
        self.assertIn("pub const POPUP_TOP_MARGIN: i32 = 25;", HELPERS)
        self.assertIn("i64::from(output_height.max(1)) * 7 / 10", HELPERS)
        self.assertIn("monitor_at_surface(&surface)", HELPERS)
        self.assertNotIn("connect_map(move |window|", HELPERS)
        self.assertIn("window.set_default_size(width, height)", HELPERS)
        self.assertNotIn("gtk::ScrolledWindow", HELPERS)
        self.assertNotIn("gtk::ScrolledWindow", MONITOR)
        self.assertNotIn("gtk::ApplicationWindow::new", MONITOR)
        self.assertIn("native_ui::configure_webview_popup", CALENDAR)
        self.assertIn("output_width.saturating_sub(24)", HELPERS)
        self.assertIn("height = content_height.clamp(1, webview_popup_height_limit(window))", HELPERS)
        self.assertIn("pub fn resize_webview_popup(", HELPERS)
        self.assertIn("pub fn webview_popup_height_limit(", HELPERS)
        self.assertIn("self.content_height.set(clamped)", CALENDAR)
        self.assertIn("resize_webview_popup(&self.window, 420, clamped)", CALENDAR)
        self.assertIn("maxHeight: {max_height}", CALENDAR)
        self.assertIn('size:42:512', CALENDAR)
        for source in (CALENDAR,):
            self.assertNotIn("408, 600", source)
            self.assertNotIn("378, 490", source)
            self.assertIn("native_ui::POPUP_TOP_MARGIN", source)
            self.assertNotIn("set_margin(Edge::Top, 50)", source)
            self.assertIn("set_exclusive_zone(0)", source)


if __name__ == "__main__":
    unittest.main()
