# ADR-0004: Rust Extension API is a first-class Kira Engine contract

- **Date:** 2026-11-07
- **Status:** Accepted (architecture; implementation pending)
- **Extends:** [ADR-0003: Kira Engine vs Kira Desktop](0003-kira-engine-desktop-split.md), [ADR-0002: mlua](0002-embedded-lua-mlua.md)
- **Detailed contract:** [Rust Extensions & Lua Bridge](../RUST_EXTENSIONS.md)

## Context

Lua is appropriate for reusable GTK widgets, layouts and UI behavior, but CPU-intensive calculations, OS integration and security-sensitive calls belong in Rust. Without a first-class Rust extension mechanism, Kira Engine would either become Desktop-specific or force complex processing onto the GTK main thread.

## Decision

**Kira Engine must be flexible by design:** generic Rust hosts can register **versioned Rust modules/services/capabilities** that are exposed as convenient `require("...")` modules to Lua widgets via mlua, without embedding desktop-specific APIs in the Engine.

- `kira.ui` belongs to Engine; extensions such as `app.analysis` are registered by a generic host.
- `kira.desktop.*` capabilities belong to Desktop and are injected only by Desktop host. **Kira Engine must not depend on Desktop/Core/Niri**.
- Trivial, proven nonblocking calls may be synchronous; CPU-bound tasks and OS/network I/O use **asynchronous Rust providers**. No waiting on GTK main thread.
- A unified **typed request / result / error / event bridge** marshals input/output. Rust workers never access GTK objects or Lua VM; callback delivery and `ui.state` updates occur on GTK main thread.
- Subscriptions and async request handles are disposable. View lifecycle scopes manage `subscribe`, cancellation, error handling and late-result suppression automatically.
- Lua may contain normal Lua 5.4 functions/closures. Engine must not add a custom compiler or force all business logic into Lua.
- Permission and capability validation remain host responsibilities, enforced by Engine bridge. Embedded mlua is **not** an OS sandbox, and untrusted scripts require stronger process isolation.

## Consequences

Kira Engine can power an ordinary GTK app that injects computational modules, **and** Kira Desktop, which injects Niri/system modules, using one Lua UI API. Engine will need a stable extension registration API and lifecycle/runtime tests, but does not need hardcoded OS integrations or a Lua→Rust compiler.

## Acceptance

1. Register a Rust async compute service in a generic Engine host and call it from Lua without freezing GTK.
2. Subscribe from Lua widget to Rust stream; events update a bound GTK label on main thread.
3. Unmount cancels tracked work and prevents stale callbacks; repeat mount/unmount does not leak.
4. Desktop injects its own provider independently; Engine dependency graph remains Desktop-free.
5. Permission-denied/unregistered module and worker errors handled safely.

**The mechanism is specified, not yet implemented.**
