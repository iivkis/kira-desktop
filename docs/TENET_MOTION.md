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

## Acceptance

For each animated scene document its intent, contact event, original recorded history, cancellation/reset path, accessibility fallback and lifecycle cleanup. Test repeated clicks, interruption, widget closure, screen scale changes and reduced motion in an isolated nested Niri session. No assertion of finished TENET quality without visual review.
