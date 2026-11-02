# WebView Pool & Scheduler · Kira Desktop

**Status:** native behavior contract, 2026-11. The permanent Top Bar WebView is not a pool member.

## Two distinct window modes

A **transient widget** is a trusted layer-shell overlay anchored below the Top Bar: Calendar/System/Settings on the right, Launcher centered. Settings uses a fixed 600px preferred popup width instead of the usual 420px. **Only one transient widget may be open at a time.** A Top Bar request for a different widget queues the target, asks the current widget to run its recorded TENET exit, and waits for the generation-matched `closed:<generation>` acknowledgement (or the native watchdog) before hiding, releasing its WebView lease, and opening the queued target. No side-by-side overlay columns or left-dock relocation are allowed. Requests during exit are last-wins; clicking the outgoing widget again cancels its reverse and clears the pending target.

An **ordinary window** is requested explicitly through the pop-out control in every widget header. The Shell creates a **decorated GtkApplicationWindow without layer-shell initialization**, making it an xdg-toplevel managed by Niri alongside other applications. The existing *live* WebView is reparented from the transient host into the regular window, without `load_html`, Svelte unmount, or a generation change. This preserves the selected date, active tab, search query, scroll position, animations and interactive state. The transient GTK surface disappears. The promoted WebView retains its lease and data subscriptions while the ordinary window is open. Closing the ordinary window performs the reversible TENET exit, unmounts Svelte, reparents WebView to its hidden popup host and releases the lease. Clicking its Top Bar icon raises the existing ordinary window instead of cloning the same widget.

The pool makes **multiple ordinary windows** possible without pretending layer-shell widgets are desktop application windows. Detached xdg-toplevels do **not** consume popup dock geometry or shrink the width of a future transient widget.

## Pool lifecycle

- **Startup:** exactly one inert warm host and one permanent Top Bar host. No idle Svelte widget DOM, telemetry graph, app catalog or per-window poll loop.
- **Acquire:** use an existing lease (including a promoted window); otherwise use the hottest idle WebView, else construct one on demand. The max is four pooled WebViews. Never evict an active host.
- **Idle:** a single hot spare is retained. Surplus idle WebViews are retired after 45 seconds. Regular windows are never retired while live.
- **Poll:** one 30ms shared GTK source services active Launcher/System leases; the slower metrics tick remains 250ms and stops entirely once all pooled leases are idle.
- **Isolation:** per-host content manager, locked-down native IPC, generation counters, stateful Svelte mount and renderer recovery. The native `detach:<generation>` command is allowlisted and rejected for stale generations, hidden windows and closing popups.
- **Motion:** GSAP plays/reverses the same TENET history at 1.44x, an additional 20% over the former 1.2x rate. Popup native fade is 111ms vs 133ms. The incoming widget never reveals until the outgoing popup has actually finished its exit. Reduced motion completes immediately. Native close watchdog is 920ms, with bounded WebKit animation fallback.
- **Placement:** one visible popup at 420px preferred width (Settings: 600px), capped by available monitor width and height. Normal Niri windows are resizable and independent. The pop-out button hides once promoted.
- **Renderer failure:** a failed WebView is hidden and safely reloaded, without widening the RPC allowlist.

## Settings / About System

The Top Bar gear opens a Settings surface through the dedicated `kiraSettings:toggle` native channel. Settings starts in widget mode with a fixed 600px width, 560px capped height and a section selector. Popping it out reuses the same live WebView and shows a 220px sidebar in a decorated, resizable Niri xdg-toplevel (initially 940×700). Section definitions are shared, with **About System** implemented; future categories are explicitly disabled instead of opening empty/fake pages. Native Shell delivers a generation-matched, validated `kira:settings:about` snapshot on mount. The data come from bounded local OS files, read-only and with null for unavailable information. No generic shell command, network call or expanded Core RPC is involved.

## Verification

Exercise Calendar → System → Launcher switching, an interrupted reverse, quick toggles, rapid multi-click last-wins, three promoted windows plus one transient popup, full pool backpressure, closing a promoted window, stale `detach`/size/close generations, reduced-motion paths and 45-second idle pruning. Confirm the same WebView/DOM survives pop-out by changing Calendar date or System tab before promoting. On Niri, confirm the regular window can be tiled, resized and focused like an application, and the next popup returns to its original anchor.
