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
    def test_floating_platform_does_not_paint_an_opaque_rectangle(self):
        for needle in (
            "set_default_size(704, 104)",
            "set_margin(Edge::Bottom, 18)",
            'window.add_css_class("kira-dock-window")',
            "dock_css.load_from_data",
            "gtk::STYLE_PROVIDER_PRIORITY_APPLICATION",
            "RGBA::new(0.0, 0.0, 0.0, 0.0)",
        ):
            self.assertIn(needle, DOCK)
        self.assertIn("data-dock-platform", UI)
        self.assertIn("bg-transparent text-kira-text", UI)
        self.assertIn("size-14 shrink-0", UI)
        self.assertIn("data-dock-icon-placeholder", UI)
        self.assertNotIn("border-t-2 border-kira-accent", UI)
        style = (ROOT / "frontend/src/style.css").read_text()
        self.assertIn(".kira-dock-platform {", style)
        self.assertIn('html[data-kira-surface="dock"] #app', style)

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
