# Kira Desktop

**Status:** v0.1 Draft, working foundation, **not yet an MVP desktop environment**.

Kira Desktop is a modular, declarative, NixOS-first shell for Niri/Wayland. Rust owns system capabilities, Svelte 5 owns the UI and KDL will define layouts.

- [Technical specification (Russian)](docs/SPECIFICATION.md)
- [Development guide](docs/DEVELOPMENT.md)
- [Kira Default design system](docs/DESIGN_SYSTEM.md)
- [TENET Motion System · required animation standard](docs/TENET_MOTION.md)
- [CI checks](.github/workflows/ci.yml)
- [Native Nix build checks](.github/workflows/nix.yml)
- [Visual QA / packaged Shell smoke-test guide](docs/QA_VISUAL.md)

## Implemented foundation

| Component | Current capabilities |
| --- | --- |
| `crates/kira-ipc` | JSON-RPC 2.0 contracts, bounded Unix socket client, unit tests |
| `crates/kira-core` | Tokio daemon exposing `ping` and `get_state` over a configurable user-owned Unix socket |
| `crates/kira-shell` | GTK4/WebKitGTK 6 layer-shell host with a WebKit-to-Rust-to-Core bridge and a native Gio application Launcher |
| `frontend/` | Svelte 5, Tailwind CSS v4 via Vite, typed Kira Default design tokens, editorial top bar, native Launcher trigger, Core status and workspace SDK, Vitest |
| `Taskfile.yml` | Shared commands for development and CI |
| `flake.nix`, `nix/package.nix` | Nix development shell and installable packaged Core/Shell/frontend |
| `Cargo.lock`, `flake.lock`, `frontend/package-lock.json` | Locked dependencies for reproducibility |

## Safe nested preview on NixOS

The recommended development entry point runs a **separate, windowed Niri** with
its own Kira Core socket and isolated Niri config, without replacing your current
desktop session:

```sh
nix develop --command task dev:preview
```

The default preview uses `cargo build --locked` and a local Vite build. Cargo
reuses `target/debug` between runs, avoiding a full Nix release rebuild after
each Rust or Svelte edit. On first run, WebKitGTK Rust bindings must still
compile. For the packaged release build instead, use:

```sh
nix develop --command task dev:preview:release
```

To reopen a previously built development preview with **no build step**,
use `task dev:preview:run` inside `nix develop`.

Close the window with **Alt+Shift+Q** (inside nested Niri) or press **Ctrl+C** in
the launching terminal. This only stops preview-owned processes. No `nixos-rebuild
switch`, global `pkill`, systemd manipulation or modification of
`~/.config/niri` is involved.

See [safe preview guide (Russian)](docs/PREVIEW.md). The preview lifecycle is
tested with mock Niri/Core/Shell processes in CI, but the first visual smoke test
on your GPU and Niri session must still be performed locally.

## Quick start on NixOS

Ensure Niri and a working Wayland session are available, then:

```sh
git clone https://github.com/iivkis/kira-desktop.git
cd kira-desktop
nix build
```

Run these in **separate terminals** inside the Niri session:

```sh
./result/bin/kira-core
```

```sh
./result/bin/kira-shell
```

For development and tests:

```sh
nix develop
npm ci --prefix frontend
task check
task shell:check
task build
```

Nix CI on Linux has successfully compiled the native Shell, built the package and run the Rust tests. **The top bar was manually rendered successfully inside a nested Niri window**; a full desktop session and packaged-system installation still require additional testing.

## Compact source context for agents (RepoSpool)

[`.repospool`](.repospool) is a **generated**, deterministic
`REPOSPOOL/1.0` snapshot of committed UTF-8 sources. It provides a compact
file/line index, the original selected file bytes, and the pinned source
commit. It does not guess code semantics or replace GitHub source links.

[`.repospoolignore`](.repospoolignore) keeps the snapshot focused on Core,
IPC, Shell, Svelte, Nix, tests, CI and the design specification. Dependency
lockfiles, local build/cache outputs, the bootstrap lockfile workflow and
historical preview troubleshooting are omitted **only from the snapshot**,
not from the repository. Missing context can still be read directly from GitHub.

On each `main` source commit,
[`Publish RepoSpool context`](.github/workflows/publish-repospool.yml)
regenerates and verifies the snapshot using a pinned upstream RepoSpool
revision, then publishes it in a separate artifact-only commit. Accordingly,
the `C` source revision inside `.repospool` can differ from the current
`main` HEAD by that publication commit. Verify origin and revision before
using it to make repository changes.

To generate or verify locally with the upstream CLI:

```sh
nix run github:iivkis/repospool -- generate .
nix run github:iivkis/repospool -- verify --artifact .repospool --repo .
```


## Pull request checks

The CI workflow reports two stable jobs on pull requests: **Core + Svelte checks**
and **Native Shell check**. The second job invokes
`nix develop .#ci --command cargo check --locked -p kira-shell --all-targets`
when Rust Shell/IPC code, Cargo/Nix dependencies, or the CI configuration changes.
Unrelated UI/docs-only PRs skip the native dependency setup without leaving a
missing check status. The slim `.#ci` dev shell excludes the compositor and
visual-preview tools; Nix binary inputs come from its configured caches, and
GitHub Actions caches the Rust dependencies. The full Nix release package remains
on its separate nightly/manual workflow.

To **enforce** the native job on protected branches, require the status check
`Native Shell check` in the repository's branch protection/ruleset, alongside
`Core + Svelte checks`.

## IPC subscription liveness

