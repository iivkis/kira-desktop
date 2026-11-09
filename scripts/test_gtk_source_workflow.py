"""Guarantee the automatic GTK source-check remains distinct from the fast CI.

The test reads workflow source only; the real compiler executes in the
separate Nix-backed GitHub Actions job on a native-gtk push.
"""
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


class GtkSourceCheckContract(unittest.TestCase):
    def test_gtk_is_compiled_on_native_branch_push(self):
        path = ROOT / ".github" / "workflows" / "gtk-check.yml"
        workflow = path.read_text(encoding="utf-8")
        self.assertIn("branches: [native-gtk]", workflow)
        self.assertIn("push:", workflow)
        self.assertIn("workflow_dispatch:", workflow)
        self.assertIn("cachix/install-nix-action@", workflow)
        self.assertIn("nix develop .#ci", workflow)
        self.assertIn("cargo check --locked --workspace --all-targets", workflow)
        self.assertIn("cargo clippy --locked --workspace --all-targets -- -D warnings", workflow)
        self.assertIn("cargo test --locked --workspace --all-targets", workflow)
        self.assertIn("timeout-minutes: 30", workflow)

    def test_two_minute_core_signal_remains_independent(self):
        fast = (ROOT / ".github" / "workflows" / "ci.yml").read_text(
            encoding="utf-8"
        )
        self.assertIn("timeout-minutes: 2", fast)
        self.assertNotIn("cargo check --locked --workspace --all-targets", fast)


if __name__ == "__main__":
    unittest.main()
