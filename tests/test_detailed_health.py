"""Svelte System Monitor and bounded read-only Rust backend contract."""
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RUST = (ROOT / "crates/kira-shell/src/system_panel.rs").read_text()
MONITOR = (ROOT / "crates/kira-shell/src/system_metrics.rs").read_text()
PROCESSES = (ROOT / "crates/kira-shell/src/process_monitor.rs").read_text()
HOST = (ROOT / "crates/kira-shell/src/main.rs").read_text()
OVERLAY = (ROOT / "crates/kira-shell/src/calendar.rs").read_text()
ENTRY = (ROOT / "frontend/src/main.ts").read_text()
APP = (ROOT / "frontend/src/App.svelte").read_text()
HEALTH = (ROOT / "frontend/src/lib/components/SystemHealth.svelte").read_text()
WIDGET = (ROOT / "frontend/src/lib/components/SystemWidget.svelte").read_text()
MODEL = (ROOT / "frontend/src/lib/system-model.ts").read_text()
SDK = (ROOT / "frontend/src/lib/sdk/core.ts").read_text()
NIX = (ROOT / "nix/package.nix").read_text()


class DetailedHealthContractTests(unittest.TestCase):
    def test_same_warm_webview_hosts_calendar_and_system(self):
        self.assertIn('register_script_message_handler("kiraSystem", None)', HOST)
        self.assertIn('if value.to_string() == "toggle"', HOST)
        self.assertIn("calendar::toggle_system(", HOST)
        self.assertIn("postNativeCommand('kiraSystem', 'toggle')", SDK)
        self.assertIn('onclick={onOpen}', HEALTH)
        self.assertIn('aria-haspopup="dialog"', HEALTH)
        self.assertIn('set_exclusive_zone(0)', OVERLAY)
        self.assertIn('set_namespace(Some("kira-overlay"))', OVERLAY)
        self.assertIn('configure_webview_popup(&window, 420, 1)', OVERLAY)
        self.assertIn("self.active.set(Some(kind))", OVERLAY)
        self.assertIn("state.show(Surface::System)", OVERLAY)
        self.assertIn("state.show(Surface::Calendar)", OVERLAY)
        self.assertIn("kira:overlay:show", OVERLAY)
        self.assertIn("kira:overlay:hide", OVERLAY)
        self.assertIn("mount(SystemWidget", ENTRY)
        self.assertIn("mount(CalendarWidget", ENTRY)
        self.assertIn("host.hide()", ENTRY)
        self.assertIn("unmount(instance)", ENTRY)
        self.assertIn('data-kira-ui-root', WIDGET)
        self.assertIn('aria-label="Kira System Monitor"', WIDGET)
        self.assertNotIn("webkit6::WebView::builder()", RUST)

    def test_resources_preserved_with_real_safe_kernel_sources(self):
        for path in ("/proc/stat", "/proc/meminfo", "/proc/uptime",
                     "/proc/loadavg", "/sys/class/power_supply"):
            self.assertIn(path, MONITOR)
        self.assertIn("query_filesystem_info(", RUST)
        self.assertIn('kira:system:data', OVERLAY)
        self.assertIn('kira:system:processes', OVERLAY)
        self.assertIn('onSystemOverlayData(', WIDGET)
        self.assertIn('onSystemProcesses(', WIDGET)
        self.assertIn("MEMORY PRESSURE", MODEL)
        self.assertIn("LOW STORAGE", MODEL)
        self.assertIn("LOW BATTERY", MODEL)
        self.assertIn("NOT DETECTED", WIDGET)
        self.assertIn("COPY REPORT", WIDGET)
        self.assertIn("sendCalendarUi('copy-report')", SDK)
        self.assertIn("display.clipboard().set_text(&model.report())", OVERLAY)
        for source in (RUST, OVERLAY, MONITOR):
            self.assertNotIn("std::process::Command", source)
            self.assertNotIn("Command::new", source)

    def test_processes_sample_only_while_visible_and_full_table_is_svelte(self):
        for source in (WIDGET, MODEL):
            self.assertNotIn("gtk::", source)
        self.assertIn("ProcessSampler::default()", RUST)
        self.assertIn("std::thread::spawn", RUST)
        self.assertIn("mpsc::sync_channel(1)", RUST)
        self.assertIn("set_processes_active(false)", OVERLAY)
        self.assertIn("Some(UiCommand::ProcessesTab(active))", OVERLAY)
        self.assertIn("tab:processes", OVERLAY)
        self.assertIn("tab:overview", OVERLAY)
        self.assertIn("setSystemMonitorTab", ENTRY)
        self.assertIn("01 / OVERVIEW", WIDGET)
        self.assertIn("02 / PROCESSES", WIDGET)
        self.assertIn("Filter by process name or PID", WIDGET)
        self.assertIn("sortedProcesses(", WIDGET)
        self.assertIn("matching.slice(0, visibleCount)", WIDGET)
        self.assertIn("visibleCount = 36", WIDGET)
        self.assertIn("Math.min(180, visibleCount + 36)", WIDGET)
        self.assertIn("REFINE FILTER FOR MORE", WIDGET)
        self.assertIn("aria-live=\"off\"", WIDGET)
        self.assertIn("function nextSort(", MODEL)
        self.assertIn("function sortedProcesses(", MODEL)
        self.assertIn("String(row.pid)", MODEL)
        for file in ("/proc", "/proc/stat", 'stat', 'status'):
            self.assertIn(file, PROCESSES)
        self.assertIn("pid, stat.start_ticks", PROCESSES)
        self.assertNotIn("kill(", RUST)
        self.assertNotIn("gtk::Grid", RUST)
        self.assertNotIn("gtk::SearchEntry", RUST)
        self.assertNotIn("gtk::ProgressBar", RUST)

    def test_native_views_and_styles_are_removed(self):
        self.assertNotIn("mod process_page;", HOST)
        self.assertFalse((ROOT / "crates/kira-shell/src/process_page.rs").exists())
        self.assertFalse((ROOT / "crates/kira-shell/src/system_panel.css").exists())
        self.assertNotIn("system_panel.css", NIX)
        self.assertIn("onSystemMetrics(", APP)
        self.assertIn("stopMetrics();", APP)


if __name__ == "__main__":
    unittest.main()
