"""Display settings implementation contract: no fake data or blind configuration."""
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MANAGER = (ROOT / "crates/kira-shell/src/display_manager.rs").read_text()
SHELL = (ROOT / "crates/kira-shell/src/calendar.rs").read_text()
SETTINGS = (ROOT / "frontend/src/lib/components/SettingsWidget.svelte").read_text()
DISPLAY = (ROOT / "frontend/src/lib/components/DisplaySettings.svelte").read_text()
SDK = (ROOT / "frontend/src/lib/sdk/core.ts").read_text()


class DisplayContract(unittest.TestCase):
    def test_multi_screen_options_are_real_niri_requests(self):
        self.assertIn('Command::new("niri")', MANAGER)
        self.assertIn('["--json", "outputs"]', MANAGER)
        for command in ['"mode"', '"scale"', '"transform"', '"position"', '"vrr"', '"on"', '"off"']:
            self.assertIn(command, MANAGER)
        self.assertNotIn('Command::new("sh")', MANAGER)
        self.assertIn('output["modes"]', MANAGER)
        self.assertIn('settings_displays: RefCell<Option<crate::display_manager::Model>>', SHELL)

    def test_rollback_is_native_and_survives_window_close(self):
        self.assertIn('const CONFIRM_SECS: u64 = 15', MANAGER)
        self.assertIn('RecvTimeoutError::Timeout', MANAGER)
        self.assertIn('RecvTimeoutError::Disconnected', MANAGER)
        self.assertIn('revert(&before, &events)', MANAGER)
        self.assertIn('self.settings_displays.borrow_mut().take()', SHELL)
        self.assertIn('!result.iter().any(|out| out.enabled)', MANAGER)
        self.assertIn('mode_id(item).as_deref() == Some(mode)', MANAGER)
        self.assertIn('host["vrr_supported"] != true', MANAGER)
        self.assertIn('Some(UiCommand::DisplayConfirm(generation, id))', SHELL)
        self.assertIn('Some(UiCommand::DisplayRevert(generation, id))', SHELL)

    def test_both_settings_modes_render_the_real_display_editor(self):
        self.assertIn("section === 'displays'", SETTINGS)
        self.assertIn('<DisplaySettings {generation} />', SETTINGS)
        self.assertIn('onDisplayStatus(generation, receive)', DISPLAY)
        self.assertIn('aria-label="Схема расположения мониторов"', DISPLAY)
        self.assertIn('onpointerdown', DISPLAY)
        self.assertIn('disabled={busy || isPending || (selectedDraft.enabled && activeCount === 1)}', DISPLAY)
        self.assertIn('onchange={event => change({ mode:', DISPLAY)
        self.assertIn('onchange={event => change({ scale:', DISPLAY)
        self.assertIn('onchange={event => change({ transform:', DISPLAY)
        self.assertIn('applyDisplays(generation, draft)', DISPLAY)
        self.assertIn('confirmDisplays(generation, pendingId)', DISPLAY)
        self.assertIn('revertDisplays(generation, pendingId)', DISPLAY)
        self.assertIn("display-apply:", SDK)


if __name__ == "__main__":
    unittest.main()
