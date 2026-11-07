# Kira Lua Runtime UI API — M1 contract

**Статус:** проект публичного API будущего `mlua` GTK-хоста; **реализации ещё нет**.
**Контекст:** [Engine/Desktop separation](ENGINE_DESKTOP.md) · [V2 architecture](V2_ARCHITECTURE.md) · [Lua styling](V2_STYLES.md) · [counter](../examples/v2/counter.lua).

**Владелец API: Kira Engine.** `require("kira.ui")`, GTK widgets, `ui.state`, `ui.bind`, `ui.theme` и `ui.compose` должны работать в **любом** GTK-приложении на Engine, без Niri, Core и Desktop. Отдельный optional `require("kira.desktop")` будет регистрировать **только Kira Desktop host** с разрешёнными OS capabilities; это не часть Engine и пока не реализовано.

## Главный принцип

Это **настоящий Lua 5.4**, исполняемый через `mlua` внутри Rust-приложения. Разрешены `local`, функции, замыкания, арифметика, условия, циклы и разрешённые модули. Не нужен отдельный транслятор, специальный парсер DSL или кодогенерация Rust.

`ui.box { ... }` синтаксически является обычным **вызовом Lua-функции**: Rust зарегистрирует эту функцию через `mlua::Lua::create_function`. Она возвращает `WidgetSpec` (объект, который знает Rust renderer). GTK widget физически создаётся в Rust на main thread.

## Модули

```lua
local ui = require("kira.ui")
local theme = require("styles")
local count = ui.state(0)
```

`kira.ui` is the host-provided, versioned API module; `styles` is resolved by a controlled local module loader under a configured widget root. Arbitrary C modules, absolute paths, network dependencies and modules outside approved roots must not be loaded by that resolver. Import code is **executed once according to Lua require caching**, not statically parsed.

## Виджеты M1

```lua
return ui.box {
  id = "counter",
  orientation = "vertical",
  spacing = 12,
  style = theme.panel,

  ui.text { id = "count", text = ui.bind(count) },
  ui.button {
    id = "add",
    text = "Добавить",
    on_click = function()
      count:set(count:get() + 1)
    end,
  },
}
```

- `ui.box{orientation="vertical"|"horizontal", spacing=integer>=0, children...}` → native `gtk::Box`.
- `ui.text{text=string|Binding}` → native `gtk::Label`.
- `ui.button{text=string|Binding, on_click=function, ...}` → native `gtk::Button`.
- `id` is optional at the Lua level, but stable identifiers are recommended for debugging and reload. Children are ordinary positional elements in a Lua table, in order.
- The host validates widget properties and reports Lua errors; it does not expose arbitrary raw GTK objects initially.

## State и реактивность

`ui.state(initial_value)` returns an observable **StateHandle**, implemented as Rust userdata with Lua `:get()` and `:set(value)` methods. M1 supports integer/number/string/boolean values; `:set` rejects incompatible changes. Observers see changes through `ui.bind(state)`. `ui.bind` produces a binding object passed to `text`; Rust observes the state and sets the GTK label on the UI thread.

This uses **explicit set** instead of magic table field assignment: in ordinary Lua `__newindex` on a table does not intercept writes to keys that already exist. Avoid promising auto reactivity from `state.count = ...` on a plain Lua table.

If a handler changes a state, the existing widget updates; no full tree rebuild is needed for a simple bound property. More complex reactive computed values and dynamic collections can be added later.

## События и владение

`on_click = function() ... end` remains a real Lua closure. Rust stores the function in the Lua VM registry or an owned equivalent for as long as the GTK widget and event subscription exist. `connect_clicked` routes to that function on GTK main thread. On teardown, unregister handlers and release registry references and state subscriptions. Avoid reference cycles between Lua callbacks and GTK widgets; test repetitive mount/unmount.

Lua callbacks can throw `error(...)`, so Rust must handle `mlua::Error` without unwinding out of a GTK signal callback. Never block GTK main thread on OS/network IO inside Lua callbacks; system operations go through typed async Rust capabilities.

## Доступ к системе

Niri IPC, D-Bus, process spawning, display configuration and shell commands are not exposed raw to Lua. Later API might expose explicit permission-gated methods such as `kira.system.focus_workspace(id)` with validation in Rust; those are **not implemented as part of M1**.

`mlua` is an embedding library, not a universal security sandbox. M1 scripts are trusted local code. Future third-party widgets need permission design and process isolation.

## Проверки M1

- Execute [counter](../examples/v2/counter.lua), not merely parse it; `require` loads host module and local theme.
- Construct GTK4 widgets; click → Lua closure → observable StateHandle → bound GtkLabel.
- Verify no unrelated GTK calls from non-main threads and no unhandled Lua panic.
- Test invalid props, invalid state type, callback error recovery, missing module, non-existent theme, reload/teardown and memory/CPU budget.
- GTK CSS/hover tested in native GTK runtime; no generated Rust or `kira-lua-compiler`.
