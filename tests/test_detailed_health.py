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
CHARTS = (ROOT / "frontend/src/lib/components/SystemCharts.svelte").read_text()
MODEL = (ROOT / "frontend/src/lib/system-model.ts").read_text()
SDK = (ROOT / "frontend/src/lib/sdk/core.ts").read_text()
NIX = (ROOT / "nix/package.nix").read_text()


class DetailedHealthContractTests(unittest.TestCase):
    def test_warm_pool_hosts_calendar_and_system_independently(self):
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
        self.assertIn("view.show(Surface::System)", OVERLAY)
        self.assertIn("view.show(Surface::Calendar)", OVERLAY)
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
        self.assertIn("НЕ ОБНАРУЖЕНО", WIDGET)
        self.assertIn("КОПИРОВАТЬ ОТЧЁТ", WIDGET)
        self.assertIn("sendCalendarUi('copy-report:' + generation)", SDK)
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
        self.assertIn("Some(UiCommand::ProcessesTab(generation, active))", OVERLAY)
        self.assertIn('message.strip_prefix("tab:")', OVERLAY)
        self.assertIn('"overview" => false', OVERLAY)
        self.assertIn("setSystemMonitorTab", ENTRY)
        self.assertIn("      ОБЗОР", WIDGET)
        self.assertIn("      ПРОЦЕССЫ", WIDGET)
        self.assertIn("health !== 'ALL SYSTEMS NOMINAL'", WIDGET)
        self.assertNotIn("CURRENT SAMPLE", WIDGET)
        self.assertNotIn("NATIVE FEED", WIDGET)
        self.assertNotIn("LOCAL TELEMETRY / READ ONLY", WIDGET)
        self.assertIn("Фильтр по названию процесса или PID", WIDGET)
        self.assertIn("sortedProcesses(", WIDGET)
        self.assertIn("matching.slice(0, visibleCount)", WIDGET)
        self.assertIn("visibleCount = 36", WIDGET)
        self.assertIn("Math.min(180, visibleCount + 36)", WIDGET)
        self.assertIn("УТОЧНИТЕ ФИЛЬТР", WIDGET)
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

    def test_expired_telemetry_and_process_rows_cannot_look_live(self):
        """A stale badge must also remove stale numeric values and process rows."""
        self.assertIn("const liveMetrics = $derived(sampleFresh ? metrics : null)", WIDGET)
        self.assertIn("const liveProcesses = $derived(processesFresh ? processes : null)", WIDGET)
        self.assertIn("isSampleFresh(processesLastReceivedAt, now, 8_000)", WIDGET)
        self.assertIn("sortedProcesses(liveProcesses?.rows ?? []", WIDGET)
        self.assertIn("usageLabel(liveMetrics?.root_total_kib", WIDGET)
        self.assertIn("usageLabel(liveMetrics?.swap_total_kib", WIDGET)
        self.assertIn("temperatureLabel(liveMetrics?.cpu_temperature_c", WIDGET)
        self.assertIn("ДАННЫЕ ПРОЦЕССОВ УСТАРЕЛИ", WIDGET)
        self.assertIn("ДАННЫЕ GPU НЕДОСТУПНЫ", WIDGET)
        self.assertNotIn("usageLabel(metrics?.root_total_kib", WIDGET)
        self.assertNotIn("usageLabel(metrics?.swap_total_kib", WIDGET)
        self.assertNotIn("{temperatureLabel(metrics?.cpu_temperature_c", WIDGET)
        self.assertNotIn("{#each processes?.rows", WIDGET)

    def test_pulse_freshness_cannot_accept_a_future_timestamp(self):
        """A clock adjustment must never make an old pulse look fresh."""
        self.assertIn("isSampleFresh(lastPulseAt, performance.now(), 2_500)", CHARTS)
        self.assertIn("lastPulseAt = performance.now()", CHARTS)
        self.assertIn("history = appendPulse(history, sample, performance.now()", CHARTS)
        self.assertNotIn("pulseClock", CHARTS)
        self.assertIn("const refreshTick = now;", CHARTS)
        self.assertIn("recentPulseHistory(history, performance.now())", CHARTS)
        self.assertNotIn("now < lastPulseAt ||", CHARTS)

    def test_native_views_and_styles_are_removed(self):
        self.assertNotIn("mod process_page;", HOST)
        self.assertFalse((ROOT / "crates/kira-shell/src/process_page.rs").exists())
        self.assertFalse((ROOT / "crates/kira-shell/src/system_panel.css").exists())
        self.assertNotIn("system_panel.css", NIX)
        self.assertIn("onSystemMetrics(", APP)
        self.assertIn("stopMetrics();", APP)


if __name__ == "__main__":
    unittest.main()
