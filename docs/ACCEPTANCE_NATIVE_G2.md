# Native G2 acceptance · system monitor and Niri displays

**Status:** manual acceptance checklist; CI/Cargo model tests cannot exercise the real GTK/Wayland compositor. All display changes are experimental until the checks below pass.

## Safety preconditions

Run from a user Wayland session, **not** as a replacement for the live desktop:

```sh
nix develop --command task dev:preview
```

The preview must report a nested Wayland display, a **separate nested NIRI_SOCKET**, a temporary `NIRI_CONFIG`, a separate `XDG_CONFIG_HOME`, and the temporary Kira KDL. It must fail closed when the nested IPC socket is unavailable. Never copy your production Niri config into this test and never run output changes in a production session without an explicit recovery plan.

## System Monitor

- [ ] Open **Система**: GTK Overview shows real sampled CPU, temperature (when supported), available/total memory, load, uptime, battery and GPU. Missing data are shown as unavailable, never zero.
- [ ] Switch to **Процессы**: process entries report PID, name, host CPU share, and PSS or RSS. Sort by CPU, cap visible rows at 80, avoid extreme GTK churn.
- [ ] Keep the popup on Overview or close it: `/proc` process enumeration is stopped. Reopening Processes resumes sampling without blocking keyboard/mouse input.
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

- [ ] Detach Calendar, Launcher, System and Settings with **Отделить** and confirm each becomes a decorated, independently movable and resizable Wayland window. Clicking the same Top Bar item must raise the existing detached window, not create another GTK controller.
- [ ] Change Calendar selection, Launcher search text and System page in their detached windows. Switch to other transients, attach again and reopen: date, query, scrolling and widget identities must survive.
- [ ] Close a detached window using its window-manager control: it should reattach invisibly, preserve state and reopen from the bar. Try rapid attach/detach and Escape, including keyboard focus and screen-reader labels.
- [ ] Keep a detached System window open: continuous sampling should stay active only while the view is visible. A detached Settings view with an unconfirmed display Apply must still roll back when closed.
- [ ] Inspect fractional scaling, focus, monitor geometry, clipping, reduced motion and transient overlap in real nested Niri. Tray remains pooled; detached tray cascades are not implemented or accepted.

## Visual / input and release gates

- [ ] Keyboard navigation, tab order, Escape, visible focus, screen-reader labels and scrolling work on both pages.
- [ ] At 100%, 125%, 150%, 200% fractional scale and on 800×600 / multi-output layouts, no controls overflow the popup or become unreachable.
- [ ] In two-output sessions, the popup opens on the intended current output rather than assuming monitor index zero. **Known gap:** current placement still uses the first output; do not mark this accepted until fixed and tested.
- [ ] `cargo fmt --all -- --check`, `task shell:check`, `task check` and `nix build .#default` pass with no WebKit dependency.
- [ ] Attach actual nested Niri screenshots/logs and results to the migration PR. Until then this is a **testable native implementation**, not validated production feature parity.

## Related contracts

- [KIRA_UI.md](KIRA_UI.md): reusable component first, Desktop composition second.
- [CONFIG_OWNERSHIP.md](CONFIG_OWNERSHIP.md): ownership and Apply/Confirm/Save guarantees.
- [MIGRATION_NATIVE_GTK.md](MIGRATION_NATIVE_GTK.md): overall migration gates and outstanding tray/notification/window parity.
