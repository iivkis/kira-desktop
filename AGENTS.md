# Kira Desktop contributor and agent rules

## TENET Motion System (mandatory for ALL animation work)

**Read [docs/TENET_MOTION.md](docs/TENET_MOTION.md) before creating, editing,
reviewing or replacing any animation** in any Kira Desktop widget, surface or
component. It is the normative motion specification, not optional inspiration.
This includes GSAP effects, ECharts transitions, GTK/native visual motion,
hover/focus reactions, animated data readouts, and future widgets.

The five required laws are **Reverse Causality, Dual Time Flow, Physical
Continuity, Temporal Memory, and Causal Consistency**. For each animation:

- Choose FORWARD (+1), INVERTED (-1), or a synchronized DUAL scene **by
  event sequence**, never by merely reversing the CSS direction or axis.
  Simple functional feedback may remain FORWARD; do not invert everything.
- For an inverted effect, record one event's **original states, paths, easing,
  impacts, and secondary traces**. Reverse that exact history. A new user
  interaction may randomize a new event, but the *return* must not reroll it.
- Keep both time flows anchored to one real contact/activation point and
  restore exact final geometry with no stuck fragments or jumps. Handle
  interruptions, repeat clicks, Escape, unmount and motion changes.
- Keep all actual actions, time displays, telemetry, focus, keyboard behavior,
  and accessibility truthful and immediate. Respect reduced motion and
  use only the approved Kira palette.
- In PR notes, identify the timeline directions, contact event, recorded
  trajectory and reset path. Test playback/restore, repeated clicks,
  reduced motion, lifecycle cleanup and the offline WebKit bundle. Verify
  native visual behavior in nested Niri when available.

Existing animations are reviewed against this contract **when touched**;
do not claim an unreviewed legacy animation already complies.

## Temporal material deformation (mandatory for clickable WebKit surfaces)

Do NOT fake deformation using a blurred lens, pseudo-element border, glow,
radial gradient or `backdrop-filter`. The approved `tenetHoverLens` action
(`frontend/src/lib/tenet-hover-lens.ts`) uses **real SVG
feDisplacementMap deformation of SourceGraphic pixels** driven over time
by GSAP. It records actual pointer positions and reverses their physical
field history on exit; when settled, the original image is exactly
restored. The clickable hitbox cannot move. One filter per mounted widget,
one active target, strict unmount cleanup, reduced-motion and touch
support are required. Do not recreate the rejected fake glowing rectangle.

See `docs/TENET_MOTION.md` for the normative motion design and testing rules.

## Visual stack (mandatory)

- **Apache ECharts** is the project's charting library for all new or redesigned
  charts, gauges, trends and quantitative visualizations. Do not hand-draw charts
  with ad-hoc CSS bars, SVG paths or an additional charting library.
  Prefer tree-shakable `echarts/core` imports, register only required modules
  and choose SVG rendering for compact WebKitGTK overlays.
- **GSAP** is the project's JavaScript animation library. Use it for deliberate
  motion such as entry transitions, numeric emphasis and state changes rather
  than introducing a second JS animation framework or complex CSS keyframes.
- Respect `prefers-reduced-motion` everywhere, and retain accessible labels,
  meaningful numeric fallback text, keyboard focus and functional no-motion UI.
  Never animate fake data into a chart or replace missing sensor data with zero.
- **Lifecycle matters:** WebKit widgets mount only while requested. Instantiate
  ECharts and GSAP effects on mount, resize charts when geometry changes,
  dispose chart instances, kill GSAP tweens and detach observers on unmount.
  Never create unmanaged timers, background data polling or repeated listeners.
- **Offline WebKit constraint:** The packaged frontend is a single
  self-contained HTML document. All npm code is bundled locally, with no CDN,
  runtime fetches, unexpected JS chunks or external stylesheets. Keep
  `frontend/scripts/inline-assets.mjs` and its tests passing.
- Retain the canonical five-color Kira palette in
  `frontend/src/lib/themes/kira-default.css`, the 42px top bar, and the
  locked-down Rust/GIO ↔ WebKit bridge. Charts are read-only presentation.

## Build and review

Changes to runtime JS dependencies require updates to both
`frontend/package.json` and `frontend/package-lock.json`, plus the
frontend Nix dependency hash in `nix/package.nix`. Run `task check`,
`task frontend:build` and (for frontend lock updates) `nix build .#frontend`.
Test supported and unavailable metrics, bounded history, reduced motion,
and open/close cleanup. Visual-QA check the popup in isolated nested Niri.
