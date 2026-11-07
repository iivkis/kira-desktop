# Kira Desktop V2 — migration from Svelte/WebKit to Lua → Rust → GTK

**Stage:** architecture approved; existing main-derived V1 code is still present. This page describes the path forward, **not completed work**.

## Principles

- All new user-facing widgets are authored in Lua and compiled to GTK Rust. No new Svelte widget features on V2.
- Existing Rust Core, Niri protocol and security boundaries are reused where possible.
- Existing V1 UI code is a reference implementation for behavior, accessibility and appearance, not the permanent V2 stack.
- Do not delete working V1 code before the native replacement has build + functional + visual acceptance.
- Do not claim native performance improvement until measured.

## Milestones and exit criteria

### M0 — architecture decision (completed in this commit)

- Branch kira-desktop-v2 from main.
- ADR, compiler/source-language specification, example, migration rules and contributor guidance.
- Explicitly state implementation remains V1 at this point.

### M1 — Lua compiler MVP

Normative source subset and type/state/event semantics: [V2_LANGUAGE.md](V2_LANGUAGE.md). The Lua syntax proposal remains **non-executable** until the native end-to-end build passes.

- Add crates/kira-lua-compiler Rust CLI and fixture-based tests.
- Parse the proposed statically analyzable Lua v0 subset into AST then a typed UI IR.
- Generate deterministic Rust source and GTK CSS for Box, Label, Button.
- Generate compiled state bindings and simple on_click logic.
- Negative tests: invalid widget, unsupported Lua semantics, unknown API, invalid state access, source diagnostics.
- Build and run examples/v2/counter.lua as a native GTK application without any Lua interpreter.
- Verify generated Rust is deterministic and GTK CSS is produced exclusively from allowlisted Lua style fields.
- Test matching integer arithmetic semantics in debug and release.

### M2 — native Shell proof of concept

- Add a native GTK4 UI entry path without WebKitGTK and connect it to the existing Shell/Core IPC boundaries.
- Create a native 42px top bar in isolated nested Niri, including real workspace state and validated focus.
- Test popup geometry, focus, keyboard navigation, high DPI, multiple outputs and disconnect handling.
- Start collecting cold-start and RSS/PSS comparison measurements.

### M3 — widget migration

- Move Launcher, Calendar, System Monitor, Settings, Tray and Notifications to compiled Lua/GTK one by one.
- Preserve safe D-Bus and Niri actions on Rust side, with explicit allowlisted high-level capabilities.
- Implement native GTK equivalents of Kira Default palette and accessibility.
- Rebuild TENET motion on GTK/Rust with specific tests for continuity, interruption, reverse/settle and reduced motion. GSAP is not carried into new runtime.
- Keep working legacy code for each widget until corresponding native tests pass.

### M4 — composition and NixOS packaging

- Add Lua widget composition and build-time module inclusion, with a versioned compiler syntax/IR contract.
- Define the KDL/Nix configuration ownership boundary and safe generated outputs.
- Integrate compiler into Cargo/Nix build deterministically; support watch+recompile for development.
- Remove WebKit/Svelte/Vite/Node/JS dependencies from runtime/build after full UI feature parity.
- Perform packaged-Niri visual QA and native performance profiling.

## Source ownership

| Area | V2 direction | Current state |
| --- | --- | --- |
| UI language | Lua | Svelte/TypeScript legacy exists |
| Translator | Rust kira-lua-compiler | Not created |
| Rendering | gtk4-rs native | GTK hosts WebKit legacy |
| Style | Lua Style → GTK CSS | Tailwind/CSS WebKit legacy |
| UI logic | Lua handlers → Rust closures/state | Svelte event handlers legacy |
| System services | Rust Core/Shell + IPC | Existing starting point |
| Packaging | Nix + Cargo | Existing pipeline must change |
| Motion | Native Rust/GTK TENET | GSAP legacy |
| WebView/Node/Vite | Not in final V2 UI | Present only during migration |

## Repository hygiene

The generated V1 `.repospool` snapshot, ignore specification and publication workflow are removed from this **V2 branch**. The V1 implementation code, runtime dependencies, tests and Nix pipeline remain for parity checks. See [V2 review](V2_REVIEW.md).

## Validation rule

Until M1/M2 exist, running the current tasks, building the current frontend, or seeing green legacy CI **does not validate Lua→Rust→GTK**. New V2 CI must compile generated Rust and exercise actual GTK widgets in nested Wayland. Documentation should consistently distinguish intended vs working functionality.
