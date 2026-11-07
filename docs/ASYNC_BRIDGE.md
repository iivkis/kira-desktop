# Kira Engine: двусторонний асинхронный Rust ↔ Lua мост

**Статус:** нормативный контракт V2; новый runtime **ещё не реализован**.  
**ADR:** [0005](adr/0005-direct-async-mlua.md) · [Разделение Engine/Desktop](ENGINE_DESKTOP.md) · [Lua API](V2_LANGUAGE.md).

## Инвариант

**Kira Engine не использует внутренний IPC между Rust и Lua.** Lua VM живёт **в том же процессе**, что и Rust GTK4 host. Обмен осуществляется непосредственно через `mlua` и Rust callbacks, функции, userdata и in-memory typed events. Не строить JSON-RPC, Unix sockets, отдельный Core daemon или сериализующий транспорт ради внутренних вызовов.

Внешние системные границы остаются: **Niri IPC** для отдельного композитора и **D-Bus** для отдельных системных процессов. Это обязанности Rust-кода **Kira Desktop**, а не Lua↔Rust транспорта Engine. Целевые Core/system services исполняются **внутри Rust-процесса Kira Desktop**. Старые `crates/kira-ipc` и `kira-core` daemon остаются лишь миграционным V1-кодом, пока от них зависит работающая сборка.

## Lua → Rust: async

Rust host регистрирует модули (`app.analysis`, `kira.desktop.metrics`) с обычными и асинхронными методами через `mlua`. Для async использовать `Lua::create_async_function`, feature `async`, с выполнением Lua внутри **корутины**. В Lua нет ключевого слова `await`: Kira Engine предоставляет `require("kira.tasks")` с `tasks.spawn(function)`, запускающим Lua функцию через `Function::call_async` на GLib main context. При ожидании async Rust функции Lua coroutine отдаёт управление main loop.

```lua
local ui = require("kira.ui")
local tasks = require("kira.tasks")
local analysis = require("app.analysis") -- Rust-host registered

local state = ui.state("Готово")
return ui.box {
  ui.text { text = ui.bind(state) },
  ui.button {
    text = "Анализ",
    on_click = function(ctx)
      ctx.scope:track(tasks.spawn(function()
        local data = analysis.compute({ count = 200000 }) -- async Rust function
        state:set(data.summary)
      end))
    end,
  },
}
```

`tasks.spawn` возвращает `TaskHandle:cancel()`; ошибки доставляются в обработчик Engine, а не вылетают из GTK signal. `ctx.scope` привязан к живому view и отменяет работу при размонтировании. **CPU-heavy работу нельзя выполнять прямо в Lua coroutine или синхронно в `create_async_function`**: она должна реально выполняться в CPU pool. IO выполняется в host async executor или worker.

## Rust → Lua: events и async response

Engine регистрирует Lua callbacks в `mlua` registry и предоставляет `require("kira.events")`:

```lua
local events = require("kira.events")
scope:track(events.on("analysis.progress", function(e)
  progress:set(e.percent)
end))
scope:track(events.handle("analysis.describe", function(request)
  return { title = "Кол-во: " .. tostring(request.count) }
end))
```

Rust API (проектируемые имена):

```rust
engine.emit("analysis.progress", update)?;          // Rust → Lua event
let response = engine.call_lua_async("analysis.describe", request).await?;
// Rust → Lua request with result; no IPC, runs on GTK main context
```

- `events.on` подписывает Lua callback и возвращает `Subscription:unsubscribe()`.
- `events.handle` регистрирует именованную Lua-функцию и возвращает disposable handle.
- `emit` доставляет событие из Rust на GTK main thread, `call_lua_async` исполняет Lua callback через `Function::call_async` и возвращает async Rust result.
- Lua также может вызвать разрешённую Rust-функцию для отправки локального события. Имена событий и capabilities проверяются host.

**Двусторонность означает и вызов, и результат в обе стороны**, а не только Rust push в Lua.

## Потоки, lifetime и ошибки

```text
Lua GTK (main) -- mlua async function --> Rust service / CPU pool
Lua GTK (main) <-- callback / result --- bounded in-process delivery
Rust worker -- typed in-memory queue --> Engine GTK main thread --> Lua handler
```

- GTK widgets, Lua VM, callbacks/registry keys остаются на **GTK main thread**. На старте mlua без feature `send`. GLib `MainContext::spawn_local` опрашивает non-`Send` Lua futures.
- В workers находятся **owned Rust values**, не `mlua::Value`, `mlua::Function` или GTK userdata. In-memory queue допускается для межпоточной доставки, но это **не IPC**.
- `scope:track(task/subscription)` автоматически отменяет/отписывает на unmount/reload, удаляет registry refs. Результаты после unmount **не** вызывают Lua.
- Нужны bounded queues, throttling/coalescing телеметрии, cancellation, timeouts, типизированные errors, исключение reentrancy/deadlock и panic boundary. Асинхронность сама по себе не исправляет блокирующий Lua-loop, нужны budgets.
- Host может использовать Tokio/CPU pool, но универсальный Engine не требует Tokio, Niri, Kira Desktop, `kira-core`, `kira-ipc` или `gtk4-layer-shell`.
- Engine предоставляет `kira.ui`, `kira.tasks`, `kira.events` и registry extension modules. Desktop регистрирует `kira.desktop.*` с минимально необходимыми системными permissions. **`mlua` не является sandbox**, M1 только для доверенных локальных Lua скриптов.

## Первый проверяемый результат

1. Lua async `analysis.compute` работает без остановки GTK и обновляет bound label.
2. Rust event `analysis.progress` вызывает Lua callback; Rust async request `analysis.describe` получает Lua-result.
3. Повтор mount/unmount удаляет registry subscriptions и отменяет tasks; late results игнорируются.
4. Generic Engine работает **без Desktop/Niri/internal IPC**. Desktop использует Engine и напрямую встраивает Rust Core services. Внешний Niri IPC при этом функционирует.
5. Cargo target V2 не требует `kira-ipc` и отдельного собственного Core daemon. Legacy V1 остаётся до parity, не удалять сейчас ценой сломанной сборки.

**API и имена выше предложены и зафиксированы, реализация ещё предстоит.**
