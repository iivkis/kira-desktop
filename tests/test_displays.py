"""Display settings implementation contract: no fake data or blind configuration."""
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MANAGER = (ROOT / "crates/kira-shell/src/display_manager.rs").read_text()
SHELL = (ROOT / "crates/kira-shell/src/calendar.rs").read_text()
SETTINGS = (ROOT / "frontend/src/lib/components/SettingsWidget.svelte").read_text()
DISPLAY = (ROOT / "frontend/src/lib/components/DisplaySettings.svelte").read_text()
SDK = (ROOT / "frontend/src/lib/sdk/core.ts").read_text()
CONFIG = (ROOT / "crates/kira-shell/src/config_layers.rs").read_text()


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

    def test_watchdog_remains_bounded_when_niri_ipc_stalls(self):
        self.assertIn('const NIRI_IPC_TIMEOUT: Duration = Duration::from_secs(2)', MANAGER)
        self.assertIn('child.try_wait()', MANAGER)
        self.assertIn('child.kill()', MANAGER)
        self.assertIn('read_capped(stdout, NIRI_MAX_OUTPUT_BYTES)', MANAGER)
        self.assertIn('read_capped(stderr, NIRI_MAX_ERROR_BYTES)', MANAGER)
        self.assertIn('deadline_expired(*deadline, Instant::now())', MANAGER)
        self.assertIn('fn a_queued_confirmation_does_not_extend_the_rollback_deadline()', MANAGER)

    def test_session_and_declarative_settings_are_separate_transactions(self):
        self.assertIn('Request::Save => {', MANAGER)
        self.assertIn('crate::config_layers::save(target)', MANAGER)
        self.assertIn('confirmed = staged.take()', MANAGER)
        self.assertIn('actual.len() != target.len()', MANAGER)
        self.assertIn('pub fn save(desired: &[Desired]) -> Result<(), String>', CONFIG)
        self.assertIn('crate::config_layers::status()', MANAGER)
        self.assertIn('fn inspect(', CONFIG)
        self.assertIn('MAX_SOURCE', CONFIG)
        self.assertIn('if count > 1', CONFIG)
        self.assertIn('atomic(&fragment, &new_fragment)', CONFIG)
        self.assertIn('validate_config(&config)', CONFIG)
        self.assertIn('rollback_file(&fragment', CONFIG)
        self.assertIn('fs::symlink_metadata(&config)', CONFIG)
        self.assertIn('pub fn status()', CONFIG)
        self.assertIn('sendCalendarUi(\'display-save:\' + generation)', SDK)
        self.assertIn('Some(UiCommand::DisplaySave(generation))', SHELL)
        self.assertIn('Сохранить в конфиг', DISPLAY)

    def test_display_editor_keyboard_focus_and_responsive_preview(self):
        self.assertIn("let mapViewport = $state<HTMLDivElement | null>(null)", DISPLAY)
        self.assertIn("observer.observe(mapViewport)", DISPLAY)
        self.assertIn("Math.max(1, mapWidth - 28)", DISPLAY)
        self.assertIn("onclick={() => selected = rect.connector}", DISPLAY)
        self.assertIn("focus-visible:outline-kira-accent", DISPLAY)
        self.assertIn('aria-describedby="kira-display-confirmation-warning"', DISPLAY)
        self.assertIn("confirmButton?.focus()", DISPLAY)
        self.assertIn("if (pendingId === id && !busy)", DISPLAY)

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
