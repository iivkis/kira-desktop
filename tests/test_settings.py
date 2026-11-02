"""System Settings integrates with the existing popup/xdg-toplevel pool."""
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
NATIVE = (ROOT / "crates/kira-shell/src/calendar.rs").read_text()
MAIN = (ROOT / "crates/kira-shell/src/main.rs").read_text()
INFO = (ROOT / "crates/kira-shell/src/system_info.rs").read_text()
POOL = (ROOT / "crates/kira-shell/src/webview_pool.rs").read_text()
GEOMETRY = (ROOT / "crates/kira-shell/src/popup_layout.rs").read_text()
UI = (ROOT / "frontend/src/lib/components/SettingsWidget.svelte").read_text()
ENTRY = (ROOT / "frontend/src/main.ts").read_text()
APP = (ROOT / "frontend/src/App.svelte").read_text()
SDK = (ROOT / "frontend/src/lib/sdk/core.ts").read_text()
CSS = (ROOT / "frontend/src/style.css").read_text()


class SettingsContracts(unittest.TestCase):
    def test_settings_is_a_normal_pool_surface_and_topbar_command(self):
        self.assertIn('Settings,', POOL)
        self.assertIn('Self::Settings => "settings"', POOL)
        self.assertIn('kiraSettings', MAIN)
        self.assertIn('calendar::toggle_settings(', MAIN)
        self.assertIn('pub fn toggle_settings(', NATIVE)
        self.assertIn('pool.request(', NATIVE)
        self.assertIn('Surface::Settings,', NATIVE)
        self.assertIn('view.show(Surface::Settings)', NATIVE)
        self.assertIn('toggleSettings()', APP)
        self.assertIn('aria-label="Open system settings"', APP)
        self.assertIn('const CALENDAR_SURFACE:', NATIVE)

    def test_fixed_widget_and_full_xdg_window_are_one_live_webview(self):
        self.assertIn('pub const SETTINGS_POPUP_WIDTH: i32 = 600', GEOMETRY)
        self.assertIn('popup_layout::settings_popup_width(output_width)', NATIVE)
        self.assertIn('normal.set_default_size(940, 700)', NATIVE)
        self.assertIn('normal.set_child(Some(&self.view))', NATIVE)
        self.assertIn('window.dispatchEvent(new CustomEvent(\'kira:overlay:detached\'', NATIVE)
        self.assertIn('data-settings-mode={detached ? \'window\' : \'widget\'}', UI)
        self.assertIn('aria-label="Разделы настроек"', UI)
        self.assertIn('id="kira-settings-section"', UI)
        self.assertIn('height: min(560px, var(--kira-overlay-max-height, 560px))', CSS)
        self.assertIn('html[data-kira-detached="true"] .kira-settings', CSS)
        self.assertIn('onPopout', UI)
        self.assertIn('onSettingsAbout(generation,', UI)

    def test_window_navigation_is_responsive_with_one_flow_indicator(self):
        self.assertIn("use:tenetFlowIndicator={{ active: section, axis: 'both' }}", UI)
        self.assertEqual(UI.count("data-kira-flow-indicator"), 1)
        self.assertIn("data-kira-tab={item.id}", UI)
        self.assertIn("max-[720px]:hidden", UI)
        self.assertIn("max-[720px]:block", UI)
        self.assertIn("max-[720px]:px-5", UI)
        self.assertIn("aria-current={section === item.id ? 'page' : undefined}", UI)
        self.assertIn("focus-visible:outline-kira-accent", UI)
        self.assertIn("Изменения дисплеев применяются и сохраняются отдельно", UI)
        self.assertNotIn("Данные устройства. Локально и только для чтения.", UI)

    def test_about_preserves_truthful_readonly_facts_without_repetitive_cards(self):
        self.assertIn("ТОЛЬКО ЧТЕНИЕ", UI)
        self.assertIn("border-l-[3px] border-kira-accent bg-kira-surface", UI)
        self.assertIn("grid grid-cols-1 gap-x-8 sm:grid-cols-2", UI)
        self.assertIn("Недоступные значения не подменяются приблизительными", UI)
        self.assertIn("sectionIsAvailable(id)", UI)

    def test_no_arbitrary_executables_or_fake_system_values(self):
        self.assertIn('read_text("/etc/os-release"', INFO)
        self.assertIn('read_text("/proc/cpuinfo"', INFO)
        self.assertIn('read_text("/proc/meminfo"', INFO)
        self.assertIn('std::thread::available_parallelism()', INFO)
        self.assertNotIn('std::process::Command', INFO)
        self.assertNotIn('unwrap_or(0)', INFO)
        self.assertIn('kira:settings:about', NATIVE)
        self.assertIn('self.generation.get()', NATIVE)
        self.assertIn('isSettingsAbout', SDK)
        self.assertIn("value.generation === generation", SDK)
        self.assertIn('kind === \'settings\'', ENTRY)
        self.assertIn('Kira Settings', ENTRY)


if __name__ == "__main__":
    unittest.main()
