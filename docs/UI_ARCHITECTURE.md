# UI architecture: Rust host, Svelte 5 widgets

The architecture contract in [SPECIFICATION.md](SPECIFICATION.md) is explicit:
**all built-in and third-party widgets are authored in Svelte 5**. GTK4,
gtk4-layer-shell and WebKitGTK provide native Wayland surfaces and a trusted
bridge, not a second UI component framework.

## Ownership

| Layer | Responsibility | Must not own |
| --- | --- | --- |
| Kira Core (Rust) | Validated configuration, Niri integration, daemon IPC | Rendering widgets |
| Kira Shell (Rust/GTK/WebKit) | Layer-shell windows, focus, lifecycle, permissions; GIO and system-data adapters | Calendars, search lists, layouts or process tables built from GTK widgets |
| Svelte 5 / Tailwind | Top Bar, launcher, calendar, system monitor, processes, future user widgets | Raw shell commands, unvalidated Niri/OS privileges |
| Typed WebKit bridge | Minimal allowlisted native calls; read-only events | Generic command execution or navigation to outside content |

The top bar's 42 px exclusive zone **cannot contain popovers**. The
Calendar and System Monitor share **one** non-exclusive GTK layer-shell
window and its prewarmed WebKit view, which mounts the selected Svelte
component from the same self-contained frontend bundle. The browser cannot create an ordinary
top-level window with the required layer-shell Wayland behavior by itself.

## Migration, in small reviewable steps

1. **Calendar Svelte implementation and host integration (this PR).**
   The active Calendar is `CalendarWidget.svelte`, mounted inside an
   independent non-exclusive GTK layer-shell window with WebKit. The
   same self-contained HTML bundle is marked with
   `data-kira-surface="calendar"` before `load_html()`; only the
   fixed-purpose `kiraCalendarUi` handler (close, validated ISO date copy)
   is registered there. The Top Bar retains its original 42px reservation.
   The GTK date-grid view and its native CSS have been removed.
2. **Calendar graphic acceptance.** Frontend, GTK/Nix, and security contract
   tests run in CI. The actual separate Wayland window still needs a
   manual nested-Niri graphical smoke-test at multiple scales before release.
3. **Launcher.** Move search, rows, icons, selection, and keyboard handling
   to Svelte. Keep GIO XDG application discovery and launch in Rust behind
   a minimal typed, allowlisted IPC contract. Never accept a shell command
   or application executable path from untrusted UI input.
4. **System Monitor / processes (implemented).** The existing warm WebKit
   overlay now mounts `SystemWidget.svelte` or `CalendarWidget.svelte` on
   demand, never both. Rust keeps read-only kernel metrics, a bounded
   /proc sampler and clipboard report generation. Processes are sampled
   only while their tab is active; Svelte owns the search/sort/table.
5. Remove GTK-only presentation assets, duplicate native color tokens and
   unused view models *only after* the Svelte replacements are functioning.
   Update docs and enforce widget ownership in regression tests.

## Acceptance before releasing the migrated calendar

- NixOS CI: native Shell, frontend type-check, model tests and full package.
- Nested Niri: popup stays inside 360x640 and 600x480 logical displays,
  remains dismissible, and does not enlarge the 42 px Top Bar.
- Keyboard: focus visible on selected Orange day, Tab, Enter, Escape,
  navigation and copy. Pointer: day selection, adjacent months, Today.
- UI events can only reach explicitly allowlisted Rust host operations.
- No duplicate native GTK calendar stays in the shipping Shell.

The calendar is connected in the Shell now. A green CI validates compilation
and contracts, **not** final visual behavior under a real Wayland compositor.
Nested Niri graphical QA remains mandatory before release.

## Lazy widget lifetime and memory ownership

The Calendar layer-shell popup prewarms its **WebKit host** when the bar starts,
but does not mount a Svelte widget until the user opens it. The trusted GTK
host dispatches `kira:calendar:show` with a generation ID, then waits for
`mounted:<generation>` and a double animation frame before revealing the
window. On hide it dispatches `kira:calendar:hide`; the Svelte lifecycle
serially awaits `unmount()`, releasing Calendar DOM, effects, window listeners
and its one-second clock interval. Stale asynchronous mounts/frames are ignored.

The widget is **lazy-instantiated**; its component code is already present
in the single offline HTML bundle, but the DOM, effects, timers and component
instance are created only on show. The live instance is unloaded on hide.
This does **not** unload the prebundled JS bytes or the base WebKit engine. WebKit stays warm for near-instant reopening. RSS/PSS may not
immediately fall because the JavaScript engine retains allocated pages and
shared libraries. Calendar and System Monitor share one WebView;
Launcher is still native GTK pending its Svelte migration.
