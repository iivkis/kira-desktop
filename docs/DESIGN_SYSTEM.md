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

The project owner's five-color **Black / Night Bordeaux / White Smoke /
Dusty Taupe / Stone Brown** palette is now the default. No other base
HEX colors are permitted in Kira Default.

| Name | HEX | Intended role |
| --- | --- | --- |
| Black | `#0a0908` | App background and text on selected workspaces |
| Night Bordeaux | `#49111c` | Elevated surfaces and subdued wine-toned materials |
| White Smoke | `#f2f4f3` | Main text on black and Bordeaux |
| Dusty Taupe | `#a9927d` | Readable secondary text, active states and focus accent |
| Stone Brown | `#5e503f` | Structural border detail, not small text |

The source of truth is
[`frontend/src/lib/themes/kira-default.ts`](../frontend/src/lib/themes/kira-default.ts);
the loaded CSS lives in
[`kira-default.css`](../frontend/src/lib/themes/kira-default.css).
Contract tests verify that the exact five literal colors and their
semantic aliases remain synchronized.

## Public CSS contract

| Semantic variable | Canonical HEX or alias | Intended usage |
| --- | --- | --- |
| `--kira-color-background` | `#0a0908` | Main background |
| `--kira-color-surface` | `#49111c` | Elevated Bordeaux surface |
| `--kira-color-muted` | `#a9927d` | Readable secondary text |
| `--kira-color-accent` | `var(--kira-color-muted)` | Selection and keyboard focus |
| `--kira-color-text` | `#f2f4f3` | Main readable foreground |
| `--kira-color-earth` | `#5e503f` | Brown structural detailing |

Derived tokens: `--kira-color-on-accent`,
`--kira-color-text-secondary`, `--kira-color-surface-subtle`
and `--kira-color-border` blend or alias these five colors.
Layout and motion tokens begin with `--kira-space-`,
`--kira-radius-` and `--kira-motion-`.

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

Representative WCAG relative contrast ratios:

- White Smoke on Black: **18.01:1**.
- White Smoke on Night Bordeaux: **13.77:1**.
- Dusty Taupe on Black: **6.72:1**.
- Dusty Taupe on Night Bordeaux: **5.14:1**.
- Black on Dusty Taupe: **6.72:1**, suitable for selected chip text.
- **Do not use Stone Brown as small text on Black:** only **2.55:1**.
  It is used for structural borders and mixed with Taupe for visibility.

Regular-size text requires at least 4.5:1, and meaningful non-text indicators
at least 3:1 as applicable under WCAG. Do not indicate focus by color alone:
workspaces also use a mark, labels and `aria-pressed`.
Provide visible keyboard outlines and honor reduced-motion preferences.

## Current component implementation

- `frontend/src/App.svelte`: premium 42 px top bar, balanced left/center/right
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
keyboard accessibility, configurable KDL theme overrides, Launcher, Dock,
notifications, and their consistent states.
