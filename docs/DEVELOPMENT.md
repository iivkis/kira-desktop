# Developing Kira Desktop (bootstrap milestone)

## Prerequisites

Use NixOS with Niri, Wayland and Nix flakes enabled. Run `nix develop` from the repository root. The flake dev shell provides Rust, Node.js, Task, pkg-config, GTK4, gtk4-layer-shell and WebKitGTK 6 development libraries.

Until `flake.lock` is committed, the first `nix develop` will resolve `nixpkgs` and generate a lock file. Review and commit it.

## Local commands

1. `task frontend:install` — install frontend dependencies.
2. `task check` — Rust formatting and Core/IPC Clippy/tests, Svelte/TypeScript validation and Vitest.
3. `task shell:check` — separately check GTK4 Shell with native libraries from `nix develop`.
4. `task build` — Vite production assets and Rust release binaries.
5. `task core:run` — start the daemon on `$XDG_RUNTIME_DIR/kira-desktop.sock`.
6. `task shell:run` — compile frontend assets, start GTK4/WebKitGTK top bar in Niri.

`task check` is intentionally a fast non-graphical check, not evidence that GTK Shell builds. CI on Ubuntu 24.04 cannot use an upstream GTK4 Layer Shell development package, so an actual native Shell check is a separate task for NixOS.

Running `kira-shell` without `frontend/dist/index.html` produces a clear error and exits rather than showing a blank WebView. `KIRA_UI_DIR` can override the directory containing `index.html`.

## Current IPC contract

One JSON-RPC 2.0 request per line, one response per line over a Unix Domain Socket.

```json
{"jsonrpc":"2.0","id":1,"method":"ping"}
```

```json
{"jsonrpc":"2.0","id":1,"result":"pong"}
```

`get_state` returns only bootstrap metadata. No Niri events, bridge, subscriptions, authentication, or plugin privileges are implemented yet. Do not expose this socket beyond the local trusted user session.

## Complete the Nix package

The first source commit deliberately does not fabricate dependency hashes or lockfiles. Before enabling `packages.default`:

1. Run `cargo generate-lockfile` with a real Rust toolchain and commit `Cargo.lock`.
2. Run `npm install --package-lock-only --prefix frontend` with a real npm registry, test `npm ci --prefix frontend`, and commit `frontend/package-lock.json`.
3. Run `nix flake lock` and commit `flake.lock`.
4. Replace `lib.fakeHash` in `nix/package.nix` with the actual `npmDepsHash` emitted by `nix build`, then rebuild.
5. Verify that `result/bin/kira-shell` finds its packaged static assets and launches within Niri. Add `KIRA_UI_DIR` to the wrapper.
6. Add a Nix-based native Shell build job and optional Wayland E2E tests.

Until then, `nix develop` and `nix flake check` are the initial Nix features, not a validated finished package.

## Remaining architectural work

The Shell does not yet communicate with Core, the UI is not wired to system state, KDL configuration is unimplemented, and the development preview is not a finished widget/theme system. All need separate test-backed changes.
