# Native G2 acceptance · system monitor and Niri displays

**Status:** manual acceptance checklist; CI/Cargo model tests cannot exercise the real GTK/Wayland compositor. All display changes are experimental until the checks below pass.

## Safety preconditions

Run from a user Wayland session, **not** as a replacement for the live desktop:

```sh
nix develop --command task dev:preview
```

The preview must report a nested Wayland display, a **separate nested NIRI_SOCKET**, a temporary `NIRI_CONFIG`, a separate `XDG_CONFIG_HOME`, and the temporary Kira KDL. It must fail closed when the nested IPC socket is unavailable. Never copy your production Niri config into this test and never run output changes in a production session without an explicit recovery plan.

## Automatic genuine-Wayland smoke (not a parity certificate)

The new auto-triggered `graphical-smoke.yml` builds the packaged native
GTK desktop and captures 102 *actual* nested-Niri PNG frames over three
independently isolated viewports (800×600, 1280×800, 1920×1080),
asserting real grim screenshot dimensions and bounded 34-frame sequences
per viewport, checking limited bar and toast predicates. This complements the fast
Core/IPC and GTK compile/Clippy/test jobs. It does not test keyboard
focus, fractional scales, multi-output geometry, full widget content
or paired source/main TENET trajectories. Those scenarios still require
the manual checklist and frame evidence below.

## Design and motion regression gate (still unverified)

- [ ] During a half-open/half-closed popup, change real monitor, output origin, scale or size: old GDK frame callbacks must cancel and old physical scene hide before new Wayland hints; the new output begins at its own first painted frame, without stale original trajectory or stolen focus.
- [ ] Open, close partway, and immediately reopen each pooled popup **without changing output or size**. The original TENET contact must be pixel-stable, with no new geometry hint or abrupt seam origin. Repeat while moving the Top Bar to another real monitor with identical resolution, on same-monitor output width/origin/scale changes even when the panel stays nominally 420px wide, and on available-height change: the old contact must be invalidated and captured against the new physical geometry, without a stale cross-output flight. Verify focus, reduced motion, single-transient ownership and detached reattachment after each case.
- [ ] Delay the first Wayland map of every popup then open it: the clipped materialization must begin at recorded frame zero, not jump past its seam/sections while GTK allocates. Interrupt opening with Escape and immediately reopen; compare original forward/reverse flight coordinates, no rectangular flash and stable focus. Disable `gtk-enable-animations` during a running scene and confirm an immediate factual endpoint. Repeat at 125%, 150%, 200% scale.
- [ ] In SystemCharts, click beyond the currently revealed path while the trace is growing or being retargeted. The inspector must **not** choose an unrevealed measurement, and clicks in the left-axis gutter or at width below 80px must be ignored. Once the genuine sample becomes painted, clicking the matching X selects exactly it; stale GPU readings remain unknown. Compare click marker and selected text against paired native/source frame sequences.
- [ ] Inspect all five native transient and detached Close/Attach controls for at least a 34px target, the five canonical theme colors, visible focus and one transient popup. Do not treat the static design linter as a pixel/focus screenshot.

## System Monitor

