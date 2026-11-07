# Kira Desktop

**Status: native-gtk transition, experimental. Not a production desktop environment.**

Kira Desktop is a Rust-native, NixOS-first desktop shell for the **Niri/Wayland** compositor. Its application interface is built from **GTK4 widgets and GTK CSS**. There is no WebKit, browser runtime, Svelte bundle, JavaScript bridge, or embedded web UI in the target architecture.

**The native-gtk branch is a migration baseline, not feature parity with main.** The first milestone brings up native Top Bar, Niri workspaces, calendar, launcher and a minimal system/settings surface. The former Svelte layouts, charting, rich tray and notification presentation are not yet ported. Do not describe them as finished.

## Start here

- [Native architecture and philosophy](docs/ARCHITECTURE_NATIVE.md)
- [Migration plan, removed stack and acceptance](docs/MIGRATION_NATIVE_GTK.md)
- [UI ownership contract](docs/UI_ARCHITECTURE.md)
- [Design system](docs/DESIGN_SYSTEM.md)
- [TENET motion principles](docs/TENET_MOTION.md)
- [Configuration ownership](docs/CONFIG_OWNERSHIP.md)
- [Supported KDL config](docs/CONFIG.md)

## Technology and ownership

| Layer | Technology | Owns |
| --- | --- | --- |
| Kira Core | Rust, Tokio | Niri EventStream, KDL validation, subscriptions and bounded IPC actions |
| Kira IPC | Unix sockets, JSON-RPC 2.0 | Stable typed messages and connection health |
| Kira Shell | Rust, GTK4, gtk4-layer-shell, GIO | Native widgets, Wayland surfaces, focus, UI state, application launch |
| Rendering | GTK4 CSS, GTK/GDK/GSK | The native visual tree, accessibility, drawing and effects |
| Packaging | Nix flake, Crane, Cargo | Reproducible Rust package and development environment |

GTK is **the UI**, not a frame wrapped around a web page. Niri remains the window manager, not an in-process compositor. Configuration is declarative-first and fail-closed.

## Development

On NixOS inside a Wayland session:

~~~sh
nix develop
task check
task shell:check
task build
~~~

For an isolated nested Niri preview, run:

~~~sh
nix develop --command task dev:preview
~~~

Do not run the Shell as a replacement for your live desktop until native Niri, focus, scale and input acceptance has been completed. The preview must never rewrite the host's Niri config.

## Migration guarantee

The main branch preserves the existing WebKit implementation. This branch deliberately removes its **architectural authority and packaging dependency**. System adapters will be ported to native UI by explicit milestones, not reintroduced through a hidden browser or a generic command bridge.

Kira Default uses Black, Prussian Blue, Orange, Alabaster Grey and White. The Top Bar reserves **42 logical pixels**. Accurate data, reduced motion, keyboard operation and bounded resource use take precedence over visual effects.
