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

For a reproducible first live protocol and frame check, use
`python3 scripts/accept_nested.py --output /tmp/new-notify-evidence` from
`nix develop`. The coordinator starts a fresh `dbus-run-session` *before*
nested Niri, owns neither the parent's daemon nor its watcher, sends real
Freedesktop `Notify`, replacement and close calls, and records timestamped
entry/exit/tray PNGs plus `scenario-manifest.json`. A missing compositor,
shaped input, private bus or server ownership fails the run, not passes it.
The output remains `unreviewed` and needs independent paired captures
against frozen `main`.

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

### Output-aware placement model (unreviewed)

The native Shell no longer computes toast geometry from GDK monitor index zero
while allowing the compositor to choose another output. `kira-ui` now tests a
pure selection policy: prefer the real mapped Top Bar output if it can fit the
source 240px minimum and half-output card geometry; otherwise use the first
other *real, safe* GDK output, or suppress rendering. A single toast host is
explicitly attached to that output's layer-shell monitor. Output geometry or
hotplug observations retarget the existing host, dropping old shaped input
and departure frames before the new output's paint acknowledgement. The
notification queue and D-Bus service are not recreated or preempted.

This is **not** all-output acceptance: actual multi-output Niri geometry,
host reanchor timing, focus and popup collisions, and physical/fractional
pixel evidence still require an isolated compositor and paired main review.

### Output-local collision gating (model-tested, not accepted)

An active **pooled right-side transient** on the mapped Top Bar output now
pauses toast presentation on that output, without assigning dwell or stealing
input. If the chosen safe notification output is a different real GDK monitor,
that unrelated transient does not suppress its cards. When the bar monitor
cannot be identified and a transient is active, the plan fails closed. A
**detached Niri-managed xdg-toplevel** is not a pooled popup: merely keeping
Calendar, System, Settings or Tray detached must not starve the notification
FIFO. The Shell uses `PopupHost::is_transient_active` rather than counting
all detached windows as occlusions. Pure regression tests distinguish same,
different, unknown and absent monitor/overlay states.

Unresolved: compositor-visible window occlusion by *detached* windows,
per-output focus and hit-region mapping across hotplug, multiple simultaneous
output hosts (currently one selected target), physical scaling, and paired
main/native screenshots. No acceptance flags are changed by this model fix.

### Retained per-output GTK host pool (implementation, not graphical acceptance)

`kira-ui::NotificationHostSet` now owns up to eight **real GDK
monitor-bound GTK layer-shell windows**, created on demand. It retains the
current monitor's existing shaped-input surface, clears the previous
surface before switching destinations and closes detached monitor windows on
hotplug. One trusted queue and one Freedesktop D-Bus name still own all IDs;
**exactly one selected output is permitted to render any given FIFO
snapshot**. This is not broadcast-to-all-monitors and does not duplicate
action buttons or desktop notifications.

The routing model tests a mask of real pooled-transient occlusions. A
right-docked Calendar/System/Settings/Tray transient blocks its real bar
output; with another safe GDK monitor present, the active host moves to
that output. Unknown bar monitor with an active pooled transient fails
closed. Detached Niri-managed toplevels do not falsely suppress delivery.
Every output change invalidates the previous action/paint callbacks and
requeues the FIFO's visible entries, resetting their dwell until a matching
new mapped frame is acknowledged. Hosts have a hard retained-window cap
of eight and old monitor input regions are destroyed on disconnection.

**Unverified:** real Niri Wayland target mapping, focused output policy,
physical occlusion by detached windows, fractional scale/hotplug while
a card is pressed, plus simultaneous independent per-output notification
queues. No visual/TENET/pixel flags are set by this work.

### Stable output affinity and revocable action/paint leases (unreviewed)

A moving or temporarily blocked Top Bar no longer causes the active
notification to hop between arbitrary GDK enumeration indices. The reusable
`toast_output_plan_stable` policy tries the real bar monitor, then the
previous still-usable **GDK object**, then another usable output. Blockers
are computed only from actual pooled right-side transients. Unknown popup
output affinity remains fail-closed; detached Niri-managed windows are not
mistaken for pooled transients.

A scale-factor change or a logical output resize on the **same** monitor
now invalidates its old shaped-input region, its animation/card snapshot and
its paint lease. `NotificationHostSet::select` reports `Changed` for
in-place geometry/scale changes, and Shell requeues the visible FIFO entries
until the new mapped GTK frames are acknowledged. A GDK monitor reorder
alone does not force a reroute while the previous safe fallback still exists.

Each GTK card now carries an epoch-bound action callback. A queued button
activation from an old card (even when the same host is later re-selected or
the same Freedesktop ID is reused) cannot invoke a new notification. The
Shell also requires current bus ownership, correct current physical output
and a **painted active ID** before accepting any on-screen Close/Action or
paint ACK. Programmatic D-Bus `CloseNotification` remains independent,
as required by the specification.

CI tests cover order changes, blocked outputs, in-place geometry and scale,
lease invalidation, and action authorization. They **do not** demonstrate
fractional scaling, click-through, focus, hotplug or frame parity on a
real nested compositor. The single shared FIFO still selects only one
destination at a time, not independent simultaneous per-output queues.

