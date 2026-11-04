# Kira Desktop: notifications (draft implementation)

**Status:** experimental implementation in PR #66. Not released; nested-Niri visual QA remains mandatory.

## Contract

- One lazy GTK4 / WebKitGTK toast host, separate from the four-slot widget pool.
- Anchor: bottom right; at most 50% of the **logical** output height.
- At most three cards; each card's actual height is at most one third of the host height.
- DOM order is oldest first, flex direction is reversed: oldest sits at the bottom, newer cards stack above.
- FIFO admission: when another card is visible, wait at least 1.5 seconds **after the last painted card** and respect the new event's own earliest admission time.
- Minimum exposure: **3 seconds after the Svelte renderer acknowledges a painted frame** (not after GTK allocation). Normal default: 5 seconds; a positive client timeout is clamped to 3–30 seconds.
- Per Freedesktop, `expire_timeout=0` never automatically expires. Urgency=critical also stays visible until dismissed. Such cards consume one of three slots; this is intentional, not silent expiry.
- Closing by the application (`CloseNotification`) or user is immediate. Replacement retains position and visual dwell origin. A closed/expired ID is no longer replaceable.
- Full waiting + visible queue is bounded to 100 events. New events beyond capacity return a D-Bus error without evicting older FIFO messages. The separate in-memory history is bounded to 100.
- When a right-side Kira widget leaves insufficient space, or a detached widget's actual compositor rect is unavailable, hide the toast host and requeue without charging display dwell time.
- The input region is limited to the cards, not the whole transparent host. Auto-show never requests keyboard focus; keyboard focus on click is OnDemand.
- First-start deadlock prevention: bootstrap the single WebKit host already mapped but **opacity=0 with an empty native input region**. Only once Svelte says ready does GTK size, install card-only hit rectangles, and reveal the page; the bootstrap surface is therefore never interactive.
- Widget opening is treated as unknown geometry until native/Svelte paint is acknowledged; no toast may overlap an unmeasured transient widget. Clearance conservatively includes the Top Bar's 42px reserved height.
- Repeated GTK geometry callbacks do **not** regenerate an unchanged Svelte snapshot or restart pending paint acknowledgements.
- Tiny outputs that cannot fit a 240 logical-pixel card column keep events queued. Even the invisible bootstrap WebView is initially sized below the 50% output-height cap.
- GTK monitor geometry, scale-factor and output-list changes trigger layout re-evaluation, so hotplug and resolution changes do not leave a stale 50%-height window.

## Transport and security

The draft uses GIO's existing session D-Bus support **inside kira-shell** to own `org.freedesktop.Notifications`. It does not steal an existing daemon's name. Each active notification is bound to its original unique D-Bus sender: another app cannot overwrite it through `replaces_id` or call `CloseNotification` on its ID. Sender identity is internal and never forwarded to the WebView. Its declared capabilities are limited to `body` and `actions`. All strings are bounded and sent into the trusted, navigation-locked, single-file Svelte UI as JSON data, not script or HTML. JavaScript can only acknowledge paint, close an ID or invoke one of its declared action keys. The action invokes the D-Bus `ActionInvoked` signal and closes the notice.

This placement of the D-Bus service in Shell is a deliberate **MVP architecture limitation**: long-term Kira Core should own the notification transport/lifetime and Shell should only own GTK/WebKit geometry. Notifications stop being received when Shell exits. The session history is not yet surfaced as a notification center, and the Top Bar does not yet display unread counts.

A nested `KIRA_PREVIEW` session disables D-Bus ownership by default to avoid taking over the host user's live session bus. For isolated visual testing, start the whole preview **inside a separate `dbus-run-session`** and explicitly set `KIRA_NOTIFICATIONS_PREVIEW=1`. Do not enable this override on the host session bus.

## Manual test matrix before merge

1. Inside isolated nested Niri, send 4 notifications with `notify-send` at once. Verify bottom-up order, at most three cards, no earlier than 1.5-second gaps, and FIFO after the first expires.
2. Verify minimum 3-second *painted* dwell with slow WebKit startup, hidden/covered WebView and renderer restart.
3. Close and replace an active notification by D-Bus ID. Attempt replacement of an expired ID; it must allocate a new ID.
4. Send `expire_timeout=0` and urgency=critical notifications, then manually dismiss them. Neither should expire automatically.
5. Click the default action or a named action; verify one `ActionInvoked` and one `NotificationClosed`. Check keyboard navigation after explicit click.
6. Open Calendar, System Monitor, Settings and Launcher while toasts are visible; test the fallback when insufficient space remains, and repeat with detached Niri windows.
7. Capture 360×640, 600×480 and 1920×1080 screens at scale 100%, 125%, 150%, 200%. Confirm actual input-region click-through, shadows, alignment and zero overlap.
8. Test a second monitor, unplug, rotate, fullscreen and lock screen. Only claim compatibility actually observed on supported Niri/GTK versions.
9. Test Firefox/Chromium, Discord and Telegram with their own notification permissions enabled. Check that a second notification daemon is never preempted.
10. Measure WebKit PSS, check native/Core renderer recovery and confirm no window/lease leaks after repeated notifications.

## Automated checks

Run `task check`, `nix develop .#ci --command cargo check --locked -p kira-shell --all-targets`, `nix develop .#ci --command cargo test --locked -p kira-shell --bin kira-shell`, and `task frontend:build`. GitHub CI checks compilation and model tests but does not replace nested-Niri graphic acceptance.
