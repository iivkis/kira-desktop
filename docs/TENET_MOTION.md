# TENET Motion System · native GTK4

**Status:** mandatory design-language contract for all new/changed native motion. The earlier GSAP/WebKit implementation rules are retired; the five causal laws remain.

## Five laws

1. **Reverse Causality.** An animated consequence may visually precede its decorative cause, but the real user action is never forged or delayed.
2. **Dual Time Flow.** A forward and an inverted visual track may share one contact event and timeline.
3. **Physical Continuity.** Reverse playback uses the **recorded original path and easing**, not a newly generated mirrored approximation.
4. **Temporal Memory.** Scene geometry, parameters and keyframes are captured for the duration of an interaction; interruptions resume from the current visible state.
5. **Causal Consistency.** End, cancellation, Escape, rapid reactivation and window destruction always settle to exact accessible state.

## Native implementation

A widget controller owns a scene: start/end geometry, contact point, forward/inverted history, progress and cancellation. For animation use GTK tick callbacks / GDK frame clock and GTK snapshot or GSK render nodes where appropriate. Never block the GTK main context, spin a permanent busy loop, sample actual system metrics through an animation loop or animate the Niri compositor's window geometry to mimic widget movement.

Simple actions need no inverted scene. Use motion only when it communicates state. Moving selections share one visual identity; do not make separate orange elements blink between positions. Elastic settling must be restrained and interruptible. Never regenerate a reversal path mid-interaction.

## Accessibility and truth

Honor `gtk-enable-animations` / reduced-motion preferences where available, provide immediate accessible final state when animations are disabled, maintain focus and keyboard behavior, and let labels/time/telemetry reflect real values. Decorative motion is always subordinate to user intent and correctness. No fake CPU samples, stalled buttons or blocking reverse animations.

## Banned defaults

The previously rejected hover optical lens, pixel distortion, forced glow and cursor warp are not restored. WebKit DOM transforms, GSAP timelines, CSS keyframes and ECharts animation are **not** implementation contracts for the native branch.

## Native selection flow (unverified)

`kira-ui::FlowIndicator` owns one persistent, non-interactive GSK selection surface, with 420ms `back.out(1.18)` geometry and 120ms opacity. GTK retains the actual current rectangle during retargeting, draws under date/workspace buttons or as the launcher left rail, and honours reduced motion via `MotionToken`. All content and focus still belong to the original GTK buttons. Compilation and headless tests are not proof of pixel or animation parity.

## Native measured-chart inspection (unverified)

The reusable `SystemCharts` chart inspector now pins one actual `Instant`
sample, never an interpolated utilization or a fabricated GPU interval.
Arrow/Home/End, plotted-point clicks and Previous/Next/Live controls select
timestamps, and a real GDK clipboard intent copies the measured values.
The selected marker is clipped to the *painted* ChartMotion time domain and
trace extent, so delayed line reveal is not outrun by a speculative cursor.
When the last 60-second history window discards the pinned sample, the
selection clears instead of adopting another identity. User focus,
interruption, high-DPI geometry and paired Niri/source motion remain pending.

## Native keyed FLIP and scroll (unverified)

`kira-ui::ReflowRow` retains an actual GTK child/button and controller under a GTK allocation transform. `ReflowScope` records the painted origins of the first 24 stable-key rows and, after the new GTK layout, replays the saved difference over 340ms using `back.out(1.14)`. Pruned rows never receive imaginary movements; new content and the real selected model update immediately. Launcher uses reflow only for explicit search/filter input when the previous result set contains 1–20 entries, not new GIO catalogue readings; a second interaction freezes each surviving row at its last painted coordinates. WorkspaceStrip only restages changed visible workspace ordering and clears stale offsets when disconnected. The FLIP scope invalidates pending ticks without losing the current painted geometry, and its explicit cancellation settles every still-live row; with reduced motion enabled it settles immediately. GTK unmap also resets row transforms, clears the persistent marker's cached geometry (keeping the selected target), and invalidates pending layout ticks before any remap.

`kira-ui::TenetScroll` uses the real adjustment and target bounds to calculate the nearest visible position, then animates 340ms `power3.out`. Starting a new navigation samples the current adjustment and the stable layout row rather than its moving child transform; manual scroll cancels the track. Unmapping the ScrolledWindow cancels its tick and settles an outstanding programmatic destination, unless a manual scroll has already superseded it. A capture-phase wheel listener also cancels the target before GTK changes the adjustment (including at its boundary); adjustment changes remain the fallback for touch/keyboard. Both obey GTK reduced-motion settling. Native Calendar digit tracks, click-juggle clips, inverse fragments and month-grid transforms now explicitly cancel and settle on GTK unmap, retaining factual clock/date state. These lifecycle guards still require graphical verification. Native nested-Niri recordings remain the acceptance gate. A host-side, opt-in `scripts/capture_native_motion.py` records timestamped PNGs strictly from a verified nested socket, never the parent display. See [NATIVE_MOTION_ACCEPTANCE.md](NATIVE_MOTION_ACCEPTANCE.md); no paired captures have been reviewed.

## Acceptance

For each animated scene document its intent, contact event, original recorded history, cancellation/reset path, accessibility fallback and lifecycle cleanup. Test repeated clicks, interruption, widget closure, screen scale changes and reduced motion in an isolated nested Niri session. No assertion of finished TENET quality without visual review.
