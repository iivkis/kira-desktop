# Kira Desktop · System Tray

**Status:** first implementation in `feat/status-tray-foundation`. Native Niri session
acceptance and cross-surface drag motion are still to be verified.

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
  Svelte keyed row; the existing `tenetReflow` FLIP helper animates changes
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
| Click pinned icon | SNI `Activate(0, 0)`, or show its D-Bus menu if `ItemIsMenu` |
| Click row in popup | Same native activation |
| Right click a pinned Top Bar icon | Open its `com.canonical.dbusmenu` in the pooled tray popup, or fallback to `ContextMenu(0,0)` |
| Click pin control beside row | Move application identity between visible and hidden groups |
| Right click row | Open its D-Bus menu, or fallback to native `ContextMenu(0,0)` |
| Click a D-Bus menu leaf | Send only its validated `Event(id, clicked)` action |
| Escape inside app menu | Return to the tray list; Escape again closes the popup |
| Escape or close control | Reversibly dismiss popup |
| Quit/restart Shell | Restore pin choices from local state |
| StatusNotifierItem unregistered | Remove from live UI, preserving pin preference |

SNI `Passive` items are omitted. `NeedsAttention` items are marked with
an actual Orange point; no fabricated badge counts.

Pin state is stored in `$XDG_STATE_HOME/kira-desktop/tray-pins.json` (or
`~/.local/state/kira-desktop/tray-pins.json`). The Shell retains the
write authority. Svelte can only send bounded native numeric handles.

## Native architecture

1. `kira-shell/src/tray.rs`: one GIO session D-Bus service, a watcher only
   when the standard name is unowned, and a host when another watcher exists.
   The preview never takes ownership of the user's SNI watcher.
   A separate well-known `org.kde.StatusNotifierHost-PID` bus name identifies
   the actual host for third-party watchers.
2. The native Shell bridge and dedicated `kiraTray` channel leave arbitrary D-Bus
   names, methods and paths inaccessible to UI callers.
3. `calendar.rs` leases `Surface::Tray` through the existing native overlay
   pool, with generation-bound `tray-sync` and `tray` actions.
4. `tray_menu.rs` parses bounded `com.canonical.dbusmenu` layouts; only
   a unique, enabled leaf in the current menu snapshot can emit `clicked`.
   Submenu descendants are displayed as a compact indented list (up to three
   levels), not as independent GTK windows. All clicks are overlay-generation
   scoped.
5. `frontend/src/lib/tray-model.ts` checks all renderer-visible data.
6. `TrayBar.svelte` and `TrayWidget.svelte` display the **same live snapshot**.

The implementation limits active items to 64, sanitizes metadata, checks
registration ownership when a well-known service name is supplied, bounds
serialized icons, and uses an allowlist of trusted native actions.

## Known first-phase gaps

- The native host now converts bounded `IconPixmap` ARGB32 bytes into PNG;
  names from GTK icon themes take precedence. Unsupported images retain a
  textual fallback. OverlayIcon composition remains future work.
- `com.canonical.dbusmenu` is supported for bounded, flattened menu entries,
  checkbox/radio indicators and leaf clicks. Deep/lazily loaded submenus,
  live layout updates, full desktop-native nested popovers, and legacy XEmbed
  remain outside this increment. Apps without a Menu object use ContextMenu.
- `Activate(0,0)` and `ContextMenu(0,0)` are basic entry points; some
  applications require compositor-aware pointer coordinates.
- No draggable icon transfer across two WebViews yet. Explicit pin controls
  supply the same state change.
- Pinned icons beyond the width cap remain accessible in overflow; measuring
  the available panel width dynamically and Wayland visual/interaction
  acceptance remain required before merging.
- GitHub CI verifies compilation and tests. This branch has not yet received
  an interactive native Niri session acceptance pass.

## Acceptance checklist

- [ ] SNI item appears, changes title/icon/attention, then unregisters.
- [ ] Existing external watcher remains owned by its original service.
- [ ] Third-party watcher accepts the well-known Kira host registration.
- [ ] KDE and AppIndicator menus open, respond to click, and close safely;
      fall back to native ContextMenu for legacy applications.
- [ ] Preview does not seize the host's watcher.
- [ ] Pin/unpin survives restarting Shell and a re-registered app.
- [ ] Clicking a pinned icon runs the real app's `Activate` action.
- [ ] Tray popup opens underneath the 42px bar without expanding that zone.
- [ ] Repeated click, Escape, focus transfer, and `prefers-reduced-motion`
      all produce a settled true state with no retained WebView listeners.
- [ ] Keyboards and small displays retain access to every registered item.
- [ ] Verify native logs for invalid registrations and slow D-Bus services.

## Review hardening (2026-11-05)

- Do not show a registration placeholder before its real `GetAll` properties
  arrive. Only verified, bounded SNI snapshots are rendered.
- Refresh application data for legacy NewIcon/NewStatus and standard
  `PropertiesChanged`; ignore superseded asynchronous replies.
- Treat `NameOwnerChanged` with a new nonempty owner as a real ownership
  transition; never keep calling an old item's service after it is replaced.
- A changed external watcher invalidates pending roster reads, preventing a
  stale reply from restoring items from the previous host.
- Pin from a stable `Id` after property resolution. Fail closed if persistence
  fails rather than displaying a preference that cannot be saved.
- Respect `ItemIsMenu` for primary click; native context-menu behavior stays
  consistent between pinned icons and the overflow.
- Use the overflow badge for attention on clipped pinned icons too; never
  fabricate an unread count.

## Native app menu integration (2026-11-05)

- The StatusNotifierHost now owns its own well-known D-Bus name instead of
  trying to register only a transient session connection name.
- Right-click from the Top Bar explicitly opens the existing pooled tray popup
  only when the app advertises a canonical `Menu` object.
- Native `AboutToShow` then `GetLayout` requests are async, time-bounded,
  revision-guarded, and limited to 80 entries with a 256 KiB reply cap.
- Only the current overlay generation, the current live app handle and a
  unique enabled leaf from the saved menu may trigger a D-Bus `clicked` event.
  The frontend never receives an arbitrary D-Bus destination or method.
- Popup open is FORWARD; dismissal remains INVERTED under TENET. The embedded
  menu is a distinct interaction state within the same popup, without a
  second Wayland surface. A menu can be dismissed with Escape or its back
  control; reduced-motion returns immediate true state.