### Unique-sender isolation for `replaces_id` (model/protocol contract)

The trusted Shell now records each accepted notification ID's **unique GIO
D-Bus peer name** in `kira-services::NotificationOwnership`. A live
`replaces_id` is honored only for the same unique sender; a foreign sender,
invalid/stale ID or zero receives a new server-generated ID. In particular,
an application cannot overwrite another application's notification content,
declared action keys or FIFO position by guessing its numeric ID. The
displayed `app_name` is not an authentication identity.

Only accepted queue admissions acquire a lease. Explicit close, GTK
action/close and normal expiry retire it; persistent cards parked beyond
visual dwell keep the lease for later valid same-sender replacement.
`CloseNotification(id)` remains the Freedesktop ID-based operation, so
existing separate-process notification clients can close an ID and still
receive the ordinary `NotificationClosed` reason.

The private nested-Niri acceptance scenario now attempts to replace one
`gdbus` subprocess's notice from a **different** subprocess, requiring
a different returned ID and cleaning up both IDs. Pure Rust tests cover
foreign/same-peer replacement, parked persistent leases, closed IDs and
saturation. Actual live Niri operation is **not** claimed without a
completed isolated acceptance run; 0/15 widgets remain verified.


### Freedesktop name-loss epoch (not live-verified)

On real name loss or explicit release, Shell now clears the bounded
waiting/visible/parked notification queue and sender-identity ledger,
invalidates any scheduled queue deadline and hides every shaped GTK
surface. A later successful name acquisition starts with a fresh
notification session rather than replaying old D-Bus IDs from the
previous owner epoch. This is covered by a Rust model regression; it
does not substitute for live D-Bus owner-churn tests or Wayland acceptance.



### D-Bus service epoch ID continuity and dual-sender acceptance

The Freedesktop queue now has `NotificationQueue::reset_session()`:
loss/release of its well-known name clears all active/waiting/parked and
history entries, sender ownership leases, and pending deadline revisions,
but **retains the monotonic notification ID cursor**. A delayed
`CloseNotification` from the previous name-ownership epoch can therefore
not close a new ID accidentally. Test cases cover active, waiting,
parked persistent, wrap-around, and no pending deadlines.

GIO can retain a registered D-Bus object at its unique connection name
even after losing `org.freedesktop.Notifications`. The Shell now rejects
all such stale requests unless it owns the well-known name **and** the
invoking D-Bus connection matches the current registered service bus.

A new independently packaged, **never autostarted** binary
`kira-notification-acceptance` constructs two distinct persistent GIO
connections solely inside the verified nested-preview private D-Bus.
It tests same-unique-sender `replaces_id` retaining the ID, foreign
sender rejection through a fresh ID, and stale closed-ID replacement
rejection with real calls to the actual Kira D-Bus service. It removes
all test IDs and writes fixed machine-readable proof flags. The
`scripts/acceptance_scenarios.py` manifest records these flags as
`unreviewed`; absence of the binary, failed attestation, missing proof,
or protocol failure aborts the acceptance run. This improves protocol
evidence, **not** graphics, focus or motion certification.


### G9: executable private-bus notification policy smoke in CI

`kira-services::notification_queue` now exports the atomic, GTK-free
`admit_notification`, `close_notification` and
`revoke_notification_session` policy operations. The real Shell uses
the same admission and close functions as the headless fixture, so
sender ownership, overloaded-queue rejection and lease publication
cannot silently diverge between the two implementations. Pure tests
cover trusted same-peer replacement, foreign replacement, rejected
admission, closed IDs and revocation.

A new installed **test-only** `kira-notification-policy-fixture`
exports typed Freedesktop Notifications methods on an independently
attested `dbus-run-session`, **without GTK/Wayland**. Its advertised
server identity is deliberately `Kira Policy Fixture`, not
`Kira Desktop`. The private `kira-notification-acceptance
--policy-fixture` client requires that exact identity and creates
two separate, persistent GIO peers. CI runs
`scripts/smoke_notification_policy_bus.py` after the Nix package
build and checks real D-Bus same-peer replacement, cross-peer
non-overwrite, closed-ID reuse rejection and cleanup. The normal
nested-Niri client mode still requires the **real** `Kira Desktop`
server identity, not the fixture.

The test-only fixture is neither a second production notification
daemon nor a substitute for running the native Shell. It must never
autostart; both fixture and probe refuse unisolated bus flags, and
the runner verifies the temporary owned socket using
`scripts/private_bus.py`. Success confirms the protocol ABI and
**shared Rust policy**, NOT rendered popup frames, Niri focus, output
hit regions or visual parity. Frozen source behaviour that differs
from sender-scoped replacement remains explicitly non-verified.


The Shell's GTK action/close path also uses the same atomic close
operation. An action emits `ActionInvoked` and the subsequent
`NotificationClosed` only **after** the still-active queue ID is
successfully closed and its sender lease retired. A duplicate/stale
callback cannot emit an app action or close another toast.
