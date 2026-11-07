# Kira V2 — Kira Engine and Kira Desktop contributor rules

**Authoritative decision:** [ADR-0003](docs/adr/0003-kira-engine-desktop-split.md) + [Engine/Desktop contract](docs/ENGINE_DESKTOP.md).
**Runtime decision:** [ADR-0002: Lua via mlua](docs/adr/0002-embedded-lua-mlua.md); [ADR-0001](docs/adr/0001-lua-rust-gtk.md) superseded.
**Public API:** [Lua UI](docs/V2_LANGUAGE.md) · [Lua styles](docs/V2_STYLES.md) · [Migration](docs/V2_MIGRATION.md).

## Two products, one-way dependency

1. **Kira Engine** is a standalone **Rust + mlua (Lua 5.4) + GTK4** framework/runtime for generic native Lua GTK applications.
2. **Kira Desktop** is the **desktop environment built on Kira Engine**, with Niri/Wayland layer-shell, Linux services, panels and shell-specific Lua widgets.
3. Dependency direction: `kira-desktop -> kira-engine`, **never** `kira-engine -> kira-desktop`. Engine must not depend on Niri, `kira-core`, `kira-ipc`, `gtk4-layer-shell`, `kira.desktop` or desktop configuration.
4. Engine owns `require("kira.ui")`, generic GTK widgets, Lua VM/module loader, state/bind, events, styles, callback ownership, lifecycle and error reporting.
5. Desktop owns its `require("kira.desktop")` capability provider, GTK layer-shell windows, Core/IPC, Niri, D-Bus, Linux permissions, system configuration and Desktop Lua widget sources. Its capabilities are injected into Engine explicitly; they are **not** automatically present for generic Engine applications.
6. Plan Rust library `crates/kira-engine` and Desktop binary `crates/kira-desktop`. **Neither crate exists yet**. Existing `crates/kira-shell` and `frontend/` are V1 legacy, not Engine. Do not create placeholder crates or edit Cargo.lock by hand just to claim the split is implemented.

## Lua and runtime rules

- Execute **real Lua 5.4** via `mlua` at runtime. Do not create a Lua-to-Rust compiler/transpiler, AST DSL, `generated_ui.rs` or JS/WebView frontend for V2.
- `kira.ui` is implemented as Rust-registered Lua functions (e.g. `ui.box`, `ui.text`, `ui.button`, `ui.state`, `ui.bind`, `ui.theme`, `ui.compose`). Native GTK4 rendering belongs to Engine, not Desktop.
- Keep GUI widgets and Lua event callbacks on GTK main thread initially, no premature `mlua/send`. Events and typed messages from Core go through main context.
- Keep real Lua closures alive for GTK signals, dispose callbacks/subscriptions on unmount, catch `mlua::Error`; avoid UI-thread blocking operations.
- Runtime GTK CSS is produced by Engine from checked Lua styles, no Sass/Grass/PostCSS/Tailwind.
- Lua VM is **not a process sandbox**. M1 runs trusted locally installed scripts. Explicit, validated Desktop capability APIs; no raw system powers or untrusted plugins by default. Module loader and runtime limits must be designed and tested.

## Testing and deliverables

- **Engine acceptance:** generic counter GTK app runs without Niri, Desktop, `kira-core` or `gtk4-layer-shell`. Real Lua click handlers update GTK text/state and styles, and unmount cleans up resources.
- **Desktop acceptance:** desktop UI written in Lua mounts through the **same Engine** in nested Niri, with a separately injected `kira.desktop` capability provider. Test focus, multi-monitor, reduced motion and system-state truthfulness.
- Check Cargo dependency graph: Engine cannot transitively pull Desktop/Niri-specific components.
- Reproducible Cargo/Nix build; add `mlua` and update Cargo.lock **only with Cargo**, then test actual GTK linking. Existing V1 checks and docs tests do **not** demonstrate Engine or Desktop V2 functionality.
- Keep Kira Default visual identity and TENET principles; GSAP-specific V1 implementation does not carry to V2.
- Do not remove V1 app/CI until native replacement passes parity and nested Niri QA. Work only in `kira-desktop-v2`, leave main untouched.
