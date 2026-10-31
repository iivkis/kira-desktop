# Application Launcher (Stage 3, first GTK slice)

The Top Bar's **KIRA** brand is now a button. Clicking it toggles an
independent GTK4 layer-shell application launcher (no compositor config
changes, no expansion of Kira's 42px panel). This is an intentionally
**native GTK UI first increment**, not yet the planned Svelte surface or
a Core-owned XDG catalog.

## Using the launcher

1. In the disposable nested preview, click the **KIRA** logo in Top Bar.
2. The launcher opens at the upper-left of the nested Niri output. It
   requests keyboard focus while visible, not when Kira starts.
3. Start typing to search installed applications (case-insensitive).
   The list shows up to 64 matching, non-hidden desktop entries.
4. Use **Up / Down** to change selection and **Enter** to launch the
   selected entry, or click a row. Use **Escape** to dismiss.
5. Click the logo again to toggle the launcher closed.

There is **no auto-launch** during preview or `qa:visual:fast/release`.
The launcher only starts an application after an explicit click/Enter
in its GTK window. A launch error is written to the Shell log; richer
inline error UI is planned in a later increment.

### Verify on NixOS

```sh
git fetch origin feat/xdg-app-launcher
git switch --track origin/feat/xdg-app-launcher
nix develop --command task dev:preview
```

Search for `foot` if installed and explicitly launch it (only if desired).
It inherits the **nested** `WAYLAND_DISPLAY` from the Shell, so testing
does not modify Niri's host configuration. The GUI is a separate Overlay
layer surface with **zero exclusive zone**, so it does not reserve an
extra 470px at the top of the output.

### Security and desktop-entry semantics

- Kira does **not** parse or shell-evaluate `.desktop Exec` values.
  GIO supplies `AppInfo::all()`, `should_show()` and `launch()`, which
  handle installed desktop-entry details and launch behavior.
- Kira filters out non-display apps, missing desktop IDs, duplicates,
  extremely long IDs/names, and caps rendered rows.
- From the Svelte WebKit bridge, the only new request is
  `{"jsonrpc":"2.0","id":1,"method":"toggle_launcher"}`.
  It has no parameters. Requests for arbitrary execution, desktop IDs,
  executable paths or file inputs remain forbidden.
- The GTK Shell holds the actual GIO `AppInfo` objects and only uses
  objects taken from the discovered catalog for launch. It never
  forwards a user-supplied command to a shell.

## Next iterations

This first working slice intentionally uses GTK widgets. Issue #15
tracks a more extensible architecture: a typed read-only XDG catalog
in Core, dedicated Svelte launcher UI, icons, favorites, richer fuzzy
search and per-output positioning. Those improvements require a
separate implementation and tests rather than being claimed completed.

The five-color Kira Default palette is used by the native popup.
Manual visual QA at varying scales and screen widths remains in #13.
