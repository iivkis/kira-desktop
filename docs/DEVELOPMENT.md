# Developing Kira Desktop

## Requirements

Kira Desktop targets NixOS, Niri and Wayland. Flakes must be enabled. The repository commits the generated `Cargo.lock`, `flake.lock` and `frontend/package-lock.json`, as well as the verified frontend `npmDepsHash`.

The GitHub Actions workflows are split into fast, non-graphical checks (`.github/workflows/ci.yml`) and native Nix integration (`.github/workflows/nix.yml`).

## Kira Default visual design

The premium visual baseline is implemented using semantic CSS tokens in
`frontend/src/lib/themes/kira-default.css` and typed color metadata in
`frontend/src/lib/themes/kira-default.ts`. `WorkspacePreview.svelte` and
`CoreStatus.svelte` demonstrate theme-aware components without hardcoded HEX
values. The workspace widget now receives Niri events and supports explicit mouse clicks to focus a known workspace.

See [DESIGN_SYSTEM.md](DESIGN_SYSTEM.md) and specification §15 for accessibility,
spacing, contrast and future widget conventions. The small HTML fallback in
`frontend/index.html` uses the exact canonical colors because it runs before
CSS assets finish loading.

## Safe nested development preview

```sh
nix develop --command task dev:preview
```

The default preview builds with Cargo in `target/debug` and Vite in
`frontend/dist`, reusing compiled Rust dependencies. A full Nix package build
is reserved for `task dev:preview:release`; `task dev:preview:run` launches
already-built binaries without compiling anything.

The preview validates an isolated minimal Niri KDL config, launches Niri
**windowed without `--session`**, then receives the actual nested
`WAYLAND_DISPLAY` from a `spawn-at-startup` handshake. Only after verifying
the nested display differs from the parent does it start Core and Shell.

`KIRA_CORE_SOCKET` points to a private temporary directory, so an existing
Kira Core process cannot be touched. `KIRA_PREVIEW=1` enables the GTK
`NON_UNIQUE` application flag to avoid activating a primary-session Shell.

Closing the nested window or pressing Ctrl+C stops only processes owned by the
preview. Config and logs are temporary; recent logs are printed on unexpected
exit. Tests use mock executables, not a GPU:

```sh
task dev:preview:test
```

See [PREVIEW.md](PREVIEW.md) for additional safety checks and troubleshooting.

## Native Nix package

The following build has passed on Linux in GitHub Actions with the pinned Nix inputs:

```sh
nix build .#default
```

The package installs `kira-core`, `kira-shell` and the compiled UI assets; the GTK wrapper sets `KIRA_UI_DIR` to the installed frontend directory.

To manually smoke-test from an existing Niri session:

```sh
nix build
./result/bin/kira-core  # terminal 1
./result/bin/kira-shell # terminal 2
```

The native Nix package build is tested in CI; verify appearance and behavior manually under Niri. The user confirmed that the nested packaged QA smoke passed locally (Core bootstrap, WebKit/Svelte DOM, clean shutdown), but visual screenshot review remains pending.

## Build caching and release iteration

**Why the previous release build was slow:** the old
`rustPlatform.buildRustPackage` compiled third-party Rust bindings (GTK4,
JavaScriptCore/WebKitGTK, Soup3) and Kira's own Rust code in one derivation.
A change to a Rust file could invalidate that whole build and trigger the
expensive compilation again. The runtime GTK/WebKit C libraries themselves
are normally prebuilt Nix store inputs; the `Compiling gtk4 v...` log is the
**Rust crate**.

The packaged build is now split with pinned **Crane** into:

1. `cargoArtifacts`: compiles third-party Rust crates from
   `Cargo.toml`/`Cargo.lock` using a dummy workspace. Editing Kira's
   `.rs` files or Svelte files does **not** invalidate this output.
2. `rustBinaries`: builds `kira-core` and `kira-shell` from
   `crates/` against the cached artifacts. It is not influenced by
   Svelte-only changes.
3. `frontend`: independent fixed-hash npm dependency fetch and Vite bundle.
4. Final `kira-desktop`: inexpensive output assembly and GTK wrapper
   pointing `KIRA_UI_DIR` at bundled UI.

The `nix develop` shell also provides **sccache** and automatically sets
`RUSTC_WRAPPER` for Cargo. Compiler artifacts persist under
`${XDG_CACHE_HOME:-$HOME/.cache}/kira-desktop/sccache` (default 10 GiB
limit) and can be reused across target directories and branches when
compiler version and flags match. Existing `target/debug` artifacts are still
used by Cargo itself. To inspect actual compiler-cache hits:

```sh
nix develop --command sccache --show-stats
```

