# ADR-0002: Run Lua widgets with mlua inside native Rust GTK Shell

- **Date:** 2026-11-07
- **Status:** Accepted; implementation pending
- **Branch:** kira-desktop-v2
- **Supersedes:** [ADR-0001](0001-lua-rust-gtk.md)
- **Extended by:** [ADR-0003: Kira Engine / Kira Desktop split](0003-kira-engine-desktop-split.md). The mlua decision remains active; GTK/Lua runtime ownership now belongs to independent Kira Engine.

## Context

V2 previously proposed a custom Rust transpiler that accepts only a statically analyzable Lua subset, generates Rust GTK source and recompiles the UI. This imposes a bespoke compiler and undermines Lua's ordinary functions, closures, modules and scripting ergonomics.

## Decision

1. **Embed Lua 5.4 using mlua** in the Rust-native GTK4 Shell; execute user-authored Lua widgets and themes at runtime.
2. **Do not develop a Lua-to-Rust translator**. `ui.box`, `ui.text`, `ui.button` and `ui.theme` become real Lua functions registered through Rust `mlua` bindings.
3. Use a small, versioned Rust `kira.ui` module for widget specification, state/bindings, callbacks and style composition; mount validated results as native gtk4-rs widgets.
4. Lua callbacks run on the GTK main thread, with explicit state handles and event-subscription lifecycle management; OS services remain trusted Rust-side capabilities.
5. Use Lua themes and Rust's runtime GTK CSS provider (not external CSS preprocessors).
6. Retain NixOS-first, Niri/Wayland, Kira design and the V1 shell only as a temporary migration baseline.

## Technical constraints

- Start with `mlua` `lua54` feature; compare `vendored` vs system Lua for reproducible Nix packaging. Lock Cargo dependency versions when adding it to the workspace.
- Start without `mlua/send`: GTK4 objects are not thread-safe, Lua VM/GUI lifecycle stays on the GTK main thread.
- `mlua` has APIs for creating Rust functions, userdata, registry-held callbacks and VM limits/hooks, but it is **not a process sandbox**. M1 scripts are trusted local code.
- Module loading must be controlled; do not expose unsafe Lua libs, arbitrary C modules, unrestricted filesystem, network, D-Bus or shell execution by default.
- Errors in Lua handlers must be caught before returning to GTK signals. Remove callback registry refs on unmount, and ensure repeat mount/unmount does not leak memory.
- Runtime reloading is a future feature. Unlike recompilation, changing Lua files does not require recompiling the Rust shell.

## Consequences

- Much smaller custom language/toolchain surface area.
- Native GTK UI rendering and direct scripting flexibility, at the cost of a resident Lua VM and the need for a carefully designed Rust/Gtk bridge.
- No promises about performance or hot reload until measured.
- V1 code and CI remain temporarily until functionality is migrated and validated in nested Niri.

## References

[Architecture](../V2_ARCHITECTURE.md) · [Lua API](../V2_LANGUAGE.md) · [Style API](../V2_STYLES.md) · [Migration](../V2_MIGRATION.md).
