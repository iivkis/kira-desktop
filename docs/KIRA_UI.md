# Kira UI · reusable GTK4 component library

**Normative for native-gtk. First build the reusable UI library, then build Kira Desktop from it.** This is a hard sequencing rule, not a later cleanup task.

## Architectural boundaries

- `crates/kira-ui` is a reusable GTK4/gtk4-layer-shell library. It owns public view models, native controls, shared CSS, accessibility and popup lifecycle. It can be embedded in another GTK application without Core, Niri, IPC, filesystem writes or privileged commands.
- `crates/kira-services` owns pure Linux system models and trusted native adapters; no GTK or WebKit dependency. Existing tested native policies belong here rather than being deleted with their former WebKit presentation.
- `crates/kira-shell` is the **Kira Desktop composition root**. It translates typed Core snapshots and GIO discovery into `kira-ui` view models, routes callbacks through approved services and starts the application. It must not become another UI component framework.

## Required implementation sequence

1. **Design the reusable component's API:** input view model, events/intents, lifecycle, empty/offline/error states and keyboard/a11y contract. Do not use raw IPC messages, Niri commands or shell strings in the public interface.
2. **Implement in kira-ui:** build the GTK widget tree, visual styles, focus behavior and bounds. It must work without launching Kira Core.
3. **Test the library independently:** cover pure selection, filtering, geometry and state transitions in Rust tests. UI smoke/visual acceptance remains separate from headless tests.
4. **Connect trusted services:** system discovery, launch, metrics, display transactions and notification policies run outside GTK main-thread blocking paths and deliver owned data.
5. **Compose Kira Desktop:** use tested library components from `kira-shell`, translating only typed state and intents. No copy-paste widget implementation.
6. **Accept on Wayland:** run nested Niri, check multiscreen focus/geometry/scaling, a11y, keyboard operation and packaged `nix build .#default`.

## Current component contracts

| Component | Input | Output | Notes |
| --- | --- | --- | --- |
| `Bar` | GTK Application, focus intent handler | 42px layer-shell top bar; title, clock and action slots | Exposes compact density class |
| `WorkspaceStrip` | Plain workspace items, connected status, limit, names toggle | Opaque workspace focus intent | Reuses stable GTK buttons; always shows focused workspace |
| `Launcher` | Ordered `ApplicationEntry { id, name }` | Launch intent by desktop app ID | Async discovery and launch remain Shell/service responsibilities |
| `PopupHost` | Popup kind and view factory | One visible transient; Escape and hide handling | Preserves widget tree between hide/show; detached mode still pending |
| `InfoPanel` | Heading, current detail text | Native accessible read-only summary | Text updated without recreating widget |
| `calendar_view` | None | Native GTK Calendar | Rich calendar/sky view future work |

## Guardrails and acceptance

No WebKit fallback, browser runtime, direct shell execution or arbitrary command execution in UI. Component callbacks express intent only. Kira UI stays independent of the actual compositor and Core transport. Native does not mean secure plugin isolation; third-party native widgets remain out of scope.

The recovered `kira-services` display/notification/metrics modules are **not** evidence of visible feature parity until a native GTK view and integration tests exist. Multi-output popup positioning, detached windows, reduced-motion TENET animations, richer launcher performance and packaged Niri smoke remain explicit follow-up work.
