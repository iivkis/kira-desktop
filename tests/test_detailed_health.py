"""Read-only system monitor integration and UI affordance checks."""
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RUST = (ROOT / "crates/kira-shell/src/system_panel.rs").read_text()
MONITOR = (ROOT / "crates/kira-shell/src/system_metrics.rs").read_text()
CSS = (ROOT / "crates/kira-shell/src/system_panel.css").read_text()
HOST = (ROOT / "crates/kira-shell/src/main.rs").read_text()
APP = (ROOT / "frontend/src/App.svelte").read_text()
COMPONENT = (ROOT / "frontend/src/lib/components/SystemHealth.svelte").read_text()
SDK = (ROOT / "frontend/src/lib/sdk/core.ts").read_text()
NIX = (ROOT / "nix/package.nix").read_text()


class DetailedHealthContractTests(unittest.TestCase):
    def test_overlay_is_fixed_purpose_and_nonexclusive(self):
        self.assertIn('register_script_message_handler("kiraSystem", None)', HOST)
        self.assertIn('if value.to_string() == "toggle"', HOST)
        self.assertIn('postNativeCommand(\'kiraSystem\', \'toggle\')', SDK)
        self.assertIn('onclick={onOpen}', COMPONENT)
        self.assertIn('aria-haspopup="dialog"', COMPONENT)
        self.assertIn('set_exclusive_zone(0)', RUST)
        self.assertIn('set_anchor(Edge::Top, true)', RUST)
        self.assertIn('set_anchor(Edge::Right, true)', RUST)
        self.assertIn('crate::native_ui::configure_popup(&window, &scroll, 408, 600)', RUST)
        self.assertIn('crate::native_ui::POPUP_TOP_MARGIN', RUST)
        self.assertIn('kira:system-metrics', HOST)
        self.assertIn('onSystemMetrics(', APP)
        self.assertIn('stopMetrics();', APP)

    def test_real_sources_no_arbitrary_commands(self):
        for path in ('/proc/stat', '/proc/meminfo', '/proc/uptime',
                     '/proc/loadavg', '/sys/class/power_supply'):
            self.assertIn(path, MONITOR)
        self.assertIn('query_filesystem_info(', RUST)
        self.assertIn('None::<&gtk::gio::Cancellable>', RUST)
        self.assertNotIn('std::process::Command', MONITOR)
        self.assertNotIn('std::process::Command', RUST)
        self.assertIn('Memory: ', RUST)
        self.assertIn('MEASURING', RUST)
        self.assertIn('NOT DETECTED', RUST)

    def test_shared_native_palette_and_gtk_styles(self):
        self.assertIn('crate::native_ui::install_styles(', RUST)
        self.assertIn('include_str!("system_panel.css")', RUST)
        self.assertIn('system_panel.css', NIX)
        self.assertIn('.kira-system-hero', CSS)
        self.assertIn('.kira-system-workbench', CSS)
        self.assertIn('.kira-system-services', CSS)
        self.assertIn('progressbar.kira-system-meter', CSS)
        self.assertIn('button.kira-system-copy', CSS)
        self.assertNotIn('@define-color', CSS)
        self.assertNotIn('box-shadow:', CSS)
        self.assertIn('kira-process-header-active', CSS)

    def test_panel_and_existing_overlays_mutually_exclusive(self):
        launcher = (ROOT / "crates/kira-shell/src/launcher.rs").read_text()
        calendar = (ROOT / "crates/kira-shell/src/calendar.rs").read_text()
        self.assertIn('crate::system_panel::dismiss(application)', launcher)
        self.assertIn('crate::system_panel::dismiss(application)', calendar)
        self.assertIn('crate::calendar::dismiss(app)', RUST)
        self.assertIn('crate::launcher::dismiss(app)', RUST)


if __name__ == '__main__':
    unittest.main()

class ProcessesTabContractTests(unittest.TestCase):
    def test_custom_tabs_and_safe_data_model(self):
        page = (ROOT / "crates/kira-shell/src/process_page.rs").read_text()
        reader = (ROOT / "crates/kira-shell/src/process_monitor.rs").read_text()
        self.assertIn('01  OVERVIEW', RUST)
        self.assertIn('02  PROCESSES', RUST)
        self.assertIn('set_visible_child_name("overview")', RUST)
        self.assertIn('set_visible_child_name("processes")', RUST)
        self.assertIn('page_processes.set_active(true)', RUST)
        self.assertIn('page_overview.set_active(false)', RUST)
        self.assertIn('process_page.poll()', RUST)
        self.assertIn('gtk::SearchEntry::new()', page)
        self.assertIn('Sort::Cpu', page)
        self.assertIn('Sort::Memory', page)
        self.assertIn('Sort::Pid', page)
        self.assertIn('Sort::Name', page)
        self.assertIn('fn compare_rows(', page)
        self.assertIn('fn clicked(self, field: Sort)', page)
        self.assertIn('fn each_column_sorts_both_directions()', page)
        self.assertIn('fn missing_metrics_are_last_in_both_directions()', page)
        self.assertNotIn('sort_controls', page)
        self.assertIn('VISIBLE_ROWS: usize = 36', page)
        self.assertIn('ProcessSampler::default()', page)
        self.assertIn('std::thread::spawn', page)
        self.assertIn('Arc<AtomicBool>', page)
        for file in ('/proc', '/proc/stat', 'stat', 'status'):
            self.assertIn(file, reader)
        self.assertIn('pid, stat.start_ticks', reader)
        self.assertIn('fn parse_stat(', reader)
        self.assertNotIn('std::process::Command', reader)
        self.assertNotIn('Command::new', page)
        self.assertNotIn('kill(', page)

    def test_visual_style_uses_shared_palette_without_framed_notebook(self):
        for selector in ('.kira-system-tabs', 'button.kira-system-tab',
                         '.kira-process-page', '.kira-process-row',
                         'entry.kira-process-search', 'button.kira-process-header'):
            self.assertIn(selector, CSS)
        self.assertNotIn('@define-color', CSS)
