"""Synthetic unit fixtures test only the color predicate, never acceptance."""
from pathlib import Path
import sys
import tempfile
import unittest

from PIL import Image, ImageDraw

sys.path.insert(0, str(Path(__file__).resolve().parent))
from dev_preview import PreviewError
from check_native_visual_smoke import (
    ACCENT, BLACK, PRUSSIAN, WHITE,
    bar_present, palette_counts, toast_present,
)


class NativeVisualContractTests(unittest.TestCase):
    def setUp(self):
        t = tempfile.TemporaryDirectory(prefix="kira-pixel-contract-test-")
        self.addCleanup(t.cleanup)
        self.path = Path(t.name) / "unit-fixture.png"

    def test_real_toast_signature_requires_both_solids_in_bottom_right(self):
        fixture = Image.new("RGB", (120, 90), (64, 64, 64))
        fixture.save(self.path)
        self.assertFalse(toast_present(palette_counts(self.path, bottom_right=True)))
        painter = ImageDraw.Draw(fixture)
        painter.rectangle((78, 50, 115, 83), fill=PRUSSIAN)
        painter.rectangle((78, 50, 80, 83), fill=ACCENT)
        fixture.save(self.path)
        self.assertTrue(toast_present(palette_counts(self.path, bottom_right=True)))
        painter.rectangle((78, 50, 80, 83), fill=PRUSSIAN)
        fixture.save(self.path)
        self.assertFalse(toast_present(palette_counts(self.path, bottom_right=True)))

    def test_genuine_top_bar_needs_three_independent_source_solids(self):
        fixture = Image.new("RGB", (120, 90), (64, 64, 64))
        painter = ImageDraw.Draw(fixture)
        painter.rectangle((0, 0, 120, 41), fill=BLACK)
        painter.rectangle((0, 0, 44, 41), fill=PRUSSIAN)
        fixture.save(self.path)
        self.assertFalse(bar_present(palette_counts(self.path, bottom_right=False)))
        painter.rectangle((8, 8, 13, 20), fill=WHITE)
        fixture.save(self.path)
        self.assertTrue(bar_present(palette_counts(self.path, bottom_right=False)))


if __name__ == "__main__":
    unittest.main()
