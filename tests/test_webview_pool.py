"""Source-level contracts for bounded, independent GTK/WebKit popup leases."""
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
POOL = (ROOT / "crates/kira-shell/src/webview_pool.rs").read_text()
HOST = (ROOT / "crates/kira-shell/src/calendar.rs").read_text()
ENTRY = (ROOT / "frontend/src/main.ts").read_text()
SDK = (ROOT / "frontend/src/lib/sdk/core.ts").read_text()
NATIVE = (ROOT / "crates/kira-shell/src/native_ui.rs").read_text()


class WebviewPoolContracts(unittest.TestCase):
    def test_one_demand_driven_warm_host_and_explicit_backpressure(self):
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
        self.assertIn("view.disposed.set(true)", HOST)

    def test_one_live_poll_source_for_all_active_slots_only(self):
        self.assertIn("poll_source: RefCell<Option<gtk::glib::SourceId>>", HOST)
        self.assertIn("fn ensure_polling(self: &Rc<Self>)", HOST)
        self.assertIn("if pool.scheduler.borrow().active_count() == 0", HOST)
        self.assertIn("pool.poll_source.borrow_mut().take()", HOST)
        self.assertIn("view.poll_launcher()", HOST)
        self.assertIn("view.poll_pulse()", HOST)
        self.assertIn("view.tick()", HOST)
        self.assertIn("self.system.borrow_mut().take()", HOST)
        # The timer lives on the pool, not every GTK widget/window build.
        build = HOST.split("fn build(", 1)[1].split("fn ensure(", 1)[0]
        self.assertNotIn("timeout_add_local(Duration::from_millis(30)", build)
        self.assertNotIn("timeout_add_local(Duration::from_millis(250)", build)

    def test_each_surface_has_a_separate_active_window_and_ipc_generation(self):
        for kind in ("Calendar", "Launcher", "System"):
            self.assertIn(f"pool.acquire(Surface::{kind})", HOST)
            self.assertIn(f"view.show(Surface::{kind})", HOST)
        self.assertIn("pool.concurrent()", HOST)
        self.assertIn("fn update_launcher_keyboard_mode(&self)", HOST)
        self.assertIn("KeyboardMode::Exclusive", HOST)
        self.assertIn("KeyboardMode::OnDemand", HOST)
        self.assertIn("self.window.present()", HOST)
        self.assertIn("self.generation.get() == generation", HOST)
        self.assertIn("manager.register_script_message_handler", HOST)
        self.assertIn("fn popup_right_margin", NATIVE)
        self.assertIn("set_exclusive_zone(-1)", HOST)

    def test_recycled_slot_cannot_accept_stale_close_or_tab_actions(self):
        for command in ("Close", "CopyDate", "ProcessesTab", "CopyReport"):
            self.assertIn(f"Some(UiCommand::{command}(generation", HOST)
        self.assertIn("parse_ui_generation(value)", HOST)
        self.assertIn("self.generation.get() == generation", HOST)
        self.assertIn("const lease = visibleGeneration ?? 0", ENTRY)
        self.assertIn("onClose: () => closeCalendarOverlay(lease)", ENTRY)
        self.assertIn("onClose: () => closeLauncherOverlay(lease)", ENTRY)
        self.assertIn("onClose: () => closeSystemOverlay(lease)", ENTRY)
        for prefix in ("close:", "copy-report:", "tab:", "copy:"):
            self.assertIn(prefix, SDK)


if __name__ == "__main__":
    unittest.main()
