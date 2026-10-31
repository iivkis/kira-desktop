"""Protect the CI invariant: success requires a native GTK/WebKit Shell build.

The lightweight Rust/TypeScript checks cannot compile kira-shell on runners
without the GTK4/WebKitGTK native dependency stack. GitHub workflow success
must therefore depend on the reusable Nix package build, not just task check.
"""
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]


class NativeShellCiContract(unittest.TestCase):
    def test_ci_always_runs_native_shell_after_fast_checks(self):
        ci = (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8")
        self.assertIn("native-shell:", ci)
        self.assertIn("needs: check", ci)
        self.assertIn("uses: ./.github/workflows/nix.yml", ci)

    def test_native_workflow_is_reusable_and_compiles_package(self):
        nix = (ROOT / ".github/workflows/nix.yml").read_text(encoding="utf-8")
        self.assertIn("workflow_call:", nix)
        self.assertIn("nix build .#default -L --show-trace", nix)
        self.assertIn("Compile packaged Core, GTK/WebKit Shell", nix)


    def test_crane_source_keeps_embedded_launcher_css(self):
        package = (ROOT / "nix/package.nix").read_text(encoding="utf-8")
        launcher = (ROOT / "crates/kira-shell/src/launcher.rs").read_text(
            encoding="utf-8"
        )
        self.assertIn('include_str!("launcher.css")', launcher)
        self.assertIn("craneLib.filterCargoSources", package)
        self.assertIn('"/crates/kira-shell/src/launcher.css"', package)
        self.assertIn("lib.cleanSourceWith", package)


if __name__ == "__main__":
    unittest.main()
