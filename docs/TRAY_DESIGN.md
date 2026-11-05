# Kira Desktop · System Tray

**Status:** first implementation in \`feat/status-tray-foundation\`. Native Niri session
acceptance, icon pixmaps, and cross-surface drag motion are still to be verified.

## Design philosophy: calm in repose, precise in motion

Kira's official style remains **Neo-Brutalism + Editorial Design + Expressive
Minimalism**, not a second design language for the tray. The system tray is a
functional location for applications that register a notification-area item.
It is **not** a copy of Niri's minimized-window/task switcher.

- **Truth over decoration:** no invented running apps, unread counts, or static
  example icons. An absent SNI host or client is shown honestly.
- **One identity per item:** a stable application ID is the pinning identity;
  a transient numeric handle is only valid within this Shell process.
- **Material hierarchy:** the 42px bar stays compact, in the system-utilities
  segment. The tray popup occupies its own zero-exclusive-zone Wayland surface.
- **Functional accent:** canonical Black, Prussian Blue, Orange, Alabaster
  Grey, and White only. Orange is for real attention and selected pin state.
- **KISS and DRY:** GIO owns the external system protocol, Svelte renders;
  the existing popup pool, typed bridge, theme and GSAP remain shared.

## Motion philosophy: TENET without decorative fiction

The mandatory [TENET Motion System](TENET_MOTION.md) applies:

- **FORWARD:** focus, hover, click acknowledgments, and a short icon reorder.
  The actual D-Bus action is never delayed for an animation.
- **Temporal continuity:** a pin toggle retains the same item identity and
  Svelte keyed row; the existing \`tenetReflow\` FLIP helper animates changes
  inside the native popup. GSAP owns JS motion, never a second engine.
- **INVERTED:** popup dismissal reverses the same saved opening scene.
  Closing, Escape, another widget, or repeated clicks must settle correctly.
- **Cross-surface transfer:** a future coordinated native/JS geometry relay
  should animate a *single* icon trajectory between popup and top bar.
  The first implementation keeps state synchronized but does not claim a
  pixel-perfect shared-element animation across two distinct WebViews.
- **No obstruction:** reduced-motion immediately displays true state,
  and animation never blocks keyboard access or D-Bus dispatch.

## Interaction contract

| Action | Result |
| --- | --- |
| Click chevron in Top Bar | Toggle native tray popup |
| Click pinned icon | SNI \`Activate(0, 0)\` on its registered D-Bus object |
| Click row in popup | Same native activation |
| Click pin control beside row | Move application identity between visible and hidden groups |
| Right click row | SNI \`ContextMenu(0, 0)\` request |
| Escape or close control | Reversibly dismiss popup |
| Quit/restart Shell | Restore pin choices from local state |
| StatusNotifierItem unregistered | Remove from live UI, preserving pin preference |

SNI \`Passive\` items are omitted. \`NeedsAttention\` items are marked with
an actual Orange point; no fabricated badge counts.

Pin state is stored in \`$XDG_STATE_HOME/kira-desktop/tray-pins.json\` (or
\`~/.local/state/kira-desktop/tray-pins.json\`). The Shell retains the
write authority. Svelte can only send bounded native numeric handles.

## Native architecture

1. \`kira-shell/src/tray.rs\`: one GIO session D-Bus service, a watcher only
   when the standard name is unowned, and a host when another watcher exists.
   The preview never takes ownership of the user's SNI watcher.
2. \`ShellDelivery\` and the dedicated \`kiraTray\` channel leave arbitrary D-Bus
   names, methods and paths inaccessible to UI callers.
3. \`calendar.rs\` leases \`Surface::Tray\` through the existing native overlay
   pool, with generation-bound \`tray-sync\` and \`tray\` actions.
4. \`frontend/src/lib/tray-model.ts\` checks all renderer-visible data.
5. \`TrayBar.svelte\` and \`TrayWidget.svelte\` display the **same live snapshot**.

The implementation limits active items to 64, sanitizes metadata, checks
registration ownership when a well-known service name is supplied, bounds
serialized icons, and uses an allowlist of trusted native actions.

## Known first-phase gaps

- No \`IconPixmap\` conversion yet; only safe GTK theme icons are encoded,
  with textual fallback if no usable icon exists.
- No \`com.canonical.dbusmenu\` rendering or legacy XEmbed support.
  Right-click asks the item to show its native ContextMenu.
- \`Activate(0,0)\` and \`ContextMenu(0,0)\` are basic entry points; some
  applications require compositor-aware pointer coordinates.
- No draggable icon transfer across two WebViews yet. Explicit pin controls
  supply the same state change.
- Dynamic narrow-screen icon overflow and Wayland visual/interaction
  acceptance remain required before merging.
- This branch has not been built in a NixOS/Niri desktop in the current
  session. Run the frontend and Rust checks before merging.

## Acceptance checklist

- [ ] SNI item appears, changes title/icon/attention, then unregisters.
- [ ] Existing external watcher remains owned by its original service.
- [ ] Preview does not seize the host's watcher.
- [ ] Pin/unpin survives restarting Shell and a re-registered app.
- [ ] Clicking a pinned icon runs the real app's \`Activate\` action.
- [ ] Tray popup opens underneath the 42px bar without expanding that zone.
- [ ] Repeated click, Escape, focus transfer, and \`prefers-reduced-motion\`
      all produce a settled true state with no retained WebView listeners.
- [ ] Keyboards and small displays retain access to every registered item.
- [ ] Verify native logs for invalid registrations and slow D-Bus services.
