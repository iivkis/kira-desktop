"""Contracts for one transient popup and multiple promoted Niri WebViews."""
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
POOL = (ROOT / "crates/kira-shell/src/webview_pool.rs").read_text()
HOST = (ROOT / "crates/kira-shell/src/calendar.rs").read_text()
ENTRY = (ROOT / "frontend/src/main.ts").read_text()
SDK = (ROOT / "frontend/src/lib/sdk/core.ts").read_text()
NATIVE = (ROOT / "crates/kira-shell/src/native_ui.rs").read_text()
CONTROLS = (ROOT / "frontend/src/lib/components/WindowControls.svelte").read_text()


class WebviewPoolContracts(unittest.TestCase):
    def test_bounded_warm_pool_never_steals_promoted_views(self):
        self.assertIn("pub const MAX_POPUP_VIEWS: usize = 4", POOL)
        self.assertIn("pub const HOT_IDLE_TARGET: usize = 1", POOL)
        self.assertIn("Duration::from_secs(45)", POOL)
        self.assertIn("pub fn prewarm_one", POOL)
        self.assertIn("Reservation::Full", POOL)
        self.assertIn("self.slots.retain", POOL)
        self.assertIn("pool.scheduler.borrow_mut().prewarm_one", HOST)
        self.assertIn("Reservation::Created(id)", HOST)
        self.assertIn("Reservation::Reused(id)", HOST)
        self.assertIn("view.window.close()", HOST)

    def test_one_poll_source_covers_popup_and_detached_leases(self):
        self.assertIn("poll_source: RefCell<Option<gtk::glib::SourceId>>", HOST)
        self.assertIn("fn ensure_polling(self: &Rc<Self>)", HOST)
        self.assertIn("if pool.scheduler.borrow().active_count() == 0", HOST)
        self.assertIn("view.poll_launcher()", HOST)
        self.assertIn("view.poll_pulse()", HOST)
        self.assertIn("view.tick()", HOST)
        self.assertIn("self.system.borrow_mut().take()", HOST)

    def test_popup_switch_is_serialized_after_reverse_close_ack(self):
        self.assertIn("pending: RefCell<Option<OpenRequest>>", HOST)
        self.assertIn("fn popup(&self) -> Option<Rc<CalendarOverlay>>", HOST)
        self.assertIn("view.requested.get() && !view.detached.get()", HOST)
        self.assertIn("fn request(self: &Rc<Self>, kind: Surface, toggle: bool, open: OpenRequest)", HOST)
        self.assertIn("self.pending.replace(Some(open))", HOST)
        self.assertIn("current.request_close()", HOST)
        self.assertIn("self.pending.borrow_mut().take()", HOST)
        self.assertIn("fn run_pending(self: &Rc<Self>)", HOST)
        self.assertIn("self.run_pending()", HOST)
        self.assertIn("Some(UiCommand::Closed(generation))", HOST)
        self.assertIn("self.generation.get() == generation", HOST)
        self.assertIn("pool.request(\n", HOST)
        for kind in ("Calendar", "System", "Launcher"):
            self.assertIn(f"Surface::{kind},", HOST)
            self.assertIn(f"pool.acquire(Surface::{kind})", HOST)
            self.assertIn(f"view.show(Surface::{kind})", HOST)

    def test_promotion_reparents_same_webview_into_normal_niri_window(self):
        self.assertIn("fn detach(self: &Rc<Self>)", HOST)
        self.assertIn("self.window.set_child(None::<&gtk::Widget>)", HOST)
        self.assertIn("normal.set_child(Some(&self.view))", HOST)
        self.assertIn("normal.set_decorated(true)", HOST)
        self.assertIn("normal.connect_close_request", HOST)
        self.assertIn("self.window.set_child(Some(&self.view))", HOST)
        self.assertIn("fn focus_detached(&self) -> bool", HOST)
        self.assertIn("regular_window: RefCell<Option<gtk::ApplicationWindow>>", HOST)
        self.assertIn("kira:overlay:detached", HOST)
        self.assertNotIn("load_html(", HOST.split("fn detach(self: &Rc<Self>)", 1)[1].split("fn focus_detached", 1)[0])
        self.assertIn("onPopout: () => detachOverlay(lease)", ENTRY)
        self.assertIn("sendCalendarUi('detach:' + generation)", SDK)
        self.assertIn("onPopout", CONTROLS)
        self.assertIn("if (!detached)", CONTROLS.replace("{#if !detached}", "if (!detached)"))
        for name in ("CalendarWidget.svelte", "SystemWidget.svelte", "LauncherWidget.svelte"):
            self.assertIn("WindowControls", (ROOT / "frontend/src/lib/components" / name).read_text())

    def test_generation_isolation_and_safe_native_bridge(self):
        self.assertIn("let lastGeneration = 0", ENTRY)
        self.assertIn("const lease = visibleGeneration ?? 0", ENTRY)
        self.assertIn("parse_ui_generation(value).map(UiCommand::Detach)", HOST)
        self.assertIn("Some(UiCommand::Detach(generation))", HOST)
        self.assertIn("self.generation.get() == generation", HOST)
        self.assertIn("manager.register_script_message_handler", HOST)
        self.assertIn("pub const POPUP_RIGHT_MARGIN: i32 = 12;", NATIVE)


if __name__ == "__main__":
    unittest.main()
