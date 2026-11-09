# Native motion acceptance: nested-Niri evidence protocol

**Status: no paired captures reviewed.** Source reference: `main` at
`305139949f49176487ef9759fc32c228eadff408`. Native branch: `native-gtk`.
This document does not certify any of the 15 widgets in `MAIN_WIDGET_PARITY.json`.

## Experimental virtual Wayland CI (native-only, real pixels)

`graphical.yml` now has an independent, experimental native
graphical run on `native-gtk`. It is deliberately separate from
mandatory Rust/Nix CI while software-rendered Niri stability is established.

- `nix build .#default` packages the real Rust/GTK Shell, Core,
  SNI and Notifications acceptance clients.
- `nix develop .#graphical-ci` invokes
  `scripts/headless_wayland_acceptance.py`. It refuses an already
  active Wayland/Niri session, and allocates new mode-0700 HOME, XDG and
  Wayland runtime directories. A headless Weston creates the **outer**
  Wayland socket, with Mesa llvmpipe software EGL/OpenGL for Niri.
- `accept_nested.py` launches a fresh private D-Bus bus;
  `dev_preview.py` creates its **own** Niri IPC and inner
  Wayland socket before starting the package. Actual D-Bus notification
  calls, genuine SNI registration and `grim` frame captures are
  validated by `validate_nested_evidence.py`.
- When it succeeds, the workflow uploads `review-index.html`,
  unreviewed PNGs and raw capture metadata. When EGL, compositor, Core,
  D-Bus or PNG validation fails, the job **fails**; no image is
  generated as a stand-in.

On a non-graphical Linux shell with the dependencies from this repo:

```sh
nix build .#default
nix develop .#graphical-ci --command python3 \
  scripts/headless_wayland_acceptance.py \
  --package-dir result --output artifacts/virtual-niri \
  --width 1280 --height 800
```

The virtual run is **native-only**, does not provide keyboard or fractional
scale acceptance, and does not compare frozen main. The first experimental
run reached real Weston but failed to initialize nested Niri EGL; the runner
now provisions Nix Mesa llvmpipe/GLVND explicitly. A green future run
still cannot set any widget `verified` flags by itself.

## Paired source/native frame review (manual provenance required)

After obtaining captures of the **same action** on the pinned main and
native-gtk inside *independent* isolated compositors, compare them:

```sh
python3 scripts/compare_visual_evidence.py \
  --main-manifest /tmp/main/launcher/rapid-filter-manifest.json \
  --native-manifest /tmp/native/launcher/rapid-filter-manifest.json \
  --output /tmp/kira-paired/launcher-rapid-filter
```

This tool rejects symlinked, corrupted or incomplete PNGs, mismatched
viewport dimensions, unequal frame counts, mismatched labels and unsupported
timestamps. It creates a self-contained, side-by-side static HTML index
alongside both sets of real frames and capture timing. The bundle is
**UNREVIEWED**: manifest labels cannot prove the main binary's identity,
sampling times are not presentation timestamps, and a human must still
check input, focus, all states, motion and pixels against frozen main.
No code in either tool writes the parity ledger.

## Reproducible, isolated capture

