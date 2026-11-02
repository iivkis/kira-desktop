"""Read-only Dock startup and trusted GIO contract."""
import unittest
from pathlib import Path
ROOT = Path(__file__).resolve().parents[1]
DOCK = (ROOT / "crates/kira-shell/src/dock.rs").read_text()
SHELL = (ROOT / "crates/kira-shell/src/main.rs").read_text()
UI = (ROOT / "frontend/src/lib/components/DockWidget.svelte").read_text()
SDK = (ROOT / "frontend/src/lib/sdk/core.ts").read_text()
ENTRY = (ROOT / "frontend/src/main.ts").read_text()

class DockTests(unittest.TestCase):
    def test_native_shell_isolated_and_safe(self):
        for needle in ["dock::start(app, &html)", "KIRA_DOCK_DISABLE"]:
            self.assertIn(needle, SHELL)
        for needle in ['set_namespace(Some("kira-dock"))',
                       "set_anchor(Edge::Bottom, true)", "set_exclusive_zone(0)",
                       "restrict_webview_navigation(&view)",
                       "RendererRecovery::default()", "KIRA_DOCK_APPS",
                       "launcher::Catalog::discover()", "catalog.launch(id)",
                       "catalog.request_icons(&ids)"]:
            self.assertIn(needle, DOCK)
        self.assertNotIn("std::process::Command", DOCK)
    def test_dom_bridge_and_lifecycle(self):
        self.assertIn("dataset.kiraSurface === 'dock'", ENTRY)
        self.assertIn("mount(DockWidget", ENTRY)
        self.assertIn('aria-label="Kira Dock"', UI)
        self.assertIn('aria-label="Dock applications"', UI)
        self.assertIn("requested.clear();", UI)
        self.assertIn("onDockCatalog(", UI)
        self.assertIn("sendDockCommand('launch:' + id)", SDK)
        self.assertIn("ids.length > 8", SDK)

if __name__ == "__main__":
    unittest.main()
