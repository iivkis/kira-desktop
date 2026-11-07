# ADR-0001: Lua as source language, Rust as compiler, GTK as native UI

- **Date:** 2026-11-07
- **Status:** Accepted (architectural direction, implementation pending)
- **Branch:** kira-desktop-v2
- **Decision owners:** Kira Desktop project

## Context

Kira Desktop V1 renders its UI in Svelte/WebKitGTK while Rust handles Linux services and the native window host. The intended V2 changes the language and rendering architecture, rather than merely replacing Svelte with Lua scripts executed at runtime.

## Decision

1. Write Kira Desktop widgets, styles, interactive state and UI logic in Lua.
2. Implement a Rust-based source-to-source translator/compiler which parses a validated, statically analyzable Lua subset, emits Rust + gtk4-rs, and generates any required GTK CSS/resources.
3. Compile generated Rust into a native GTK4 Wayland Shell. No Lua VM or browser engine is required in the final V2 UI process.
4. Keep Rust for trusted OS/system services, Niri integration and constrained APIs.
5. Keep NixOS-first reproducible delivery and retain Kira's visual identity.
6. Treat the existing Svelte/WebKit code as transitional legacy, not a coequal ongoing UI target.

## Why

- One compact source language for UI structure, styling and behavior.
- Native GTK widgets instead of an embedded browser runtime.
- Generation of inspectable, optimizable Rust code.
- Explicit trust boundary between authored UI logic and privileged Linux operations.

## Consequences

- A real compiler must be designed: parser, typed IR, validation, Rust codegen, diagnostics and test fixtures. The M1 contract uses an intentionally restricted Lua 5.4 DSL; see [Lua Language v0](../V2_LANGUAGE.md).
- Arbitrary dynamic Lua semantics do not map transparently onto static Rust. Initial V2 accepts a documented Lua subset and rejects unsupported constructs.
- Build-time translation implies recompilation for widget changes; runtime plugin loading and truly instantaneous hot reload are not guaranteed.
- The generated Rust binary has the Shell's OS privileges. Parsing Lua without executing it does **not** create a runtime sandbox; the translator must enforce explicit capabilities.
- GTK does not provide the same CSS/DOM/GSAP semantics as WebKit. The visual system and TENET motion must be reimplemented natively.
- Performance and memory benefits are goals to measure, not proven properties.
- During migration, V1 production sources and tests remain until native parity is verified.

## Rejected directions

- Continue Rust + Svelte + WebKitGTK as V2 architecture.
- Embed a Lua interpreter that directly drives GTK at runtime while calling that «translation».
- Replace Niri with a custom compositor as part of the UI migration.

## Implementation reference

[Full V2 architecture](../V2_ARCHITECTURE.md) · [migration milestones](../V2_MIGRATION.md) · [initial Lua syntax proposal](../../examples/v2/counter.lua).
