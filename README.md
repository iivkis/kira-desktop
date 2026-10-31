# Kira Desktop

**Status:** v0.1 Draft, working foundation, **not yet an MVP desktop environment**.

Kira Desktop is a modular, declarative, NixOS-first shell for Niri/Wayland. Rust owns system capabilities, Svelte 5 owns the UI and KDL will define layouts.

- [Technical specification (Russian)](docs/SPECIFICATION.md)
- [Development guide](docs/DEVELOPMENT.md)
- [Kira Default design system](docs/DESIGN_SYSTEM.md)
- [CI checks](.github/workflows/ci.yml)
- [Native Nix build checks](.github/workflows/nix.yml)

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

## Current limitations

The panel can communicate with Core through a typed request/response API and display its connection status, but workspace indicators are mock UI: Niri IPC integration, KDL hot reload, event subscriptions, user-overridable themes, docks, and third-party widgets remain unimplemented.

This project is not ready to replace a working desktop environment.
