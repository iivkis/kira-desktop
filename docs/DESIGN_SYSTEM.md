# Kira Default · native GTK4 design system

**Status:** native-gtk normative visual direction. The theme is implemented through GTK CSS and native widget style classes, **not** HTML, Tailwind or Svelte.

## Philosophy

Quiet, editorial, information-dense and functional. Black is the stage, Prussian Blue structures surfaces, Orange communicates selection and intent. Avoid fake readings, gratuitous glass effects, hover distortion and unstable layout shifts.

## Canonical palette

| Name | Hex | Semantic purpose |
| --- | --- | --- |
| Black | `#000000` | Background |
| Prussian Blue | `#14213d` | Panel and raised card surface |
| Orange | `#fca311` | Selected state, keyboard focus, action accent |
| Alabaster Grey | `#e5e5e5` | Muted labels and secondary text |
| White | `#ffffff` | Primary text |

The first-party theme uses these five colors exclusively. Selected Orange surfaces must use **Black** foreground for legibility. Do not use opacity to invent a sixth visual palette color unless required by the native compositing context and separately reviewed.

## Geometry and density

- Top Bar: exactly **42 logical pixels**, full output width, exclusive zone 42.
- Native transient widgets must fit the current output; prefer right dock for Calendar/System/Settings and top-center Launcher.
- Popup exclusive zone = 0, one transient popup at a time.
- Detached surfaces are normal Niri-managed windows with functional resizing and keyboard focus.
- Content should size naturally. Long content uses native GtkScrolledWindow, not a fixed-height browser viewport.

## Shared component ownership

Every visual role is first a `kira-ui` component with a view model, callback API, semantic style classes, accessible state and tests. `kira-shell` composes these components and must not duplicate their widget trees. Theme and shared density are implemented in `crates/kira-ui/src/style.css`.

## GTK4 component rules

Use accessible GTK widgets: GtkButton, GtkLabel, GtkCalendar, GtkSearchEntry, GtkListBox, GtkStack, GtkPopover where appropriate. Express visual roles with GTK CSS classes. Apply a visible focus outline and accessible names. Avoid handmade control widgets when GTK already provides keyboard and assistive behavior.

For quantitative graphics use GtkDrawingArea/GSK with text equivalents. Samples, axes, units and missing-data state are owned by Rust models. Do not animate measurement values into fiction.

## Animation

[TENET_MOTION.md](TENET_MOTION.md) defines causal memory, reversible scenes and continuity. The target animation engine is GLib/GTK frame clock and GSK, not GSAP. The first native milestone may use instant transitions; functional completion and reduced-motion correctness come first.

## Quality

Review at 100%, 125%, 150%, 200% scale and on small displays. Check click targets, contrast, keyboard navigation, focus, screen readers, single-popup geometry, Niri window behavior, data freshness and graceful disconnected modes.