`sccache` does not make unrelated build profiles interchangeable, and cannot
reuse artifacts if Rust or native dependency versions change. The release Nix
sandbox intentionally does not use this mutable local cache.

To run a fast, **incremental local** smoke without Nix release compilation:

```sh
nix develop --command task qa:visual:fast
```

For true release QA:

```sh
nix develop --command task qa:visual:release
```

The first Nix build after this packaging change must compile the new
dependency derivation **once**, even if old `buildRustPackage` outputs are
already in the Nix store. Subsequent changes to Kira's Rust sources should
reuse it if `Cargo.lock`, manifests, native library inputs, toolchain and
Crane lock remain the same. A Nix garbage collection removing the derivation
can force a rebuild, so avoid `nix store gc` while actively iterating.

Use `nix build .#default -L` to inspect what compiles. To measure
derivation reuse, inspect:

```sh
nix path-info --derivation .#default
nix build .#default --dry-run
```

Nix stores build outputs by hashes; Cargo's local `target/debug` and
Nix's `/nix/store` are **separate caches**, and are not automatically
interchangeable. An exact release derivation should be a no-op on subsequent
runs; a changed dependency set or Nix toolchain legitimately requires a
fresh dependency compilation.

### Optional shared CI / multi-machine cache

Nix's local store persists on the same machine, but GitHub-hosted runners
are ephemeral. To share compiled dependency derivations between CI runs,
an optional **Cachix** cache can be enabled by configuring:

- Repository variable `KIRA_CACHIX_NAME`: existing Cachix cache name.
- Repository secret `CACHIX_AUTH_TOKEN`: write token for that cache.
- On a trusted local NixOS machine, configure the same cache as an
  explicitly trusted binary substituter using Cachix's published instructions.

The GitHub workflow only enables this when the variable is configured.
Forked pull requests do not upload packages. Do not paste cache tokens
into source files or commit `nix.conf` changes with private keys.
Without a shared cache, dependency reuse remains local to each Nix store.

## Developer tasks

```sh
nix develop
npm ci --prefix frontend
task check           # formatting, Clippy, IPC tests, TypeScript, Vitest
task shell:check     # compile the native GTK/WebKit Shell
task dev:preview     # fast Cargo + Vite nested preview
task dev:preview:run # reopen already built preview, no compilation
task dev:preview:release # full Nix release preview
task qa:visual:fast    # passive Niri/Core/Svelte smoke (no screenshots)
task qa:visual:release # passive packaged Nix smoke (no screenshots)
task qa:visual:capture:fast    # explicit four-scale PNGs via grim, no clipboard
task qa:visual:capture:release # explicit packaged four-scale PNG capture
task build           # frontend + all release binaries
task core:run        # launch the Core daemon
task shell:run       # rebuild frontend and launch the Shell
task nix:check       # nix flake check
task nix:build       # nix build
```

Use separate terminals for the two running processes. `XDG_RUNTIME_DIR` must point to the active user session runtime directory. The Shell returns an explicit error when its UI resources are missing. `KIRA_UI_DIR` can override the UI directory for development or packaging.

## Review hardening

The Shell WebView only allows navigation to its exact packaged `index.html` URI
and ignores attempts to open a new window. The Rust bridge allowlists read-only queries plus one narrowly scoped
`focus_workspace` action by an already known Niri workspace ID. This is defense-in-depth for trusted UI code,
not a security boundary for untrusted extensions.

Core now bounds individual JSON-RPC messages to 4096 bytes and removes the
Unix socket it owns after SIGINT/SIGTERM, checking inode identity first. The
preview traps SIGTERM and terminates process groups even when their leaders
have already exited, reducing the chance of orphaned processes.

## Core ⇄ Shell ⇄ Svelte bridge

Svelte uses the typed SDK at `frontend/src/lib/sdk/core.ts`, sending JSON-RPC requests to `window.webkit.messageHandlers.kira.postMessage`. The Rust Host forwards read-only `ping`, `get_state`, `get_workspaces`, `get_config`, plus a single validated `focus_workspace` request over Unix Domain Socket on a worker thread. Its GTK main loop returns replies as `kira:rpc` events.

The SDK correlates numeric request IDs, applies timeouts, and reports disconnected Core as an error. Browser-only Vite preview has no WebKit bridge and displays `Core: unavailable` rather than a fake connection.

```json
{"jsonrpc":"2.0","id":1,"method":"ping"}
```

```json
{"jsonrpc":"2.0","id":1,"result":"pong"}
```

The IPC transport uses newline-delimited JSON. The bootstrap `get_state` response contains version and status only. Niri workspaces now update through Core's Niri EventStream and a persistent read-only Core → Shell subscription, forwarded as typed `kira:workspaces` WebKit DOM events to Svelte. The Core socket is intended for the trusted local user session only.

## Read-only Niri workspaces (Stage 1 first increment)

