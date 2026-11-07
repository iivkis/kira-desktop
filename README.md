# Kira Engine + Kira Desktop (V2)

**Ветка:** `kira-desktop-v2` · **Статус:** архитектура зафиксирована, **Kira Engine ещё не реализован**. Унаследованная V1 пока остаётся в репозитории.

В проекте два самостоятельных слоя:

**Kira Engine** — универсальный нативный движок/фреймворк **Rust + mlua (Lua 5.4) + GTK4** для создания GTK-приложений на Lua. Engine предоставляет `kira.ui`: виджеты, реактивное состояние, обработчики, стили и жизненный цикл. Lua исполняется внутри Rust-процесса; **никакого Lua→Rust-транслятора и WebView**.

**Kira Desktop** — **рабочее окружение, построенное на Kira Engine**, с Niri/Wayland, layer-shell, панелями, лаунчером, настройками и интеграцией Linux. Desktop зависит от Engine и добавляет контролируемый `kira.desktop` API, не меняя универсальную часть движка.

```text
Lua GTK app ----> KIRA ENGINE <---- KIRA DESKTOP Lua widgets
                     |                      |
            Rust / mlua / GTK4          Desktop Rust host
            state / UI / CSS           Niri / Wayland / Core / IPC
```

**Строгое правило:** Kira Engine не зависит от Kira Desktop, Niri, `kira-core` или `gtk4-layer-shell` и должен запускать обычное GTK-приложение самостоятельно. Desktop использует движок как библиотеку. Это похоже на разделение **Tauri framework / Tauri app**, только вместо WebView у нас **Lua + GTK4**.

## Архитектура и решения

- [Kira Engine vs Kira Desktop](docs/ENGINE_DESKTOP.md) — **основной контракт** независимости, API и владения.
- [ADR-0003](docs/adr/0003-kira-engine-desktop-split.md) — решение о разделении Engine/Desktop.
- [V2 architecture](docs/V2_ARCHITECTURE.md) — полная схема runtime.
- [ADR-0002](docs/adr/0002-embedded-lua-mlua.md) — решение запускать Lua через mlua; [ADR-0001](docs/adr/0001-lua-rust-gtk.md) исторически отменён.
- [Lua UI API](docs/V2_LANGUAGE.md) · [Lua Style API](docs/V2_STYLES.md).
- [План миграции](docs/V2_MIGRATION.md) · [ревью](docs/V2_REVIEW.md) · [AGENTS.md](AGENTS.md).

## Generic Engine example

Примеры [counter.lua](examples/v2/counter.lua) и [styles.lua](examples/v2/styles.lua) **не знают о Niri или Desktop** и станут первыми тестами автономного Kira Engine:

```lua
local ui = require("kira.ui")
local count = ui.state(0)

return ui.box {
  orientation = "vertical",
  ui.text { text = ui.bind(count) },
  ui.button {
    text = "Добавить",
    on_click = function()
      count:set(count:get() + 1)
    end,
  },
}
```

Это реальный синтаксис Lua, но `kira.ui` пока не зарегистрирован: примеры **ещё не работают**.

## Статус репозитория

| Компонент | Сейчас | Целевой V2 |
| --- | --- | --- |
| `crates/kira-engine` | **Отсутствует** | Общий Rust/mlua/GTK4 runtime |
| `crates/kira-desktop` | **Отсутствует** | Desktop binary на Engine |
| `ui/desktop` | **Отсутствует** | Lua-виджеты оболочки |
| `crates/kira-core`, `kira-ipc` | Rust legacy используется | Desktop-side backend |
| `crates/kira-shell`, `frontend/` | GTK/WebKit + Svelte V1 | Сохранить для миграции, затем убрать |

Пока `task check`, `nix build` и существующий CI проверяют V1 и документацию, а **не работающий Engine**. Все новые crates будут добавляться только вместе с кодом, корректным Cargo.lock и реальной проверкой GTK.

Работа ведётся в `kira-desktop-v2`; `main` не меняется.
