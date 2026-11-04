# Kira Desktop: notifications (draft implementation)

**Status:** experimental implementation in PR #66. Not released; nested-Niri visual QA remains mandatory.

## Contract

- One lazy GTK4 / WebKitGTK toast host, separate from the four-slot widget pool.
- Anchor: bottom right; at most 50% of the **logical** output height.
- At most three cards; each card's actual height is at most one third of the host height.
- DOM order is oldest first, flex direction is reversed: oldest sits at the bottom, newer cards stack above.
- FIFO admission: when another card is visible, wait at least 1.5 seconds **after the last painted card** and respect the new event's own earliest admission time.
- Minimum exposure: **3 seconds after the Svelte renderer acknowledges a painted frame** (not after GTK allocation). Normal default: 5 seconds; a positive client timeout is clamped to 3–30 seconds.
- Per Freedesktop, `expire_timeout=0` and urgency=critical keep their **logical D-Bus ID** active until explicitly closed. Their **visual toast** still rotates out after its bounded dwell (normally 5 seconds) into a bounded in-memory parked set, without sending `NotificationClosed`. This keeps FIFO live even after three permanent notifications.
- Closing by the application (`CloseNotification`) or user is immediate, including for off-screen persistent IDs. Replacement of a waiting or visible ID retains position and visual dwell origin; replacement of an off-screen parked ID re-enters FIFO at the tail. Per protocol, any nonzero `replaces_id` is returned unchanged, even from a fresh D-Bus connection or when the original ID is no longer active.
- Full waiting + visible + logically-active parked queue is bounded to 100 events. New events beyond capacity return a D-Bus error without evicting older FIFO messages. The separate in-memory history is bounded to 100. Parked events have no UI center in this MVP, so non-expiring messages can currently be closed only through `CloseNotification` (by any session-bus client that knows the ID), or displayed again through `replaces_id`. A future notification center should expose these records.
- When a right-side Kira widget leaves insufficient space, or a detached widget's actual compositor rect is unavailable, hide the toast host and requeue without charging display dwell time.
- The input region is limited to **paint-acknowledged cards only**, never the whole transparent host or a newly admitted card that WebKit has not painted. Auto-show never requests keyboard focus; keyboard focus on click is OnDemand.
- First-start deadlock prevention: bootstrap the single WebKit host already mapped but **opacity=0 with an empty native input region**. The native host remains invisible and non-interactive until the Svelte frame ACK, then installs the acknowledged-card hit rectangles and reveals the page. Since WebKit can report a fully transparent but mapped page as `document.visibilityState="hidden"`, ACK readiness is based on Svelte DOM/layout (with a bounded rAF fallback), while native GTK alone decides whether the window is mapped and safe to show. After the stack empties the allocated WebView stays mapped at opacity=0 with an **empty** input region, preventing suspended/restarted DOM scripts and stale-card flashes when the next notification arrives. It does not take keyboard focus.
- Widget opening is treated as unknown geometry until native/Svelte paint is acknowledged; no toast may overlap an unmeasured transient widget. Clearance conservatively includes the Top Bar's 42px reserved height.
- Repeated GTK geometry callbacks do **not** regenerate an unchanged Svelte snapshot or restart pending paint acknowledgements.
- Tiny outputs that cannot fit a 240 logical-pixel card column keep events queued. Even the invisible bootstrap WebView is initially sized below the 50% output-height cap.
- GTK monitor geometry, scale-factor and output-list changes trigger layout re-evaluation, so hotplug and resolution changes do not leave a stale 50%-height window. When GTK has no valid current monitor, the host fails closed and queues events rather than guessing a fallback size.

## Transport and security

The draft uses GIO's existing session D-Bus support **inside kira-shell** to own `org.freedesktop.Notifications`. It does not steal an existing daemon's name. The Freedesktop API defines global integer IDs without sender-specific authorization: `replaces_id` and `CloseNotification` work across D-Bus connections (including separate `notify-send` processes). This means the API cannot guarantee that applications are prevented from acting on IDs they know. The server advertises only `body` and `actions` capabilities. All strings are bounded and passed as escaped JSON data into the trusted, navigation-locked, single-file Svelte UI, not executable HTML or JavaScript. The renderer's native bridge is restricted to notification-specific commands. JavaScript can only acknowledge paint, close an ID or invoke one of its declared action keys. The action invokes the D-Bus `ActionInvoked` signal and closes the notice.

This placement of the D-Bus service in Shell is a deliberate **MVP architecture limitation**: long-term Kira Core should own the notification transport/lifetime and Shell should only own GTK/WebKit geometry. Notifications stop being received when Shell exits. The session history is not yet surfaced as a notification center, and the Top Bar does not yet display unread counts.

A nested `KIRA_PREVIEW` session disables D-Bus ownership by default to avoid taking over the host user's live session bus. For isolated visual testing, start the whole preview **inside a separate `dbus-run-session`** and explicitly set `KIRA_NOTIFICATIONS_PREVIEW=1`. Do not enable this override on the host session bus.

## Manual test matrix before merge

1. Inside isolated nested Niri, send 4 notifications with `notify-send` at once. Verify bottom-up order, at most three cards, no earlier than 1.5-second gaps, and FIFO after the first expires.
2. Verify minimum 3-second *painted* dwell with slow WebKit startup, hidden/covered WebView and renderer restart. Check for stale-card flashes on re-entry and confirm that the native input region grows only after each new card is paint-acknowledged.
3. Close and replace an active notification by D-Bus ID **from different `notify-send` processes**. For nonzero `replaces_id`, return that same ID. After expiry/closure, it must create a fresh active notification using the requested ID; newly allocated IDs still remain unique.
4. Send four `expire_timeout=0`/critical notifications: verify the first three rotate out of the toast after at least 3 seconds, the fourth enters FIFO, the parked IDs remain logically active without `NotificationClosed`, and a subsequent `CloseNotification` for a parked ID succeeds.
5. Click the default action or a named action; verify one `ActionInvoked` and one `NotificationClosed`. Check keyboard navigation after explicit click.
6. Open Calendar, System Monitor, Settings and Launcher while toasts are visible; test the fallback when insufficient space remains, and repeat with detached Niri windows.
7. Capture 360×640, 600×480 and 1920×1080 screens at scale 100%, 125%, 150%, 200%. Confirm actual input-region click-through, shadows, alignment and zero overlap.
8. Test a second monitor, unplug, rotate, fullscreen and lock screen. Only claim compatibility actually observed on supported Niri/GTK versions.
9. Test Firefox/Chromium, Discord and Telegram with their own notification permissions enabled. Check that a second notification daemon is never preempted.
10. Measure WebKit PSS, check native/Core renderer recovery and confirm no window/lease leaks after repeated notifications.

## Automated checks

Run `task check`, `nix develop .#ci --command cargo check --locked -p kira-shell --all-targets`, `nix develop .#ci --command cargo test --locked -p kira-shell --bin kira-shell`, and `task frontend:build`. GitHub CI checks compilation and model tests but does not replace nested-Niri graphic acceptance.
