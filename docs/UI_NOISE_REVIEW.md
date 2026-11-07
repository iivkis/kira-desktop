# Kira Desktop — visual noise review

> **V1 / справочник для миграции.** Документ описывает прежнюю реализацию Svelte/WebKit. Новая архитектура определяется [V2](V2_ARCHITECTURE.md) и [контрактом Lua](V2_LANGUAGE.md). Принципы безопасности и визуального дизайна полезны при переносе, но требования GSAP/WebKit не распространяются на нативный GTK UI.

Date: 2026-11-03  
Scope: Top Bar, Launcher, Calendar, detached sky atlas, System Monitor, Settings, display configuration.  
Method: audit of the live Svelte surface hierarchy, UI copy, visual treatments and keyboard/ARIA contracts against KISS, DRY and the Kira Default neo-brutalist palette. Screenshots provided in design review illustrate the baseline; nested-Niri visual acceptance remains outstanding.

## Design decision

Every visible element must do at least one job:

1. Identify a control or clearly name a distinct section.
2. Explain a value that cannot reasonably explain itself.
3. Report an actual condition, error, selection, or unavailable value.
4. Prevent user harm or irreversible changes.

Anything that only announces the product name again, numbers an editorial section, repeats a heading/placeholder, describes an ordinary default state or draws attention to a technical implementation detail does not pass. The absence of clutter must **not** imply the absence of data.

## Changes by surface

| Surface | Removed or reduced | Preserved |
| --- | --- | --- |
| Top Bar | The always-visible READY word and redundant separator | Health indicator, visible warnings, workspace focus/urgency, clock and Settings access |
| Launcher | APPLICATION INDEX, FIND / OPEN, CATALOG heading, unfiltered count, generic Installed application fallback, keyboard legend, per-row separators | Search, meaningful descriptions, icons, selected-item rail, filtered result count, empty/errors, arrows/Enter/Escape and visible focus |
| Calendar | CHRONOLOGY / LOCAL TIME / DATE INDEX / SELECTED DATE chapter captions, duplicate live marker, redundant week/day-of-year footer facts | Local clock, current date, month/year and ISO week grid, selected date and ISO for copy, Today/Copy actions, keyboard grid and clock TENET effects |
| System Monitor | HOST INSTRUMENTS, LOCAL TELEMETRY, normal-state NOMINAL / CURRENT SAMPLE / CORE ONLINE / NIRI ONLINE / NATIVE FEED captions, editorial numbering, duplicate CPU/RAM captions, chart methodology taglines, repeated process counts | Real gauges, activity history and axis labels, storage/swap and GPU readings, error/stale states, sortable process data, on-demand details and Copy Report |
| Settings | Branding repeated inside header, generic read-only notices, SYSTEM IDENTITY, LOCAL HOST, FACTS count, redundant selector label, constant side note, borders between every About fact | Actual OS/device facts with honest missing values, selected section, accessible navigation and disabled destinations |
| Display configuration | Redundant DISPLAY TOPOLOGY / NIRI tag and introductory description, duplicate confirmation countdown text, verbose map caption, VRR definition when already available | Connected/enabled counts, display geometry, apply/confirm/revert/save behavior, countdown, validation and persistent-config ownership rules (technical details on demand) |
| Detached sky atlas | CELESTIAL ATLAS chapter number, duplicate date, manually entered coordinates label, repeated compass legend | Actual astronomical rendering, coordinate/time controls and timezone, honest demo state, magnitude limit, source attribution and non-navigation caveat |

## Hierarchy and repetition

- A single primary title per surface is stronger than a tiny all-caps eyebrow followed by a second title.
- Black and Prussian Blue separate major zones. Orange is for selections, actions and material meaning, not decoration on every metadata line.
- A border should separate genuinely distinct interfaces (header/body, selected rail, field input), not every process, application or factual row.
- A normal state can be quiet. Warnings and stale/unknown data must never disappear.
- An empty description is preferable to the fabricated generic phrase "Installed application".
- Technical meanings such as RSS and PSS are available in the column tooltips/accessible names rather than a permanently visible footer.
- Reuse existing WindowControls, palette tokens and TENET actions. Do not create a UI abstraction that makes three small headings harder to maintain (DRY does not require universal components).

## Explicit guardrails

- No new core/native IPC, no change to native layer-shell docking or detached window leases.
- No change to telemetry sampling, calendar arithmetic, stored display settings or rollback semantics.
- Preserve labels for inputs and controls via ARIA even when a visual caption is removed.
- Keep animation selectors (e.g. `data-kira-contact`, `data-kira-enter`) and stateful TENET actions intact.
- Settings display Apply is **not** persistent Save. The explicit confirmation/rollback and config ownership details are not optional.
- Use existing Kira Default palette and popup shadow contract; no additional cards, colors or decorative animations.

## Acceptance

Automated: Svelte/TypeScript and Python contracts, offline self-contained bundle, browser overlay smoke for calendar/launcher/system/settings, chart rendering and keyboard focus.

Manual outstanding: inspect at 100/125/150/200% scales in nested Niri, with a detached window behind a popup. Pay particular attention to long launcher names, calendar clock + controls, locale-dependent dates, low-contrast text, custom display confirmation and the preserved shadow. Automated tests cannot prove aesthetic quality.
