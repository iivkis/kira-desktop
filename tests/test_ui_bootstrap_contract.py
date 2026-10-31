"""Keep GTK, Svelte and WebKit bootstrap health checks framework-independent."""
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SELECTOR = "main[data-kira-ui-root]"


class BootstrapContractTests(unittest.TestCase):
    def test_svelte_root_matches_webkit_and_bootstrap(self):
        app = (ROOT / "frontend/src/App.svelte").read_text()
        bootstrap = (ROOT / "frontend/src/lib/bootstrap.ts").read_text()
        shell = (ROOT / "crates/kira-shell/src/main.rs").read_text()

        self.assertIn("data-kira-ui-root", app)
        self.assertIn(SELECTOR, bootstrap)
        self.assertIn(SELECTOR, shell)
        self.assertNotIn("main.bar", bootstrap)
        self.assertNotIn("main.bar", shell)

    def test_failure_is_not_overwritten_by_generic_timeout(self):
        bootstrap = (ROOT / "frontend/src/lib/bootstrap.ts").read_text()
        html = (ROOT / "frontend/index.html").read_text()
        self.assertIn("bootstrapError", bootstrap)
        self.assertIn("window.__kiraBoot.bootstrapError", html)


if __name__ == "__main__":
    unittest.main()
