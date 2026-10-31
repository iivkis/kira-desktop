# Kira Desktop

**Status:** v0.1 Draft, early bootstrap. This is **not yet a finished desktop shell**.

Kira Desktop is a modular, declarative **NixOS-first** environment targeting Niri/Wayland. Rust owns system capabilities; Svelte 5 owns the UI; KDL will define desktop layout and settings.

- [Technical specification (Russian)](docs/SPECIFICATION.md)
- [Development instructions](docs/DEVELOPMENT.md)

## Current source layout

| Component | Current scope |
| --- | --- |
| `crates/kira-ipc` | JSON-RPC 2.0 `ping`/`get_state` request and response contracts, unit tests |
| `crates/kira-core` | Async Unix domain socket daemon, newline-delimited JSON-RPC |
| `crates/kira-shell` | GTK4 layer-shell top bar hosting a local WebKitGTK 6 page |
| `frontend/` | Svelte 5 + TypeScript + Vite, 42 px visual preview, Vitest test |
| `Taskfile.yml` | Shared developer/CI commands |
| `flake.nix`, `nix/package.nix` | Nix development shell and preliminary package recipe |
| `.github/workflows/ci.yml` | Lightweight native Linux CI; no graphical E2E yet |

## Development on NixOS

```bash
nix develop
task frontend:install
task check
task build
```

In a running Niri/Wayland session, launch **separate terminals**:

```bash
task core:run
task shell:run
```

For a manual Core protocol smoke test (with `socat` installed):

```bash
printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"ping"}' | socat - UNIX-CONNECT:"$XDG_RUNTIME_DIR/kira-desktop.sock"
```

The UI is a **static preview**: workspace pills do not yet talk to Niri, and the Shell does not yet connect to Core. Never confuse a static visual with a working system integration.

## Known bootstrap limitations

`Cargo.lock`, `frontend/package-lock.json` and `flake.lock` still need to be generated with the actual toolchains and committed. `nix/package.nix` contains a placeholder npm dependency hash. **`nix build` cannot be claimed working yet.** See [development instructions](docs/DEVELOPMENT.md) for the remaining steps.
