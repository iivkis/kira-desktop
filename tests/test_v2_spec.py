"""Contract checks for V2 mlua architecture; not a native GTK or Lua runtime test."""
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[1]
V2_FILES = (
    "README.md", "AGENTS.md", "docs/V2_ARCHITECTURE.md",
    "docs/V2_LANGUAGE.md", "docs/V2_STYLES.md", "docs/V2_MIGRATION.md",
    "docs/V2_REVIEW.md", "docs/adr/0001-lua-rust-gtk.md",
    "docs/adr/0002-embedded-lua-mlua.md",
    "examples/v2/counter.lua", "examples/v2/styles.lua",
)
V1_DOCS = (
    "docs/CONFIG.md", "docs/CONFIG_OWNERSHIP.md", "docs/DESIGN_SYSTEM.md",
    "docs/DEVELOPMENT.md", "docs/NOTIFICATIONS.md", "docs/PREVIEW.md",
    "docs/QA_VISUAL.md", "docs/SKY_MAP.md", "docs/SPECIFICATION.md",
    "docs/TENET_MOTION.md", "docs/TRAY_DESIGN.md", "docs/UI_ARCHITECTURE.md",
    "docs/UI_NOISE_REVIEW.md", "docs/VISUAL_REVIEW.md", "docs/WEBVIEW_POOL.md",
)

class V2DocsContract(unittest.TestCase):
    def test_all_files_exist(self):
        for name in V2_FILES:
            with self.subTest(name=name):
                file = ROOT / name
                self.assertTrue(file.is_file())
                self.assertTrue(file.read_text(encoding="utf-8").strip())

    def test_local_links_resolve(self):
        pattern = re.compile(r"\[[^\]]+\]\(([^)\s]+)\)")
        for name in V2_FILES:
            if not name.endswith(".md"):
                continue
            doc = ROOT / name
            for href in pattern.findall(doc.read_text(encoding="utf-8")):
                if "://" in href or href.startswith(("#", "mailto:")):
                    continue
                target = href.split("#", 1)[0]
                with self.subTest(source=name, target=href):
                    self.assertTrue((doc.parent / target).resolve().is_file())

    def test_v1_docs_are_migration_guides(self):
        for name in V1_DOCS:
            with self.subTest(name=name):
                header = (ROOT / name).read_text(encoding="utf-8")[:700]
                self.assertIn("V1", header)
                self.assertIn("V2_ARCHITECTURE.md", header)

    def test_old_repospool_artifacts_stay_removed(self):
        for path in (".repospool", ".repospoolignore",
                     ".github/workflows/publish-repospool.yml"):
            with self.subTest(path=path):
                self.assertFalse((ROOT / path).exists())

    def test_actual_lua_runtime_api_contract(self):
        counter = (ROOT / "examples/v2/counter.lua").read_text(encoding="utf-8")
        for token in ('local ui = require("kira.ui")',
                      'local theme = require("styles")',
                      'ui.state(0)', 'return ui.box {', 'ui.text {',
                      'ui.button {', 'ui.bind(count)', 'on_click = function()',
                      'count:set(count:get() + 1)',
                      'ui.compose(theme.control, theme.primary, { padding = 16 })'):
            with self.subTest(token=token):
                self.assertIn(token, counter)

    def test_runtime_theme_fixture(self):
        source = (ROOT / "examples/v2/styles.lua").read_text(encoding="utf-8")
        for token in ('local ui = require("kira.ui")',
                      'return ui.theme {', 'tokens = {', 'styles = {',
                      'hover = {', 'disabled = {', 'background = "$accent"'):
            with self.subTest(token=token):
                self.assertIn(token, source)

    def test_active_adr_supersedes_transpiler(self):
        old = (ROOT / "docs/adr/0001-lua-rust-gtk.md").read_text(encoding="utf-8")
        new = (ROOT / "docs/adr/0002-embedded-lua-mlua.md").read_text(encoding="utf-8")
        self.assertIn("Superseded by", old)
        self.assertIn("mlua", new)
        self.assertIn("Do not develop a Lua-to-Rust translator", new)

    def test_runtime_not_falsely_marked_complete(self):
        readme = (ROOT / "README.md").read_text(encoding="utf-8")
        architecture = (ROOT / "docs/V2_ARCHITECTURE.md").read_text(encoding="utf-8")
        self.assertIn("ещё не", readme)
        self.assertIn("ещё не реализован", architecture)
        self.assertIn("mlua", readme)

if __name__ == "__main__":
    unittest.main()
