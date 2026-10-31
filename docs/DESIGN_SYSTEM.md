# Kira Desktop · Design System

**Status:** implemented visual system for the Svelte Top Bar and native GTK
Launcher. This document operationalizes [specification §15](SPECIFICATION.md#15-премиальный-ui-и-фирменная-палитра-обязательное-требование).
Dock, notifications and other desktop surfaces remain future milestones.

## Kira official visual direction

**Neo-Brutalism + Editorial Design + Expressive Minimalism** is the
official visual language of Kira Desktop.

- **Neo-Brutalism:** constructed blocks, explicit 2 px borders, meaningful
  contrast and hard offset shadows. No generic glass or soft SaaS surfaces.
- **Editorial Design:** a strong type hierarchy (eyebrow, heading,
  description, metadata), measured spacing and deliberate grouping.
- **Expressive Minimalism:** one canonical accent, every element functional,
  short quiet motion rather than decoration.

### Visual rules

1. Components look constructed, not blurred. Borders express structure.
2. Typography carries hierarchy, especially in the Launcher.
3. Keep the palette limited and do not introduce decorative gradient colors.
4. Focus and selection are equally evident to pointer and keyboard users.
5. Indicate state with shape or text as well as color.
6. Preserve accessible text contrast, useful labels and real button semantics.
7. Respect reduced-motion settings; avoid animation-only state changes.

### Component principles

- **Top Bar:** a 42 px segmented control strip with structured separators,
  a real KIRA Launcher button, workspace chips, Core badge and clock chip.
- **Launcher:** native GTK overlay command panel with editorial header,
  app search, icon shells, filtered catalog, selection and keycap footer.
- **Workspace chips:** distinct selected, active, urgent and keyboard focus
  states. Niri events remain the authoritative source.
- **Core status:** compact high-signal editorial badge with text and indicator.

### Geometry and motion

Use a 4 px rhythm (prefer 8 px), 6–14 px radii, strong 2 px borders,
120–180 ms transitions and hard offset depth. All interactions must remain
functional when animation is disabled. The native Shell's 42 px reserved
top-bar height is unchanged.

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

The source of truth is
[`frontend/src/lib/themes/kira-default.ts`](../frontend/src/lib/themes/kira-default.ts).
The frontend exposes the five variables through
[`kira-default.css`](../frontend/src/lib/themes/kira-default.css), while
native GTK uses corresponding named colors in
[`launcher.css`](../crates/kira-shell/src/launcher.css).
Contract tests verify the palette and semantic aliases stay synchronized.

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
`--kira-radius-` and `--kira-motion-`. Structural tokens
`--kira-border-strong`, `--kira-shadow-brutal` and
`--kira-shadow-panel` define the new offset-shadow language.

Example for a future Svelte widget:

```svelte
<div class="widget">Widget content</div>

<style>
  .widget {
    padding: var(--kira-space-3);
    border-radius: var(--kira-radius-md);
    background: var(--kira-color-surface);
    color: var(--kira-color-text);
  }
</style>
```

**Never hardcode colors in component styles.** Themes must be changed at
the semantic token layer. User-overridable themes via KDL are still a
future increment; the current KDL configuration only adjusts bar behavior.

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

- `frontend/src/App.svelte`: structured 42 px top bar, balanced left/center/right
  regions, brand and local clock.
- `frontend/src/lib/components/WorkspacePreview.svelte`: real Niri workspaces
  with accessible clickable buttons, live focus/active/urgent state, and
  stable workspace IDs for the intentional `FocusWorkspace` action.
- `frontend/src/lib/components/CoreStatus.svelte`: status badge with
  text and a dot, tied to the existing real Core bridge; an accessible
  `role="status"` communicates transitions.
- `frontend/src/style.css`: responsive layout and focus styling. The UI
  stays 42 px tall to match the native layer-shell reservation.
- `frontend/index.html`: uses matching fallback colors before Svelte mounts.
- `crates/kira-shell/src/launcher.rs` and `launcher.css`: GTK layer-shell
  application catalog, search, pointer/keyboard selection, KIRA brand action,
  and canonical five-color Neo-Brutalist visuals.

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
sizes. The automated tests verify palette constants, color contrast,
required token names and reduced-motion support. They do **not** replace
a screenshot or interactive accessibility test of the WebKit surface.

Next design-system milestones: shared interactive primitives, configurable KDL
theme overrides, Dock, notifications and their consistent states.