In an existing local Wayland session (never on the active desktop's Niri KDL):

1. Run `nix develop`, then `nix build .#default`.
2. Start `python3 scripts/dev_preview.py --debug` in a terminal. It builds a disposable
   Niri KDL, verifies that the nested Wayland/Niri sockets differ from the
   parent, and runs native Core and Shell with temporary paths. Do not copy
   its generated Niri config into the active desktop.
3. Note the actual `[preview] Nested display: …` printed by the preview,
   then in a **separate host terminal** (with the original parent
   `WAYLAND_DISPLAY`) run:

   ```sh
   python3 scripts/capture_native_motion.py \
     --display wayland-NESTED \
     --output /tmp/kira-motion/launcher \
     --label rapid-filter --frames 14 --interval-ms 33
   ```

   Replace `wayland-NESTED` with the preview's real value. The capture tool
   refuses an identical, missing or non-socket host display. `grim` only
   receives the verified nested display. It cannot modify Niri configuration.
4. Begin a selected interaction while the frames are being captured. Repeat
   the same action and viewport on the frozen main reference in an independently
   isolated compositor. Do not restore its WebKit frontend inside `native-gtk`.
5. Retain **both** runs' PNGs and the generated `*-manifest.json` files.
   The manifest records actual elapsed sample times, not a fictional 30fps
   cadence. Capture command overhead means frame timestamps are not exact
   animation progress; use a calibrated high-frame-rate recorder for temporal
   certification.

## Real private-bus notification and SNI smoke

The optional fully coordinated command below requires **an existing parent
Wayland session**, `nix develop` (with `dbus-run-session`, `gdbus`, `grim`,
Niri and GTK4) and `nix build .#default`. It is never run on headless CI.
Unlike a plain preview, it constructs a **fresh private D-Bus session**
before launching nested Niri or Kira. Besides the 128-bit handshake and
a distinct parent/child bus address, it checks that the live D-Bus address
refers to an owned filesystem Unix socket directly in the coordinator's
mode-0700 temporary directory (not an alias or fallback address). Production
`main` and the user's active Niri, KDL, notification daemon, tray watcher,
and XDG state are untouched.

```sh
nix develop
nix build .#default
python3 scripts/accept_nested.py --package-dir result \
  --output /tmp/kira-acceptance-$(date +%s)
```

The coordinator uses a minimal purpose-built session bus config inside a
mode-0700 temporary directory (and mode-0600 config). It does not rely on
`/etc/dbus-1/session.conf` or activate host user services. The socket
vanishes with the runner. No host D-Bus, Niri KDL or state paths are used.

The coordinator creates a disposable Kira Core/Shell, a real **GIO
StatusNotifierItem** with ARGB32 `IconPixmap` that sends `NewStatus`, a
native watcher, and a native Notification Host on the same isolated session
bus. It probes the exact Freedesktop service, exercises real
`Notify`/`CloseNotification`/`replaces_id` calls, and uses `grim` against
the validated nested Wayland socket to capture *entry*, *exit*, and *live
tray* frame sequences. The acceptance runner checks PNG chunk CRCs, IEND, viewport
consistency, safe frame paths and monotonic sample times, then generates
`review-index.html` for offline per-frame inspection. A scenario manifest
records bounded IDs, actual event/sample times and untested behaviors. Every
manifest and screenshot is explicitly
`unreviewed`; this command does **not** certify visual parity, keyboard
interaction, exact input-region hit-testing or Niri compositor timestamps.

CI also runs `scripts/smoke_sni_fixture_bus.py` against the **packaged
GIO fixture** on an entirely new headless `dbus-run-session`, checking a
real `GetAll` `IconPixmap` and the live NeedsAttention transition. This
headless protocol test is intentionally separate from nested Niri and
cannot pass the visual or hit-region gates.

The private bus is torn down after the run. **Never** enable
`KIRA_NOTIFICATIONS_PREVIEW` or `KIRA_SNI_PREVIEW` in the host session;
both Shell adapters now require a separate
`KIRA_ISOLATED_SESSION_BUS=1` attestation, which only the coordinator sets
after its bus-check. If the environment cannot prove isolation, the smoke
fails closed and ownership remains disabled. The executable
`kira-sni-acceptance` is a non-autostart acceptance fixture, not a system
daemon or a stand-in for real app menus.

Once this succeeds, run the same viewport and stimuli manually against
the frozen `main` in a *different* private nested session. Review input,
timing, motion frames and final pixels side by side before changing any of
the 15 parity flags. The first step proves bus and compositor reachability,
not source/native equivalence.

## Required cases

| Scenario | Input / expected invariant |
| --- | --- |
| Launcher short filter | Start with ≤20 visible entries, filter and clear; surviving application IDs keep their original buttons, keyboard focus and 340ms back.out(1.14) motion |
| Launcher broad filter | Start with 21+ entries; filtering does **not** animate a fabricated relocation of the first 24 |
| Launcher rapid interruptions | Enter three different filters inside 340ms; no jump to stale endpoints, no overlapping old history |
| Launcher navigation | Repeated Up/Down to an offscreen row; nearest visible position, 340ms power3.out, accessible selection without launching an application |
| Manual wheel interruption | Start animated scroll, then wheel/touch; manual adjustment wins, no previous target rebounds |
| Workspace change | Move focus rapidly among chips, shrink/expand visible workspace range; retained indicator remains attached to the correct chip |
| Disconnect/reconnect | Hide the strip and show it again; stale FLIP offsets do not reappear |
| Calendar | Change selected day and month and replay clock digits; close the popup during digit exchange, click-juggle, inverse fragments and month-grid motion, then reopen; no stale contact, clipping, transforms or scrambled selected-day layer |
| System Capacity | Observe actual root filesystem usage and swap with no swap configured, exhausted disk, missing or invalid metrics and a stopped worker; verify 6s stale-state and unmap reset, progress a11y, responsive two-tile layout and paired main/native frames. Visual and motion evidence is still missing |
| System Hardware & disclosure | With Overview open, test known and absent CPU/GPU sensors, 0/1/many GPU cards, 6s worker interruption, clipped long GPU names, keyboard expand/collapse and state retention across tab switches. Compare source disclosure motion, readouts and a11y under nested Niri |
| Top Bar warning inspector | In isolated nested Niri, cause a real Core-rejected workspace focus, an invalid KDL reload that retains the last good config, and a failing GIO desktop launch. With 2+ simultaneous faults, verify source ordering and ID-stable paging, long selectable/copyable text, the 900px long/short label breakpoint, Tab/Shift+Tab focus trap, Escape/× focus return, automatic close when faults clear, and no stale reopened inspector on a later fault. Measure the bar at exactly 42 logical px *during* inspection, not only while hidden. Repeat under reduced motion and 360×640 and 1920×1080. These scenarios require actual compositor focus, paired frozen-main screenshots and human review; headless model tests do not verify them |
| System Health | Kill/restart Core and stop/restart the Niri subscription independently, then inject or measure CPU/memory/disk/battery thresholds. Verify 5s Core probe, 2s lightweight worker, 6s stale suppression (no false READY or stale LOW MEM), quiet status, status-button focus/tooltip and real System popup action. Capture source/native transitions and reduced-motion behavior |
| System Charts | Record 65s of real 500ms CPU/RAM pulses and ~2s GPU peaks, verifying sparse GPU points attach to one later pulse (no fake intermediate values), adaptive axes/gaps and a shared timestamp domain. Hide/reopen mid-graph to prove history/pending GPU reset; test 8s GPU and 2.5s gauge expiry, reduced motion, and paired main/native gauge geometry. Visual/motion acceptance remains outstanding |
| System Tray | In isolated nested Niri and DBus, register / update / unregister genuine SNI items through both local and external watcher. Verify real icon/title/status refresh, 64-item bounds, stable pin identity across restart, safe unique-owner dispatch, primary Activate and secondary ContextMenu, attention when clipped, Escape and popup toggling. Verify no native watcher takeover in preview and no fake icons. Probe ARGB32 pixmap sizing, invalid byte lengths and theme-less clients, 80-entry/3-level dbusmenu bounds, unique enabled leaf clicks and preserved menu state across checkbox/radio actions; verify LayoutUpdated revision/unique-owner guards, AboutToShow lazy nodes and retained 340ms ReflowRow FLIP with rapid reorders; test direct-child navigation, duplicate hidden-branch rejection, Back/Escape, lazy empty pages and disappearing parent recovery; floating cascading menus, OverlayIcon and paired source FLIP proof still remain incomplete |
| Notifications | In isolated nested Niri and separate `dbus-run-session`, enable `KIRA_NOTIFICATIONS_PREVIEW=1`. Send four simultaneous real D-Bus notifications, then replace/close IDs across processes and click declared actions. Verify 3-card maximum, 1.5s paint-to-admission gap, 3s minimum measured dwell, 0-timeout persistent ID rotation, click-through outside the exact card region, no toast over a right-side popup (including Tray), no bus-name preemption, and no synthetic test cards. Repeat 360×640/1920×1080 plus scale/hotplug. Real 180ms 12px power2.out entry must pass 60fps and reduced-motion capture; test immediate input release with an 180ms non-targetable inverse exit, interruption mid-entrance and disappearance under covered popup or geometry change; multi-output screenshots remain unaccepted |
| Reduced motion | Disable GTK animations in the **isolated session**, then repeat; factual state updates immediately and every retained FLIP row settles without stale offsets |
| Popup lifecycle | Open/close, Escape, resize, switch quickly during 340ms FLIP, 420ms selection, or 340ms scroll; remapping must not revive an old tick, stale geometry or painted offset |

## Review threshold

Use the same date/time, locale, fonts, viewport, GTK scaling and animation
preferences on both implementations. Record 360×640, 800×600 and 1920×1080
plus fractional scale when supported, especially 125% and 150%. Validate
pixel bounds, motion frames, keyboard focus, screen reader labels, hit regions,
manual scrollbar interruption, and exact final state.

Attach the evidence paths and commands to a review; only after a person
compares both sequences and independent behavior tests pass may any JSON
ledger field become `true`. An unreviewed PNG series or green CI remains
**0 of 15 verified**.
