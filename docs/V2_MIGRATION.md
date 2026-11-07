# Kira V2 — migration to Kira Engine + Kira Desktop

**Статус:** архитектурный контракт принят, но ни новый Engine crate, ни новый Desktop binary **ещё не реализованы**.
[Engine/Desktop contract](ENGINE_DESKTOP.md) · [ADR-0003](adr/0003-kira-engine-desktop-split.md) · [ADR-0002](adr/0002-embedded-lua-mlua.md).

## Принципы

- **Kira Engine** — reusable Rust+mlua+GTK4 framework для обычных Lua GTK-приложений, **без Niri/Wayland shell dependencies**.
- **Kira Desktop** — desktop-shell приложение на Engine. Desktop добавляет слой Niri/Wayland, Core/IPC, system capabilities и собственные Lua-виджеты, не внедряя Desktop зависимости в Engine.
- Реальный Lua 5.4 исполняется через mlua **при работе**, без собственного Lua→Rust компилятора и без WebView на новом native UI path.
- Стили на Lua превращаются через runtime Engine bridge в GTK CSS, без отдельного CSS-препроцессора.
- V1 legacy GTK/WebKit/Svelte, Nix/CI и тесты остаются до feature parity.
- Зелёный CI V1 или тест документации не означает, что V2 работает.

## Milestones и проверяемая граница

### M0 — architecture split (completed in documentation)

- Принять [ADR-0003](adr/0003-kira-engine-desktop-split.md), явно различить универсальный Engine и использующий его Desktop.
- Уточнить независимый `kira.ui` API и дополнительный Desktop-only `kira.desktop` module.
- Объявить планируемые crate boundaries; не менять Cargo workspace до создания реального нового кода.

### M1 — standalone Kira Engine (future)

- Создать **library crate** `crates/kira-engine` с `mlua` (`lua54`), `gtk4`; проверить vendored/system Lua 5.4 в Nix. Обновить Cargo.lock настоящим Cargo.
- Сделать воспроизводимый GTK `Application` host для Engine без Niri или shell-specific dependencies.
- Реализовать `require("kira.ui")`, контролируемый module loader, Lua VM lifecycle и `ui.box/text/button` → native GTK.
- Реализовать `ui.state`, `ui.bind`, lifecycle Lua callbacks/registries и runtime `ui.theme`/`ui.compose` → GTK CSS.
- Исполнить [counter.lua](../examples/v2/counter.lua) + [styles.lua](../examples/v2/styles.lua) в GTK4; проверить реальные Lua callbacks, state label binding, hover/disabled, error recovery, memory/budget и teardown.
- Проверить **полное отсутствие** прямых и транзитивных Engine зависимостей на `kira-desktop`, `kira-core`, `kira-ipc`, Niri и `gtk4-layer-shell`.

### M2 — Kira Desktop on Kira Engine (future)

- Создать **Desktop application crate** `crates/kira-desktop`, зависящий от `kira-engine`, с desktop-specific Core/IPC, Niri и `gtk4-layer-shell`.
- Вынести Lua-виджеты оболочки в `ui/desktop/`; создать отдельный проверяемый provider `kira.desktop`, регистрируемый Desktop host.
- Смонтировать настоящую 42px top bar и работу workspace в nested Niri через тот же Engine.
- Проверить focus, keyboard/accessibility, масштаб, multi-output, disconnect handling, safe system actions и GTK main thread.

### M3 — migration to native widgets (future)

- Мигрировать Launcher, Calendar, System Monitor, Settings, Tray, Notifications на Lua/GTK Engine widgets по одному.
- Оставить Kira Default palette и нативные TENET motion principles; не перетаскивать GSAP/Web CSS.
- Измерить startup, CPU, latency, RSS/PSS и безопасность уничтожения/перезагрузки Lua views.

### M4 — packaging and parity (future)

- Cargo/Nix сборка Engine как библиотеки и Desktop как приложения; без JS/WebKit в финальном native UI pipeline.
- Separate Engine generic GTK tests, Desktop nested Niri tests и dependency-direction gate.
- Удалить старые V1 WebKit/Svelte/Node зависимости **только после** проверенного parity.

## Компоненты: сейчас vs цель

| Component | Сейчас | Целевой V2 |
| --- | --- | --- |
| Kira Engine | **Нет crate** | Rust library, mlua+GTK4 + generic `kira.ui` |
| Kira Desktop | `kira-shell` legacy V1 | Отдельный binary, использующий Engine |
| System/Core/IPC | Rust legacy | Desktop-only services, not Engine dependencies |
| Lua desktop widgets | Примеры API | `ui/desktop/` использует Engine + `kira.desktop` |
| Native UI styles | WebKit Tailwind V1 | Lua → Engine runtime GtkCssProvider |
| Transpiler | Не реализован | **Не нужен и не планируется** |

Это план внедрения, а не заявление о существовании Engine.
