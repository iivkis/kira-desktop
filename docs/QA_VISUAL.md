# Native GTK4 graphical acceptance

**No automated browser smoke applies in the native-gtk branch.** A successful Rust build does not prove correct Wayland behavior. Launch `nix develop --command task dev:preview` and verify inside the nested compositor: 42px top-bar exclusive area, compositor-owned workspace state and focus, one transient popup, GTK input and keyboard focus, app launch, calendar, truthful offline and missing-data states, safe close, scaling and accessibility.

Accept at output scales 100/125/150/200%, compact geometry and detached ordinary Niri windows once implemented. Capture only with the user's explicit opt-in and a tool bound to the *nested* WAYLAND_DISPLAY. Never claim the former WebKit/Svelte overlay smoke scripts test native GTK.

For the new native System Monitor and Display Settings, use the explicit
[ACCEPTANCE_NATIVE_G2.md](ACCEPTANCE_NATIVE_G2.md) test matrix at all supported
GTK scale factors. Do not take production-output Apply/Confirm screenshots on
the live user's compositor.
