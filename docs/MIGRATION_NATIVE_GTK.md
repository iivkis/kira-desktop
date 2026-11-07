# WebKit → native GTK4 migration

**Decision:** replace WebKit/Svelte presentation with a single GTK4-native rendering stack. **Branch:** native-gtk, branched from main on 2026-11-07.

## Baseline audit

The old main branch is a Rust/GTK window host rendering Svelte 5 via WebKitGTK 6: Tailwind CSS, ECharts and GSAP build a self-contained HTML bundle. Shell creates WebViews for the bar, a capped popup pool and notification views. Rust Core/Tokio, kira-ipc, GIO adapters, Niri protocol, KDL configuration ownership, Nix and the five-color design system are substantial reusable foundations.

The former UI ownership contract ("every widget must be Svelte") conflicts with the new decision and is **withdrawn** in this branch. Keeping WebKit as an invisible fallback would defeat the move.

## Migration scope and removals

| Remove from target architecture | Replace with |
| --- | --- |
| WebKitGTK 6, WebContext, WebView, JS handlers | GTK4 widgets, GObject signals, typed Rust controllers |
| Svelte 5, TypeScript, Vite, Tailwind, HTML bundle | Native GTK UI modules and CSS |
| ECharts / GSAP | GTK rendering/GSK and GLib frame-clock animation |
| WebView pool, warm browser host, browser recovery | Demand-driven native windows and controller lifetime |
| DOM mount/unmount ACK, browser generation bridge | GTK widget lifetime, explicit controller state and bounded IPC |
| npm build and lockfile, frontend Nix package | Rust-only Crane/Nix package and GTK runtime dependencies |
| Browser smoke and WebKit QA | Native Niri graphical and Rust contract tests |

Do not remove Core/IPC protocols, native system adapters or declarative config ownership merely because their former presentation was browser-based.

## Gates

1. **G0 Architecture reset:** native contract and migration gaps documented; obsolete UI ownership rules withdrawn. No misleading WebKit-first README.
2. **G1 Native launchable shell:** a GTK4 Top Bar with 42px exclusive zone, real workspace subscription, safe action dispatch, native clock, Launcher/Calendar and visible offline state. Build Rust and Nix without WebKit/npm.
3. **G2 Feature recovery:** migrate system metrics, process monitor, settings, Niri outputs Apply/Confirm/Save, StatusNotifier tray and freedesktop notifications into native GTK. Each feature retains safety and owns one controller.
4. **G3 Window semantics:** one transient overlay, independent detached windows, widget-state preservation, interruption/cancellation, focus, multi-monitor geometry.
5. **G4 Visual fidelity:** canonical design tokens, native charts and TENET timelines, reduced motion, visual regression and scaling acceptance at 100/125/150/200%.
6. **G5 Shipping:** Nix build, packaged isolated nested Niri smoke, documented feature parity, accessibility and memory/perf comparison to main. No blanket release claim before these pass.

## Non-negotiable behavior

- Core IPC is not a generic execution bridge.
- No shell command input from UI. Keep bounded validated actions and real observed state.
- Fail closed on missing Niri, conflicts, stale generations, unsupported outputs, malformed messages.
- Persist output settings only after a separate Confirm and explicit Save, never by editing generated Nix configurations.
- Preserve 5s Core heartbeat / 15s read timeout and automatic reconnect.
- GTK must remain responsive to stalled Core, slow application discovery and metric sampling.
- Remove old code **when it has an identified native replacement or is explicitly no longer a shipping feature**. Missing features must stay listed, rather than being recreated deceptively.

## Current caveat

Creating a native bootstrap is **not equivalent** to porting all Svelte functionality. Tray, notification visuals, detached windows, rich calendar/sky atlas, detailed process monitor and display transaction UI are independent acceptance items. Track them before merging to main. The original source remains in main for comparison and careful extraction of native adapters.
