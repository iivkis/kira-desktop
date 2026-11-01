"""Svelte Launcher: trusted GIO lookup, shared WebKit, no native GTK view."""
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "crates/kira-shell/src"
RUST = (SOURCE / "launcher.rs").read_text()
OVERLAY = (SOURCE / "calendar.rs").read_text()
HOST = (SOURCE / "main.rs").read_text()
ENTRY = (ROOT / "frontend/src/main.ts").read_text()
WIDGET = (ROOT / "frontend/src/lib/components/LauncherWidget.svelte").read_text()
MODEL = (ROOT / "frontend/src/lib/launcher-model.ts").read_text()
SDK = (ROOT / "frontend/src/lib/sdk/core.ts").read_text()


class LauncherDesignTests(unittest.TestCase):
    def test_svelte_owns_presentation_and_keyboard_navigation(self):
        for value in (
            'aria-label="Kira Launcher"', 'data-kira-ui-root',
            'KIRA / APPLICATION INDEX', 'Search applications…',
            'APPLICATION CATALOG', 'role="listbox"',
            'aria-selected={index === activeIndex}',
            "event.key === 'ArrowDown'", "event.key === 'ArrowUp'",
            "event.key === 'Escape'", "event.key === 'Enter'",
            'search?.focus()', 'onclick={() => choose(index)}',
            'No matching applications', 'KIRA',
        ):
            if value == 'KIRA':
                continue
            self.assertIn(value, WIDGET)
        self.assertIn('filterLauncherApps(apps, query)', WIDGET)
        self.assertIn('nextLauncherSelection(', WIDGET)
        self.assertIn('matches.slice(0, 80)', WIDGET)
        self.assertIn('app.id', WIDGET)
        self.assertIn('toLocaleLowerCase()', MODEL)
        self.assertNotIn('gtk::ListBox', RUST)
        self.assertNotIn('gtk::SearchEntry', RUST)
        self.assertNotIn('gtk::ApplicationWindow', RUST)
        self.assertNotIn('gtk::CssProvider', RUST)

    def test_native_app_identity_and_safe_launch(self):
        for required in (
            'gio::AppInfo::all()', 'info.should_show()',
            'pub fn launch(&self, id: usize)', 'self.apps.get(id)',
            'info.launch(&[], None::<&gio::AppLaunchContext>)',
            'apps.truncate(1024)', '"id": id',
        ):
            self.assertIn(required, RUST)
        self.assertNotIn('std::process::Command', RUST)
        self.assertNotIn('Command::new', RUST)
        self.assertNotIn('shell', RUST.lower().split('pub fn launch', 1)[-1])
        self.assertIn('self.launcher.borrow_mut().take()', OVERLAY)
        self.assertIn('Some(UiCommand::LauncherCatalog(generation))', OVERLAY)
        self.assertIn('Some(UiCommand::LaunchApp(generation, id))', OVERLAY)
        self.assertIn('self.generation.get() == generation', OVERLAY)
        self.assertIn('self.revealed.get()', OVERLAY)
        self.assertIn('calendar::open_launcher(', HOST)
        self.assertIn('register_script_message_handler("kiraLauncher", None)', HOST)
        self.assertIn('postNativeCommand(\'kiraLauncher\', \'open\')', SDK)
        self.assertIn("sendCalendarUi('launch:' + generation + ':' + id)", SDK)
        self.assertIn("sendCalendarUi('apps-request:' + generation)", SDK)

    def test_one_shared_overlay_and_retired_gtk_assets(self):
        self.assertIn('mount(LauncherWidget', ENTRY)
        self.assertIn('mount(CalendarWidget', ENTRY)
        self.assertIn('mount(SystemWidget', ENTRY)
        self.assertIn('kind === \'launcher\'', ENTRY)
        self.assertIn('Self::Launcher => "launcher"', OVERLAY)
        self.assertIn('KeyboardMode::Exclusive', OVERLAY)
        self.assertIn('set_exclusive_zone(0)', OVERLAY)
        self.assertIn('native_ui::configure_webview_popup', OVERLAY)
        self.assertFalse((SOURCE / "launcher.css").exists())
        self.assertFalse((SOURCE / "native-palette.css").exists())
        self.assertIn('data-kira-ui-root', WIDGET)
        self.assertNotIn('launcher::show(', HOST)
        self.assertNotIn('crate::launcher::dismiss(', OVERLAY)


if __name__ == "__main__":
    unittest.main()
