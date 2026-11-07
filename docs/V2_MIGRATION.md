# Kira Desktop V2 — migration to runtime Lua + mlua + GTK4

**Статус:** архитектура согласована, **mlua-хост ещё не создан**.
[ADR-0002](adr/0002-embedded-lua-mlua.md) is the active decision. [ADR-0001](adr/0001-lua-rust-gtk.md) is superseded.

## Базовые принципы

- UI widgets, styles and callbacks are ordinary Lua executed by a Rust host via `mlua`, **not transpiled to Rust**.
- Rust owns GTK4, Wayland layer surfaces, Niri IPC, D-Bus/system services and security boundaries.
- No separate CSS preprocessor. Rust converts Lua style tables to native GTK CSS **at runtime**.
- Keep V1 Svelte/WebKit code and tests until equivalent Lua/GTK functionality passes real tests; no new V2 features in Svelte.
- Nothing about Lua support is considered implemented until tested in a native GTK process.

## Milestones

### M0 — architecture change (this ADR)

- Record [ADR-0002](adr/0002-embedded-lua-mlua.md) and supersede ADR-0001.
- Replace all Lua→Rust compiler contracts and samples with executable Lua 5.4 runtime API **proposals**.
- Keep the existing Cargo/Nix lockfiles unchanged until `mlua` is genuinely added and resolved.

### M1 — minimal mlua GTK host

- Add `mlua` with `lua54` (choose vendored or system linking with reproducible Nix checks), update `Cargo.lock` using Cargo.
- Introduce `kira-lua-runtime` Rust module/crate hosted in `kira-shell`. Do **not** create `kira-lua-compiler`.
- Initialize a Lua VM on GTK main thread, register `kira.ui` and controlled local module loader.
- Implement `ui.box`, `ui.text`, `ui.button` producing specs and a Rust GTK renderer.
- Add `ui.state(value)`, `ui.bind(state)`; preserve callbacks on GTK signals using registry handles and cleanup.
- Implement runtime `ui.theme` and `ui.compose` with safe GTK CSS class generation.
- Run [counter.lua](../examples/v2/counter.lua) and [styles.lua](../examples/v2/styles.lua) in a native GTK app. Verify actual Lua click callback, bound label, hover/disabled, safe errors, memory behavior, and no WebKit dependency on the new UI path.

### M2 — native Shell and Niri

- Mount top bar and workspaces in nested Niri via existing Rust IPC/Core interfaces.
- Test focus, HiDPI, multiple outputs, disconnect/error recovery, keyboard/accessibility, no off-main-thread GTK calls.
- Define typed Rust capabilities to support permission-controlled system actions.

### M3 — migrate widgets

- Replace Launcher, Calendar, Monitor, Settings, Tray and Notifications one by one.
- Retain Kira Default visuals, implement TENET animations using native GTK/Rust, respect reduced motion.
- Test Lua VM/callback lifecycle for each popup, teardown and optional script reload.

### M4 — ship

- Integrate Lua widget sources and `mlua` with Cargo/Nix packaging, without JS/Vite/Svelte/WebKit in the final V2 UI.
- Performance-profile startup, RSS/PSS, UI latency and error recovery.
- Remove legacy frontend/deps only after parity and native visual tests.

## Важные различия

| Area | Current inherited V1 | Target V2 |
| --- | --- | --- |
| UI | Svelte/WebKit | Lua via `mlua` running on native GTK4 Shell |
| Widgets | Svelte components | Rust `kira.ui` GTK bridge |
| Callbacks | JS | Real Lua closures via mlua |
| State | JS reactivity | Rust-backed observable `ui.state` |
| Styles | Web CSS/Tailwind | Lua tables → runtime GTK CSS |
| Transpiler | None in V1 | **Not planned** |
| Packaging | Nix/Cargo/npm | Nix/Cargo with embedded Lua runtime |
| System | Rust Core | Preserve Rust Core |

Green legacy CI checks V1 and documentation, **not** a working mlua/GTK integration.
