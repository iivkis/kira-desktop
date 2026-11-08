# Native motion acceptance: nested-Niri evidence protocol

**Status: no paired captures reviewed.** Source reference: `main` at
`305139949f49176487ef9759fc32c228eadff408`. Native branch: `native-gtk`.
This document does not certify any of the 15 widgets in `MAIN_WIDGET_PARITY.json`.

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
| System Charts | Observe 65s of real CPU/RAM 500ms pulses; verify measured timestamps, adaptive axis bounds and gaps for missing samples; hide/reopen to check expired gauge state. GPU history and motion equivalence remain outstanding |
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
