# Kira Desktop · Native GTK4 architecture

**Status: Accepted target for the native-gtk branch, 2026-11-07.** This document establishes the destination. It does not assert feature parity on the first commit.

## Intent and philosophy

Kira Desktop is **a desktop shell, not a website deployed to a desktop**.

1. **Native by default.** GTK4 owns the widget tree, painting, input, accessibility, focus and lifecycle. No embedded browser is permitted on the shell UI path.
2. **Niri is the compositor.** Kira does not replace Niri or implement its own window manager. It integrates using documented Niri interfaces over an isolated Core service.
3. **Rust is the authority.** Domain state, validation, service adapters and UI controllers have explicit owners. UI widgets display state and issue typed intents, not commands.
4. **Declarative-first.** NixOS/Home Manager/user KDL remain the source of truth until an explicit managed or session transaction is authorized. No silent mutation of foreign config.
5. **State must be honest.** Disconnected Niri means offline. Missing sensor data mean unavailable. Never invent workspaces, system readings or display capabilities.
6. **Small and observable.** Avoid JS processes, offscreen WebViews, busy loops and hidden duplicated state. Sample on demand and define bounded refresh policies.
7. **Design without browser artifacts.** The visual language, accessibility, keyboard semantics and TENET causality survive; DOM-specific layout tricks do not.
8. **Reviewable increments.** A native Shell can be temporarily less feature-complete than main. No component is called migrated until tested in a real nested Niri session.

## Layers

```text
Niri / Wayland compositor
   ↑         ↑
   │ Niri IPC│        GTK4 + gtk4-layer-shell / GDK / GSK
   │         │               ↑ native state, commands
Kira Core (Rust/Tokio) ← typed JSON-RPC → Kira Shell (Rust/GTK4/GIO)
   │                                       │
   └ KDL validation / Niri streams        ├ Top Bar / workspaces
                                           ├ Calendar / Launcher
                                           ├ Settings / System / Tray
                                           └ Notifications / detached windows
```

| Responsibility | Owner | Boundary |
| --- | --- | --- |
| Niri EventStream, workspace truth | Core | Typed snapshots, bounded focus request |
| KDL parse/hot reload/validation | Core | Last-known-good state; error surfaced in Shell |
| Protocol and reconnect semantics | kira-ipc | Unix socket, bounded JSON-RPC, 5s heartbeat / 15s timeout |
| GTK view state, controls, geometry | Shell | Main-thread GTK object ownership |
| Application discovery/launch | Shell GIO adapter | Application identities, never interpolated shell commands |
| Display changes | Shell trusted Niri adapter | Validate, Apply, Confirm, Save with rollback |
| Metrics and notifications | Shell Linux/GIO adapters | Read only where possible; bounded, rate-limited, honest |
| Theme and accessibility | GTK CSS, widgets, a11y | Canonical palette, focus, keyboard, reduced motion |
| Charts and specialized visuals | GtkDrawingArea or GSK renderer | Sampled data remain separate from animation state |

## Window contract

- One **42 logical px** Top Bar using GTK4 + layer-shell, top/left/right anchored, exclusive zone 42.
- At most one transient overlay (exclusive zone 0), positioned to fit current output. Launcher may use the top-centered anchor; other widgets the top-right dock.
- Promoted widgets are regular decorated xdg-toplevel GTK windows managed by Niri. They are independent of transient popup switching and may coexist.
- A widget's controller owns its state. Reparent the same GTK widget tree for pop-out when state continuity is required; never recreate content merely to simulate promotion.
- Popup creation is demand-driven, not a warm WebView lease. Clear signals, timers and subscriptions when surfaces close.

## Rendering and motion

Use GTK4 layout and style classes, GDK textures, GTK snapshot/GSK nodes where needed. Adopt GtkDrawingArea only for genuinely custom visualizations, with accessible textual equivalents. Use GLib frame clocks and GTK tick callbacks for short, interruptible animation; do not spin custom timing threads for UI. TENET requires a recorded timeline for reversible effects; changing a CSS class is not a substitute for temporal memory.

The canonical palette is #000000 (background), #14213d (structured surfaces), #fca311 (selection), #e5e5e5 (secondary text), #ffffff (primary text). A widget is not production-ready without keyboard/focus, reduced-motion and small-display acceptance.

## Trust boundaries

Native does **not** automatically mean safe. A reusable widget must receive **only** explicit capabilities. Do not ship an arbitrary-command interface, general filesystem adapter, or unreviewed dynamically loaded native code under a mere permission declaration. Untrusted third-party widgets are deferred until isolation and a stable ABI are designed.

## Migration strategy

The initial branch replaces browser authority and restores a minimal native Shell; old rich implementations are historical context in main, not hidden dependencies here. Port the existing functional behavior in vertical slices; validate every slice and delete the superseded implementation. Full parity includes tray, notifications, desktop settings transactions, detachable widgets and observability. See [MIGRATION_NATIVE_GTK.md](MIGRATION_NATIVE_GTK.md).
