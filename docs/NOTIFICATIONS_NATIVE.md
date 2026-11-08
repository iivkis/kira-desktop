# Native GTK4 Notifications · implementation and acceptance

**Status: partial; not a main→native-gtk parity certificate.** The source-of-truth
is frozen `main` at `305139949f49176487ef9759fc32c228eadff408`, especially
`docs/NOTIFICATIONS.md`, `NotificationHost.svelte` and
`NotificationCard.svelte`. There are no independent paired native/main
captures or TENET motion certifications.

## Ownership and architecture

- `crates/kira-services/src/notification_queue.rs` retains the already
  unit-tested source FIFO and paint-gated lifetime. Maximum 100 active
  events, maximum three visible cards, 1.5-second admission gap, 3-second
  minimum dwell and default 5-second dwell. Persistent critical or
  `expire_timeout=0` notices vacate toast slots but keep their logical IDs.
- `crates/kira-ui/src/notification_host.rs` is reusable and has no D-Bus,
  Niri RPC, shell commands or WebKit. It owns one transparent bottom-right
  GTK4 layer-shell surface, a native card and shaped input region. Sender,
  title, body and action labels are plain GTK label text, never markup.
  GTK emits only `NotificationIntent::Close(id)` or
  `NotificationIntent::Action(id, declared_key)` to the host callback.
- `crates/kira-shell/src/notifications.rs` is the single trusted GIO session
  D-Bus server for `org.freedesktop.Notifications`. It uses
  `DO_NOT_QUEUE`, leaves a live external daemon in place, validates payload
  size and declared actions, sends `NotificationClosed` and
  `ActionInvoked` only for real IDs, and never fabricates sample notices.
  Core-owned notification transport is future work.
- There is no WebKit, Svelte, GSAP or browser-owned notification server in
  this branch. The layer-shell window has **zero exclusive zone** and
  `KeyboardMode::OnDemand`. Opening a notification does not request focus.

## Render and geometry safety

The host is created and mapped invisibly with an empty Cairo input region.
It does not claim the Freedesktop bus name if GDK cannot shape pointer input,
if no real monitor geometry can fit a minimum 240px host, or if construction
of its GDK surface fails. The window height never exceeds half the logical
output height; card height is at most one third of the window height.
An active right-side Calendar/System/Settings/Tray pooled popup pauses and
requeues toasts rather than guessing their compositor overlap.

Each changed native snapshot clears input, stages at most three GTK cards and
waits for two **mapped** GTK frame ticks with positive allocation. Only
matching, still-current ordered IDs may then be acknowledged to the source
queue, made visible and assigned a shaped clickable region. Tick callbacks
from superseded snapshots are revision-guarded. GTK frame ticks are not,
by themselves, independently certified Wayland compositor presentation
timestamps; that timing difference from main's WebKit paint ACK still needs
Niri visual/manual acceptance. Empty/hidden regions remain click-through.

Only real queue deadlines schedule one-shot main-loop timeouts; there is no
permanent D-Bus/UI notification polling. Geometry/scale/hotplug and right-side
popup changes trigger resynchronization, but per-output placement is not yet
certified in a multi-monitor Niri session.

## Security and preview isolation

A `KIRA_PREVIEW` child compositor normally inherits the **parent user's
live session D-Bus**. Therefore notification ownership is disabled in preview
unless the user explicitly opts into `KIRA_NOTIFICATIONS_PREVIEW=1`. Only run
this override inside a genuinely separate `dbus-run-session`. Do **not** set
it on the host's normal user session bus. Never stop the existing notification
daemon or replace the user's active Niri configuration to test the feature.

The notification service permits only `Notify`, `CloseNotification`,
`GetCapabilities` and `GetServerInformation`, plus standard outbound
signals. The input payload is limited to 64KiB, bounded strings and up to
eight action pairs. A button may invoke only a currently visible action key
stored in the trusted queue, not an arbitrary D-Bus destination or method.
Successful GUI actions emit `ActionInvoked` and `NotificationClosed`
(reason 2), D-Bus closes use reason 3, and ordinary visual expiry uses
reason 1. Persistent visual expiry does **not** close the logical D-Bus ID.

## Acceptance, remaining gaps

In a dedicated nested Niri compositor and **isolated session D-Bus**, run
the native Shell and send four real `notify-send` messages rapidly. Use
`gdbus` or `busctl --user` to test `replaces_id`, same ID from another
sender, `CloseNotification`, action signals, critical and permanent notices,
100-event saturation, and a second notification daemon. Capture how a
Calendar/System/Settings popup blocks toast admission, then verify FIFO
resumes and that the background is not a transparent click interceptor.
Compare 360×640, 600×480 and 1920×1080 and multi-monitor/scale changes
with the pinned `main` implementation.

**Motion implementation:** the host now animates each genuinely new ID's
real GTK card from y=12px and opacity=0 to y=0 and opacity=1 over 180ms
with source `power2.out`. It runs after paint-gated region reveal and
does not delay or restart the queue's D-Bus dwell. Existing IDs do not
replay the entrance on unrelated changes. `MotionToken` cancels
superseded frame-clock callbacks and reduced motion settles immediately.

**Inverse dismissal:** when a previously painted ID leaves the queue,
the same real GTK frame is reparented to a non-targetable overlay and
moves backward along the identical 12px path, with inverse power2 timing
and alpha 1→0 over 180ms. Early dismissal starts from the card's
observed intermediate opacity/position rather than jumping to the fully
shown start. Removing the logical D-Bus ID and releasing its input region
are immediate, independent of this visual exit. A covered popup, changed
monitor geometry or hidden surface suppresses the departure instantly.
Superseded outgoing frames are cancelled and removed, with no ghost
notifications or new permanent polling.

**Still outstanding:** compositor-calibrated paint acknowledgment,
independent per-output placement, focus/a11y/collision QA and paired
frame/time/pixel evidence; no native movement is visually certified. Until these are reviewed, both
`notifications` and `notification-card` remain **partial**, with no
`verified` flag set.
