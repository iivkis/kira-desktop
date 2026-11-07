# Kira V2 architecture — Kira Engine + Kira Desktop

**Статус:** принято в [ADR-0003](adr/0003-kira-engine-desktop-split.md), **runtime ещё не реализован**.
**Основная спецификация границы:** [ENGINE_DESKTOP.md](ENGINE_DESKTOP.md).
**Другие решения:** [mlua ADR-0002](adr/0002-embedded-lua-mlua.md) (действует); [ADR-0001](adr/0001-lua-rust-gtk.md) (отменён).
**API:** [Lua UI](V2_LANGUAGE.md) · [Lua Styles](V2_STYLES.md) · [Migration](V2_MIGRATION.md).

## 1. Продукты

**Kira Engine:** независимый фреймворк для обычных GTK4 приложений на Lua 5.4. **Rust + mlua + GTK4**. Встроенная Lua VM исполняет обычный Lua-код (local, функции, замыкания, require через controlled loader). Rust регистрирует реальные Lua функции `kira.ui`, принимает WidgetSpec, монтирует GTK-виджеты, управляет state/bind, signal callbacks, lifecycle, темами через GtkCssProvider и ошибками Lua. **Engine не знает Niri и Kira Desktop.**

**Kira Desktop:** отдельная NixOS/Niri/Wayland оболочка **на Engine**. Она создаёт Shell окна с gtk4-layer-shell, подключает Niri IPC, Core, системные сервисы, конфигурацию, разрешённые возможности через `kira.desktop` и реализует свои Lua widgets поверх Engine UI API. **Desktop зависит от Engine, не наоборот.**

```text
Ordinary Lua GTK app               Kira Desktop Lua widgets
         |                                     |
         +-----------------+-------------------+
                           v
                     KIRA ENGINE
                  mlua + Rust + GTK4
             state, WidgetSpec, signals, CSS
                           ^
                           |
                  Kira Desktop Rust host
            layer-shell, Niri, Core/IPC, Linux
```

В диаграмме Desktop host использует Engine для UI, а подключает системные capability providers снаружи. В отличие от Tauri здесь нет WebView, фронтенда JS или HTML. Это **Tauri-подобное framework vs application разделение**, не реализация Tauri.

## 2. Владение и направление зависимостей

Планируемая Cargo структура:

```text
crates/kira-engine/          # NEW reusable library: Rust+mlua+GTK4
crates/kira-desktop/         # NEW application: depends on kira-engine
crates/kira-core/            # current desktop system service
crates/kira-ipc/             # current desktop IPC
crates/kira-shell/           # current V1 legacy reference only
ui/desktop/                  # NEW Lua shell widget sources
examples/v2/counter.lua     # generic Engine fixture (not Desktop-specific)
examples/v2/styles.lua
```

Каталоги `crates/kira-engine`, `crates/kira-desktop` и `ui/desktop` **ещё не существуют**. Не переименовывать существующий V1 `kira-shell` в `kira-engine`: его WebKit и слой Niri не должны оказаться в базовом Engine. Добавлять новый crate только с реализацией.

**Разрешено:** `kira-desktop → kira-engine` и `kira-desktop → kira-core/kira-ipc`.
**Запрещено:** `kira-engine → kira-desktop/kira-core/kira-ipc/niri/gtk4-layer-shell` на уровне универсального Engine runtime.

Desktop-specific возможности инжектируются снаружи через версионированный Rust host/capability interface, доступный только при создании Engine внутри Desktop.

## 3. Lua runtime и GTK

1. Generic GTK app host или Desktop host создаёт `Kira Engine` instance на GTK main thread.
2. Engine создаёт `mlua::Lua` с контролируемой средой, на M1 без `send`, регистрирует `kira.ui` и ограниченный module loader.
3. App Lua code выполняется через mlua и вызывает Rust-реализованные `ui.box`, `ui.text`, `ui.button`, `ui.state`, `ui.bind`, `ui.theme`, `ui.compose`. Это **функции реального Lua**, не AST DSL.
4. Engine монтирует WidgetSpec в native GTK4; хранит Lua callbacks и subscriptions пока view существует, обрабатывает Gtk signals и освобождает ресурсы при unmount.
5. Бэкграунд-сервисы Desktop передают typed messages в GTK main context, а не трогают GTK/Lua из worker thread.
6. Lua styles валидируются Engine и преобразуются **во время исполнения** в GTK CSS для `GtkCssProvider`; никаких Sass/Grass/PostCSS/Tailwind и Lua→Rust генерации.

## 4. Lua API уровни

**`require("kira.ui")`** (Engine): standalone-совместимые виджеты, layout, events, state, binding, styles, lifecycle. Работает и без Desktop.

**`require("kira.desktop")`** (Desktop): optional module с разрешёнными OS/Niri действиями и state sources через Desktop Rust/Core API. Его нет в обычном Engine app. Уточнить реальные функции перед реализацией; не обещать API, который не существует.

Rust Desktop host также управляет focus, keyboard navigation, layer-shell, popup geometry, display safety, accessibility и нативным motion. Никакого нового compositor.

## 5. Доверие и производительность

`mlua` не является изоляцией от OS. M1 рассчитан на **доверенные локальные Lua-файлы**. Lua loader не должен предоставлять arbitrary C modules, raw D-Bus, файловые пути вне root, system commands или unrestricted GTK objects. В будущем непроверенные плагины потребуют изоляции отдельным процессом.

GTK UI и Lua callbacks должны исполняться в GTK main context. Ошибка Lua handler не должна ронять сигнал GTK: catch/report, dispose lifecycle refs, budget execution/memory. Встроенная Lua VM имеет стоимость по памяти, измерять её, а не обещать чудеса. Hot reload после safe teardown требует отдельной реализации.

## 6. Acceptance milestones

**Engine-alone M1:** `examples/v2/counter.lua` + `styles.lua` запускаются в **обычном** GTK Application без Niri/desktop; GTK Button вызывает настоящую Lua closure, Rust-backed state меняет label и Lua style применяется. Нет WebKit, Svelte и Lua→Rust транслятора.

**Desktop M2:** тот же Engine создаёт Lua-based bar/popup внутри GTK layer-shell в nested Niri. Workspace data попадают через Desktop `kira.desktop` provider; Engine не импортирует Niri. Проверить typed IPC, focus, масштаб, keyboard, multi-output и teardown.

**Delivery:** Nix/Cargo packaging со встроенным mlua (Lua 5.4); независимая проверка dependency graph Engine и regression tests Desktop. Legacy V1 code сохраняется только до parity. Существующие green CI **не** доказывают новую архитектуру.
