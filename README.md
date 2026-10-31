# Kira Desktop

**Status:** v0.1 Draft, working foundation, **not yet an MVP desktop environment**.

Kira Desktop is a modular, declarative, NixOS-first shell for Niri/Wayland. Rust owns system capabilities, Svelte 5 owns the UI and KDL will define layouts.

- [Technical specification (Russian)](docs/SPECIFICATION.md)
- [Development guide](docs/DEVELOPMENT.md)
- [Kira Default design system](docs/DESIGN_SYSTEM.md)
- [CI checks](.github/workflows/ci.yml)
- [Native Nix build checks](.github/workflows/nix.yml)
- [Visual QA / packaged Shell smoke-test guide](docs/QA_VISUAL.md)

## Implemented foundation

| Component | Current capabilities |
| --- | --- |
| `crates/kira-ipc` | JSON-RPC 2.0 contracts, bounded Unix socket client, unit tests |
| `crates/kira-core` | Tokio daemon exposing `ping` and `get_state` over a configurable user-owned Unix socket |
| `crates/kira-shell` | GTK4/WebKitGTK 6 layer-shell host with a WebKit-to-Rust-to-Core bridge |
| `frontend/` | Svelte 5, TypeScript, Vite, Kira Default premium tokens, themed top bar, typed SDK, Core connection status, Vitest |
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


## Visual QA before Niri IPC integration

Run the isolated, packaged Shell smoke test on a graphical NixOS/Wayland
session to capture 100%, 125%, 150% and 200% scale screenshots:

```sh
nix develop --command task qa:visual:release
```

The command verifies Core `get_state` and actual WebKit/Svelte mount,
records PNGs plus a machine-readable manifest under ignored
`artifacts/visual-qa/`, and cleans up preview-owned processes. **The
screenshots require human review:** automated capture is not a visual PASS.
See [visual QA guide](docs/QA_VISUAL.md) for narrow-window checks, acceptance
criteria and known environmental requirements.

## Current limitations

The panel can communicate with Core through a typed request/response API and display its connection status, but workspace indicators are mock UI: Niri IPC integration, KDL hot reload, event subscriptions, user-overridable themes, docks, and third-party widgets remain unimplemented.

This project is not ready to replace a working desktop environment.
