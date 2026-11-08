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

## Mandatory build order

**Phase 1: kira-ui first.** Specify and implement reusable GTK4 components and typed view models, callbacks, CSS, accessibility and tests. Desktop-specific widget trees must never precede these components. This library must work without Core or Niri.

**Phase 2: trusted services.** Recover renderer-independent Rust models and adapters from `main` in `kira-services` or Core; retain their regression tests. Display management, config ownership, notification queue and system metrics are restored as modules, but are not yet all wired to visible UI.

**Phase 3: desktop composition.** Wire tested components to typed Core / service data in `kira-shell`. Verify Nix, focus, geometry, keyboard and Niri behavior.

## Strict main widget parity

See [MAIN_WIDGET_PARITY.md](MAIN_WIDGET_PARITY.md) and the [pinned JSON ledger](MAIN_WIDGET_PARITY.json): at this stage **0 of 15 built-in surfaces meet all content, interaction, animation and pixel-parity gates**. Workspace windowing and Launcher search now port model behavior and more of main's content to reusable GTK; they still lack the original TENET scenes and measured graphical equality. Never describe the migration as widget-identical based on Cargo, CSS palette or static screenshot alone. Every component needs paired screenshots and frame-sequence evidence against frozen main.

## Gates

1. **G0 Architecture reset:** native contract and migration gaps documented; obsolete UI ownership rules withdrawn. No misleading WebKit-first README.
2. **G1 Reusable UI foundation:** `kira-ui` public APIs, shared theme, stable state and independent tests, built before Desktop.
3. **G1.5 Native launchable shell:** a GTK4 Top Bar with 42px exclusive zone, real workspace subscription, safe action dispatch, native clock, Launcher/Calendar and visible offline state. Build Rust and Nix without WebKit/npm.
4. **G2 Feature recovery:** migrate system metrics, process monitor, settings, Niri outputs Apply/Confirm/Save, StatusNotifier tray and freedesktop notifications into native GTK. Each feature retains safety and owns one controller.
5. **G3 Window semantics:** one transient overlay, independent detached windows, widget-state preservation, interruption/cancellation, focus, multi-monitor geometry.
6. **G4 Visual fidelity:** canonical design tokens, native charts and TENET timelines, reduced motion, visual regression and scaling acceptance at 100/125/150/200%.
7. **G5 Shipping:** Nix build, packaged isolated nested Niri smoke, documented feature parity, accessibility and memory/perf comparison to main. No blanket release claim before these pass.

## Non-negotiable behavior

- Core IPC is not a generic execution bridge.
- No shell command input from UI. Keep bounded validated actions and real observed state.
- Fail closed on missing Niri, conflicts, stale generations, unsupported outputs, malformed messages.
- Persist output settings only after a separate Confirm and explicit Save, never by editing generated Nix configurations.
- Preserve 5s Core heartbeat / 15s read timeout and automatic reconnect.
- GTK must remain responsive to stalled Core, slow application discovery and metric sampling.
- Remove old code **when it has an identified native replacement or is explicitly no longer a shipping feature**. Missing features must stay listed, rather than being recreated deceptively.

## Calendar content recovery and native temporal groundwork

- Replaced stock GTK Calendar with reusable `kira-ui::Calendar`: faithful six-row Monday-first eight-column grid with ISO week numbers, adjacent-month dates, live clock, local day labels, month/year navigation and ISO clipboard intent.
- Pure `calendar_model` tests replicate main's Gregorian/ISO edge cases, year 1..9999 bounds, DST-safe civil arithmetic and selection identity.
- `kira-ui::motion` adds reusable interruptible frame-clock timing, reduced-motion handling and a native GtkDrawingArea replay of main's four deterministic inverted calendar fragment paths. The full GSAP spring/stagger/clock-digit and popup master scene is **not** yet equivalent or graphically validated.
- The detached-only offline SkyMap now consumes the pinned Hipparcos/XHIP stars and d3-celestial line figures, through pure tested `sky_model` and reusable GTK/Cairo `SkyMap`. A responsive FlowBox shows it beside the original date rail in the native ~1040×720 Calendar toplevel and hides it in the compact popup. It shares the civil-date selection, requires explicit observer coordinates and local time, and animates interrupted sidereal movement on the GTK frame clock. Real nested-Niri visibility, high-DPI allocations, pixel comparison and matched motion sequences remain G3/G4 acceptance items. See [CALENDAR_NATIVE_PARITY.md](CALENDAR_NATIVE_PARITY.md) and [SKY_MAP.md](SKY_MAP.md).

