# Native GTK4 System Tray · implementation and acceptance

**Status: partial, not a visual or motion acceptance certificate.** Frozen
`main` baseline: `305139949f49176487ef9759fc32c228eadff408`,
especially `docs/TRAY_DESIGN.md`, `TrayBar.svelte`, `TrayWidget.svelte`,
`tray-model.ts`, `crates/kira-shell/src/tray.rs`.

## Architecture and real data

- `kira-ui::TrayUi` renders two reusable GTK4 widgets from one typed
  `TrayState`: a compact 42px-bar strip with up to four pinned icons and an
  overflow button; and a pooled native popup with the complete current list,
  pin controls and primary/secondary actions. StatusNotifier `Passive`
  items are excluded. `NeedsAttention` is derived only from a live item.
- `crates/kira-shell/src/tray_host.rs` owns the GIO D-Bus watcher/host,
  reads `org.kde.StatusNotifierItem` properties, and publishes
  fully resolved live items. An item never appears before its first bounded
  successful `GetAll` reply. NewIcon/NewTitle/NewStatus and
  PropertiesChanged trigger revision-guarded refresh, and
  NameOwnerChanged invalidates stale owners.
- On the normal desktop Kira claims `org.kde.StatusNotifierWatcher`
  only with `DO_NOT_QUEUE`; when an external watcher exists Kira adopts
  its roster and registers its own `org.kde.StatusNotifierHost-PID`.
  Nested `KIRA_PREVIEW` never takes the watcher name; it may attach
  to an existing watcher without evicting it.
- The Shell accepts only typed `TrayIntent::Activate`,
  `TrayIntent::Secondary`, `TrayIntent::TogglePin`,
  `TrayIntent::MenuClick(handle, leaf_id)` and `TrayIntent::DismissMenu` from
  the UI. A menu click is accepted only when the saved unique-owner/path
  session still exists and the requested ID is a unique, enabled leaf.
  Methods target the verified **unique owner** of the registered SNI item,
  not a caller-controlled destination/path. Actions are fixed to
  `Activate(0,0)` or `ContextMenu(0,0)`, with fixed
  `com.canonical.dbusmenu` `AboutToShow/GetLayout/Event(clicked)` for
  verified menu objects. The UI cannot construct an arbitrary D-Bus method
  or command. Application IconName is limited to safe local GTK theme names;
  the 32-frame bounded `IconPixmap` decoder selects the real nearest-24px
  ARGB32 network-order frame, converts to RGBA and displays a GDK texture.
  Missing unsupported icons use a real title initial, not invented apps.
- Pin preference keys prefer the application's bounded `Id` and fall
  back to its registration. State is stored in
  `$XDG_STATE_HOME/kira-desktop/tray-pins.json`, capped to 128 keys
  and 64KiB. A failed atomic save does not optimistically change the UI.

## Known limitations and test requirements

This increment implements a bounded first-phase
`com.canonical.dbusmenu` tree inside the **same** pooled GTK popup:
up to 80 entries, three levels, checked/radio state, enabled/disabled
and separators. `AboutToShow` runs before `GetLayout`, leaf clicks send
only `Event(clicked)` for the saved verified session, and an async
`GetLayout` refresh retains the menu after a click. Invalid, stale or
out-of-order replies never replace the current menu; clients without a
valid menu layout fall back to `ContextMenu`. Escape and popup closure
invalidate outstanding menu operations. True lazy submenus, deeper trees
and external layout-update signal subscriptions remain outstanding.
IconPixmap is supported with a bounded, renderer-local RGBA GDK texture;
OverlayIcon composition and multi-resolution selection beyond the nearest
24px frame remain missing.
GTK list rows currently rebuild on updates; retained keyed TENET 340ms FLIP,
responsive bar icon limits, cross-surface icon transfer, popout and exact
source typography remain outstanding.

In isolated nested Niri, register test applications through local and
external SNI watchers. Check that registered real items appear and change
title/icon/attention on NewIcon, NewStatus and PropertiesChanged. Switch the
D-Bus unique owner while keeping the same well-known name and verify an
old handle never activates the new process. Unregister and check cleanup.
Verify the preview does not take another watcher's well-known bus name.
Pin/unpin across restart; test an unwritable state directory and 64 live
items without fabricated overflow. Validate Activate and ContextMenu using
an SNI app that actually implements them. Supply malformed ARGB32 frames,
128px frames and a real theme-less pixmap app; verify no fake theme icon
overrides an available pixmap. Open a real KDE/AppIndicator dbusmenu,
exercise disabled, radio and checkbox entries, malicious duplicate IDs,
no-op empty layout, out-of-order GetLayout, and unregistration mid-click. Close and reopen the pooled tray
popup while rapidly changing items; inspect keyboard Escape and window focus.

Capture paired frames and motion against the frozen main reference at
360×640, 800×600 and 1920×1080, with fractional scaling and reduced
motion. Neither `tray-bar` nor `tray-widget` may be marked verified
before those comparisons and the missing menu/motion contracts are resolved.
