# Kira Desktop · Design System

**Status:** implemented visual system for the Svelte Top Bar, native GTK
Launcher and custom Calendar. Visual acceptance is still open. This document operationalizes [specification §15](SPECIFICATION.md#15-премиальный-ui-и-фирменная-палитра-обязательное-требование).
Dock, notifications and other desktop surfaces remain future milestones.

## Kira official visual direction

**Neo-Brutalism + Editorial Design + Expressive Minimalism** is the
official visual language of Kira Desktop.

- **Neo-Brutalism:** deliberate geometry, high-contrast states and visible
  structure where it carries meaning. It does **not** mean borders and offset
  shadows on every element.
- **Editorial Design:** a strong type hierarchy (eyebrow, heading,
  description, metadata), measured spacing and deliberate grouping.
- **Expressive Minimalism:** one canonical accent, every element functional,
  short quiet motion rather than decoration.
- **KISS:** keep each UI feature and interaction easy to trace. Prefer a small
  direct function over an abstract component framework; new layers must solve
  a demonstrated problem.
- **DRY:** define shared palettes and genuinely identical infrastructure once.
  Keep distinct behaviors separate, even when they both use GTK buttons.

### Visual rules

1. Organize the Top Bar as three intentionally different **materials**:
   Prussian Blue identity, Black workspace navigation, subdued Blue system.
2. Keep the main orange fill on the current workspace; add only a narrow
   orange baseline/mark to connect brand and active navigation.
3. Build a compact typographic hierarchy: wordmark and workspace kicker/title.
   The right edge is a **single vertically centered utility row**, not
   stacked SYSTEM/LOCAL labels with inconsistent inner widget heights.
4. Keep the full Top Bar at exactly 42 logical pixels; the navigation is
   real Niri state and the text is sourced from the current KDL config.
5. Divide the Launcher into a Black editorial intro and a Blue working area
   for search and catalog; avoid filling the entire selected row with orange.
6. A slim orange selection rail and orange item title identify the active
   application; the catalog retains descriptive text and system icons.
7. Preserve visible keyboard focus, urgent/offline states, reduced-motion,
   and readable contrast. Decoration must not disguise function.

### Implementation rules: KISS + DRY

1. **Single native palette:** `native-palette.css` declares the five GTK
   colors for Launcher, Calendar and the System Monitor. `native_ui.rs` assembles it with each
   overlay stylesheet, and `test_native_theme.py` verifies the values match
   Svelte's canonical palette. Do not duplicate `@define-color` in widgets.
2. **Small shared operations:** the common overlay helper only installs style
   providers, finds windows and dismisses them. Keep Launcher search/launch
   Calendar date-picker state and read-only System Monitor telemetry in
   their own modules.
3. **No premature abstraction:** two similarly named elements are not
   necessarily the same component. Extract code only when it has the same
   semantics and at least two real callers.
4. **Behavior before snapshots:** prefer unit tests of dates, state and real
   interactions; supplement with visual Niri QA for layout and focus. Never
   treat checking CSS class strings as proof that UI looks correct.
5. **Preserve public contracts:** Niri's 42px exclusive zone, the bounded
   WebKit bridge, KDL config and keyboard access are not negotiable during
   style or internal structure refactors.

### Component principles

- **Top Bar:** a 42 px strip with three continuous horizontal zones:
  Blue brand, Black workspaces and metadata, softer Blue system status/clock.
  The right zone is a condensed single line: combined Core + Niri health,
  subtle divider and local time, all sharing one optical vertical center.
  Date and diagnostic details are available via native title tooltips.
  The time is a semantic button that toggles a separate native calendar overlay;
  the Top Bar remains 42 px tall.
- **Calendar overlay:** top-right native GTK layer surface with no exclusive
  zone. Black time hero, Prussian Blue month/day selector and Black detail
  footer; the time updates live, a custom Monday-first 6×7 GTK Grid of
  accessible buttons supports month/year navigation, adjacent-month dates,
  ISO weeks and arrow/PageUp/PageDown/Home keyboard navigation. Selecting
  a date exposes the correct ISO week-year and day of year.
  Today and Copy Date are both flat orange typographic commands, without
  background fills, framed chips or active-tab underlines. Copy Date keeps
  hover/pressed text feedback and a keyboard-only focus outline, and writes
  to the clipboard only on click.
  Clicking time again or pressing Escape closes; focus loss also dismisses
  after activation. The calendar and Launcher close each other when opened.
- **Launcher:** Black editorial introduction and Prussian Blue workbench.
  Search, catalog, stable application icons, and an orange selection rail
  communicate function without painting the entire row orange.
- **Workspace controls:** orange filled selection only; hover/active/urgent
  and visible keyboard focus remain distinct. Niri events are authoritative.
- **System health:** the Top Bar is a compact **system** indicator, not just
  a Core heartbeat. READY requires Core/Niri connectivity **and** actual
  CPU/memory samples; LOW DISK, LOW MEM, LOW BAT or BUSY report measured
  pressure, and CHECKING is shown until first valid samples. Hover offers
  a short truthful snapshot; clicking opens the native System Monitor.
  PARTIAL indicates that CPU and RAM have been sampled but root disk data
  is unavailable, so the status never claims full readiness without it.
  It has the same Black heading / Prussian Blue workbench / Black details
  composition as the Calendar and Launcher, uses small orange progress
  meters and real values from Linux /proc, sysfs and Gio's root filesystem
  information. It shows Core and Niri, CPU, RAM, swap, root storage,
  1/5/15 load, uptime and battery (if present). Missing readings are
  UNAVAILABLE / NOT DETECTED rather than zero. A fixed-purpose WebKit
  `kiraSystem` channel only accepts `toggle` and cannot run commands.
  The monitor updates every two seconds and can copy a plain-text report.
  The system popover contains **two custom flat tabs**: 01 OVERVIEW
  (the existing diagnostics) and 02 PROCESSES (the read-only mini task
  monitor). The second tab reads readable tasks from Linux /proc only
  while active, displaying process name, PID, sampled CPU percentage
  relative to the entire host, and resident memory (RSS), with a search
  field, reversible sorting by clicking PROCESS/PID/CPU/RAM headers
  (orange active state and direction arrow), an independently scrollable
  top-36 list
  and honest counts. Samples update every two seconds on a worker.
  Initial CPU has no historical baseline and is displayed as a dash,
  and unreadable RSS/processes are not replaced with fake zero.
  The UI never reads process command-line arguments or offers termination.
  A PID plus kernel start-time identity prevents reuse from attributing
  another process's CPU history. No new Core JSON-RPC methods or
  user-data collection are introduced.

### Charting and motion libraries (project standard)

**Apache ECharts** is the only supported chart engine for quantitative
visualizations, including resource gauges/bars and live time-series.
Import ECharts modularly via `echarts/core` and use the SVG renderer in
compact WebKitGTK popovers. The system monitor shows actual sampled CPU,
RAM, GPU and other available percentages, with a bounded 60-second history.
A missing/unsupported sensor is `UNAVAILABLE`, never a zero-shaped graph.
Visible text and labels supplement SVG for accessibility.

**GSAP** is the standard for interface animations, including staggered
tab entrances and gentle reading updates. Animations are short, scoped to
their own Svelte widget and fully disabled under
`prefers-reduced-motion: reduce`. On widget unmount, dispose ECharts
instances and kill GSAP tweens/observers. No canvas/WebGL loops, network
requests or detached subscriptions may survive a closed overlay.

Both libraries are **bundled into the one self-contained offline HTML**
that WebKitGTK loads; never use CDNs, runtime dynamic imports, or extra
asset chunks. Preserve the canonical five-color palette and the 42 px
native top bar. The system monitor's Processes view retains searchable,
sortable RSS and PSS process data while the Overview chart uses only the
existing read-only native telemetry feed.

### System Monitor Overview visual acceptance

The Overview is an **editorial instrument panel**, not a stack of identical
diagnostic bars. The visual hierarchy is intentional:
1. **Primary:** two large ECharts SVG dials for actual CPU and RAM usage, each
   with accessible HTML numbers and accurate unknown state.
2. **Secondary:** activity history in **separate, clearly labeled CPU and RAM
   lanes**. Low CPU loads must remain visible using a bounded, adaptive
   percentage axis; each lane's axis displays its real endpoints, and GPU has
   its own lane only while a real utilization sensor is available. History is
   bounded to 600 genuine 100ms CPU/RAM samples, never fabricated.
3. **Tertiary:** compact Swap and Root Storage figures, then a concise hardware
   / thermal row and OS metadata, with no empty card for unavailable GPU data.
   Nonzero fractional swap use is not rounded down to a misleading 0%.
4. **Scrolling:** GTK/WebKit scrollbar chrome is hidden on the monitor but
   mouse, touchpad and keyboard scrolling remain operable. Content must not
   be cropped to conceal overflow.
5. **Motion:** ECharts animates only actual sample changes. GSAP marks tab
   entrances and readout updates, with reduced-motion support and lifecycle
   disposal. Visual tests must check the **real packaged overlay** at a
   narrow width in isolated Niri, not infer beauty from passing unit tests.

### 100ms CPU/RAM pulse

The **SYSTEM / PULSE** dials and activity lanes receive genuine Linux
`/proc/stat` and `/proc/meminfo` measurements **every 100 milliseconds**,
in a dedicated read-only worker, only while the System Monitor's Overview
tab is visible. A bounded, single-slot channel sends the latest pulse
through the existing GTK event loop to a strictly validated
`kira:system:pulse` event. ECharts keeps no more than 600 real samples
(60 seconds). No 100ms interpolation, browser polling or fake zero values.

The full system feed, including GPU, CPU/GPU temperatures, battery, root
storage, and KDL/Core status, still refreshes every two seconds. GPU
readings appear only once per actual slow sample, not twenty times as
invented high-frequency measurements. Process RSS/PSS scanning still runs
only on the Processes tab at its original slower cadence.

Closing Overview, switching tabs, or dismissing the overlay stops the
fast worker and clears the chart listener. A closed WebKit view never
continues sampling at 10 Hz. Motion respects `prefers-reduced-motion`.

### Popup geometry

Calendar and System Monitor share a **25 logical pixel** top margin,
half the previous 50px. A single GTK helper caps each popup's
scrollable viewport at **70% of the logical height of its monitor**,
recalculating after the Wayland surface maps. Both remain non-exclusive
overlays; Top Bar still reserves 42px.

### Geometry and motion

Use a 4 px rhythm (prefer 8 px), compact 4–10 px radii, mostly borderless
controls, and **one deliberate surface contrast per functional region**.
An orange line can anchor a section, but offset shadows and nested borders
are not a shortcut to depth. Use 120–180 ms transitions without positional
jumps. Keep interaction functional without motion. Niri's reserved 42 px
height is unchanged, including the bottom accent rule.

## Kira Default · canonical palette

The default dark theme uses exactly five fully opaque RGBA hex colors,
provided by the project owner. No additional base colors are permitted.

| Name | CSS variable | RGBA HEX | Intended role |
| --- | --- | --- | --- |
| Black | `--black` | `#000000ff` | Background, text on Orange |
| Prussian Blue | `--prussian-blue` | `#14213dff` | Surfaces, structural depth |
| Orange | `--orange` | `#fca311ff` | Selection, focus, interactive accent |
| Alabaster Grey | `--alabaster-grey` | `#e5e5e5ff` | Secondary text and borders |
| White | `--white` | `#ffffffff` | Primary text on dark surfaces |

The canonical values are declared in
[`frontend/src/lib/themes/kira-default.ts`](../frontend/src/lib/themes/kira-default.ts).
The frontend exposes them through
[`kira-default.css`](../frontend/src/lib/themes/kira-default.css).
Native GTK declares the same five colors **once** in
[`native-palette.css`](../crates/kira-shell/src/native-palette.css),
assembled with each overlay stylesheet by
[`native_ui.rs`](../crates/kira-shell/src/native_ui.rs).
Contract tests verify the values match; each overlay owns only its
component selectors, not a second color palette.

## Public CSS contract

| Semantic variable | Canonical alias | Intended usage |
| --- | --- | --- |
| `--kira-color-background` | `var(--black)` | Background |
| `--kira-color-surface` | `var(--prussian-blue)` | Elevated surfaces |
| `--kira-color-muted` | `var(--alabaster-grey)` | Secondary text |
| `--kira-color-accent` | `var(--orange)` | Selection and keyboard focus |
| `--kira-color-text` | `var(--white)` | Primary text |
| `--kira-color-earth` | `var(--prussian-blue)` | Legacy structural alias |
| `--kira-color-on-accent` | `var(--black)` | Legible text on Orange |

Derived tokens: `--kira-color-on-accent`,
`--kira-color-text-secondary`, `--kira-color-surface-subtle`
and `--kira-color-border` blend or alias these five colors.
Layout and motion tokens begin with `--kira-space-`,
`--kira-radius-` and `--kira-motion-`. Legacy structural tokens
remain for compatibility but are **not** instructions to draw borders
or hard shadows on every component.

## Tailwind CSS v4 frontend contract

`frontend/src/style.css` imports `tailwindcss` and the canonical color
tokens, and maps semantic colors with CSS-first `@theme inline`.
`frontend/vite.config.ts` uses `@tailwindcss/vite`. Keep utility classes
**literal** in Svelte templates: Tailwind's scanner cannot infer runtime
class-name concatenation. Conditionals must contain complete class strings.

Example for a future Svelte widget:

```svelte
<div class="rounded-md bg-kira-surface p-3 text-kira-text">
  Widget content
</div>
```

Use `bg-kira-accent text-kira-on-accent` for an active control, and explicit
`focus-visible:outline-2 focus-visible:outline-kira-accent` for keyboard
focus. Keep responsive and reduced-motion variants close to their markup.
Use `frontend/src/style.css` only for global browser constraints and
theme mapping, not for another manual component stylesheet.

**Never hardcode colors in Svelte utility class values.** Theme changes
belong to semantic tokens, not each component. GTK is a separate
native CSS surface sharing the same five-color palette. User-overridable
themes via KDL remain a future feature; current KDL config adjusts bar
behavior only.

## Color contrast and accessibility

Representative WCAG relative contrast ratios (each canonical alpha is `ff`):

- White on Black: **21.00:1**.
- White on Prussian Blue: **15.97:1**.
- Alabaster Grey on Black: **16.67:1**.
- Alabaster Grey on Prussian Blue: **12.68:1**.
- Orange on Black: **10.39:1**.
- Orange on Prussian Blue: **7.90:1**.
- Black on Orange: **10.39:1**, used on selected chips.
- **White on Orange is 2.02:1**: use Black for text on Orange.

Regular-size text requires at least 4.5:1, and meaningful non-text indicators
at least 3:1 as applicable under WCAG. Do not indicate focus by color alone:
workspaces also use a mark, labels and `aria-pressed`.
Provide visible keyboard outlines and honor reduced-motion preferences.

## Current component implementation

- `frontend/src/App.svelte`: identity / workspace / system architecture.
  The configurable title is a compact second-line workspace descriptor;
  compact and narrow screens collapse secondary labels.
- `frontend/src/lib/components/WorkspacePreview.svelte`: real Niri workspaces
  with accessible clickable buttons, live focus/active/urgent state, and
  stable workspace IDs for the intentional `FocusWorkspace` action.
- `frontend/src/lib/components/SystemHealth.svelte` and
  `frontend/src/lib/system-health.ts`: a compact, centered readiness indicator
  derived from real Core and Niri states. Accessible `role="status"`
  announces changes; unavailable details remain visible instead of an
  unexplained indicator.
- `frontend/src/style.css`: Tailwind v4 import, semantic `@theme inline`
  aliases and minimal global styles. Responsive composition and focus rules
  live in Svelte utility classes; the native 42 px reservation is unchanged.
- `frontend/index.html`: uses matching fallback colors before Svelte mounts.
- `crates/kira-shell/src/calendar.rs` and `calendar.css`: native time/date
  overlay, live timezone-aware clock and fully custom date-picker Grid (no GtkCalendar). The dedicated
  WebKit `kiraCalendar` bridge accepts exactly the literal `toggle`;
  it never forwards commands to Core.
- `crates/kira-shell/src/launcher.rs` and `launcher.css`: GTK layer-shell
  editorial introduction, Blue workbench, Gio application catalog, orange
  rail selection, pointer/keyboard activation and canonical five-color theme.

The KIRA button uses the dedicated `kiraLauncher` WebKit handler, which accepts
only the literal `open` message. Actual process launch uses registered Gio
`AppInfo` desktop entries, not interpolated shell commands. The Launcher is
non-persistent and has no exclusive zone. Esc closes it; Up/Down select and
Enter launches the selected application.

## Verification

```sh
nix develop --command task check
nix develop --command task frontend:build
nix develop --command task dev:preview
```

Run a manual visual review in nested Niri at common display scales and
sizes. Track remaining confirmed and suspected visual issues in
[VISUAL_REVIEW.md](VISUAL_REVIEW.md). The automated tests verify palette, contrast, semantic Tailwind v4 tokens,
structural zones, the native row selection rail, and accessible state
controls. The production Vite build compiles utilities into one inlined
WebKit CSS asset. Automated tests do **not** replace screenshot or
interactive GTK/WebKit review. Native compilation is checked by the Nix
workflow.

Next design-system milestones: shared interactive primitives, configurable KDL
theme overrides, Dock, notifications and their consistent states.
