# ADR-0005: Direct bidirectional async mlua API, no internal IPC

- **Date:** 2026-11-07
- **Status:** Accepted (design; implementation pending)
- **Extends:** [ADR-0004](0004-rust-extensions-lua-bridge.md), [ADR-0003](0003-kira-engine-desktop-split.md), [ADR-0002](0002-embedded-lua-mlua.md)
- **Normative:** [Async bridge](../ASYNC_BRIDGE.md)

## Decision

1. Lua and Rust run **in the same Kira Engine process** and communicate via **mlua direct bindings**, **not IPC**. No internal Unix-socket, JSON-RPC, mandatory daemon or transport serialization for Engine/Desktop's internal bridge.
2. The bridge is **bidirectional and asynchronous**. Lua calls Rust `Lua::create_async_function` methods through Engine-managed coroutines (`Function::call_async`). Rust sends events and calls registered Lua functions asynchronously, also through `mlua`.
3. Run Lua and GTK on main context. Use `mlua` feature `async` and GLib main-context local executor, **without `send` initially**. CPU-bound Rust tasks use workers; Rust worker results return through a bounded **in-process** queue. No blocking GTK/main-loop wait.
4. Provide reusable, capability-checked Rust host module registration and Lua helper modules `kira.tasks` and `kira.events`, with cancellation, timeouts, subscriptions, scope-bound cleanup, late-result suppression and typed error handling.
5. Kira Desktop hosts its Core/system Rust services **in-process** and exposes them through restricted `kira.desktop.*` bindings. Engine remains independent from Desktop, Niri, Core and `kira-ipc`.
6. **External** Niri IPC / D-Bus remain where separate processes exist. Legacy V1 `kira-ipc` and Core daemon stay only during verified migration; remove them after V2 replacement passes tests, not immediately.
7. `mlua` does not sandbox untrusted scripts, and async coroutines do not make a synchronous Lua loop nonblocking. Security and main-thread budgets remain explicit.

## Consequences and acceptance

No heavy internal IPC, smaller runtime surface, no custom transpiler. Rust can efficiently expose high-performance logic to Lua and call Lua back, with a single documented async lifecycle.

Acceptance: Lua→Rust async worker call, Rust→Lua event and async request/response, cancellation/unmount, bounded queues, errors and GTK responsiveness; Engine independently works without Desktop/legacy Core IPC. **Architecture only, not yet implemented.**
