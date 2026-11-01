# Kira Desktop — visual design review

**Reviewed:** 2026-11-01  
**Branch:** `feat/neo-editorial-shell` · PR #18  
**Reference:** [DESIGN_SYSTEM.md](DESIGN_SYSTEM.md), [SPECIFICATION.md §15](SPECIFICATION.md#15-премиальный-ui-и-фирменная-палитра-обязательное-требование), and user-provided nested-Niri screenshots.

## Scope and verdict

Reviewed the **currently implemented** Svelte Top Bar, GTK Launcher,
custom GTK Calendar, canonical color tokens, typography, hover/focus/selected/
disabled/error states, popup geometry, and visual QA. Dock, notifications,
settings surfaces, and configurable themes **do not yet exist** and therefore
cannot be signed off.

**Design direction: GOOD. Visual acceptance: CONDITIONAL.** The Black /
Prussian Blue / Orange / Alabaster Grey / White palette is preserved. The
Top Bar uses three functional color zones and a compact system group; Launcher
and Calendar use Black editorial headings, Blue working regions and restrained
local Orange states. Double-bordered cards and offset widget shadows are
absent from the implemented UI. The most recent user screenshots show
substantially improved compositional coherence.

This audit is based on repository source and screenshots already supplied
in the conversation. **No new pixel-by-pixel rendering or visual run in Niri
was performed by the reviewer.** Build, type-check and source-string tests
cannot substitute for actual display-scale and interaction acceptance.

## Verified deviations fixed as part of this review

| Priority | Issue and evidence | Correction |
| --- | --- | --- |
| P1 | [Calendar day selection](../crates/kira-shell/src/calendar.css): selected day is Orange, but the generic focus outline is also Orange. Contrast of the focus edge against its selected fill is **1:1**. | Add an inset **Black** outline specifically for `.kira-day-selected:focus-visible`. Contrast Black/Orange is approximately **10.39:1**. |
| P1 | [Calendar adjacent-month days](../crates/kira-shell/src/calendar.css) used `rgba(229,229,229,0.5)` over Prussian Blue, approximately **4.20:1**, despite being interactive 12px text. Specification requires normal text >= 4.5:1. | Increase opacity to **0.60** (approximately **5.44:1**); add a numeric contrast regression test. |
| P1 | [Workspace focus](../frontend/src/lib/components/WorkspacePreview.svelte) used a White outline for a selected Orange chip: White/Orange is only **2.02:1**, and the positive offset places focus against dark surrounding material. | Change selected-state focus to an **inset Black** outline on Orange. Ordinary workspace focus remains Orange. |

The fixes above are source-backed changes, not claimed visually verified
until a screenshot/keyboard test is performed in Niri.

## Outstanding findings

| Priority | Surface | Finding | Required acceptance |
| --- | --- | --- | --- |
| **P1** | Calendar, Launcher | [Calendar](../crates/kira-shell/src/calendar.rs) uses a fixed **378×490** default size, Top margin **50**, Right **12**. [Launcher](../crates/kira-shell/src/launcher.rs) uses **600×520**, Top margin **58**. No monitor-aware clamping or adaptive layout is visible. **Potential** clipping on narrow/short outputs or large text; not proven in the provided screenshots. | Test logical displays **360×640, 600×480, 800×600**, window scaling 100/125/150/200%; add bounds-aware position and scroll/collapse if failure reproduced. Ensure that close/Today/Copy remain reachable. |
| **P1** | Visual QA | [qa_visual.py](../scripts/qa_visual.py) optionally captures nested Niri at four scales but does **not** open Launcher/Calendar, exercise interaction states, or judge screenshots automatically. Existing static CSS tests verify selector presence, not pixels. | Expand explicit opt-in screenshot matrix to include both open overlays, selected and focused controls, and warning states; review images manually and save acceptance notes. No automatic false-green “visual pass”. |
| **P2** | Top Bar | [App.svelte](../frontend/src/App.svelte) uses a **42px** `overflow-hidden` root and several `shrink-0` warnings/right-hand controls. With many workspace names and error messages, a narrow viewport may clip useful content. Current tests look for class strings rather than overflow outcomes. | Test 360, 540, 800, 1024 and 1280 logical widths with **1/8/32 workspaces**, long localized names, seconds enabled and Core/Niri/calendar error labels. Preserve clock and keyboard access; degrade secondary metadata first. |
| **P2** | Typography | Top Bar overline is **8px**; secondary labels in GTK calendar/Launcher are often **10px**, date help **10px**, descriptions **11px**. Size alone is not a WCAG violation, but can undermine the promised readable “premium” hierarchy. | Review readability at 100/125/150/200% and varying font settings; target at least **10–11px for functional metadata** where layout allows. Large editorial headings can remain distinctive. |
| **P2** | Typographic identity | The Svelte theme declares an Inter/Segoe/Noto stack while both native GTK styles rely on the environment's GTK font. Cyrillic date labels are locale-rendered, but controls read `TODAY`, `COPY DATE`, `APPLICATION CATALOG`, etc. This may feel inconsistent when moving between three surfaces. | Specify a shared **typographic hierarchy** and Cyrillic-capable fallback on GTK; decide whether interface copy is intentionally English or localized. Verify month-name widths and fallback glyphs. |
| **P2** | Calendar controls | [Calendar CSS](../crates/kira-shell/src/calendar.css) includes the proper keyboard-focus selectors, but `Today`, `Copy Date`, arrow groups, selected and current days still require real pointer+keyboard state review. An HTML `aria-haspopup="dialog"` exists on the clock, but no `aria-expanded` state is tracked across the separate GTK window. | Tab/Shift+Tab, Enter, arrow navigation, ESC, PageUp/Down, Home, copying, and opening/closing by pointer and keyboard; ensure consistently visible focus and expected accessible state across process boundaries. |
| **P3** | Documentation | Specification §15 previously called Launcher and Calendar future work; the implementation now contains both. | Update status accurately (done in this audit); maintain explicit *implemented vs visually accepted* distinction. |

## Design-system conformity matrix

| Principle | Finding | Evidence / limits |
| --- | --- | --- |
| Five canonical colors | **Pass** | Typed palette + Svelte token aliases + named GTK color declarations match five agreed values. Derived rgba and color-mixes are allowed. |
| Neo-Brutalist geometry | **Pass** | Single strong selected workspace, orange active rail in Launcher, own date picker with restrained states. No return of stacked borders and hard offset widget shadows. |
| Editorial hierarchy | **Pass with caveats** | Clear hero/working/footer zones and strong scale contrast. Microcopy sizes and cross-framework font parity still need verification. |
| Expressive minimalism | **Pass** | Few accent fills, informationally useful surfaces, flat Today/Copy text actions. |
| Keyboard and contrast | **Fixed deviations, pending acceptance** | Focus and adjacent-day contrast fixes in code. Actual GTK keyboard-focus visuals still unconfirmed. |
| Responsive geometry | **Unverified / at risk** | Responsive Top Bar classes exist; GTK windows have fixed default dimensions. |
| Visual regression evidence | **Not accepted** | CI is valuable for compilation and behavior, but no reviewed screenshots for the current branch at all required sizes/states. |

## Recommended next acceptance pass

1. **Block on accessibility:** build and inspect the new inset Black focus ring on both Calendar's selected Orange day and focused Orange workspace; confirm adjacent-month days are readable.
2. **Block on clipping:** check 360×640, 600×480, 800×600 and 1024×768 logical sizes, then 100/125/150/200% scales. Popovers must stay visible and be dismissible.
3. **Verify status/error variants:** Niri disconnected, Core disconnected/connecting, long workspace names, multiple workspaces, clock seconds and local date.
4. **Verify native controls:** Launcher search/row selection/scroll/Enter/Esc; Calendar Today/Copy/month-year navigation/adjacent months, tooltips and Tab focus.
5. **Only then approve visual identity** and update [QA_VISUAL.md](QA_VISUAL.md) with actual inspected screenshots and results. Do not merge a “visual-ready” claim based only on CI.

**Verdict:** architecture and brand language meet the intent; the remaining
scope is measured polish and verified responsive/accessibility behavior, not
another aesthetic redesign.

## KISS / DRY follow-up: native overlay infrastructure

The Launcher and Calendar previously each declared all five identical
`@define-color` values and repeated GTK CSS-provider installation and window
lookup/dismiss logic. Shared canonical GTK declarations now live in
`native-palette.css` and the small `native_ui.rs` helper. This deliberately
**does not** unify their layouts or behavior: these surfaces have different
interaction models. Nix explicitly retains the new stylesheet, and one
cross-surface test checks canonical palette alignment and assembly.

## KISS / DRY follow-up: unused Svelte theme tokens

The unused `--kira-border-strong`, `--kira-shadow-brutal` and
`--kira-shadow-panel` definitions were removed after checking all committed
source references. They had no consumers beyond their declaration and tests.
The five canonical colors, semantic aliases, motion tokens and GTK palette
are unchanged. The theme contract now prevents these obsolete variables
from reappearing unintentionally.
