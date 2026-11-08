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
| `WorkspaceStrip` | Plain workspace items, connected status, limit, names toggle | Opaque workspace focus intent | Reuses keyed GTK buttons, retained horizontal selection and 340ms position reflow on observed layout-order changes; graphical parity unverified |
| `Launcher` | Ordered `ApplicationEntry { id, name }` | Launch intent by desktop app ID | Async discovery and launch remain Shell/service responsibilities |
| `PopupHost` | Popup kind and view factory | One visible transient; Escape and hide handling | Preserves widget tree between hide/show; detached mode still pending |
| `InfoPanel` | Heading, current detail text | Native accessible read-only summary | Text updated without recreating widget |
| `SystemMonitor` / `ProcessTable` | Typed metrics, GPU readings, bounded process snapshots | Overview / Processes, case-insensitive multi-token name/PID search, CPU/name/PID/RSS/PSS sorting, explicit copy-report intent with delivery errors | Missing values stay unknown, at most 80 displayed/reported rows; snapshots expire after 8s without a fresh worker sample (no permanent UI polling), and scan limitations are labelled; sampling stays on a worker. Pixel and motion parity remain unverified |
| `DisplaySettings` | Typed output/mode descriptions, transaction phase and persistence status | Refresh, Apply(draft), Confirm(id), Revert(id), Save intents | Native 15-second countdown; host can disable mutations; no Niri command or raw JSON in library |
| `Calendar` | Local live date/time, optional ISO-date copy intent | Native editable six-row Monday-first 8-column ISO week grid, month/year/day navigation, Today, ISO date + feedback | Dates/ISO tests ported from main; 400ms digits, 380ms click flights/26ms stagger, 440ms month compositor and 520ms inverse selected-day traces are native. A retained selection fill is wired; detached SkyMap and paired visual proof pending |
| `FlowIndicator` | Measured GTK target geometry, axis (horizontal/both), fill or rail style | One 420ms `back.out(1.18)` indicator retargeted from its actual in-flight rect; 120ms monotone opacity and reduced-motion settling | Used by Calendar, Launcher and Workspaces; frame/motion evidence still required |
| `ReflowRow` / `ReflowScope` / `TenetScroll` | Stable row IDs, GTK allocation snapshots and real scroll adjustment | Source 340ms keyed FLIP for at most 24 retained rows, disabled for Launcher filters starting above 20 entries; frozen previous paint on interruptions, full row-offset reset on scope cancellation or GTK unmap, 340ms interruptible power3.out nearest-visible scrolling with layout-row bounds, unmap settlement and immediate reduced-motion settling | Launcher filtering and keyboard movement, WorkspaceStrip order changes; stable GTK width expansion, disconnected workspace offsets reset, user scroll wins; native frame acceptance outstanding |
| `MotionToken` / `CalendarFragmentScene` / `ClockDigits` / `PopupTimeline` / `PopupSection` | Event-lifetime cursor, per-digit 400ms history, source section stagger and recorded four-flight paths | Frame-clock digit exchange and saved-sector click juggling, source-timed GSK month compositor, measured-day inverse fragments, GSK-clipped reversible popup, GTK section transforms and saved INVERTED traces | Reduced-motion settling and basic interruption modeled; nested-Niri motion, focus and geometry acceptance remain unverified |

## Guardrails and acceptance

No WebKit fallback, browser runtime, direct shell execution or arbitrary command execution in UI. Component callbacks express intent only. Kira UI stays independent of the actual compositor and Core transport. Native does not mean secure plugin isolation; third-party native widgets remain out of scope.

The native System Monitor and DisplaySettings GTK views are now wired to their existing Rust services, but **passing headless Cargo tests does not demonstrate real Niri display transactions or visual parity**. See [ACCEPTANCE_NATIVE_G2.md](ACCEPTANCE_NATIVE_G2.md) for mandatory nested graphical scenarios. StatusNotifier tray, native notification presentation, multi-output popup positioning, detached windows, source-complete TENET animations, richer launcher performance and packaged Niri smoke remain explicit work.

**Live-output safety gate:** `kira-shell` disables Apply/Confirm/Save outside the controlled nested preview. A 15s worker timeout survives GTK main-thread stalls, but not termination of the entire Shell process. Before enabling production display changes, move the watchdog to Core or a separate supervisor and test process-death rollback. Setting `KIRA_PREVIEW` manually is **not** a security boundary; use only the verified nested preview script.
