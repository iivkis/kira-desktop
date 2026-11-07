# UI architecture · Rust-owned GTK4 widgets

**Normative since native-gtk.** The previous requirement that all widgets be authored in Svelte 5 is repealed.

## Ownership matrix

| Component | Owns | Cannot own |
| --- | --- | --- |
| Core (Rust/Tokio) | Niri EventStream, KDL validation, config subscriptions, authorized focus RPC | GTK widget tree |
| IPC crate | Versioned JSON-RPC and bounded Unix-socket client | Filesystem or compositor authority beyond explicit methods |
| Shell (Rust/GTK4) | GTK widgets, accessibility, focus, popup/window lifetime and GIO adapters | Core's state source-of-truth or arbitrary command bridge |
| GTK CSS / GDK / GSK | Native visual skin, drawing, snapshot/effects | Model truth or permissions |
| Trusted feature controller | Presentation state and typed user intent | Raw executable strings, Nix-owned configuration or unrestricted file writes |

The Top Bar is a top-anchored 42px exclusive layer-shell surface. Popup surfaces reserve zero exclusive zone; at most one transient is active. Detached windows are ordinary Niri-managed toplevels and survive transient switches.

## Component structure

Prefer pure data models and small native view controllers. Separate models from GTK object construction and separate asynchronous Linux/GIO operations from the GTK main context. Data delivery is one-way: background worker → owned result → GTK main context → widget. Interactions go through typed controller methods, Core requests or fixed GIO actions.

A GtkCalendar replaces browser date grids, a GtkSearchEntry + list replaces browser launcher DOM, and system metrics are exposed through native labels/charts. Future complex controls use GTK composite widgets, not WebKit as a convenience fallback.

Read [ARCHITECTURE_NATIVE.md](ARCHITECTURE_NATIVE.md) and [MIGRATION_NATIVE_GTK.md](MIGRATION_NATIVE_GTK.md) for the complete design and explicit feature gaps.