## G2/G3 increment: source-backed monitoring interactions

- `kira-ui::ProcessTable` now follows the frozen main's five sortable process columns with reversible direction, real PID and Linux process state, stable unknown-last comparisons, 36-row progressive disclosure capped at 180 and separate eight-second freshness invalidation. A new tab session drops old rows, and Shell only accepts worker process messages while the System Processes tab is genuinely active.
- `kira-ui::SystemReport` adds the source's Overview copy action, sourced only from the latest measured `SystemOverview`, expiring after six seconds and on GTK unmap. CPU, temperature, RAM, SWAP, disk, load, uptime, battery and up to eight real GPU observations use a bounded, newline-sanitized report with explicit missingness; only Shell invokes the GDK clipboard. The same callback continues to serve the separately bounded process report.
- Source-equivalent process FLIP trajectories, selected-sort indicator animation, 180px/scale layout and independent nested-Niri comparisons are not yet verified. `System Monitor` remains partial in the evidence ledger.

## G2 increment: native monitoring and displays

- **kira-ui first:** reusable `SystemMonitor` and `DisplaySettings` receive only typed view models and emit explicit intents. UI models have isolated tests for missing values, ordering/bounds and display confirmation timing.
- **Trusted adapters:** `kira-services` supplies CPU/memory/GPU metrics, bounded `/proc` process sampling, and the previously recovered Niri output worker with atomic configuration ownership, explicit Confirm and a 15-second watchdog rollback.
- **Desktop composition:** `kira-shell` samples metrics only while the System popup is visible, enumerates processes only on the Processes page, and routes display choices through a separate `kira-shell --display-supervisor` process. The bounded JSON-lines protocol carries typed display intentions and responses. The worker owns the 15-second deadline and survives Shell-group termination. Closing Settings requests an immediate rollback; stdin EOF also triggers rollback. Nix CI checks the out-of-process worker against a fake Niri command endpoint, including a real parent-process `SIGKILL` and rollback after loss of its command pipe. The fallback Shell-thread worker is read-only if the external supervisor cannot start. **Production mutations remain disabled** pending real nested Niri crash/rollback, hotplug and accessibility acceptance.
- **Still gated:** running the dedicated `task dev:preview:display-crash` real nested acceptance and recording its output (blocking production mutation), multi-monitor positioning, live output hotplug, failure/rollback under interrupted Niri, KDL/Nix ownership conflicts, scaling/accessibility and graphical acceptance. The feature is experimental until [ACCEPTANCE_NATIVE_G2.md](ACCEPTANCE_NATIVE_G2.md) passes.

## G3 increment: retained detachable native windows

- The reusable `kira-ui::PopupHost` supports detaching Launcher, Calendar, System and Settings into decorated Wayland xdg-toplevels, then reattaching the exact GTK clip tree to the layer-shell transient. No controller, selected date, search state, scroll position or metric model is recreated. The bar raises an existing detached view rather than spawning a duplicate.
- System sampling and Settings rollback decisions follow detached visibility; only the transient layer counts as notification obstruction. Tray stays in its single pooled popup until detached SNI menu cascades pass independently.
- Native popup placement now reads the mapped Top Bar's GDK monitor on every open, pins the transient to that output, and updates width, top margin and scroll bounds after output changes; when the bar has no mapped surface the compositor chooses the output. The retained GTK content tracks its last focused child and restores it after detaching or reopening only while the destination window is active. Detached Escape returns to the retained pool. Real Niri focus restoration, rapid attach/detach motion, hotplug, scaling, keyboard and accessibility QA and paired visual/motion evidence remain outstanding. Window Controls stays partial, not verified.

## Current caveat

Creating a native bootstrap is **not equivalent** to porting all Svelte functionality. Tray, notification visuals, detached windows, rich calendar/sky atlas, native chart parity, and *graphical acceptance* of the new process-monitor and display transaction UI remain separate acceptance items. Track them before merging to main. Recovered backend policies do not constitute feature parity. Multi-monitor popup positioning remains an acceptance item. The original source remains in main for comparison and careful extraction of native adapters.