Kira Core connects to the exact absolute `NIRI_SOCKET` supplied by its
environment. The nested preview ensures the value points to its **child**
compositor, never the parent. With no Niri socket, Core still serves
`ping` and `get_state`, while `get_workspaces` returns
`{"connected":false,"workspaces":[]}`, **not** hardcoded workspace 1–3.

Core primarily reads the Niri `"EventStream"` and its full initial
`WorkspacesChanged` snapshot plus subsequent `WorkspaceActivated` and
`WorkspaceUrgencyChanged` events. The **only** available action is `FocusWorkspace` by stable workspace ID,
requested explicitly through a mouse click. No move/window/output/configuration
or arbitrary Action request is accepted. On disconnect or malformed events,
Core clears stale state and reconnects with a one-second delay.

The public Core contract is:

```json
{"jsonrpc":"2.0","id":2,"method":"get_workspaces"}
```

Example (illustrative values; not fabricated in the actual UI):

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "result": {
    "connected": true,
    "workspaces": [
      {
        "id": "42",
        "idx": 1,
        "name": null,
        "output": "DP-1",
        "is_active": true,
        "is_focused": true,
        "is_urgent": false
      }
    ]
  }
}
```

`id` is a stable Niri `u64` serialized to a **string** so JavaScript
never rounds it. `idx` is the workspace position **per output**, not a
global identifier. `is_active` and `is_focused` are distinct: on multiple
outputs each can have an active workspace but only one is globally focused.
Unknown Niri event variants are ignored; message size is bounded.

The Shell allows the read-only `get_workspaces` RPC plus the exact
`focus_workspace` method, blocking all other write-like operations. The Svelte Top Bar renders real workspace names/indices,
focus/activation/urgency, or an explicit `NIRI OFFLINE` state; it no longer
uses the fake `[1, 2, 3]` visual demo. **Core listens to events**;
the frontend receives a pushed snapshot whenever Core detects a change.
Core-to-UI workspace subscriptions are now implemented; updates use a persistent read-only Core socket and WebKit DOM events.

### Click to focus a workspace

The top bar workspace chips are now accessible buttons. Left-click or
keyboard-activate a nonfocused workspace to send the single
`focus_workspace` JSON-RPC request:
`{"jsonrpc":"2.0","id":5,"method":"focus_workspace","params":{"id":"42"}}`.

**IDs must be decimal strings** (u64), not indexes, so changing output
order or multiple outputs with workspace index 1 do not target the
wrong workspace. Core verifies that Niri is connected, that the
workspace ID exists in the current snapshot, and that the parameters
contain **only** the `id`. It then sends the single Niri IPC
`Action.FocusWorkspace.reference.Id` request over the exact
`NIRI_SOCKET` already supplied to Core. The WebKit bridge explicitly
allowlists only this action, not generic Niri requests or shell commands.

The widget does not invent a new focused state after a click; it waits
for Niri's EventStream. Disabled/offline/current chips do not send
actions, and a rejected action shows `WS !` with a diagnostic tooltip.
Keyboard shortcuts remain independent and work as before. The
isolated preview continues pointing Core to the child Niri socket,
so this action cannot target the parent compositor in preview.

### Immediate Core → Shell → Svelte workspace updates

The shell opens exactly one persistent Core IPC subscription with the internal
`subscribe_workspaces` request (newline-delimited JSON-RPC, id `0`). Core
sends an initial typed `WorkspaceState` snapshot, then another frame each
time Niri changes its workspace state. The GTK Shell reads the socket on a
worker thread and dispatches a `kira:workspaces` DOM CustomEvent on its
main thread. Svelte subscribes on mount, unsubscribes on cleanup, and makes
one initial `get_workspaces` request only to cover any startup race.

**There is no 1.5-second Svelte polling timer anymore.** Core reconnects
to Niri after a disconnect; Shell reconnects to Core and pushes
`connected:false` while unavailable so the UI does not retain stale state.
`subscribe_workspaces` is an internal Core socket operation and is *not*
exposed to WebKit's JS request allowlist. Kira sends only Niri `FocusWorkspace` for a user-clicked stable workspace ID; no other Niri actions are supported.

This removes the artificial 1–2 second lag. Actual latency and keyboard
focus transitions still require an interactive NixOS/Wayland check.

### Smoke-test nested Niri workspace state

```sh
nix develop --command task qa:visual:fast
nix develop --command task qa:visual:release
```

Both smoke modes now wait for the Niri event stream's initial workspace
snapshot **through Core's actual `get_workspaces` RPC**. They do not resize
the screen, capture screenshots or modify the host compositor.

For a manual interaction check:

```sh
nix develop --command task dev:preview
```

The generated **nested-only** Niri KDL uses `Mod`, which resolves to
**Alt** in a windowed Niri. This avoids the four-modifier
`Alt+Shift+Ctrl+2` shortcut that could conflict with a host layout switch.

- Focus the **nested Niri** and press **Alt+T** to launch `foot`
  inside it (`foot` is included in `nix develop`).
- Press **Alt+2** to focus workspace 2 and **Alt+T** to open another
  terminal there; **Alt+1/2/3** switches workspace.
- For moving the **focused window**, press **Alt+Ctrl+2** (or **Alt+Ctrl+1** to move it back).
  The bindings use `move-window-to-workspace focus=false N`: the window
  moves, but the focus stays on the original workspace. Switch manually
  with **Alt+1/2** to see the moved window and verify Kira's focus.
- **Alt+Shift+Q** exits nested Niri and cleans up the preview.

Niri dynamically creates and removes empty workspaces; leave a
terminal on each if comparing two persistent workspaces. If the
shortcut is not handled, inspect `echo "$NIRI_SOCKET"` followed by
`niri msg --json workspaces` and `niri msg --json windows` **inside
the nested terminal**. Do not run these commands if that socket is
missing. Compare actual `is_focused` and `is_active` with the Top Bar.

Bindings exist **only** in the throwaway preview config. They do not
change the host Niri or any user config. They do not run when calling
read-only QA smoke, and they do not add workspace actions to Kira Core.
If Niri IPC is unavailable the bar should say `NIRI OFFLINE`.

Official protocol references:
- https://github.com/niri-wm/niri/wiki/IPC
- https://docs.rs/niri-ipc/latest/niri_ipc/enum.Event.html
- https://docs.rs/niri-ipc/latest/niri_ipc/struct.Workspace.html

## Validated KDL settings and hot reload (Stage 2)

Kira Core now accepts `KIRA_CONFIG_FILE=/absolute/path/file.kdl` or
defaults to `$XDG_CONFIG_HOME/kira-desktop/config.kdl`
(`~/.config/kira-desktop/config.kdl` without XDG_CONFIG_HOME).

This release deliberately implements a small **strict KDL profile**,
not every possible KDL expression. Configurable UI properties currently
include compact bar layout, center title, clock seconds, workspace names
and max visible chips. The file is limited to 64 KiB / 128 lines, and
unknown keys or invalid types fail without changing the active config.
`kira-core --check-config /absolute/path` validates offline without
starting Core, Niri or a WebKit window.

Internally Core watches the file every 250ms, publishes the last valid
`ConfigState` (including optional diagnostic) over `get_config` and the
private `subscribe_config` stream. Shell forwards read-only
`kira:config` DOM events. An invalid edit keeps the last good layout,
but shows a `CONFIG !` warning with diagnostics.

The **nested preview** always creates its own temporary editable KDL,
shows its path in the terminal and does not overwrite the user's config.
To test a persistent custom file opt in with absolute `KIRA_CONFIG_FILE`.

See [CONFIG.md](CONFIG.md) for exact supported syntax, safety model,
examples and preview steps. Niri KDL remains a separate compositor
configuration and is not modified by this feature.

## Application Launcher, first native GTK slice

The KIRA brand button now opens an independent GTK4 layer-shell popup
with zero reserved space and keyboard input only while opened. GIO
enumerates installed XDG desktop entries, filters hidden entries and
launches a selected `AppInfo` directly. The WebKit bridge can only
toggle this popup via parameterless `toggle_launcher`; it **cannot**
provide executable commands or desktop entry files.

This is a first usable native GTK increment. The planned dedicated
Svelte UI and Core-owned application catalog are tracked in #15 and
are not yet implemented. For exact manual QA instructions and safety
constraints see [LAUNCHER.md](LAUNCHER.md).

## Updating dependencies

The initial lockfiles were generated by `.github/workflows/lockfiles.yml`. Update them deliberately using the appropriate tooling and commit changes together with any required Nix hash update. The separate Nix integration workflow must pass after dependency changes.

## Next milestones

1. Visual QA for Kira Default across scales and packaged Shell smoke test. The user confirmed prior capture runs, but the PNGs still require visual review. The default QA command is now passive; explicit screenshot capture uses grim without clipboard side effects. See [QA_VISUAL.md](QA_VISUAL.md).
2. Read-only Niri workspace state from Core EventStream is implemented in the Stage 1 draft PR. Core-to-Svelte typed workspace pushes are implemented.
3. Initial strict KDL configuration profile with type validation, Core hot reload and last-good rollback is implemented in draft PR #14; full KDL grammar and custom widget layouts remain future increments.
4. Real workspace, launcher and dock widgets.
5. System-service integrations and user widget loading.

Do not claim implementation or isolation of untrusted plugins until the code and tests exist.
