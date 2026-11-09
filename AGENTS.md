# Kira Desktop: native-gtk contributor contract

**Architecture decision:** [docs/ARCHITECTURE_NATIVE.md](docs/ARCHITECTURE_NATIVE.md) is normative for the native-gtk branch. [docs/MIGRATION_NATIVE_GTK.md](docs/MIGRATION_NATIVE_GTK.md) records stage gates, feature gaps and removal policy.

## Mandatory UI-first development rule

1. Specify reusable GTK component APIs, states, accessibility and tests in `crates/kira-ui` **before** implementing the desktop feature.
2. Build and test the GTK component without Core or Niri. Components consume plain view models and emit typed intent callbacks. No `kira-ipc`, app discovery, process execution or Niri commands in `kira-ui`.
3. Keep trusted system models and native adapters in `crates/kira-services` or Core, independently of GTK; preserve tested existing Rust policies.
4. **Only then** wire library components to Core/services in `crates/kira-shell`. Shell is a composition root, never a second component framework.
5. Complete component tests, Nix build and nested Niri graphical acceptance. See [docs/KIRA_UI.md](docs/KIRA_UI.md).

## Mandatory main widget parity gate

- The frozen reference is `main` at `305139949f49176487ef9759fc32c228eadff408`. Read [docs/MAIN_WIDGET_PARITY.md](docs/MAIN_WIDGET_PARITY.md) and the machine-readable [widget ledger](docs/MAIN_WIDGET_PARITY.json) before changing any built-in widget.
- Preserve **every source widget's content, states, interactions, data freshness, keyboard focus and TENET animation**, including the complete reverse paths and reduced-motion resets. Port to reusable `kira-ui` first, not `kira-shell`.
- `python3 scripts/check_widget_parity.py` is required in CI. Never claim “identical” or mark a widget `verified` without independently recorded contract tests, matched visual captures and paired motion frame sequences. The check prevents unsupported claims; it is **not** a screenshot validator.
- The current ledger says **0 of 15 verified**. Keep failed/missing widgets explicit, especially Calendar/SkyMap, SystemCharts, Tray, Notifications and WindowControls. Do not substitute a generic fade or GtkCalendar for the original TENET content.

## One native presentation stack

- Built-in views and future trusted widget views use **Rust + GTK4**. Prefer GTK widgets, CSS and accessible GObject properties. Use GDK/GSK or GtkDrawingArea only when stock widgets cannot express the design.
- **No WebKitGTK, Svelte, Vite, Tailwind, JavaScript, GSAP, ECharts or web widget pool.** Do not restore them as a shortcut, temporary renderer, test harness or second UI framework.
- Reusable kira-ui components, not kira-shell ad-hoc widgets, own GTK4 + gtk4-layer-shell native Wayland panels and transient overlays; regular windows remain Niri-managed xdg-toplevels. Keep 42px Top Bar exclusive zone and prevent overlays reserving extra space.
- Core owns Niri integration and KDL validation. Shell owns presentation, focus and lifecycle, and calls typed, bounded Core/GIO adapters. Never expose arbitrary shell commands or broad filesystem writes to widget code.
- Never block GTK's main context with socket IO, process enumeration or slow metrics. Workers exchange owned data; only GTK main-context code touches GTK objects.
- The single visible transient popup rule remains. Detached ordinary windows are independent and must survive switching the transient popup. Recreating GTK state during a reparent is not acceptable if state preservation is a requirement.
- Output mutations remain preview-only. An independent `kira-shell --display-supervisor` now owns the Apply/Confirm/Save transaction and its rollback deadline, but real nested Niri SIGKILL, hotplug and rollback acceptance are still required before any production mutation. Never mistake process separation or green CI for verified crash safety or feature parity.
- Preserve the observed/config/managed/session ownership boundary in [docs/CONFIG_OWNERSHIP.md](docs/CONFIG_OWNERSHIP.md). Apply, Confirm and Save are different operations; invalid or externally owned configuration fails closed.

## Visual and motion contracts

- Read [docs/DESIGN_SYSTEM.md](docs/DESIGN_SYSTEM.md) and [docs/TENET_MOTION.md](docs/TENET_MOTION.md) before changing UI, themes or animation.
- Use only canonical Kira colors (#000000, #14213d, #fca311, #e5e5e5, #ffffff) for the built-in Kira Default theme. Preserve contrast, visible focus, keyboard navigation, accessible names and real-time data correctness.
- TENET is about causal consistency, continuity, reversible trajectories and temporal memory, **not** a JS animation framework requirement. Stage GTK animation changes after basic functionality, and respect reduced motion.
- Do not resurrect the rejected hover distortion/lens experiment.

## Quality gates

- `cargo fmt --all -- --check`; `cargo clippy --locked --workspace --all-targets -- -D warnings`; `cargo test --locked --workspace --all-targets`. A shell test run with 0 tests is insufficient: test `kira-ui` and `kira-services` models.
- `task check`, `task shell:check` and `nix build .#default` where native dependencies and Nix are available.
- **Two-tier CI:** ordinary push/PR CI is capped at two runner minutes and checks Rust formatting, Core/IPC Clippy/tests, Python safety tests and pinned widget contracts. A fast green run does **not** imply any GTK/Nix/Wayland validation. For every native UI change, execute the manual **Full native GTK/Nix/Wayland validation** workflow (`.github/workflows/graphical.yml`) on the exact target SHA before shipping or claiming tested UI; it retains all-target Rust Clippy/tests, the distributable package, fake-Niri rollback, SNI/Notifications and real nested Niri pixels. If full validation was not run, report that explicitly. Job timeouts bound execution, not GitHub queue time.
- Manual nested Niri validation for multi-output, focus, keyboard input, fractional scaling, pop-up lifecycle and all motion. CI is not proof of native graphical correctness.
- Missing parity must be named in the migration plan and pinned ledger, not faked or hidden. No browser-related test requirements apply after browser components are removed.
