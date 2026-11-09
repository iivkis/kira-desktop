"""CI scope must fail open for untrusted refs and keep source changes tested."""
import subprocess
from pathlib import Path
import sys
from types import SimpleNamespace
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
from ci_change_scope import changed_paths, needs_native_build


class ScopeTests(unittest.TestCase):
    A = "a" * 40
    B = "b" * 40

    def test_docs_only_does_not_rebuild_native(self):
        self.assertFalse(needs_native_build([
            "docs/MAIN_WIDGET_PARITY.md", "docs/MAIN_WIDGET_PARITY.json",
            "README.md", ".github/workflows/graphical.yml",
            ".github/workflows/nix.yml",
        ]))
        self.assertFalse(needs_native_build([]))

    def test_every_runtime_and_smoke_input_triggers_build(self):
        for path in (
            "crates/kira-ui/src/bar.rs", "crates/kira-shell/src/main.rs",
            "crates/kira-core/src/main.rs", "nix/package.nix",
            "config/kira.kdl.example", "scripts/smoke_sni_fixture_bus.py",
            "scripts/ci_change_scope.py", "Cargo.toml", "Cargo.lock",
            "flake.lock", "flake.nix", ".github/workflows/ci.yml",
            "Taskfile.yml",
        ):
            with self.subTest(path=path):
                self.assertTrue(needs_native_build([path]))

    def test_base_missing_or_invalid_is_fail_open(self):
        self.assertIsNone(changed_paths("0" * 40, "", runner=lambda *args, **kw: None))
        self.assertIsNone(changed_paths("unknown", self.B, runner=lambda *args, **kw: None))
        # A zero SHA is syntactically valid but Github rejects the fetch;
        # the resulting unknown diff still requires a native build.
        def missing(argv, **kwargs):
            return SimpleNamespace(returncode=1, stdout=b"")
        self.assertIsNone(changed_paths("0" * 40, self.B, runner=missing))

    def test_compares_complete_commit_range_not_only_last_commit(self):
        calls = []
        def runner(argv, **kwargs):
            calls.append(argv)
            if argv[:3] == ["git", "cat-file", "-e"]:
                return SimpleNamespace(returncode=1, stdout=b"")
            if argv[:2] == ["git", "fetch"]:
                return SimpleNamespace(returncode=0, stdout=b"")
            return SimpleNamespace(
                returncode=0,
                stdout=b"docs/QA_VISUAL.md\0crates/kira-ui/src/bar.rs\0",
            )
        names = changed_paths(self.A, self.B, runner=runner)
        self.assertEqual(names, ["docs/QA_VISUAL.md", "crates/kira-ui/src/bar.rs"])
        self.assertTrue(needs_native_build(names))
        self.assertEqual(calls[-1][-2:], [self.A, self.B])

    def test_unreadable_diff_never_silently_skips_packages(self):
        def broken(argv, **kwargs):
            raise subprocess.TimeoutExpired(argv, 20)
        self.assertIsNone(changed_paths(self.A, self.B, runner=broken))
