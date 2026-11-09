# Handoff: продолжение Kira Desktop Native GTK4

**Снимок:** 2026-11-09 · ветка `native-gtk` · HEAD до создания этого документа: `66f69c4a80f6924bbd3f0e337ca323d59955854b`.
**Последний подтверждённый успешный CI:** [GitHub Actions 42502574995](https://github.com/iivkis/kira-desktop/actions/runs/42502574995) на указанном HEAD. Оба job зелёные: Rust Core/IPC и Native GTK4 Shell, включая Nix package, isolated fake-Niri SIGKILL rollback smoke и изолированный headless SNI smoke.
**Базовый исходник:** frozen `main` at `305139949f49176487ef9759fc32c228eadff408`. Не меняйте baseline без отдельного review.

Этот файл предназначен следующему исполнителю, продолжающему **реальную реализацию**, а не новый обзор проекта. Перед любой работой заново проверить HEAD и CI, так как ветка могла сдвинуться. Этот документ не является сертификатом графической приёмки.

## Коротко: реальное состояние

- UI: одна нативная система **Rust + GTK4 / gtk4-layer-shell, GDK/GSK, Cairo**, без WebKit, Svelte, JavaScript, GSAP, браузерного fallback или npm-пакета.
- 15 из 15 встроенных поверхностей имеют состояние **partial** в `docs/MAIN_WIDGET_PARITY.json`; **0 из 15 verified**. Для каждой `verified.content`, `interaction`, `motion` и `pixels` всё ещё false.
- **Сборка и unit/contract CI зелёные, визуальная/анимационная приёмка против main не проведена.** Ни HEAD, ни компиляция не заменяют парные screenshots/frame captures.
- `main` остаётся нетронутой. `native-gtk` экспериментальная, не production-ready.
- **Display Apply/Confirm/Save только внутри проверенного preview**, а в обычной сессии observation-only. Внешний display-supervisor и fake-Niri SIGKILL smoke реализованы, но реальный nested-Niri SIGKILL, hotplug и multi-output rollback остаются обязательными до снятия блокировки.

## Архитектура и запреты

Прежде чем писать код, прочитать `AGENTS.md`, `docs/ARCHITECTURE_NATIVE.md`, `docs/KIRA_UI.md`, `docs/MAIN_WIDGET_PARITY.md`, `docs/MAIN_WIDGET_PARITY.json`, `docs/DESIGN_SYSTEM.md` и `docs/TENET_MOTION.md`.

1. **Сначала reusable `crates/kira-ui`**: типизированная модель, GTK-компонент, доступность/клавиатура, тесты без Core/Niri. Компонент получает plain snapshots и выдаёт typed intents. Никаких команд Shell, произвольного FS, `/proc` или Niri IPC в UI.
2. Затем `crates/kira-services` или Core для доверенных сборщиков, протокола, валидации и транзакций. `crates/kira-shell` только связывает их с компонентами и управляет жизненным циклом.
3. Niri остаётся композитором. GTK Top Bar = 42 логических px exclusive zone; обычный transient popup = зона 0, не более одного активного; detached windows = полноценные Niri-managed decorated xdg-toplevels и **тот же retained GTK tree**, без создания второго контроллера.
4. Данные честные: нет Niri/сервиса/сенсора => состояние unavailable, никакой фальшивой телеметрии. Нет недоверенных D-Bus имен/путей, shell-инъекций и команд из UI.
5. Цвета Kira Default: `#000000`, `#14213d`, `#fca311`, `#e5e5e5`, `#ffffff`. Реальный keyboard focus, accessible labels, reduced motion, source-aligned TENET (обратимость, interruption-safe), а не произвольный fade.
6. **Безопасность:** никогда не заменять живой Niri, KDL, пользовательский D-Bus daemon, tray watcher или notification service для теста. Для D-Bus acceptance нужен **отдельный** `dbus-run-session` / `scripts/accept_nested.py`. Не устанавливать preview-переменные владения шиной в нормальной host-сессии.

## Продолжение после последнего подтверждённого CI 42506642002

Следующая серия реальных коммитов после HEAD `e00b6dd63ca92a4e64b74f38b0a8c8bade401804` добавляет нативный Settings (в предыдущем этапе), активные side-anchored GTK Popover каскады Tray с owner-validated ancestor/sibling actions, и output-local suppression Notifications (pooled vs detached). Первые GTK unit/Clippy проверки этих изменений проходили; **повторно проверить CI именно последнего HEAD** перед выводом о зелёной сборке. Frozen main и все 15 `verified` по-прежнему не менялись. Live Wayland и парные кадры здесь не предоставлены. В частности, частичная headless D-Bus и model QA не подтверждают реальные GtkPopover child surfaces, focus и hit regions. Следующий исполнитель должен начать с этого незавершённого графического G3/G4 gate.

## G4: bounded notification output-host pool and SNI protocol acceptance fixture

After the prior known-green HEAD `37b75b5e9d28d3bac3fea0125848601f69a3f6c6`,
the next iteration implements `kira-ui::NotificationHostSet` with a
maximum eight retained monitor-bound GTK windows. Exactly one output is
selected per notification FIFO; real pooled transients divert toasts to a
different safe monitor when possible. Old action/paint callbacks are
silenced; migration requeues the visible cards and requires fresh frame
ACK before counting the new output's dwell. Unplugged GDK surfaces close
without leaving stale shaped-input regions. The existing D-Bus server
remains a single owner. The acceptance-only SNI binary and headless
private-bus smoke now exercise real `com.canonical.dbusmenu` methods:
typed hierarchy, lazy branch, checkbox, and fixed leaf Event.

**CI status for this paragraph:** code changes are committed but must be
checked against the latest workflow HEAD, not this handoff snapshot. No
real nested-Niri/Wayland presence has been established. Real child popup
focus, multi-output mapping, compositor click-through and TENET/pixel
capture remain separate acceptance gates. The ledger stays 0/15 verified.

## G5: stable notification output affinity, scale and action leases

Starting from prior green HEAD `6c2c957dc56d1589d2572bf02a7e90a4f574b528`
([CI #42537946072](https://github.com/iivkis/kira-desktop/actions/runs/42537946072)),
the next implementation fixes three real lifecycle boundaries:

- `kira-ui` selects the active real GDK monitor stably through enumeration
  reorder when the mapped Top Bar output is blocked or too narrow;
- same-monitor logical resize or scale-factor change invalidates card paint,
  input and FIFO dwell; stale epoch-bound GTK button callbacks cannot act;
- Shell checks painted card identity, current physical target and bus
  ownership on every UI action/ACK. Private SNI protocol smoke now tests
  idempotent lazy menus, disabled/unknown no-op Events and reversible
  checkmarks on a private bus.

**CI for the subsequent HEAD must be checked separately.** These tests are
model and headless protocol tests. The 15-widget ledger remains 0 verified;
native Wayland motion, scale, hotplug, keyboard focus, TENET/pixel evidence,
and full parallel multi-output toast queues remain unaccepted.

## G6: revoke stale native Tray menu actions across owner changes

From a freshly confirmed green HEAD `45732c8a262c89d455937f2de3230180ec8d0120`
(CI `42554001490`), Shell-issued UI epochs have been added to the native
Tray leaf/expand/back/dismiss intents. Every publish of the current
owner-verified `com.canonical.dbusmenu` layout changes its epoch, and the
Shell rejects stale GTK controls before any D-Bus call. This prevents an
old callback with the same numeric handle/ID from operating on a new owner,
or `LayoutUpdated` tree. Pure Rust model tests cover the revision boundary.

**Do not declare this step accepted without checking the newest HEAD CI.**
Native nested-Niri owner churn, focus, geometry, TENET and paired frozen-main
pixel capture still have no evidence. All 15 parity entries remain partial.

## G7: sender-scoped Freedesktop notification replacement

Starting from green `native-gtk` HEAD
`4a0508742dd5f87286022a910057a08722da6520`
(CI `42555358708`), the next feature adds pure
`kira-services::NotificationOwnership` plus Shell GIO integration.
A unique D-Bus peer may replace only its own active notification; a
foreign or expired numeric `replaces_id` allocates a fresh ID.
Close/expiry/action retire the owner lease, while parked persistent
notices retain it. `CloseNotification` preserves ID-based external
compatibility. The isolated nested-Niri protocol acceptance script now
asserts cross-process replacement **does not** overwrite an existing
notification, rather than assuming separate `gdbus` processes have
the same sender identity.

**Check CI on the latest HEAD**, not a historical green workflow.
The live preview test has not run in this environment. Real
nested-Niri GTK, output-hotplug, TENET and pixel evidence are
still required. The parity ledger remains 0/15 verified.

The same iteration now also **revokes the complete notification service
session** when its Freedesktop bus name is lost or explicitly unowned:
no stale waiting/visible/parked IDs, sender leases or pending queue
deadline may survive into a later reacquisition. A Rust regression test
covers the transition. This does not claim graphical Wayland acceptance.

## G8: no cross-epoch notification ID reuse and real dual-peer GIO evidence

Starting from green `native-gtk` HEAD
`c0b6106273bedf610fa2b48182cae9c400c2371c`
(CI `42556866585`), this iteration adds:

- `NotificationQueue::reset_session` preserving ID allocation across
  loss and reacquisition of `org.freedesktop.Notifications`. Stale
  asynchronous close requests cannot hit immediately reused IDs.
- Shell GIO method dispatch requires current name ownership **and**
  the exact live D-Bus connection, even when a stale object registration
  remains reachable through a unique bus name.
- A separately packaged, private-preview-only GIO acceptance executable
  tests two actual persistent peer connections: same-sender replacement,
  foreign-sender rejection, closed-ID rejection and cleanup. Nested-Niri
  acceptance records the real result as unreviewed, with a fail-closed
  missing-binary path. Python and Rust tests cover the guards.

**Do not claim complete CI or nested-Wayland acceptance until the newest
HEAD workflow is checked.** Real screenshot/input/TENET/multi-output
testing and 0/15 verified status are unchanged.

## Что уже реализовано в native-gtk

| Функция | Важные файлы и действующее поведение |
| --- | --- |
| Top Bar / Niri workspaces / Launcher | `crates/kira-ui/src/bar.rs`, `workspace.rs`, `launcher.rs`; real Core/Niri snapshots, source-search ranking, persistent indicator/FLIP, bounded launch intents |
| Calendar / Sky Map | `calendar.rs`, `sky_map.rs`, `sky_model.rs`, `sky_catalog.rs`; ISO week grid, clock digits/motion, offline 2 416 stars + 743 constellation line segments + 89 labels + 39 named stars, explicit Greenwich demo, host-local observation time, atlas **only when Calendar detached** |
| System Monitor | `system.rs`, `process_table.rs`, `system_report.rs`, `system_charts.rs`; true CPU/RAM/GPU, 5 bidirectional process sort columns, 36→180 progressive rows, 8s process freshness, 6s overview-report freshness, GDK clipboard, no fabricated PSS |
| Settings / Displays | `displays.rs`, `display_map.rs`, `crates/kira-shell/src/display_bridge.rs`, `crates/kira-services/src/display_manager.rs`; native monitor layout map with drag/snap/keyboard, selected editor, draft reset, external preview-only Apply→15s Confirm/Revert→separate Save |
| Tray | `tray.rs`, `crates/kira-shell/src/tray_host.rs`, `tray_menu.rs`; real StatusNotifier GIO watcher/host, owner/path validation, bounded pixmaps, pin state, 2/4/6 responsive Top Bar icons, menu AboutToShow/GetLayout/Event(clicked) and live LayoutUpdated, menu navigation keys, detachable retained Tray with distinct close vs raise |
| Notifications | `notification_host.rs`, `crates/kira-shell/src/notifications.rs`, `crates/kira-services/src/notification_queue.rs`; real Freedesktop D-Bus only when safe, 100 queued / 3 visible, shaped input, paint-gated lifetime, 180ms entrance/exit and reduced motion |
| All popup/window roles | `crates/kira-ui/src/popup.rs`, `popup_clip.rs`, `motion.rs`; one transient + independent detached windows for Launcher, Calendar, System, Settings, **Tray**. Retained controller, reversible popup cursor, focus restoration guards, bar monitor targeting |

Многое подтверждено **чистыми тестами/CI**, но результаты `docs/MAIN_WIDGET_PARITY.md` по-прежнему partial. Конкретные оставшиеся пробелы смотрите в JSON-реестре.

## Следующий крупный шаг, приоритет 1: G3/G4 реальная приёмка

**Цель:** не создать шестнадцатый partial-виджет, а проверить уже восстановленную функциональность в настоящем изолированном nested Niri. Если живой Wayland-сеанс недоступен, не имитировать скриншоты, а переходить к приоритету 2.

1. `nix develop --command task dev:preview` для ручного GTK ввода, multi-output, fractionals, focus/reattach. При ошибках устранять конкретные дефекты в `kira-ui` и добавлять регрессионные тесты.
2. `nix develop --command task dev:preview:display-crash` для **настоящей** экспериментальной Apply с отдельным supervisor. Отдельным host-терминалом убить распечатанный PID nested Shell через `kill -KILL PID` до 15s. Проверить реальные Niri output snapshots и точный rollback, затем hotplug/отказ Niri; не считать fake-Niri CI доказательством.
3. `nix develop`; `nix build .#default`; `python3 scripts/accept_nested.py --package-dir result --output /tmp/kira-acceptance-<run-id>` (подставить реальный уникальный путь). Скрипт создаёт **собственную закрытую session D-Bus** и запускает проверяемые реальные Notify и SNI fixture. Результат остаётся **unreviewed** даже при PASS протокола.
4. Для motion снять native кадры через `scripts/capture_native_motion.py` на фактическом nested display, затем **в другой изолированной сессии** те же действия на frozen main. Манифесты имеют реальные sample timestamps; не заявлять покадровой идентичности без paired frame review.
5. Проверить 360×640, 800×600 и 1920×1080, 100/125/150/200% масштаб, hotplug, focus/Escape, reduced motion; сравнить оба UI и реальные движения, приложить безопасные PNG/manifest/review под `docs/parity-evidence/` и обновить конкретный виджет только после четырёх независимых gates.

**Первый сценарий для G3:** отделить Tray с открытым настоящим owner-validated dbusmenu; открыть другой transient; поднять прежнее окно кнопкой Top Bar; проверить сохранение меню, Up/Down/Home/End/Right/Left/Escape, LayoutUpdated и сохранение доступного focus ID, затем закрыть собственным ×, Attach, WM close и снова открыть. Не должно быть второго GTK-контроллера, повторной D-Bus-регистрации или исчезнувшего пункта, способного вызвать действие. Затем проверить bar breakpoints 699/700 и 1099/1100.

## Приоритет 2, если нет живой Wayland графики

Реализовать **полноценный Settings** в `kira-ui`: source-aligned responsive section/sidebar navigation, About/device information из доверенных реальных источников, keyboard/focus состояния и popout без потери выбранного раздела. Сейчас этот виджет только partial; текущее состояние: базовая информация и Displays. Использовать frozen `main` `frontend/src/lib/components/SettingsWidget.svelte` и `frontend/src/lib/settings-model.ts` как источник контракта, **не** переносить Svelte/WebKit. Разбить на логические инкременты со своими model tests и обновить parity-реестр.

**После этого:** notification placement/all-output geometry и collision QA; Tray floating cascading menus с проверкой owner/path и keyboard semantics; G4 motion/HiDPI сравнения для Calendar/SkyMap (в том числе physical-buffer budget), Launcher, SystemCharts, process FLIP, popup contact trace.

## Запуск и обязательные проверки

~~~sh
git switch native-gtk
git fetch origin
git status --short
git rev-parse HEAD
nix develop
task check
task shell:check
nix build .#default
python3 scripts/check_widget_parity.py
~~~

Для ручного запуска (только внутри живой Wayland-сессии, никогда как подмену текущего хоста):

~~~sh
nix develop --command task dev:preview
nix develop --command task dev:preview:display-crash
~~~

Все изменения коммитить **в `native-gtk`**, небольшими осмысленными коммитами внутри крупного законченного этапа. После каждого изменения проверять `cargo fmt --all -- --check`, `cargo clippy --locked -p kira-ui -p kira-services -p kira-shell --all-targets -- -D warnings`, `cargo test --locked -p kira-ui -p kira-services -p kira-shell --all-targets`, Core/IPC и GitHub Actions. Не объявлять CI зелёным до успешной проверки **последнего HEAD**, включая Nix, fake-display rollback и isolated SNI.

## Источники истины и ловушки

- **Живое состояние:** `git log`, HEAD, `docs/MAIN_WIDGET_PARITY.json`, код и последний GitHub Actions. Зафиксированный `main` только как baseline требований.
- **Архитектура:** `AGENTS.md`, `docs/ARCHITECTURE_NATIVE.md`, `docs/KIRA_UI.md`, `docs/CONFIG_OWNERSHIP.md`.
- **Приёмка:** `docs/ACCEPTANCE_NATIVE_G2.md`, `docs/NATIVE_MOTION_ACCEPTANCE.md`, `docs/TRAY_NATIVE.md`, `docs/NOTIFICATIONS_NATIVE.md`, `docs/CALENDAR_NATIVE_PARITY.md`, `docs/SKY_MAP.md`.
- **Некоторые старые обзорные абзацы устарели:** `README.md` до сих пор местами описывает detached windows/Tray как неполные, а старый раздел G3 в `docs/MIGRATION_NATIVE_GTK.md` говорит, что Tray остаётся только pooled. Более поздний раздел **«detached Tray with persistent native app menus»**, код и ledger уже фиксируют detach. Обновлять обзорные тексты по мере работы, не откатывать функциональность по старой формулировке.
- **Никаких вымышленных доказательств:** автоматические тесты моделей, headless SNI и fake Niri ≠ реальные framebuffer-карты, pointer hit regions, Niri SIGKILL или pixel/frame parity. Регистрировать только реально собранные и проверенные evidence.

## Готовый стартовый запрос для следующего исполнителя

> Продолжай Kira Desktop в репозитории `iivkis/kira-desktop`, ветка `native-gtk`. Прочти `docs/NATIVE_GTK_HANDOFF.md`, `AGENTS.md`, `docs/MAIN_WIDGET_PARITY.json` и acceptance-контракты; заново проверь HEAD и CI. Двигайся крупным законченным шагом: в живой изолированной Wayland/Niri среде устрани дефекты detached Tray/keyboard menu и проверь реальные display rollback, а если живого Wayland нет, реализуй недостающий Settings About/sidebar navigation в `kira-ui`, с тестами и Shell composition. Не возвращай WebKit/JS и не включай production display mutation. Коммить реальные изменения в `native-gtk`, обновляй честный реестр и подтверждай CI на последнем коммите. Никаких притворных графических сертификатов.
