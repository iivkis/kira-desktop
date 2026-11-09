# Kira Default / TENET · design and motion implementation review

**Review target:** native GTK branch, audited against `AGENTS.md`,
`DESIGN_SYSTEM.md`, `TENET_MOTION.md`, `ARCHITECTURE_NATIVE.md`,
`KIRA_UI.md`, and the frozen `main` widget parity contracts.
**Status:** source review and concrete patches; **not** a visual parity
certificate. All 15 widgets remain unverified until paired graphical evidence.

## Philosophy, not decoration

- **Quiet editorial UI:** black canvas, Prussian structure and a single
  orange accent communicate intent. Extra glow, effects and sixth colors
  would compete with information density rather than improve it.
- **Causal continuity:** TENET is a saved scene, not an easing preset. The
  first painted frame, a recorded route, reversals, interruptions and
  reduced-motion endpoints must all agree with the *actual* GTK state.
- **Factual telemetry:** motion moves a representation of recorded data;
  it must never insert a fabricated CPU/GPU measurement or a speculative
  cursor to make a graph feel smoother.
- **Reusable and native:** visual policies belong in `kira-ui`, not
  duplicated Shell code or browser effects; actual keyboard focus and GTK
  accessibility outrank a decorative transition.

## Findings and patch decisions

| Severity | Finding | Implemented source patch | Remaining verification |
| --- | --- | --- | --- |
| **P1** | The shared `MotionToken` started its duration at `Instant::now()` before a newly presented Wayland popup necessarily had a first GTK frame. On delayed map, the first rendered clip/sections could jump ahead. | Use GDK `FrameClock.frame_time()` and start at first tick. Normalize from monotonic microseconds, clamp and retain the last cursor to prevent accidental reverse movement. Keep generation cancellation and GTK reduced-motion settling. Add pure first-frame, intermediate, completion, negative/overflow and non-finite easing tests. | Delayed first-map, immediate reclose/reopen and 60fps paired popup captures |
| **P1** | `SystemCharts` mouse hit-testing could choose a real-but-not-yet-**painted** future point while the trace was clipped. Crosshair rendering rejected it but accessible details changed, violating one causal picture. It also accepted clicks in non-plot gutters. | Restrict hit-testing to valid Cairo X bounds and real measured samples at or before the current `ChartMotion.trace_extent` on the painted time domain. Ignore clicks on unrevealed points. Keep sparse/missing GPU as unavailable; no new sampling. Add geometry and cancellation-window tests. | Mouse/focus/pixel review during the chart's 350ms/180ms interrupts and changing GTK allocation |
| **P2** | Popup window-control targets were only 28 logical px, too small for consistently operable detached controls at high fractional scales. | Keep the source editorial surface but give Attach/Detach/Close controls a 34px minimum in both axes, with the existing explicit accessible names and orange focus outline. No Top Bar size changes. | Actual pointer and keyboard targets at 100/125/150/200% and narrow Niri windows |
| **P1** | A popup changing physical output mid-flight could keep partially played TENET progress from the old output after updating the contact position. | Cancel and hide the old GDK scene **before** writing new monitor/scale/Wayland hints; the fresh output opens from its first painted frame. Same geometry preserves genuine interrupted reversal. Add a pure mid-flight regression. | Paired nested-Niri captures of move, resize and scale transitions remain missing |
| **P1** | GTK compilation cannot prove that the Nix desktop starts or paints actual Wayland pixels. | Add independently triggered Nix package + Weston/nested-Niri 34-frame smoke with real top-bar/toast pixel predicates; keep full manual evidence review. | Smoke is not whole-widget content, focus, fractional scale or main visual parity |
| **P1** | A real Wayland smoke that checks only 1280×800 can let narrow and wide layout regressions pass; trusting Weston CLI dimensions is not evidence of Niri's actual framebuffer. | Add three isolated genuine-compositor viewports (800×600, 1280×800, 1920×1080), verify dimensions of every captured grim PNG against the requested viewport, require exactly 34 frames per scenario, fail closed on reused evidence and mark the aggregate manifest **unreviewed**. | Not yet 360px compact display, fractional scaling, keyboard focus, or paired TENET motion/frame parity |
| **P2** | Source philosophy was documented but there was no fast automated guard against a sixth CSS color, CSS-only motion/filters, suppressed outline, Top Bar/exclusive-zone drift or loss of the GTK frame/reduced-motion hook. | Introduce `scripts/check_native_design.py` and negative `scripts/test_native_design.py` fixtures in the already-bounded normal CI. Check active theme rules and native boundary invariants; reject browser animation and invalid accent foreground. | Source gate is **not** a GTK parser/build, a contrast analyser, an event simulator or a motion/frame screenshot comparison |

## Explicitly not claimed

1. The fast two-minute CI deliberately never compiles GTK. A separate
   automatic `.github/workflows/gtk-check.yml` now compiles real GTK4/Shell
   sources, strict full-workspace Clippy and all Rust tests with locked Nix
dependencies on native code changes. Its result
   must be checked independently: it does not prove actual Wayland
   pointer/focus behavior or lack of runtime faults. Full nested-Niri
   acceptance is still manual. The separate automatic
   `graphical-smoke.yml` now builds the package and validates 102 actual PNGs over three separately isolated 800×600,
   1280×800 and 1920×1080 nested-Niri sessions on native changes,
   including verification of actual grim image sizes. These predicates are
   **not** an independent 15-widget parity proof.
2. No source/native screenshot pairs or 60fps motion paths have been
   captured and reviewed. In particular, notifications, popup inversion,
   calendar flight paths, output hotplug/focus, multi-monitor scaling and
   ChartMotion target geometry remain **unaccepted**.
3. Disabled control opacity and Cairo-transparent overlays remain part of
   the existing native theme; the five hex colors pass a static check, not
   a visual perceptual/contrast review. Do not change the palette simply
   to improve the apparent animation score.

## Acceptance order

1. Run `python3 scripts/check_native_design.py` and
   `python3 -m unittest discover -s scripts -p 'test_*.py' -v`.
2. Manually run `.github/workflows/graphical.yml` on the **exact native
   HEAD**; inspect all-target Rust Clippy/tests, `nix build .#default`,
   fake-Niri safety and real headless-Wayland screenshots.
3. Perform paired source/native interaction captures at 360x640, 600x480,
   800x600 and 1920x1080 and 100/125/150/200% scales with identical
   fixture inputs. For interrupted popup/plot scenes, compare *painted*
   frames before and after cancellation and verify keyboard focus and
   reduced motion with a screen reader.
4. Record the actual captures, frame traces and human signoff in
   `docs/parity-evidence/<widget-id>/` before changing any of the
   `docs/MAIN_WIDGET_PARITY.json` verification flags.

The static guard is meant to make future **large** GTK UI increments
safer without replacing the independent graphical gate.