- [ ] Open **Система**: GTK Overview shows real sampled CPU, temperature (when supported), available/total memory, load, uptime, battery and GPU. Missing data are shown as unavailable, never zero.
- [ ] In **ИСТОРИЯ НАГРУЗКИ**, click the oldest, middle and latest real sample in the Cairo trace and verify the single orange marker matches the painted timeline without preceding an in-flight reveal. Tab to the graph, use Left/Right/Home/End, then the previous/next/**СЕЙЧАС** buttons. The selected CPU/RAM/GPU, age, unavailable values and point count must exactly match collected 500ms pulses, including sparse optional 2s GPU peaks. **КОПИРОВАТЬ** should copy only the selected real snapshot through the trusted GDK handler, or an explicit error if clipboard unavailable. Hold a point until its 60s window expires: it must be unpinned rather than replaced with another measurement. Disconnect the worker, wait for freshness expiry, hide/reopen the chart, test keyboard focus, reduced-motion and narrow 360px widths with wrapped controls. Repeat at 125/150/200% scaling and compare real source/native motion captures.
- [ ] Switch to **Процессы**: each row shows real PID, process name, kernel state in tooltip and host CPU share, with unknown CPU/RSS/PSS explicitly missing rather than zero. Test column buttons (Name/PID/CPU/RSS/PSS) in **both ascending and descending directions**, stable PID tie-breaks and missing values always last. The active header must have one retained orange underline following the actual button geometry. Filter or sort a <=24-row set: surviving actual GTK buttons should follow 340ms keyed TENET FLIP; when >24 rows are displayed, do not animate broad list reordering. Observe repeated 2s worker measurements: row identity, focus and scroll must survive without a misleading decorative reflow.
- [ ] Use Ctrl+F to focus the process search; enter an unmatched query and clear it with both the inline × and empty-state CTA. Tab into process rows, select with Enter/Space, navigate ↑/↓, Home/End and PageUp/PageDown; selections cannot leave the actually rendered page. The selected summary and snapshot-only **Скопировать процесс** must show only observed PID/name/state/CPU/RSS/PSS (no process signals or shell commands). Refresh data with PID renamed/reused, switch tabs or let 8s expire: no stale selection or copied metrics may survive. Check screen-reader names and reduced-motion focus stability.
- [ ] With more than 180 matching processes, first show 36 rows. **ПОКАЗАТЬ ЕЩЁ** expands 36 at a time up to 180, never enumerating all into GTK. Filter or change sort: disclosure resets to 36. Copying a filtered/sorted process report is bounded at 180 rows and must preserve actual selected sort direction.
- [ ] On **Обзор**, wait for a real worker measurement and copy **КОПИРОВАТЬ ОТЧЁТ** using the native GDK clipboard: compare CPU/RAM/SWAP/disk/load/uptime/battery/GPU against measured values; missing readings must remain unavailable and no more than eight GPUs/unsafe multiline names may be exported. Disable copy after 6 seconds without refresh, on unmap, or after failure; a new sample must reenable it.
- [ ] Keep the popup on Overview or close it: `/proc` process enumeration stops. Reopening Processes resets the old snapshot and its 8-second timeout without recreating the controller; process events arriving while another view is active must not populate an invisible/stale table. Verify detached System continues sampling only while actually visible and on its Processes tab.
- [ ] Disconnect or simulate missing `/proc` data: no fake values or process rows. Confirm resource usage is bounded with many processes.
- [ ] Press Escape, reopen the popup and switch tabs: the selected view retains state and responds correctly.

## Displays: transactional contract

For a dedicated real-compositor crash test (not a substitute for paired main/native
visual QA), run `nix develop --command task dev:preview:display-crash`.
The coordinator verifies the nested Niri socket, captures actual output mode,
scale, position, transform and VRR before Shell startup, and prints the **nested
GTK Shell PID**. In the nested **Настройки → Дисплеи**, apply one reversible
change. From a separate host terminal, execute the printed `kill -KILL PID`
before the 15-second confirmation timer expires. The coordinator keeps Niri
alive for up to ten seconds after Shell SIGKILL and compares actual output
snapshots. It prints PASS only when a changed state was observed while Shell
was alive and the baseline was restored afterwards; no observed change or
incorrect termination is INCONCLUSIVE/FAIL. This **does not touch host Niri**,
does not verify multi-output/hotplug, and does not certify visual or parity
quality. Preserve the console transcript for manual acceptance.

- [ ] In the normal (non-preview) desktop session, Settings can refresh outputs but **Apply/Confirm/Save are disabled** until process-external rollback exists. In official nested preview, the same reusable widget allows these mutation actions.
- [ ] Open **Настройки → Дисплеи** and refresh; only Niri-advertised connectors/modes/VRR capabilities are offered. No input causes a Niri command during mere selection.
- [ ] On two nested outputs with different resolutions, negative coordinates, fractional scale and 90° transform, confirm the 230px native **РАСПОЛОЖЕНИЕ** map draws correct relative rectangles and visibly highlights the selected connector. Select a disabled output from the accessible button list; its editor must still be available while the disabled screen is absent from the map.
- [ ] Drag one enabled output to each side of its neighbour: nearest edge must snap within 60 logical pixels. Verify updated numeric X/Y fields match the map, keyboard arrows move 10 logical pixels and Shift+arrows move one, and no Niri IPC mutates outputs until **Применить**. Test clamping at ±32000, narrow settings window, 125%/150% scale and unplug during a drag.
- [ ] Edit transform, scale, resolution and VRR, then **Сбросить черновик**: all visible fields and the map must return to the latest observed Niri snapshot, without Apply, Save or KDL writes. Re-fetch after a hotplug and verify the selected connector survives if still present, otherwise falls back to first.
- [ ] If the external supervisor exits unexpectedly, the editor must stop accepting drag/Apply/Confirm/Save **and Refresh** (its pipe is dead), surface a real error, and explicitly require a new Shell session. Ordinary production observation-only mode still supports Refresh via its separate read-only worker. Keep real nested Shell SIGKILL/rollback acceptance separate.
- [ ] Confirm that Apply is disabled if all monitors are disabled, the mode list is empty, or a value is outside accepted bounds.
- [ ] In nested Niri, make one harmless reversible change (e.g. small scale adjustment); press **Применить**. The UI shows a real countdown, with separate **Подтвердить** and **Откатить** buttons. No KDL file is written yet.
- [ ] Do not confirm: after 15 seconds the trusted worker rolls the output state back. Reopen Settings and refresh. The old state must be observed.
- [ ] Apply again and press **Откатить**: state returns without waiting for the timeout.
- [ ] Apply and close the popup (Escape or switch to another popup) while confirmation is pending: the Shell requests early revert and the worker executes it while alive.
- [ ] **Shell-process death / crash rollback:** An independent `kira-shell --display-supervisor` process now owns display transactions and the 15-second deadline, with a separate process group and bounded JSON-lines pipe. Automated headless CI tests exercise EOF rollback, explicit Confirm, and an actual `SIGKILL` of a disposable parent process using a fake Niri CLI. This verifies process separation and signal handling, **not** the real compositor. Still required: kill the actual GTK Shell with `SIGKILL` while nested Niri is alive and verify rollback, including interruption during Apply and Niri hotplug. No production mutation until graphical failure-path acceptance passes. The external worker itself is not immune to termination, power loss or Niri IPC failure.
- [ ] Apply and press **Подтвердить**: session state remains changed; the editor then separately offers **Сохранить**.
- [ ] Press **Сохранить**: if ownership allows it, only temporary preview-managed KDL is changed, **not the live user's config**. Confirm the temporary receipt and output fragment are created only inside the preview sandbox.
- [ ] Test a managed KDL conflict and a read-only/Nix-generated root in a throwaway test environment: Save fails closed and reports why. It must not rewrite foreign includes/output sections.
- [ ] Simulate output hotplug or an external Niri settings change between Confirm and Save: persistence rejects the stale target.
- [ ] Disconnect Niri during Apply, during pending confirmation and during Save: observe error handling and best-effort rollback; never report success without verification.
- [ ] Modify a confirmed selection before saving: Save becomes disabled until another Apply and Confirm. Reopen a confirmed settings view: session confirmation should not be silently repeated.

## Detached native window acceptance

- [ ] Open each of Launcher, Calendar, System, Settings and Tray and activate its visible **×** without using Escape. The pooled popup must close on its recorded reverse timeline and retain search, date, process and menu controllers. Check screen-reader labels and focus-visible styling.
- [ ] Detach each view and press its native **×**: only that decorated window closes, with no newly raised transient or loss of widget state. When no transient is active, pressing **Прикрепить** must immediately return the retained view to the pooled popup. If another transient is active, **Прикрепить** must park the detached view without replacing it.
- [ ] Switch rapidly between transients while an old popup is closing, then send Escape or a queued WM close to that old surface: the newly active popup must remain open and own its identity.


- [ ] Detach Calendar, Launcher, System, Settings and Tray with **Отделить** and confirm each becomes a decorated, independently movable and resizable Wayland window. Clicking the same Top Bar item must raise the existing detached window, not create another GTK controller.
- [ ] Change Calendar selection, Launcher search text and System page in their detached windows. Switch to other transients, attach again and reopen: date, query, scrolling and widget identities must survive.
- [ ] Close a detached window using its window-manager control: it should reattach invisibly, preserve state and reopen from the bar. Try rapid attach/detach and Escape, including keyboard focus and screen-reader labels.
- [ ] Keep a detached System window open: continuous sampling should stay active only while the view is visible. A detached Settings view with an unconfirmed display Apply must still roll back when closed.
- [ ] On two monitors with different geometries, open Calendar and Settings from the Top Bar's actual output. Verify native popup monitor, width, top margin and scroll bounds match the bar's GDK surface. Switch outputs or hotplug before reopening: no stale forced monitor or off-screen popup. A detached xdg-toplevel remains compositor-positioned.
- [ ] Focus a Launcher search entry and enter a query, detach then raise the same window via the bar and reattach. Focus restoration must never hijack another popup during rapid switching. Test Escape, focus-visible and screen-reader names on both layer-shell and detached windows.
- [ ] With a real SNI fixture and an open nested app menu, detach Tray and activate another transient. Click the Top Bar overflow: the same decorated Tray window must raise, with the same owner-validated checked/radio state. Its own × must close, never raise. Test Attach and WM close: reopen via bar and confirm exactly one pooled transient.
- [ ] In a real owner-scoped D-Bus menu, test Up/Down, Home/End, Right on a submenu, Left/Escape for its parent, and Enter on enabled leaves. Rapid LayoutUpdated and app unregistration must not leave a stale focused action or invoke a missing owner's ID. Specifically queue an old child-menu click, Back/Escape and dismissal across a live menu revision and a different unique owner advertising identical handle/ID values: the old GTK epoch must be rejected, while the new painted menu remains interactive.
- [ ] Inspect fractional scaling, focus, monitor geometry, clipping, reduced motion and transient overlap in real nested Niri. Native side-anchored Tray menu cascades are implemented but not accepted: verify real Wayland popup placement, focus, submenu mouse access, detach/reattach, owner changes and every Left/Escape path.
- [ ] With two or three notifications shown, focus a real close or action button using the keyboard. Deliver an unrelated new card, remove a different card and reorder the queue. The **unchanged notification** must retain the original GTK button and its keyboard focus (no child unparent), remain visibly painted without a two-frame blink, not replay its entrance, and still dispatch *exactly once* after the new two-frame paint acknowledgment. Newly added cards must remain at y=12px/opacity zero until their real 180ms animation begins. Interrupt its 180ms arrival with an unrelated update: the retained card must settle at full opacity and exact position. Replace the **same ID** with new summary/actions: old GTK buttons and queued callbacks must become inert; the new card's actions must work only after paint. Invalid/doubled IDs must be hidden safely, not create duplicate controls. Recheck actual shaped input, zero-click-through, 3-second dwell, window scaling, reduced motion and A→B→A output switches.
- [ ] On two different outputs, leave Calendar/System/Settings/Tray **detached** and verify notifications are not globally starved. Open a **pooled right-side popup** on the bar output: its overlapping toast must pause and release shaped input immediately. If notification geometry picks another output, verify it does not falsely inherit obstruction from the bar. Hotplug or lose the bar monitor while a popup is open: unknown affinity must fail closed; test the queue after reconnection. Reorder GDK monitor enumeration while a safe fallback is active: notifications must remain on the same physical monitor; restoring the usable bar output should return them there with fresh paint acknowledgment. Resize or change scale on the *same* GDK monitor with a displayed toast: its old pointer region must disappear immediately and dwell must restart after a new mapped frame. Activate an old GTK card/button after A→B→A selection or same-ID replacement: no stale ActionInvoked/NotificationClosed may fire.

## Visual / input and release gates

- [ ] Keyboard navigation, tab order, Escape, visible focus, screen-reader labels and scrolling work on both pages.
- [ ] At 100%, 125%, 150%, 200% fractional scale and on 800×600 / multi-output layouts, no controls overflow the popup or become unreachable.
- [ ] In two-output sessions, the popup uses the mapped bar's actual GDK monitor, rather than monitor index zero. The source now handles this, but real nested Niri tests, output removal and fractional scaling are still required before acceptance.
- [ ] `cargo fmt --all -- --check`, `task shell:check`, `task check` and `nix build .#default` pass with no WebKit dependency.
- [ ] Attach actual nested Niri screenshots/logs and results to the migration PR. Until then this is a **testable native implementation**, not validated production feature parity.

## Related contracts

- [KIRA_UI.md](KIRA_UI.md): reusable component first, Desktop composition second.
- [CONFIG_OWNERSHIP.md](CONFIG_OWNERSHIP.md): ownership and Apply/Confirm/Save guarantees.
- [MIGRATION_NATIVE_GTK.md](MIGRATION_NATIVE_GTK.md): overall migration gates and outstanding tray/notification/window parity.
