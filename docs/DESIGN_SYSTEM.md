# Kira Desktop · Design System

**Status:** implemented visual system for the Svelte Top Bar and native GTK
Launcher. This document operationalizes [specification §15](SPECIFICATION.md#15-премиальный-ui-и-фирменная-палитра-обязательное-требование).
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

### Visual rules

1. Organize the Top Bar as three intentionally different **materials**:
   Prussian Blue identity, Black workspace navigation, subdued Blue system.
2. Keep the main orange fill on the current workspace; add only a narrow
   orange baseline/mark to connect brand and active navigation.
3. Build a compact typographic hierarchy: wordmark, workspace kicker/title,
   and stacked system metadata with tabular clock digits.
4. Keep the full Top Bar at exactly 42 logical pixels; the navigation is
   real Niri state and the text is sourced from the current KDL config.
5. Divide the Launcher into a Black editorial intro and a Blue working area
   for search and catalog; avoid filling the entire selected row with orange.
6. A slim orange selection rail and orange item title identify the active
   application; the catalog retains descriptive text and system icons.
7. Preserve visible keyboard focus, urgent/offline states, reduced-motion,
   and readable contrast. Decoration must not disguise function.

### Component principles

- **Top Bar:** a 42 px strip with three continuous horizontal zones:
  Blue brand, Black workspaces and metadata, softer Blue system status/clock.
  Small overlines establish hierarchy without extra boxes.
- **Launcher:** Black editorial introduction and Prussian Blue workbench.
  Search, catalog, stable application icons, and an orange selection rail
  communicate function without painting the entire row orange.
- **Workspace controls:** orange filled selection only; hover/active/urgent
  and visible keyboard focus remain distinct. Niri events are authoritative.
- **Core status:** compact unboxed label with status dot. Offline and
  connecting states remain explicit; connected status is exposed accessibly.

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
- `frontend/src/lib/components/CoreStatus.svelte`: unboxed status text
  and dot, tied to the real Core bridge; accessible `role="status"`
  announces transitions.
- `frontend/src/style.css`: Tailwind v4 import, semantic `@theme inline`
  aliases and minimal global styles. Responsive composition and focus rules
  live in Svelte utility classes; the native 42 px reservation is unchanged.
- `frontend/index.html`: uses matching fallback colors before Svelte mounts.
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
sizes. The automated tests verify palette, contrast, semantic Tailwind v4 tokens,
structural zones, the native row selection rail, and accessible state
controls. The production Vite build compiles utilities into one inlined
WebKit CSS asset. Automated tests do **not** replace screenshot or
interactive GTK/WebKit review. Native compilation is checked by the Nix
workflow.

Next design-system milestones: shared interactive primitives, configurable KDL
theme overrides, Dock, notifications and their consistent states.
