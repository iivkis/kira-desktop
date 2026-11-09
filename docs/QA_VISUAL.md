# Native GTK4 graphical acceptance

**No automated browser smoke applies in the native-gtk branch.** A successful Rust build does not prove correct Wayland behavior. Launch `nix develop --command task dev:preview` and verify inside the nested compositor: 42px top-bar exclusive area, compositor-owned workspace state and focus, one transient popup, GTK input and keyboard focus, app launch, calendar, truthful offline and missing-data states, safe close, scaling and accessibility.

Accept at output scales 100/125/150/200%, compact geometry and detached ordinary Niri windows once implemented. Capture only with the user's explicit opt-in and a tool bound to the *nested* WAYLAND_DISPLAY. Never claim the former WebKit/Svelte overlay smoke scripts test native GTK.

For the new native System Monitor and Display Settings, use the explicit
[ACCEPTANCE_NATIVE_G2.md](ACCEPTANCE_NATIVE_G2.md) test matrix at all supported
GTK scale factors. Do not take production-output Apply/Confirm screenshots on
the live user's compositor.

## Automated visual smoke versus manual source parity

Ordinary source pushes reuse their **already built** Nix package for
a **34-frame real Weston → nested Niri pixel-state smoke**, with no
permanent PNG artifacts. Native Cairo/GTK paint, SNI/Notifications
and toast entry/exit are still tested. The separate 1
is manual-only: select 2 for the original 104-frame
timelines and opt in to 3 only when a human
needs a short-lived archive. See 4 and
5 for policy and durations.

For manually captured source and native sequences use
6. It rejects unmatched frame counts,
viewport size, stimulus, unsafe paths and malformed PNGs;
the static comparison index is **not** a parity certificate.
Do not mark any widget verified before genuine frozen-main provenance,
focus, reduced motion, accessibility and pixel review.
