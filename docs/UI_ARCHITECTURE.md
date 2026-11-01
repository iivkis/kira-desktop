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

The top bar's 42 px exclusive zone **cannot contain popovers**. Each
overlay must be its own non-exclusive GTK layer-shell window containing a
WebKit view that mounts the appropriate Svelte component from the same
self-contained frontend bundle. The browser cannot create an ordinary
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
4. **System Monitor / processes.** Use existing read-only metrics events,
   expose bounded process snapshots via a typed interface, and render
   charts, tables, filters, and sorting in Svelte. Leave kernel sampling and
   sensitive process capabilities in Rust.
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
