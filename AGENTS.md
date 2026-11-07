# Kira Desktop V2 contributor and agent rules

**Current decision:** [ADR-0002: embedded Lua + mlua](docs/adr/0002-embedded-lua-mlua.md).
**Reference:** [architecture](docs/V2_ARCHITECTURE.md) · [runtime Lua API](docs/V2_LANGUAGE.md) · [Lua styles](docs/V2_STYLES.md) · [migration](docs/V2_MIGRATION.md).
**Superseded:** [ADR-0001 Lua-to-Rust compiler](docs/adr/0001-lua-rust-gtk.md) is for historical context only.

## Authoritative direction

1. Widgets, state, callbacks and styles are **real Lua 5.4**, executed by `mlua` embedded in native Rust/GTK4 Shell at **runtime**.
2. **Do not create a Lua-to-Rust compiler/transpiler**, codegen pipeline or "static Lua AST DSL". `ui.*` are actual Lua functions registered by the Rust host. Lua functions, closures, local variables and controlled `require` are allowed.
3. Rust owns GTK widgets and Wayland layer surfaces, Niri, system resources, D-Bus, config safety and typed Core/IPC capabilities. Lua never receives raw unrestricted system or GTK access.
4. Render native GTK4 widgets. Do not use Svelte, WebKitGTK, Tauri, JS or GSAP for **new V2 UI**. V1 code and dependencies stay only for regression/migration until replacement parity.
5. Lua themes and styles are applied by a Rust-side **runtime GTK CSS bridge**. No Grass/SCSS/Sass/Tailwind/PostCSS build step in V2.
6. Until `mlua` is added to Cargo and the host is built, all Lua code in `examples/v2` is **API-contract code, not runnable in the current project**. Do not claim otherwise.

## Runtime bridge rules

- M1 `ui.box`, `ui.text` and `ui.button` build validated WidgetSpec objects, later mounted to native GTK by Rust on the GTK main thread.
- `ui.state(value)` uses Rust-backed observable userdata with `:get()` and `:set(...)`. `ui.bind(state)` creates explicit update subscriptions; do not promise that plain Lua table-field writes are reactive.
- Lua callbacks are real closures. Keep their `mlua::Function` references alive while GTK signals exist; release callbacks, subscriptions, and view resources on unmount. Catch/report `mlua::Error` inside GTK handlers without panic propagation.
- Run Lua and GTK on GTK main thread initially; do not enable `mlua/send` without a carefully proven design. Off-thread Niri/Core data arrives via typed messages to GTK main context.
- Use a controlled module loader, validated widget attributes, allowed GTK CSS style fields/states and approved system capabilities. Lua VM is **not a security sandbox**; M1 assumes trusted local widgets, not arbitrary downloaded plugins.
- Restrict unsafe stdlibs/native C modules and impose sensible memory/execution budgets. Keep OS work off the GTK thread.
- Reproducibility: when adding `mlua`, generate a **real** Cargo.lock update with Cargo and validate Nix linking (`lua54` + chosen vendored/system feature). Do not handwrite lockfile entries.

## Design invariants

Keep Kira Default palette and TENET philosophy (causality, continuity, accessibility, reduced motion) but reimplement native GTK/Rust behavior; GSAP-specific V1 implementation is not required. Keep truthful state, keyboard focus, display safety rollback, and repeatable nested-Niri visual QA.

## Current implementation boundary

The existing repo still contains V1 Svelte/WebKit; its CI is a regression gate, **not** evidence that mlua GTK UI works. V1 guides are explicitly labeled as migration references. No changes to `main` while developing this branch.
