# Calendar native parity · baseline main 305139949f49176487ef9759fc32c228eadff408

**Status: partial.** Content and civil-date navigation are being ported into reusable GTK4 components. This is not proof of pixel or TENET motion identity.

## Source contracts

- `frontend/src/lib/components/CalendarWidget.svelte`: original calendar contents and keyboard behavior
- `frontend/src/lib/calendar-model.ts` and `calendar-model.test.ts`: Monday-first six-row/ISO-week/date behavior
- `frontend/src/lib/calendar-tenet-model.ts` and `calendar-tenet-motion.ts`: saved four fragments, clock digit flights, selection and reversible temporal choreography
- `frontend/src/lib/components/SkyMap.svelte` and frozen `sky-catalog.ts` / `sky-model.ts`: original detached observatory, now partly recovered in `kira-ui`

## Ported and tested

- Pure `CivilDate` with valid years 1–9999, leap centuries, safe `shift_days` and `shift_month`, current vs selected vs adjacent identity and January ISO-week transitions.
- 6×7 Monday-first day grid plus eighth week-number column, including actual adjacent-month dates and ISO 8601 week labels.
- One local-clock source refreshed every second. Selection remains stable while today and clock advance.
- Buttons for previous/next year and month; arrow keys ±1/±7, Home=today, PageUp/PageDown ±month, Ctrl+PageUp/PageDown ±year.
- Today action, selected-date label and ISO 8601 copy intent routed from `kira-ui` to the native Wayland/GDK clipboard via `kira-shell`. Failed clipboard availability is reported, never papered over.
- Native reversible-time `dual_time`, deterministic `calendar_flights`, and a `GtkDrawingArea` scene anchored to the measured selected-day contact. Frame-clock tick cancellation and GTK reduced-motion settling never delay factual date changes.
- Native per-digit 400ms crossflight and source-equivalent six-sector click juggling with 380ms flights, 26ms stagger and saved reverse histories.
- Month change uses the pinned `main` 440ms `back.out(1.18)` compositor surface. The paired INVERTED four-fragment trace retains its 520ms `power3.out` flight, 25ms stagger and original contact coordinates.

## Still missing or unverified

- The digit roll, click juggling and calendar compositor models now have independent Rust tests, but no paired frame sequences demonstrate that their rendered GTK geometry and timing match `main`.
- The original `tenetFlowIndicator` 420ms selection spring is ported as a retained GTK indicator beneath real day buttons, using the visible in-flight rect on interruption. Its true pixel geometry, keyboard/focus behavior, reduced-motion settling and frame-by-frame comparison are not yet recorded. Source-complete popup switching remains missing. Native clip, seam, sections and traces also require graphical acceptance.
- Native detached Calendar now includes a retained GTK/Cairo offline SkyMap from the source 2,416-star catalog, 743 line edges, 89 constellation labels and 39 named star names. It shares selected date and exposes explicit coordinate/host-local-time controls, with 420ms cancellable sidereal tween. Real nested Niri observatory, scaling/accessibility, visual equality and frame-sequence verification are still missing.
- A paired native/WebKit recording with the same date, animation flags, locale, font, resolution, scale and Wayland environment. No pixel-perfect or temporal equivalence certificate exists.

## Acceptance matrix

In a strictly isolated nested Niri (never the active desktop's display config):

- [ ] Pin the same system date/time and ru-RU locale in `main` and native preview. Compare six week rows, week labels, adjacent-month days, selection, Today, year/month navigation and copying.
- [ ] Test 2024-02, 1900-02, 2000-02, 2026-11, 2021-01, 2018-12, year 0001 and year 9999 (last two via pure model tests).
- [ ] Use keyboard to traverse December→January and leap-day, confirm focus follows selected day and Escape closes popup without changing selection.
- [ ] Reopen Calendar and verify selected date and state survive window hide/show.
- [ ] Capture animations in both implementations at same clock contact point, including normal forward/inverted fragment return, frequent repeated interactions, interrupted reverse and reduced-motion mode.
- [ ] Compare original individual digit rolls and month/day master timeline **before** marking motion verified. A rough fade or visually similar particle scene is not enough.
- [ ] Compare 360×640, 600×480, 800×600 and 1920×1080 at 100, 125, 150, 200% GTK scale, including popups, border, spacing and contrast.
- [ ] Detach Calendar: the ~1040×720 window shows the existing date rail beside a real offline Cairo atlas. Reattach: the ~420px popup has no star atlas. Reopen detached: coordinate/time inputs and selected date remain retained.
- [ ] At 680px detached width, FlowBox must wrap the atlas below the date rail with vertical scrolling and no horizontal clipping. Test 100%, 125%, 150%, 200% scales and HiDPI memory behavior; physical backing allocation still needs verification against the 1536px source budget.
- [ ] Change selected day, clock, latitude and longitude (west negative). Check cardinal N/E/S/W orientation, below-horizon clipping, 420ms shortest-path sidereal easing and cancellation under rapid input. Invalid/empty coordinate, out-of-range year/time and local DST gap should show unavailable, never guessed sky or inferred GPS location.
- [ ] Compare the original and native catalogue points and star field snapshots in matching Niri geometry; the IAU asterism linework is illustrative and not official constellation boundaries.
- [ ] Verify screen reader labels, date focus, GDK copy result, navigation button disabling and Wayland keyboard/input hit regions.

See [MAIN_WIDGET_PARITY.md](MAIN_WIDGET_PARITY.md), [MAIN_WIDGET_PARITY.json](MAIN_WIDGET_PARITY.json) and [KIRA_UI.md](KIRA_UI.md). The source contract takes priority over an easier-to-draw replacement.
