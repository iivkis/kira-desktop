# Kira Desktop: native-gtk contributor contract

**Architecture decision:** [docs/ARCHITECTURE_NATIVE.md](docs/ARCHITECTURE_NATIVE.md) is normative for the native-gtk branch. [docs/MIGRATION_NATIVE_GTK.md](docs/MIGRATION_NATIVE_GTK.md) records stage gates, feature gaps and removal policy.

## One native presentation stack

- Built-in views and future trusted widget views use **Rust + GTK4**. Prefer GTK widgets, CSS and accessible GObject properties. Use GDK/GSK or GtkDrawingArea only when stock widgets cannot express the design.
- **No WebKitGTK, Svelte, Vite, Tailwind, JavaScript, GSAP, ECharts or web widget pool.** Do not restore them as a shortcut, temporary renderer, test harness or second UI framework.
- GTK4 + gtk4-layer-shell own native Wayland panels and transient overlays; regular windows remain Niri-managed xdg-toplevels. Keep 42px Top Bar exclusive zone and prevent overlays reserving extra space.
- Core owns Niri integration and KDL validation. Shell owns presentation, focus and lifecycle, and calls typed, bounded Core/GIO adapters. Never expose arbitrary shell commands or broad filesystem writes to widget code.
- Never block GTK's main context with socket IO, process enumeration or slow metrics. Workers exchange owned data; only GTK main-context code touches GTK objects.
- The single visible transient popup rule remains. Detached ordinary windows are independent and must survive switching the transient popup. Recreating GTK state during a reparent is not acceptable if state preservation is a requirement.
- Preserve the observed/config/managed/session ownership boundary in [docs/CONFIG_OWNERSHIP.md](docs/CONFIG_OWNERSHIP.md). Apply, Confirm and Save are different operations; invalid or externally owned configuration fails closed.

## Visual and motion contracts

- Read [docs/DESIGN_SYSTEM.md](docs/DESIGN_SYSTEM.md) and [docs/TENET_MOTION.md](docs/TENET_MOTION.md) before changing UI, themes or animation.
- Use only canonical Kira colors (#000000, #14213d, #fca311, #e5e5e5, #ffffff) for the built-in Kira Default theme. Preserve contrast, visible focus, keyboard navigation, accessible names and real-time data correctness.
- TENET is about causal consistency, continuity, reversible trajectories and temporal memory, **not** a JS animation framework requirement. Stage GTK animation changes after basic functionality, and respect reduced motion.
- Do not resurrect the rejected hover distortion/lens experiment.

## Quality gates

- `cargo fmt --all -- --check`; `cargo clippy --locked --workspace --all-targets -- -D warnings`; `cargo test --locked --workspace --all-targets`.
- `task check`, `task shell:check` and `nix build .#default` where native dependencies and Nix are available.
- Manual nested Niri validation for multi-output, focus, keyboard input, fractional scaling, pop-up lifecycle and all motion. CI is not proof of native graphical correctness.
- Missing parity must be named in the migration plan, not faked or hidden. No browser-related test requirements apply after browser components are removed.
