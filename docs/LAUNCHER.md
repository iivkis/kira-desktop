# Application Launcher (Issue #15)

The launcher is a **separate GTK layer-shell overlay** (520×420 logical px,
top-centered, with 52 px top margin). The existing bar stays 42 px tall and
keeps its 42 px exclusive zone. No Niri host configuration is changed.

## Security boundary

- The bar's WebKit UI may send only `toggle_launcher` (no parameters).
  The Shell opens or closes a native GTK window. WebKit cannot request a shell
  command, desktop file path, or `launch_application`.
- GTK requests `get_applications` from Core over the existing user-owned Unix
  socket. Core reads XDG user/system application directories and returns only
  typed `{id, name, icon}` records. Results are sorted, capped at 180 entries,
  and truncated to safe display lengths. Hidden/NoDisplay/failed TryExec entries
  shadow lower-priority duplicates.
- Clicking a row or pressing Enter on a selected row sends its **desktop ID**
  to Core. Core rescans the bounded catalog and resolves the ID to a currently
  installed entry. Only then does it invoke `gio launch <resolved-file>` via
  an argv-based process invocation, never a shell. GIO handles desktop-entry
  Exec quoting and field codes. Core does not accept arbitrary `Exec`,
  filesystem paths, or WebKit-provided commands.
- In packaged Nix builds, `KIRA_GIO_BIN` points to the GLib-provided `gio`
  binary. Standalone debug builds use `gio` from PATH. This trusted native
  environment variable is not exposed as a frontend parameter.
- Launcher UI input is only a case-insensitive filter. No search text is
  executed. The action is initiated solely by a user click or Enter.

## Keyboard and mouse

Select **APPS** in the bar or press **Ctrl+Space** while its WebKit view owns
keyboard focus. Type to search (including Unicode), move the selection with
Up/Down, launch the selected entry with Enter, and close with Escape or the
Close button. Focus returns to the bar. The popup reports empty catalogs,
no matches, Core failures, and activation failures.

The native popup intentionally uses the five Kira Default palette values.
It does not resize the top bar or alter any host compositor binding.

## Verification

Run the fast tests first:

```sh
task check
nix develop --command task shell:check
nix build .#default
```

For interactive testing, use only the existing **isolated nested Niri**
preview; do not replace or modify the running host's Niri configuration:

```sh
nix develop --command task dev:preview
```

Check the popup at 800, 1024, and 1280 logical pixels and multiple scale
factors. Verify pointer selection, Tab focus, Enter, Escape, an empty or
unavailable Core, Unicode searches, and focus return. Select and launch a
**harmless application only after explicit user confirmation**. Automated
checks must never activate a desktop entry or launch a process from a fixture.

The native GTK window still requires manual nested-Niri QA on the real
compositor and display environment; Rust/TS unit tests cannot certify
Wayland placement or keyboard grab behavior.
