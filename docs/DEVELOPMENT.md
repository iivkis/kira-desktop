# Native GTK4 development

Use `nix develop` on NixOS to obtain Cargo, gtk4, gtk4-layer-shell, Task and Niri. `task check` covers Rust formatting/Core/IPC, and `task shell:check` compiles the native Shell with GTK system libraries. `task nix:build` assembles the reproducible Rust-only package. No npm, Node, WebKit or HTML build is involved.

For a safely isolated graphical preview: `task dev:preview`. The nested compositor has its own Niri configuration, Core Unix socket and KDL sample. It must never modify the parent compositor. Inspect the GTK bar, app launcher, calendar and offline workspace label manually. Use `task dev:preview:debug` for native logs.

See [ARCHITECTURE_NATIVE.md](ARCHITECTURE_NATIVE.md), [MIGRATION_NATIVE_GTK.md](MIGRATION_NATIVE_GTK.md) and [PREVIEW.md](PREVIEW.md). The native migration is not complete and should not replace the live desktop session.