Core pushes workspace and validated KDL config updates immediately when they change.
If nothing changes, it sends a read-only JSON-RPC heartbeat every **5 seconds**
on each subscription using the same request id and the distinct
`{"heartbeat":true}` result. Heartbeats do **not** repeat workspace/config
snapshots or generate browser events. Shell's blocking IPC workers use a
**15-second socket read timeout**: a connected but unresponsive Core causes
the existing reconnect loop to run, mark workspaces offline, and retry. The
last valid config is retained while reconnecting. The heartbeat is internal
to the Core/Shell protocol, not part of the WebKit JS allowlist.

## Incremental and release build caching

- **Local Rust development:** `nix develop` sets up persistent `sccache`
  (and Cargo still reuses `target/debug`), so recompilation can reuse compiler
  outputs between development runs.
- **Reproducible release:** `nix build .#default` uses Crane to cache compiled
  third-party Rust crates separately from application code and separately from
  the frontend. Changing Svelte should not rebuild Rust; changing a Rust
  source file should not rebuild GTK/WebKit Rust crates.
- **Fresh CI machines:** an optional shared Cachix binary cache can be enabled
  through repository settings. Without it, cached artifacts persist only
  locally in each machine's Nix store. See
  [build cache guide](docs/DEVELOPMENT.md#build-caching-and-release-iteration).

A new dependency-cache derivation must be compiled once on the first build;
changing `Cargo.lock`, manifests, toolchain, native libraries or relevant
Nix inputs may legitimately invalidate the cache.

## Default theme: Black, Prussian Blue and Orange

The built-in **Kira Default** dark theme uses five fully opaque colors:

`#000000ff` Black · `#14213dff` Prussian Blue · `#fca311ff` Orange ·
`#e5e5e5ff` Alabaster Grey · `#ffffffff` White.

These colors drive the Svelte Top Bar, pre-mount WebKit loading screen and
native GTK Launcher. Black is the background, Prussian Blue makes structured
surfaces, Orange marks focus and selection, Alabaster Grey is secondary text,
and White is primary text. Selected Orange surfaces use **Black** text for
contrast. Semantic tokens derive from the five canonical palette values;
custom KDL theme overrides remain a future feature. See the
[design system](docs/DESIGN_SYSTEM.md) for exact mappings and WCAG checks.

## Validated KDL settings (Stage 2 draft)

Kira Core can read `~/.config/kira-desktop/config.kdl` (or an absolute
`KIRA_CONFIG_FILE` override), validate a **small, documented KDL profile**
and hot-reload UI settings without restarting the compositor, Core or Shell.
Settings include compact Top Bar spacing, center title, clock seconds,
workspace names and maximum visible workspace chips. If an edit is invalid,
the previous valid config stays active and the Top Bar shows `CONFIG !`.

Copy the [sample configuration](config/kira.kdl.example), validate a file
with `kira-core --check-config /absolute/path/config.kdl`, and read the
[supported KDL syntax and safe preview guide](docs/CONFIG.md).

The current KDL profile deliberately **does not implement full KDL grammar**,
arbitrary commands, Niri actions or external modules. The independent Niri
preview generates a temporary editable Kira config for experimentation.

## Real Niri workspaces (Stage 1 draft)

The Top Bar obtains **real Niri workspace state** through
`NIRI_SOCKET` → Rust Core `EventStream` → typed `get_workspaces` JSON-RPC →
GTK WebKit bridge → Svelte. Workspace IDs are strings (safe for Niri's u64),
and focus, visibility and urgency are separate flags. Clicking a workspace
chip sends only the validated Niri `FocusWorkspace` action by stable ID;
keyboard shortcuts continue to work independently.

With no Niri IPC connection, the UI shows `NIRI OFFLINE` instead of fake
workspaces. The Core reconnects automatically after disconnection.
A persistent read-only Core→Shell subscription now pushes changed workspace
snapshots as typed WebKit `kira:workspaces` events, so the Svelte UI updates
without the old 1.5-second polling delay.

To test the actual Niri workspace integration without modifying the host:

```sh
nix develop --command task qa:visual:fast
```

The QA smoke now waits for a workspace snapshot from the child compositor
through Core, in addition to the existing WebKit/Svelte verification. This
does not capture the screen or copy to the clipboard. Read more in
[the development guide](docs/DEVELOPMENT.md#read-only-niri-workspaces-stage-1-first-increment).

## Visual QA before Niri IPC integration

Run **read-only** packaged Niri/Core/Svelte smoke on a graphical
NixOS/Wayland session (no screenshots, scale changes or clipboard writes):

```sh
nix develop --command task qa:visual:release
```

To **explicitly opt in** to capturing 100%, 125%, 150% and 200% scales:

```sh
nix develop --command task qa:visual:capture:release
```

Capture uses `grim` bound to the nested Wayland display, **not** the Niri
screenshot action (which also copies to clipboard). PNGs and manifests are
stored locally in ignored `artifacts/visual-qa/`. Automated capture and CI
are not substitutes for a real visual review. See
[visual QA guide](docs/QA_VISUAL.md) for scope and acceptance criteria.

## Current limitations

The panel communicates with Core through typed IPC, shows live Niri workspaces and supports a narrowly scoped mouse-focus action. **There is no standalone Dock**: Niri is keyboard-first, so window/workspace navigation stays with the compositor and the Top Bar. The existing KIRA button opens the full application Launcher. For up to three *explicit* GIO application launch shortcuts inside the same 42px Top Bar, set `KIRA_QUICK_APPS="foot,Firefox,Files"` before starting `kira-shell` (names must match the system's GIO display names; missing apps are skipped). If unset, no shortcut row or native worker exists. The older `KIRA_DOCK_APPS` variable is accepted as a migration alias; `KIRA_DOCK_DISABLE` is obsolete. This is not a taskbar: it does not switch to running windows or indicate focus. Full KDL grammar, custom theme loading, KDL shortcut configuration and third-party widgets remain future milestones.

This project is not ready to replace a working desktop environment.
