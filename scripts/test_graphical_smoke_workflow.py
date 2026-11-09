"""Guard the independent Wayland smoke gate against accidental removal.

Source checks only. These tests do NOT claim that a CI job ran successfully
or that pixels matched the immutable main/Svelte reference.
"""
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


class GraphicalSmokeGateTests(unittest.TestCase):
    def test_native_push_and_pr_have_real_graphical_smoke(self):
        path = ROOT / ".github" / "workflows" / "graphical-smoke.yml"
        source = path.read_text(encoding="utf-8")
        self.assertIn("push:", source)
        self.assertIn("pull_request:", source)
        self.assertIn("branches: [native-gtk]", source)
        self.assertIn("workflow_dispatch:", source)
        self.assertIn("nix build .#default -L", source)
        self.assertIn("nix develop .#graphical-ci", source)
        self.assertIn("scripts/graphical_viewport_matrix.py", source)
        self.assertIn("--package-dir result --output artifacts/virtual-niri", source)
        self.assertIn("102 actual PNG frames", source)
        self.assertIn("800x600, 1280x800 and 1920x1080", source)
        self.assertIn("scripts/visual_contract_checks.py", source)
        self.assertIn("timeout-minutes: 45", source)

    def test_full_manual_evidence_and_fast_core_remain_separate(self):
        manual = (ROOT / ".github" / "workflows" / "graphical.yml").read_text(
            encoding="utf-8"
        )
        fast = (ROOT / ".github" / "workflows" / "ci.yml").read_text(
            encoding="utf-8"
        )
        self.assertIn("workflow_dispatch:", manual)
        self.assertIn("--profile", manual)
        self.assertIn("upload_evidence:", manual)
        self.assertNotIn("branches: [native-gtk]", manual)
        self.assertIn("timeout-minutes: 2", fast)
        self.assertNotIn("scripts/headless_wayland_acceptance.py", fast)


if __name__ == "__main__":
    unittest.main()
