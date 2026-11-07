# ADR-0003: Split Kira Engine from Kira Desktop

- **Date:** 2026-11-07
- **Status:** Accepted (architecture; implementation pending)
- **Branch:** kira-desktop-v2
- **Extends:** [ADR-0002: embedded Lua via mlua](0002-embedded-lua-mlua.md)
- **Background:** [ADR-0001](0001-lua-rust-gtk.md) is already superseded by ADR-0002.

## Decision

**Kira Engine** is a reusable, self-contained Rust + mlua + GTK4 framework/runtime for writing **native GTK applications in Lua**. It owns generic `kira.ui` bindings, widget rendering, observable state, signals, lifecycle, runtime style handling and Lua errors.

**Kira Desktop** is a **separate desktop-shell application built on Kira Engine**. It provides Niri/Wayland layer-shell surfaces, panels, desktop widgets, OS integrations, configuration, Core/IPC, and optional validated `kira.desktop` capabilities. Kira Desktop **depends on** Kira Engine, not vice versa.

An Engine-only GTK application **must not require** Niri, `gtk4-layer-shell`, `kira-core`, `kira-ipc`, desktop-specific Lua modules or Desktop configuration to run. Niri integration and system capabilities belong on the Desktop side, exposed only through explicit host registration.

The architectural relationship is comparable to **Tauri framework vs app**, except **Lua + mlua + native GTK4** replaces the WebView frontend. It is not based on Tauri.

## Planned ownership

- `crates/kira-engine`: independent Rust library with `mlua`, GTK4 and generic UI APIs.
- `crates/kira-desktop`: Niri/Wayland desktop app using `kira-engine`.
- `crates/kira-core` and `crates/kira-ipc`: desktop/system services, not engine dependencies.
- `ui/desktop`: Lua widget sources for the Desktop (future).
- `crates/kira-shell` and `frontend/`: retained legacy V1 migration baseline, **not** the engine.

These paths are planned; none of the new crates exist yet.

## Reasons

1. One Lua/GTK implementation can power both the desktop shell and ordinary GTK apps.
2. UI events, styling and state get one maintained API rather than a desktop-specific rewrite.
3. A minimal generic engine has a smaller security/maintenance surface and is independently testable.
4. Desktop-specific OS privileges stay out of generic Lua UI APIs.

## Constraints and acceptance

- Keep Lua 5.4 running through `mlua` at runtime, not a transpiler.
- Keep GTK and Lua widget callbacks in GTK main context initially.
- Use explicit, versioned `kira.ui` API; `kira.desktop` is an injected, restricted optional module.
- Lua is not sandboxed just by `mlua`. Start with trusted locally installed scripts.
- Do not promise hot reload or performance improvement without implementation/evidence.
- **Prove standalone Engine:** run the counter GTK example with no Niri/Desktop dependency.
- **Prove Desktop consumer:** mount Lua Desktop widget in nested Niri through the same Engine.
- Check Cargo dependencies in one direction: `kira-desktop -> kira-engine`, no reverse dependency.

## Implementation status

**Documented and accepted, but not yet coded.** Current Cargo workspace still builds inherited V1. Do not add placeholder crates, edit Cargo.lock by hand or claim the Engine is ready merely because this ADR exists.

[Full boundary contract](../ENGINE_DESKTOP.md) · [V2 architecture](../V2_ARCHITECTURE.md) · [migration](../V2_MIGRATION.md).
