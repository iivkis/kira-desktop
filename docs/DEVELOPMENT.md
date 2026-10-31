# Developing Kira Desktop

## Requirements

Kira Desktop targets NixOS, Niri and Wayland. Flakes must be enabled. The repository commits the generated `Cargo.lock`, `flake.lock` and `frontend/package-lock.json`, as well as the verified frontend `npmDepsHash`.

The GitHub Actions workflows are split into fast, non-graphical checks (`.github/workflows/ci.yml`) and native Nix integration (`.github/workflows/nix.yml`).

## Kira Default visual design

The premium visual baseline is implemented using semantic CSS tokens in
`frontend/src/lib/themes/kira-default.css` and typed color metadata in
`frontend/src/lib/themes/kira-default.ts`. `WorkspacePreview.svelte` and
`CoreStatus.svelte` demonstrate theme-aware components without hardcoded HEX
values. The former is deliberately non-interactive until Niri IPC is wired.

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
and ignores attempts to open a new window. The Rust bridge remains explicitly
allowlisted to read-only methods. This is defense-in-depth for trusted UI code,
not a security boundary for untrusted extensions.

Core now bounds individual JSON-RPC messages to 4096 bytes and removes the
Unix socket it owns after SIGINT/SIGTERM, checking inode identity first. The
preview traps SIGTERM and terminates process groups even when their leaders
have already exited, reducing the chance of orphaned processes.

## Core ⇄ Shell ⇄ Svelte bridge

Svelte uses the typed SDK at `frontend/src/lib/sdk/core.ts`, sending JSON-RPC requests to `window.webkit.messageHandlers.kira.postMessage`. The Rust Host forwards **only `ping` and `get_state`** over Unix Domain Socket on a worker thread. Its GTK main loop returns replies as `kira:rpc` events.

The SDK correlates numeric request IDs, applies timeouts, and reports disconnected Core as an error. Browser-only Vite preview has no WebKit bridge and displays `Core: unavailable` rather than a fake connection.

```json
{"jsonrpc":"2.0","id":1,"method":"ping"}
```

```json
{"jsonrpc":"2.0","id":1,"result":"pong"}
```

The IPC transport uses newline-delimited JSON. The bootstrap `get_state` response contains version and status only. There is no Niri integration or event subscription mechanism yet. The Core socket is intended for the trusted local user session only.

## Updating dependencies

The initial lockfiles were generated by `.github/workflows/lockfiles.yml`. Update them deliberately using the appropriate tooling and commit changes together with any required Nix hash update. The separate Nix integration workflow must pass after dependency changes.

## Next milestones

1. Visual QA for Kira Default across scales and packaged Shell smoke test. The user confirmed prior capture runs, but the PNGs still require visual review. The default QA command is now passive; explicit screenshot capture uses grim without clipboard side effects. See [QA_VISUAL.md](QA_VISUAL.md).
2. Niri IPC workspace state and typed event subscriptions.
3. KDL configuration engine with validation and hot reload.
4. Real workspace, launcher and dock widgets.
5. System-service integrations and user widget loading.

Do not claim implementation or isolation of untrusted plugins until the code and tests exist.
