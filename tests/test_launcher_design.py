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

    def test_square_launcher_and_lazily_resolved_native_icons(self):
        root_markup = WIDGET.split("<main data-kira-ui-root", 1)[1].split("<header", 1)[0]
        self.assertIn("overflow-hidden bg-kira-bg text-kira-text", root_markup)
        self.assertNotIn("rounded-lg", root_markup)
        self.assertNotIn("border-kira-muted", root_markup)
        self.assertIn("<img class=\"size-8 object-contain\"", WIDGET)
        self.assertIn("src={icons[app.id]}", WIDGET)
        self.assertIn("app.name.slice(0, 1)", WIDGET)
        self.assertIn("onLauncherIcons(batch", WIDGET)
        self.assertIn("requestLauncherIcons(generation, batch)", WIDGET)
        self.assertIn("needed.slice(i, i + 16)", WIDGET)
        self.assertIn("stopIcons();", WIDGET)
        self.assertIn("gtk::IconTheme::for_display(&display)", RUST)
        self.assertIn(".lookup_by_gicon(", RUST)
        self.assertIn("paintable.file()?", RUST)
        self.assertIn("file.load_bytes(None::<&gio::Cancellable>)", RUST)
        self.assertIn("const MAX_ICON_BYTES: usize = 128 * 1024;", RUST)
        self.assertIn("const ICON_BATCH_LIMIT: usize = 16;", RUST)
        self.assertIn("data:{mime};base64", RUST)
        self.assertIn("icons: RefCell<HashMap<usize, Option<String>>>", RUST)
        self.assertIn("self.launcher.borrow_mut().take()", OVERLAY)
        self.assertIn("kira:launcher:icons", OVERLAY)
        self.assertIn("Some(UiCommand::LauncherIcons(generation, ids))", OVERLAY)
        self.assertIn("icons-request:", OVERLAY)
        self.assertIn("self.generation.get() == generation", OVERLAY)
        self.assertIn("raw_ids.split(',')", OVERLAY)
        self.assertIn("ids.contains(&id)", OVERLAY)
        self.assertIn("iconDataUri.test(icon.data_url)", SDK)
        self.assertNotIn("file://", RUST)
        self.assertNotIn("std::process::Command", RUST)

    def test_scrollbar_does_not_cover_selected_row(self):
        style = (ROOT / "frontend/src/style.css").read_text()
        self.assertIn('class="kira-launcher-scroll min-h-0 flex-1 overflow-y-auto pr-1"', WIDGET)
        self.assertIn(".kira-launcher-scroll {", style)
        self.assertIn("scrollbar-width: none;", style)
        self.assertIn(".kira-launcher-scroll::-webkit-scrollbar", style)
        self.assertIn("display: none;", style)
        self.assertIn("overflow-y-auto", WIDGET)
        self.assertNotIn("gap-3 rounded-md py-2 pr-2", WIDGET)
        self.assertIn("focus-visible:-outline-offset-2", WIDGET)
        self.assertIn("size-9 shrink-0 items-center justify-center overflow-hidden", WIDGET)
        self.assertIn("bg-kira-surface-soft", WIDGET)

    def test_file_icon_and_generic_fallback_resolve_without_browser_paths(self):
        self.assertIn("icon.downcast_ref::<gio::FileIcon>()", RUST)
        self.assertIn("file_data_url(&file_icon.file())", RUST)
        self.assertIn('const NAME: &str = "application-x-executable";', RUST)
        self.assertIn("theme.has_icon(NAME)", RUST)
        self.assertIn("generic_icon_data_url", RUST)
        self.assertIn("or_else(|| generic.clone())", RUST)
        self.assertIn("const MAX_ICON_BYTES: usize = 128 * 1024;", RUST)
        self.assertIn("icons: RefCell<HashMap<usize, Option<String>>>", RUST)
        self.assertNotIn("file://", RUST)
        self.assertNotIn("std::process::Command", RUST)

    def test_native_app_identity_and_safe_launch(self):
        for required in (
            'gio::AppInfo::all()', 'info.should_show()',
            'pub fn launch(&self, id: usize)', 'self.apps.get(id)',
            'info.launch(&[], None::<&gio::AppLaunchContext>)',
            'apps.truncate(1024)', '"id": id',
        ):
            self.assertIn(required, RUST)
        # Regression: None::<T> is a Rust enum value, never a call.
        # The native Shell is not built in fast CI, so catch this
        # exact invalid syntax in the lightweight contract checks.
        self.assertNotRegex(RUST, r"None::<[^>\n]+>\s*\(")
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
        self.assertIn("detail?.kind !== 'launcher'", ENTRY)
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
