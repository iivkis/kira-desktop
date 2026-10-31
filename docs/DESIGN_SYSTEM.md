# Kira Desktop · Design System

**Status:** first implementation of Kira Default for the Svelte top bar. This
document operationalizes [specification §15](SPECIFICATION.md#15-премиальный-ui-и-фирменная-палитра-обязательное-требование);
the other desktop surfaces are not implemented yet.

## Visual direction

Kira is calm, deliberate and refined. Prefer proportion, legibility and quiet
depth over trendy effects. A premium interface should feel cohesive even when
reduced motion is enabled and visual effects are unavailable.

- Consistent 4 px spacing grid with 8 px as the preferred increment.
- Compact, carefully aligned typography; a limited range of weights.
- Soft corners (6–12 px), measured separators, restrained overlays.
- State communicated with color **and** shape/position/text.
- Micro-transitions around 150–200 ms, disabled with `prefers-reduced-motion`.
- High-contrast text, visible keyboard focus and no fake interactive controls.

## Kira Default · canonical palette

| Name | HEX | Default meaning |
| --- | --- | --- |
| Space Indigo | `#22223b` | Dark background, text on accent |
| Dusty Grape | `#4a4e69` | Raised surfaces, supporting materials |
| Lilac Ash | `#9a8c98` | Muted details on Space Indigo |
| Almond Silk | `#c9ada7` | Active and focused elements |
| Parchment | `#f2e9e4` | Primary readable text |

The canonical typed palette is defined by
[`frontend/src/lib/themes/kira-default.ts`](../frontend/src/lib/themes/kira-default.ts).
CSS variables are declared in
[`kira-default.css`](../frontend/src/lib/themes/kira-default.css).
Unit tests ensure the five values remain synchronized.

## Public CSS contract

| Semantic variable | Canonical HEX | Intended usage |
| --- | --- | --- |
| `--kira-color-background` | `#22223b` | Main surface/background |
| `--kira-color-surface` | `#4a4e69` | Elevated surface |
| `--kira-color-muted` | `#9a8c98` | Secondary details on dark background |
| `--kira-color-accent` | `#c9ada7` | Accent, selection, focus |
| `--kira-color-text` | `#f2e9e4` | Foreground text |

Derived tokens: `--kira-color-on-accent`,
`--kira-color-text-secondary`, `--kira-color-surface-subtle`,
`--kira-color-border`. Layout and motion tokens start with
`--kira-space-`, `--kira-radius-`, `--kira-motion-`.

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

**Do not hardcode HEX values inside individual components.** Theme overrides
must happen by redefining semantic variables, not by copying stylesheet rules.
User-configurable themes through KDL are a future milestone.

## Color contrast and accessibility

- Parchment on Space Indigo: **12.91:1**.
- Parchment on Dusty Grape: **6.79:1**.
- Lilac Ash on Space Indigo: **4.84:1**.
- Space Indigo on Almond Silk: **7.37:1**.
- **Avoid** Lilac Ash text on Dusty Grape (only 2.55:1).

Regular text should meet at least 4.5:1, large text and relevant non-text
indicators at least 3:1. Do not rely only on color to indicate selection.
Keyboard navigation and visible focus outlines are required when a widget
becomes interactive. Respect `prefers-reduced-motion`.

## Current component implementation

- `frontend/src/App.svelte`: premium 42 px top bar, balanced left/center/right
  regions, brand and local clock.
- `frontend/src/lib/components/WorkspacePreview.svelte`: three **visual**
  workspace chips. These are deliberately **not clickable**, because Niri IPC
  has not been implemented. Workspace 1 is highlighted as a design preview
  only, not a claim about the active Niri workspace.
- `frontend/src/lib/components/CoreStatus.svelte`: status badge with
  text and a dot, tied to the existing real Core bridge; an accessible
  `role="status"` communicates transitions.
- `frontend/src/style.css`: responsive layout and focus styling. The UI
  stays 42 px tall to match the native layer-shell reservation.
- `frontend/index.html`: uses matching fallback colors before Svelte mounts.

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

Next design-system milestones: shared interactive primitives, complete
keyboard navigation, configurable KDL themes, actual Niri workspaces,
Launcher, Dock, notifications, and their consistent states.
