"""Regression checks for the main -> native GTK widget parity acceptance gate."""
import copy
import json
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(Path(__file__).resolve().parent))
from check_widget_parity import MANIFEST, EXPECTED, validate


class ParityInventoryTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.document = json.loads(MANIFEST.read_text(encoding="utf-8"))

    def test_legacy_inventory_covers_every_widget(self):
        self.assertEqual(
            {w["source"]["path"] for w in self.document["widgets"]}, EXPECTED
        )
        self.assertEqual(validate(self.document, ROOT), [])

    def test_a_claim_of_motion_equivalence_requires_recorded_frames(self):
        document = copy.deepcopy(self.document)
        document["widgets"][0]["verified"]["motion"] = True
        violations = validate(document, ROOT)
        self.assertTrue(any("motion_capture" in error for error in violations), violations)

    def test_a_claim_of_pixel_parity_requires_real_comparison_capture(self):
        document = copy.deepcopy(self.document)
        document["widgets"][0]["verified"]["pixels"] = True
        violations = validate(document, ROOT)
        self.assertTrue(any("screen_capture" in error for error in violations), violations)

    def test_missing_component_cannot_pretend_to_be_verified(self):
        document = copy.deepcopy(self.document)
        missing = next(w for w in document["widgets"] if w["state"] == "missing")
        missing["state"] = "verified"
        violations = validate(document, ROOT)
        self.assertTrue(any("without target" in error for error in violations), violations)

    def test_main_baseline_changes_require_review(self):
        document = copy.deepcopy(self.document)
        document["baseline"]["commit"] = "0" * 40
        violations = validate(document, ROOT)
        self.assertTrue(any("baseline review" in error for error in violations), violations)


if __name__ == "__main__":
    unittest.main()
