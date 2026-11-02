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
        self.assertIn("fn apply_dock(&self, dock: PopupDock)", CALENDAR)
        self.assertIn("popup_layout::choose_dock(kind, false)", CALENDAR)
        self.assertIn("self.position_for(kind);", CALENDAR)
        self.assertIn("self.window.set_anchor(Edge::Top, dock != PopupDock::Center);", CALENDAR)
        self.assertIn("self.window.set_anchor(Edge::Right, dock == PopupDock::Right);", CALENDAR)
        self.assertIn("self.window.set_anchor(Edge::Bottom, false);", CALENDAR)
        self.assertIn("self.window.set_anchor(Edge::Left, dock == PopupDock::Left);", CALENDAR)
        self.assertIn("self.window.set_exclusive_zone(-1);", CALENDAR)
        self.assertIn("self.window.set_exclusive_zone(0);", CALENDAR)
        self.assertIn("self.window.set_margin(Edge::Top, 0);", CALENDAR)
        self.assertIn("self.window.set_margin(Edge::Right, 0);", CALENDAR)
        self.assertIn("native_ui::POPUP_RIGHT_MARGIN", CALENDAR)
        self.assertIn("pub const POPUP_RIGHT_MARGIN: i32 = 12;", HELPERS)
        self.assertIn(
            "set_margin(Edge::Top, crate::native_ui::POPUP_TOP_MARGIN)",
            CALENDAR,
        )
        # Separate dedicated hosts never re-anchor another active popup.
        self.assertIn("static POOL: RefCell<Option<Rc<OverlayPool>>>", CALENDAR)
        self.assertIn("fn concurrent(&self) -> bool", CALENDAR)
        self.assertIn("self.pool.upgrade()", CALENDAR)

    def test_only_requested_windows_consume_layout_docks(self):
        # Dormant WebViews do not reserve the System Monitor's position.
        self.assertIn("view.requested.get() && view.dock.get() == PopupDock::Right", CALENDAR)
        self.assertIn("popup_layout::choose_dock(kind, right_occupied)", CALENDAR)
        self.assertIn("self.window.set_anchor(Edge::Left, dock == PopupDock::Left)", CALENDAR)
        self.assertIn("self.window.set_anchor(Edge::Right, dock == PopupDock::Right)", CALENDAR)
        self.assertIn("PopupDock::Right => self.window.set_margin(", CALENDAR)
        self.assertIn("PopupDock::Left => self.window.set_margin(", CALENDAR)
        self.assertNotIn("popup_right_margin(", HELPERS)
        self.assertNotIn("448", HELPERS)

    def test_focus_changes_never_close_other_pooled_widgets(self):
        self.assertNotIn("connect_is_active_notify", CALENDAR)
        self.assertNotIn("was_active: Cell<bool>", CALENDAR)
        self.assertIn("fn move_to_right(self: &Rc<Self>)", CALENDAR)
        self.assertIn("self.window.set_visible(false);", CALENDAR)
        self.assertIn("Duration::from_millis(32)", CALENDAR)
        self.assertIn("pool.release(self.id)", CALENDAR)

    def test_popup_widths_reflow_to_avoid_launcher_overlap(self):
        layout = (ROOT / "crates/kira-shell/src/popup_layout.rs").read_text()
        self.assertIn("pub fn popup_width(", layout)
        self.assertIn("let columns = if active_count >= 3", layout)
        self.assertIn("popup_layout::popup_width(", CALENDAR)
        self.assertIn("fn sync_geometry(&self)", CALENDAR)
        self.assertIn("pool.sync_geometry()", CALENDAR)
        self.assertIn("crate::native_ui::popup_output_width(&view.window)", CALENDAR)
        self.assertIn("assert_eq!(popup_width(1264, 3, true), 408)", layout)

    def test_switching_launcher_preserves_existing_buffers_and_generations(self):
        # A new surface reserves its own hot or newly created WebKit view.
        # Nothing unmaps the old scene or moves its Wayland surface.
        self.assertIn("let Some((view, existing)) = pool.acquire(Surface::Launcher)", CALENDAR)
        self.assertIn("let Some((view, existing)) = pool.acquire(Surface::System)", CALENDAR)
        self.assertIn("let Some((view, existing)) = pool.acquire(Surface::Calendar)", CALENDAR)
        self.assertIn("debug_assert!(!self.requested.get())", CALENDAR)
        self.assertIn("self.position_for(kind);", CALENDAR)
        self.assertIn("self.window.set_opacity(0.0);", CALENDAR)
        self.assertIn("self.window.present();", CALENDAR)
        self.assertIn("self.request_mount();", CALENDAR)
        self.assertNotIn("defer_present", CALENDAR)
        self.assertNotIn("reposition_pending", CALENDAR)
        self.assertEqual(CALENDAR.count("webkit6::WebView::builder()"), 1)

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
        self.assertIn('notifyCalendarMounted(generation, window.matchMedia', entry)
        self.assertLess(entry.index('observeOverlaySize(root, generation)'),
                        entry.index('notifyCalendarMounted(generation, window.matchMedia'))
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
        self.assertIn("crate::native_ui::resize_webview_popup(&self.window, width, clamped)", CALENDAR)
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
