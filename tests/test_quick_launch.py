"""Niri-first Quick Launch replaces the separate Dock surface."""
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SHELL = (ROOT / "crates/kira-shell/src/main.rs").read_text()
QUICK = (ROOT / "crates/kira-shell/src/quick_launch.rs").read_text()
BAR = (ROOT / "frontend/src/App.svelte").read_text()
WIDGET = (ROOT / "frontend/src/lib/components/QuickLaunch.svelte").read_text()
SDK = (ROOT / "frontend/src/lib/sdk/core.ts").read_text()
ENTRY = (ROOT / "frontend/src/main.ts").read_text()


class QuickLaunchContracts(unittest.TestCase):
    def test_no_bottom_layer_or_shadow_dock(self):
        self.assertIn('window.set_exclusive_zone(BAR_HEIGHT)', SHELL)
        self.assertIn('mod quick_launch;', SHELL)
        self.assertIn('QuickLaunch::discover()', SHELL)
        self.assertIn('register_script_message_handler("kiraQuickLaunch", None)', SHELL)
        self.assertNotIn('mod dock;', SHELL)
        self.assertNotIn('dock::start(', SHELL)
        self.assertNotIn('dataset.kiraSurface === "dock"', ENTRY)
        self.assertNotIn("dataset.kiraSurface === 'dock'", ENTRY)
        self.assertFalse((ROOT / "crates/kira-shell/src/dock.rs").exists())
        self.assertFalse((ROOT / "frontend/src/lib/components/DockWidget.svelte").exists())

    def test_worker_only_for_configured_pins_and_no_taskbar_semantics(self):
        for expected in (
            'KIRA_QUICK_APPS', 'KIRA_DOCK_APPS', 'names.is_empty()',
            'return Ok(None);', 'MAX_SHORTCUTS: usize = 3',
            'launcher::Catalog::discover()', 'self.allowed.contains(&id)',
            'ids.iter().all(|id| self.allowed.contains(id))',
            'selected_entries(&full, &self.names)',
        ):
            self.assertIn(expected, QUICK)
        self.assertNotIn('std::process::Command', QUICK)
        self.assertIn('<QuickLaunch />', BAR)
        self.assertIn('aria-label="Quick launch"', WIDGET)
        self.assertIn('min-[740px]:flex', WIDGET)
        self.assertIn("stopApps();", WIDGET)
        self.assertIn("onQuickLaunchApps(", WIDGET)
        self.assertIn("sendQuickCommand('launch:' + id)", SDK)
        self.assertIn("ids.length > 3", SDK)


if __name__ == "__main__":
    unittest.main()
