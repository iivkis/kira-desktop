# Kira Desktop

**Status:** v0.1 Draft, early bootstrap. This is **not yet a finished desktop shell**.

Kira Desktop is a modular, declarative **NixOS-first** environment targeting Niri/Wayland. Rust owns system capabilities; Svelte 5 owns the UI; KDL will define desktop layout and settings.

- [Technical specification (Russian)](docs/SPECIFICATION.md)
- [Development instructions](docs/DEVELOPMENT.md)

## Current source layout

| Component | Current scope |
| --- | --- |
| `crates/kira-ipc` | JSON-RPC 2.0 contracts, bounded socket client, and unit tests |
| `crates/kira-core` | Async Unix domain socket daemon, newline-delimited JSON-RPC |
| `crates/kira-shell` | GTK4 layer-shell top bar with WebKitGTK 6 message bridge to Core |
| `frontend/` | Svelte 5 + TypeScript + Vite, typed SDK and visible Core connection status |
| `Taskfile.yml` | Shared developer/CI commands |
| `flake.nix`, `nix/package.nix` | Nix development shell and preliminary package recipe |
| `.github/workflows/ci.yml` | Lightweight native Linux CI; no graphical E2E yet |

## Development on NixOS

```bash
nix develop
npm ci --prefix frontend
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

Workspace pills are still **static previews**, but the status indicator now asks the real Rust Core for its bootstrap state over the WebKit → Shell → Unix Socket bridge. Full Niri state and event subscriptions are not yet implemented.

## Known bootstrap limitations

Dependency locks are now generated and committed using Cargo, npm and Nix. `nix/package.nix` still contains a placeholder npm dependency hash. **`nix build` cannot be claimed working until native Nix integration passes.** See [development instructions](docs/DEVELOPMENT.md) for the remaining steps.
