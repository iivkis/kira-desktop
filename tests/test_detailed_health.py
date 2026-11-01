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

    def test_panel_and_existing_overlays_mutually_exclusive(self):
        launcher = (ROOT / "crates/kira-shell/src/launcher.rs").read_text()
        calendar = (ROOT / "crates/kira-shell/src/calendar.rs").read_text()
        self.assertIn('crate::system_panel::dismiss(application)', launcher)
        self.assertIn('crate::system_panel::dismiss(application)', calendar)
        self.assertIn('crate::calendar::dismiss(app)', RUST)
        self.assertIn('crate::launcher::dismiss(app)', RUST)


if __name__ == '__main__':
    unittest.main()
