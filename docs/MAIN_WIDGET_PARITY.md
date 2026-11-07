# Parity audit: `main` → `native-gtk`

**Status: BLOCKED.** The GTK4 widgets are **not** currently identical to the WebKit/Svelte widgets in `main`, including their content, interactions and animations. This document and [MAIN_WIDGET_PARITY.json](MAIN_WIDGET_PARITY.json) are the source-backed migration ledger, **not** a certificate of parity.

## Frozen source baseline

- Source: [`main` at `305139949`](https://github.com/iivkis/kira-desktop/tree/305139949f49176487ef9759fc32c228eadff408).
- Widget implementations: [`App.svelte`](https://github.com/iivkis/kira-desktop/blob/305139949f49176487ef9759fc32c228eadff408/frontend/src/App.svelte) and [`frontend/src/lib/components`](https://github.com/iivkis/kira-desktop/tree/305139949f49176487ef9759fc32c228eadff408/frontend/src/lib/components).
- Exact motion: [`motion.ts`](https://github.com/iivkis/kira-desktop/blob/305139949f49176487ef9759fc32c228eadff408/frontend/src/lib/motion.ts) and [`calendar-tenet-motion.ts`](https://github.com/iivkis/kira-desktop/blob/305139949f49176487ef9759fc32c228eadff408/frontend/src/lib/calendar-tenet-motion.ts).
- Theme: [`kira-default.css`](https://github.com/iivkis/kira-desktop/blob/305139949f49176487ef9759fc32c228eadff408/frontend/src/lib/themes/kira-default.css).
- Every source path and Git blob SHA is pinned in the JSON ledger. Changing the baseline needs an explicit new review, rather than comparing against an ever-moving main.

## Comparison results

| Built-in main surface | Native GTK status | Content / behavior that still differs |
| --- | --- | --- |
| Top Bar (`App.svelte`) | Partial | Native KIRA brand/workspace/clock composition is closer; still missing tray, Core health, warning inspector and full keyboard/status semantics |
| Workspace Preview | Partial | Focused-window and urgent/output source model preserved. One GTK 420ms horizontal selection marker now follows focused chips. Source-bound 340ms keyed reflow is connected to changed workspace order; pending state and paired visual/motion evidence remain missing |
| Launcher | Partial | Source-derived search/ranking/icon/keyboard models preserved. The 420ms retained rail and source-timed 340ms keyed FLIP/vertical nearest-row scroll are wired to explicit user search and selection. Popout, complete reveal and paired visual/motion evidence remain missing |
| Calendar | Partial | Native `kira-ui::Calendar` has a six-row Monday-first eight-column ISO week grid, neighboring-month selection, navigation, live clock, Today and ISO clipboard feedback. Native GtkDrawingArea replays saved inverted fragment coordinates; the new 400ms `ClockDigits` exchanges individual numerals. 440ms month-surface motion, measured-contact inverted day fragments and saved-sector clock click-juggle are now implemented. A native 420ms persistent selected-day indicator is now integrated but unverified; detached SkyMap and paired visual/motion proof remain missing |
| Sky Map | Missing | 74KB astronomical catalog, date/location/time controls, horizon drawing and 420ms sidereal interpolation |
| System Monitor | Partial | Basic native telemetry/processes, but not original disk/swap cards, hardware disclosure, filtering/sorting, copy report, chart context or animation |
| System Charts | Missing | Original true 500ms CPU/RAM gauges + history, correct axes/data cadence and reduced-motion behavior |
| System Health | Missing | Core/Niri freshness and health warning classification/Top Bar feedback |
| Settings | Partial | Basic native info/displays, missing About device facts, responsive section navigation, sidebar and popout |
| Display Settings | Partial | Trusted Niri transactions retained; missing drag-and-drop output map, selected monitor/editor parity, confirmation overlay and animation. **Mutation remains preview-only until crash-safe rollback** |
| Tray Bar | Missing | StatusNotifier icon identities, compact pinned icons, overflow and attention states |
| Tray Widget | Missing | Pinned/hidden items, freedesktop menu tree, actions and FLIP |
| Notification Host | Missing | True native toast windows, input region, queue admission/ACK, output placement |
| Notification Card | Missing | Real sender/content/action/close card and 180ms entry/exit |
| Window Controls | Missing | Detach/reattach without loss of controller state, close/popout feedback |

**Result: 0 of 15 surfaces are verified fully equivalent**. Several behaviors have been ported and tested, but that is not a whole-widget pass. No actual paired GTK/WebKit screenshot and frame-by-frame motion evidence is attached.

## Animation contract still to port

The canonical `main` timeline is not merely a fade: it contains a contact point, FORWARD content with **INVERTED** decorative traces and a master timeline replayed backwards on dismissal. GTK must reproduce the same cause/effect sequence, geometry, interruption semantics and reduced-motion reset, rather than use an arbitrary CSS transition.

| Motion in `main` | Source implementation | Must be reproduced in `kira-ui` |
| --- | --- | --- |
| Shared popup materialization | `motion.ts` | 540ms clipped/seam reveal, 440ms content spring `back.out(1.22)`, 35ms section stagger, opacity settled separately, saved reverse trajectory and no rectangular flash |
| Active tab/selection rail | `motion.ts` | One persistent indicator, ~420ms `back.out(1.18)`, real measured geometry, interruption-safe |
| Stable-key list reflow | `motion.ts` | ~340ms `back.out(1.14)` FLIP with keyed identity; no fake motion of measurements |
| List scroll | `motion.ts` | ~340ms `power3.out`, interruptible, focus remains usable |
| Calendar digits/grid | `calendar-tenet-motion.ts` / `clock-flight.ts` | 400ms digit exchange; click juggling 380ms flights/26ms stagger; 440ms month compositor spring; 520ms inverted day fragments/25ms stagger; correct live time first |
| Sky-map movement | `SkyMap.svelte` | 420ms reversible sidereal interpolation, real observer coordinates and reduced-motion |
| Notification card | `NotificationCard.svelte` | 180ms `y:12px → 0` and `opacity:0 → 1`, `power2.out`, cancelled on disposal |
| System Charts | `SystemCharts.svelte` | Valid sample history and bounded gauges; no animation overshoot beyond measured values |

**Native progress:** `kira-ui::motion` contains the calendar four-flight vectors, dual time, a 400ms digit exchange model and one reversible popup master cursor (540ms source at 1.44× speed). `ClockDigits` renders live per-digit old/new tracks, and `PopupClip` drives a GSK reveal of a fixed Wayland window with a remembered reverse/interrupt cursor. GTK now stages up to three existing native sections with a 440ms back.out(1.22) spring, 35ms staggering and independent opacity; the popup seam and four source-defined INVERTED fragment trajectories share the same reversible master cursor. This requires nested-Niri visual verification and careful interruption/focus QA. The native clock click-juggle saves six non-repeating sector paths and retraces the same tick timeline; the calendar now animates one 440ms grid surface and records 520ms inverse fragments at the newly selected day. A single native 420ms `FlowIndicator` now renders selection for calendar, Launcher and Workspaces, sampling the current displayed geometry for interruption. Reduced-motion resets are wired through GTK; this has no paired graphical evidence. Cross-popup switching currently settles the old surface immediately. Native keyed FLIP reuses surviving GTK button identities (at most 24 tracked) with 340ms back.out(1.14). Native nearest-row scroll uses 340ms power3.out and cancels on user adjustment changes; catalog/metrics updates do not request decorative reflow. All graphical behavior remains unverified. **None of the overall widget animations is verified as identical yet.** A spring easing formula or a generic `GtkRevealer` transition alone is not evidence of TENET equivalence. Keep motion declarative and reusable inside `kira-ui`, not as GTK widget code duplicated in `kira-shell`.

## Mandatory acceptance before saying “identical”

1. **Content.** Freeze identical observed Core/GIO/Niri fixture data, locale, time, theme and font for both implementations. Inspect all labels, icons, charts, settings subsections, warnings, notices and empty/offline/error states. No placeholders for live values.
2. **Interactions.** Exercise mouse, keyboard, focus/return focus, popup switching, Escape, drag-and-drop, detached windows, search, sorting, day navigation, notifications and the Niri display safety model on both implementations. Compare actual state transitions, not just screenshots.
3. **Motion.** Record the same interaction in both builds at >=60fps if available. Compare contact position, initial/final opacity/clip, ordering, easing shape, stagger, section geometry, reverse dismissal, rapid repeated input, cancellation and reduced motion. Motion data must never falsify system readings.
4. **Visual.** Collect paired main/native captures at 360×640, 600×480, 800×600 and 1920×1080, at 100%, 125%, 150% and 200% scaling, with identical GTK/Wayland fonts and locale. Compare content geometry, palette, spacing, corners, shadow and legibility. GTK and WebKit text rasterizers may differ; agree a realistic pixel tolerance **before** marking visual parity.
5. **Evidence.** For each ledger entry, add explicit contract-test logs, screenshot comparisons, paired motion frame sequences and human review under `docs/parity-evidence/<widget-id>/`. Only then mark each of the four verification fields true and promote state to `verified`. The CI ledger checker rejects unsupported “verified” claims.
6. **Release.** The native Nix build, nested Niri smoke, accessibility/reduced-motion QA, crash-safe output rollback and all 15 signed-off widget contracts are separate release gates. Preserve `main` as the frozen reference until they pass.

## Development order

**`kira-ui` first → independent model/motion/visual tests → trusted `kira-services` and Core adapters → `kira-shell` composition → paired nested-Niri acceptance.** The GTK component library must preserve the source feature contract but cannot depend on WebKit, Niri or arbitrary command execution.

Run the explicit ledger check in any branch build:

```sh
python3 scripts/check_widget_parity.py
python3 -m unittest discover -s scripts -p 'test_*.py' -v
```

The ledger checker reports honest coverage and blocks unsupported success claims; it **cannot generate or certify visual recordings by itself**.
