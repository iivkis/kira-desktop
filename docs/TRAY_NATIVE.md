# Native GTK4 System Tray · implementation and acceptance

**Status: partial, not a visual or motion acceptance certificate.** Frozen
`main` baseline: `305139949f49176487ef9759fc32c228eadff408`,
especially `docs/TRAY_DESIGN.md`, `TrayBar.svelte`, `TrayWidget.svelte`,
`tray-model.ts`, `crates/kira-shell/src/tray.rs`.

## Architecture and real data

- `kira-ui::TrayUi` renders two reusable GTK4 widgets from one typed
  `TrayState`: a compact 42px-bar strip with the frozen main's 2/4/6 pinned
  icon budget at widths below 700, 700–1099 and 1100+ logical pixels, plus an
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
  `TrayIntent::MenuClick(handle, leaf_id)`,
  `TrayIntent::MenuExpand(handle, parent_id)`, `TrayIntent::MenuBack`
  and `TrayIntent::DismissMenu` from the UI. Menu leaf/parent intents are
  permitted only when a globally unique enabled item is on an **actually
  visible root/ancestor cascade page** of the live owner's menu. Hidden branch IDs cannot fire.
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
invalidate outstanding menu operations. An exact unique-owner/menu-path
`LayoutUpdated(version,parent_id)` signal triggers generation-guarded
root rereads only when the revision is newer and the parent ID is known.
A `children-display=submenu` entry is actionable even before its child
nodes load: the trusted Shell validates a unique enabled parent, then calls
`AboutToShow(parent_id)` followed by bounded `GetLayout(0)`. Deeper
than three levels remain unsupported. **Native floating GTK4 cascades now exist** in the one retained popup:
the bounded preorder tree projects the root and every valid ancestor page
into real side-anchored non-modal `GtkPopover` children. Unique enabled
parent IDs form a maximum three-level path; a lazy page is empty until
validated `AboutToShow/GetLayout` returns. `MenuBack` pops one level; the
root Back dismisses the menu to show pinned/hidden items. Escape
in the embedded menu shares that back behavior. An app deleting a parent
during `LayoutUpdated` automatically returns to the deepest surviving
valid ancestor. Closing the popup invalidates outstanding async replies.
IconPixmap is supported with a bounded, renderer-local RGBA GDK texture;
OverlayIcon composition and multi-resolution selection beyond the nearest
24px frame remain missing.
GTK rows now retain their real `ReflowRow` identities across SNI updates,
capture painted positions and run a source 340ms `back.out(1.14)` FLIP for
up to 24 previously rendered rows. Reduced-motion and unmapped popups
settle immediately. The bar now samples its mapped GTK width and applies the pinned source 2/4/6
icon budget; overflow reflects only real SNI identities and attention.
Dynamic available-space measurement beyond the source's width thresholds,
cross-surface icon transfer, pixel comparisons and source typography remain
outstanding.

## Retained detachable Tray and accessible native menus

The same retained GTK Tray widget and live owner-verified SNI menu can now move
between the pooled layer-shell popup and a decorated Niri-managed xdg-toplevel.
The Top Bar's overflow button raises that existing detached window without
dismissing its menu. The Tray's own × has a distinct close handler: it dismisses
the menu and returns the GTK tree to the pool rather than re-raising it.
Window-manager close, Attach and Escape keep the retained view model.

Native bounded cascading menu pages support Up/Down, Home/End, Right for an
enabled submenu, and Left/Escape for returning to validated ancestors. A
LayoutUpdated refresh preserves focus by the same enabled menu ID if still
available, otherwise the first enabled action. Delayed focus restoration is
guarded by GTK mapping and active-window checks to avoid stealing focus.
The Shell still authorizes each numeric handle against its current D-Bus
unique owner and menu session. UI never chooses an arbitrary D-Bus path.

Floating child popovers have been implemented but still need nested Niri focus,
keyboard and hit-region checks, D-Bus owner churn acceptance, responsive
bar geometry under fractional scaling, cross-surface icon motion,
pixel comparisons and source typography. Headless tests do not certify parity.

The acceptance-only command `python3 scripts/accept_nested.py --output
/tmp/new-sni-evidence` launches a disposable GIO StatusNotifierItem on a
fresh `dbus-run-session` with a locked-down temporary config; its real ARGB32 `IconPixmap`, status and
NewStatus are checked against the native watcher's live properties and
captured by nested `grim`. The additional executable is never autostarted.
Ordinary preview still cannot own the user's watcher; a private-bus
attestation is mandatory for acceptance-only ownership.

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

## Unreviewed GTK cascading menu increment

The GTK implementation retains one Tray controller and one owner-scoped D-Bus
menu session. It projects each validated parent path into its own anchored
`GtkPopover` using a BoxLayout parent, chooses preferred Right positioning
(with GTK edge flipping), keeps ancestors pointer-accessible, and unparents
old child popovers deepest-first before replacing a menu layout. The Shell
checks the current owner/menu pair, globally unique enabled IDs and membership
in the visible ancestor pages before dispatching a leaf or expanding a sibling.
The dedicated `kira-ui` tests cover orphan ancestors, duplicate IDs,
disabled entries, sibling switching and visible ancestor leaf dispatch.

**Still unaccepted:** genuine GTK Wayland child-surface placement, focus
recovery after `LayoutUpdated`, Escape at every depth, detach/reattach,
fractional scaling and clip boundaries, owner loss while a child is mapped,
source TENET frame motion and paired frozen-main screenshots. Those must be
verified in the existing private-bus nested-Niri harness, never by taking
ownership of the user's session bus. This increment does not change the
0/15 verified parity status.

## Extended private-bus protocol smoke

The separately executable `kira-sni-acceptance` fixture now advertises an
actual object-path-typed SNI `Menu` property and registers a real
`com.canonical.dbusmenu` object on the **same unique D-Bus owner**. Its
bounded root contains an enabled action, a lazy `children-display=submenu`
branch with a nested leaf, a disabled entry, a checked entry and a
separator. A real `AboutToShow(20)` populates the lazy branch and emits
`LayoutUpdated`; real fixed `Event(clicked)` calls are logged and the
checkbox changes its typed layout revision. The CI's isolated
`smoke_sni_fixture_bus.py` probes these methods with `gdbus`. This
tests private-bus transport and fixture behavior, not Kira's rendered
GTK child-surface interaction with a real StatusNotifierHost.

The fixture refuses a non-attested or ordinary user D-Bus bus and is
never started with the production Shell. It cannot mutate outputs, KDL,
host app menus or create a fabricated desktop screenshot.

### Negative-action and idempotency protocol probes

The private GIO `com.canonical.dbusmenu` smoke now performs two
`AboutToShow(20)` calls and requires the second to report no change;
sends real fixed `Event(clicked)` messages for a disabled entry and an
unknown numeric ID and checks that neither appears in the fixture's
action log; and clicks the checkbox twice, verifying two logged
transactions and distinct successive typed layouts. These are headless
private-bus **fixture/protocol** checks, not a substitute for testing
the Shell's owner-scoped event gate with actual native GTK menu controls.
