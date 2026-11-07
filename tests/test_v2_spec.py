"""V2 documentation-contract tests. Not tests of Lua compilation or GTK execution."""
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[1]
V2_FILES = (
    "README.md", "AGENTS.md", "docs/V2_ARCHITECTURE.md",
    "docs/V2_LANGUAGE.md", "docs/V2_STYLES.md", "docs/V2_MIGRATION.md", "docs/V2_REVIEW.md",
    "docs/adr/0001-lua-rust-gtk.md", "examples/v2/counter.lua",
    "examples/v2/styles.lua",
)
V1_DOCS = (
    "docs/CONFIG.md", "docs/CONFIG_OWNERSHIP.md", "docs/DESIGN_SYSTEM.md",
    "docs/DEVELOPMENT.md", "docs/NOTIFICATIONS.md", "docs/PREVIEW.md",
    "docs/QA_VISUAL.md", "docs/SKY_MAP.md", "docs/SPECIFICATION.md",
    "docs/TENET_MOTION.md", "docs/TRAY_DESIGN.md", "docs/UI_ARCHITECTURE.md",
    "docs/UI_NOISE_REVIEW.md", "docs/VISUAL_REVIEW.md", "docs/WEBVIEW_POOL.md",
)

class V2DocsContract(unittest.TestCase):
    def test_all_v2_files_exist(self):
        for name in V2_FILES:
            with self.subTest(name=name):
                f = ROOT / name
                self.assertTrue(f.is_file())
                self.assertTrue(f.read_text(encoding="utf-8").strip())

    def test_local_links_resolve(self):
        pattern = re.compile(r"\[[^\]]+\]\(([^)\s]+)\)")
        for name in V2_FILES:
            if not name.endswith(".md"):
                continue
            doc = ROOT / name
            for link in pattern.findall(doc.read_text(encoding="utf-8")):
                if "://" in link or link.startswith("#") or link.startswith("mailto:"):
                    continue
                target = link.split("#", 1)[0]
                with self.subTest(source=name, target=link):
                    self.assertTrue((doc.parent / target).resolve().is_file())

    def test_old_documents_are_labeled_migration_only(self):
        for name in V1_DOCS:
            with self.subTest(name=name):
                header = (ROOT / name).read_text(encoding="utf-8")[:700]
                self.assertIn("V1", header)
                self.assertIn("V2_ARCHITECTURE.md", header)

    def test_repo_snapshot_is_removed(self):
        for path in (".repospool", ".repospoolignore",
                     ".github/workflows/publish-repospool.yml"):
            with self.subTest(path=path):
                self.assertFalse((ROOT / path).exists())

    def test_counter_is_marked_unimplemented(self):
        counter = (ROOT / "examples/v2/counter.lua").read_text(encoding="utf-8")
        self.assertIn("NOT YET COMPILABLE", counter)
        for token in ('return ui.box {', 'ui.text {', 'ui.button {',
                      'ui.bind("count")', 'theme = "styles.lua"',
                      'style = { "control", "primary", padding = 16 }',
                      'state.count = state.count + 1'):
            with self.subTest(token=token):
                self.assertIn(token, counter)

    def test_lua_style_dsl_fixture(self):
        src = (ROOT / "examples/v2/styles.lua").read_text(encoding="utf-8")
        for item in ('return ui.theme {', 'tokens = {', 'styles = {',
                     'hover = {', 'disabled = {', 'background = "$accent"'):
            with self.subTest(item=item):
                self.assertIn(item, src)

    def test_compiler_not_claimed_complete(self):
        readme = (ROOT / "README.md").read_text(encoding="utf-8")
        language = (ROOT / "docs/V2_LANGUAGE.md").read_text(encoding="utf-8")
        self.assertIn("ещё не реализован", readme)
        self.assertIn("пока не реализован", language)

if __name__ == "__main__":
    unittest.main()
