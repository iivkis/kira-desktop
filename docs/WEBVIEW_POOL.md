# WebView Pool & Scheduler · Kira Desktop

**Status:** native architecture contract, 2026-11. The permanent Top Bar WebView is **not** a pool member. The trusted Calendar, System Monitor and Launcher share a *pool of popup hosts*, not a shared active WebView.

## Rationale

A single active popup host forced Svelte to tear down one widget before the next could appear. This prevented concurrent widget windows and interrupted the previous widget's GSAP/TENET motion timeline. Conversely, launching one speculative WebView for every hypothetical widget wastes memory and renderer processes.

## Operating policy

- **Startup:** exactly **one** inert, self-contained, navigation-locked popup WebView is prewarmed. No Svelte widget mounts, metrics sampling, GIO catalog or ECharts instance is started until an explicit show request.
- **Acquire:** if the requested surface is already leased, use its existing view (toggle Calendar/System, refocus Launcher). Otherwise reuse the most recently released idle host; if none exists, construct **one** additional GTK/WebKit host on demand.
- **Bound:** maximum **four resident popup hosts** (independent of the Top Bar). Under exhaustion, return backpressure and **never steal an active view** or discard a playing animation.
- **Release:** close only the requested widget. Notify its own WebKit page with a generation-scoped `kira:overlay:hide`, unmount Svelte effects and stop its owned native pulse/process subscriptions, then keep the bare WebKit host reusable.
- **Polling:** a single GTK timer processes Launcher results and live System pulses for *all* currently leased hosts. It starts on the first lease and stops when no hosts remain active; a warm idle WebView has no per-window 30/250ms polling timers.
- **Rotation:** a single hot spare is retained. Surplus idle hosts have a **45-second grace** for quick reopen/animation chains; a 5-second GTK timer destroys only expired *idle* surplus. Actual WebKit windows are closed only on retirement, never on ordinary widget hide.
- **Per-window isolation:** each WebView has an independent `UserContentManager`, trusted IPC allowlist, Svelte lifecycle, generation counter, size-before-paint handshake, and native/GSAP visibility timeline. No stale per-host callback is allowed to affect a newer generation.
- **Placement:** Layout considers **only actively requested windows**, not warm/closed WebView leases. A lone Calendar or System Monitor uses the original top-right anchor (12px right, 25px top). A second non-Launcher popup uses the top-left anchor, leaving the centered Launcher lane clear; after a right popup closes, the remaining left popup safely unmaps and remaps at its original right anchor without unmounting Svelte. Side panels and centered Launcher resize to 2 or 3 non-overlapping columns as needed (for example, at 1264px, 408px each for three views). The layout is recomputed from current demand, never a 448px permanent system-only offset. On narrow outputs, columns shrink to avoid hiding one widget behind another; content remains scrollable.
- **Focus:** GTK focus notifications must **never** automatically close a popup. The Top Bar click that requests the next widget deactivates the existing window *before* a new lease is acquired; the old one must stay open. Only explicit close/Escape/widget actions release the lease. Launcher retains exclusive keyboard focus only when alone, switching to OnDemand while multiple windows coexist so it cannot starve Calendar/System of keyboard input.

## Deterministic scheduler

The pure Rust `WebviewScheduler` has no GTK objects or threads. It tracks `(slot_id, owner, idle_since)`, supplies distinct `Existing`, `Reused`, `Created` and `Full` decisions, and computes reclaimable slot IDs based solely on a supplied clock. `OverlayPool` alone owns the GTK windows and reconciles these decisions on the GTK main loop. Renderer crashes remain local to their individual host, and the original trusted HTML reloads without expanding the Core RPC allowlist.

Avoid proactively creating more than one idle WebView. Avoid arbitrary background memory watchers or heuristic predictive warming until measured need exists. Active animation continuity and actual widget function take precedence over LRU eviction.

## QA / evolution

Cover: cold startup, two and three concurrent surfaces, switching focus without unmount, repeated toggles, generation-scoped stale commands, size + mount + paint acknowledgements, native reduced motion, synthetic capacity exhaustion, closed-vs-idle renderer cleanup, and 45s idle expiry. Keep native compile/Rust unit tests, Python source contracts and browser smoke green. Verify a lone System Monitor's 12px right anchor with Calendar absent, prewarmed or closed; verify the LEFT/RIGHT/CENTER triple-column layout with all three open, including a 1264px output, and that closing the right popup returns the remaining left one to the right without an interrupted Svelte scene. Check 800px narrow layout and focus switching on real Niri/Wayland.

Future widget types should register through the same fixed-purpose allowlist and scheduler lease contract, not create ten per-widget pools. The cap is a policy constant, not an aspiration to keep four renderers alive at idle.
