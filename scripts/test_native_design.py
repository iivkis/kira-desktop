"""Regression tests for the fast, source-only Kira Default/TENET gate."""
import unittest

from check_native_design import validate


CANONICAL_CSS = """
window.kira-bar { background: #000000; color: #ffffff; }
widget:focus-visible { outline: 2px solid #fca311; }
widget.selected { background: #fca311; color: #000000; }
"""
BAR = "pub const BAR_HEIGHT: i32 = 42;\nwindow.set_exclusive_zone(BAR_HEIGHT);"
POPUP = "window.set_exclusive_zone(0);"
MOTION = 'settings.property::<bool>("gtk-enable-animations"); clock.frame_time(); frame_clock_progress(0, 0, 420)'


class NativeDesignGateTests(unittest.TestCase):
    def test_canonical_gtk_contract_passes(self):
        self.assertEqual(validate(CANONICAL_CSS, BAR, POPUP, MOTION), [])

    def test_nonpalette_accent_and_light_text_on_orange_fail(self):
        css = CANONICAL_CSS + """
        widget { color: #fa9911; }
        button.selected { background: #fca311; color: #ffffff; }
        """
        errors = validate(css, BAR, POPUP, MOTION)
        self.assertTrue(any("noncanonical" in error for error in errors))
        self.assertTrue(any("orange surface" in error for error in errors))

    def test_browser_motion_filters_and_missing_focus_are_rejected(self):
        css = """widget { transition: all 250ms; filter: blur(2px); }
        @keyframes flicker { from { opacity: 0; } to { opacity: 1; } }
        """
        errors = validate(css, BAR, POPUP, MOTION)
        self.assertTrue(any("motion or visual filter" in e for e in errors))
        self.assertTrue(any("keyframes" in e for e in errors))
        self.assertTrue(any("keyboard focus" in e for e in errors))

    def test_accessible_focus_cannot_be_suppressed(self):
        css = CANONICAL_CSS + "button:focus-visible { outline: none; }"
        self.assertTrue(any("focus outline" in e for e in validate(css, BAR, POPUP, MOTION)))

    def test_fixed_wayland_ownership_and_frame_clock_are_guarded(self):
        errors = validate(CANONICAL_CSS, BAR.replace("42", "44"), "", "")
        self.assertTrue(any("42 logical" in e for e in errors))
        self.assertTrue(any("zero exclusive" in e for e in errors))
        self.assertTrue(any("reduced-motion" in e for e in errors))
        self.assertTrue(any("painted GTK frame" in e for e in errors))


if __name__ == "__main__":
    unittest.main()
