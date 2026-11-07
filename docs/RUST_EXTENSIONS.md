# Kira Engine: Rust Extensions & Lua Bridge

**Статус:** нормативное архитектурное решение V2, **публичный API и runtime пока не реализованы**.
**Решение:** [ADR-0004](adr/0004-rust-extensions-lua-bridge.md); [Engine/Desktop boundary](ENGINE_DESKTOP.md); [Lua UI API](V2_LANGUAGE.md).

## Цель

**Kira Engine не должен заставлять писать сложные вычисления или интеграции с Linux на Lua.** Lua отвечает за создание виджетов, композицию, локальные обработчики и простую UI-логику. Тяжёлые вычисления, многопоточные операции, I/O, интеграции ОС, драйверы и любой чувствительный к производительности или безопасности код пишутся **на Rust**. Rust-хосты предоставляют эти возможности Lua-виджетам как обычные модули с удобными типизированными методами, подписками и результатами.

Engine — универсальный **мост Rust ↔ Lua ↔ GTK4**, а не набор захардкоженных Linux-команд. Движок обязан предоставлять **публичный Rust Extension/Capability API** для любого приложения, не только для Kira Desktop. Kira Desktop является лишь одним Rust-хостом, который регистрирует собственные возможности через этот API. **Связь зависимостей:** Desktop → Engine, никогда наоборот.

## Разделение ответственности

| Слой | Отвечает за |
| --- | --- |
| **Lua widget** | GTK UI декларации, local state, обработчики, отображение результатов и подписки |
| **Kira Engine** | VM mlua, GTK4, registry расширений, Lua module lookup, безопасный вызов Rust, lifecycle, main-thread delivery |
| **Rust extension/provider** | Тяжёлые расчёты, network/file I/O, system APIs, асинхронные задачи, таймеры и потоки данных |
| **Kira Desktop** | Niri/Wayland, D-Bus, Core/IPC, системные права, регистрация `kira.desktop.*` providers |

**Engine не импортирует реализации расширений из Desktop.** Вместо этого Engine определяет абстракции регистрации, вызовов, событий, lifetime и прав; host подключает конкретные Rust-модули. Generic host может зарегистрировать `app.analysis`, а Desktop — `kira.desktop.metrics` и `kira.desktop.workspaces`. Модуль отсутствует, пока host его не зарегистрировал.

## Модель Rust Extensibility API (проектируемая)

```rust
// Псевдокод будущего публичного API, не компилируется сегодня.
let engine = KiraEngine::builder()
    .register_module("app.analysis", analysis_provider)
    .register_module("kira.desktop.metrics", desktop_metrics_provider)
    .build()?;
```

При реализации выбрать точные Rust traits/types и слой `mlua` регистрации. Стабильный публичный контракт должен поддерживать:

1. **Версионированный модуль** (`name`, `version`, capabilities), методы с проверкой аргументов и ясными Lua-ошибками. Можно вызывать лёгкие синхронные методы **только если они не блокируют GTK main thread**.
2. **Async command API:** метод запускает Rust worker/task, **быстро возвращает** отменяемый request handle; результат (`value`/`error`) доставляется обратно в Lua callback на GTK main thread. Никаких `blocking_recv`, ожидания network/process/IPC или тяжёлого CPU внутри Lua callback.
3. **Observable/stream API:** Rust источник публикует события/снимки состояния; Lua получает `subscribe(callback)` → объект подписки с `unsubscribe()`. Отправка на UI-поток через bounded, typed message channel; при быстрых метриках используется explicit coalescing (`latest wins`), без бесконечной очереди.
4. **Lifetime scope:** каждый смонтированный view имеет scope; `scope:track(handle)` привязывает подписку, pending request или другой disposable handle к view. При размонтировании/reload scope отписывается/отменяет все tracked handles, закрывает RegistryKey Lua callback. Поздний worker result **не вызывает callback** после dispose.
5. **Расширяемые типы:** boundary поддерживает простые типизированные и сериализуемые values (nil/bool/i64/f64/String, arrays/maps with bounded depth/size) и **opaque Rust userdata handles** для объектов с методами. Не передавать сырые GTK объекты, `mlua::Lua`, функции Lua или `mlua::Value` в background workers.
6. **Typed errors:** `permission_denied`, `invalid_argument`, `unavailable`, `cancelled`, `timeout`, `internal`. Не превращать внутренние Rust panics или stack traces в raw Lua payload; логирование через host.
7. **Capabilities:** host регистрирует grants на модуль/методы, явно разделяет read vs mutate, prompts/authorization where appropriate. Не выдавать произвольные `io`, `os.execute`, FFI, D-Bus, Niri, spawn или filesystem всем Lua-модулям по умолчанию.

## Контракт Lua UI (предлагаемый дополнительный API)

`ui.box`, `ui.button`, `ui.state`, `ui.bind` уже описаны в [Lua UI](V2_LANGUAGE.md). Для связи с Rust расширяем API:

- `on_mount = function(scope) ... end`: вызов после mount GTK view. `scope:track(disposable)` отслеживает подписки и async requests; scope отписывается на unmount.
- `on_click = function(ctx) ... end`: опциональный `ctx.scope` принадлежит жизни виджета; прежний `on_click = function() ... end` остаётся корректным Lua (лишний параметр просто игнорируется). Использовать `ctx.scope:track(...)` для запросов, запускаемых из событий.
- `service:subscribe(callback)` возвращает `Subscription` с `unsubscribe()`. `service:run_async(input, callback)` возвращает `RequestHandle` с `cancel()`. **Имена конкретных методов определяет extension**, а не Engine; Engine гарантирует только contract асинхронного callback, cancellation и safety.

Это эскиз публичных names/semantics; реализация и версии API ещё впереди.

## Пример Lua widget с Rust-подпиской и фоновой задачей

[examples/v2/rust_bridge.lua](../examples/v2/rust_bridge.lua) — иллюстрация подключения **двух независимых Rust providers**. Значения, подписки и callback живут в Lua, тяжёлая работа и системные вызовы выполняются в Rust.

```lua
local ui = require("kira.ui")
local metrics = require("kira.desktop.metrics") -- host-specific Rust provider
local analysis = require("app.analysis")         -- generic Rust extension
local cpu = ui.state("...")
local summary = ui.state("Готово")

return ui.box {
  on_mount = function(scope)
    scope:track(metrics.cpu:subscribe(function(value)
      cpu:set(string.format("%.0f%%", value))
    end))
  end,
  ui.text { text = ui.bind(cpu) },
  ui.text { text = ui.bind(summary) },
  ui.button {
    text = "Проанализировать",
    on_click = function(ctx)
      ctx.scope:track(analysis:run_async({ sample_count = 10000 },
        function(result, err)
          if err then
            summary:set("Ошибка: " .. err.code)
          else
            summary:set(result.summary)
          end
        end))
    end,
  },
}
```

**Это предполагаемый контракт, ещё не работающий пример.** `kira.desktop.metrics` доступен только при регистрации Desktop. В обычном Engine-приложении тот же шаблон работает с другой зарегистрированной Rust extension.

## Потоки данных и безопасность

```text
Lua GTK widget (main thread)
     |  command / subscribe
     v
Engine Rust capability bridge (main thread)
     |  validated typed request
     v
Rust async worker / CPU pool / Niri/Core/IPC (no Lua, no GTK objects)
     |  bounded typed message (state/result/error)
     v
Engine main-thread dispatcher (scope alive? permission still valid?)
     |  invoke Lua callback / update StateHandle
     v
GTK4 widget / CSS (main thread)
```

Engine должен быть **agnostic** к конкретному async runtime: приложение может использовать Tokio, отдельный thread pool или другой executor на стороне host. Движок не требует Tokio от каждого простого GTK-приложения и **не** создаёт скрытый blocking runtime на main thread.

Для CPU-bound задач использовать выделенный пул, для I/O — async executor host, для OS интеграций — Rust provider. Cancellation best-effort на стороне worker, но **никогда не доставлять callback после unmount**. Явные policies на скорость событий, размер буферов, backpressure, тайм-ауты и лимиты памяти. Перегрузка одной медленной Lua function не должна раздувать очередь.

## Acceptance tests

- Generic GTK Engine sample `app.analysis` регистрируется из standalone Rust host и Lua widget вызывает async calculation без Desktop/Niri.
- Desktop регистрирует `kira.desktop.metrics` и той же Lua UI схеме предоставляет системное состояние, Engine при этом не зависит от Desktop.
- Исполнение CPU-bound задачи не блокирует UI; GTK main thread responsive while work runs. Callback доставляется в main context.
- При unmount уничтожаются subscriptions, отменяются requests, **late result не вызывает Lua**. Проверять многократные mount/unmount и reload.
- Unregistered/unauthorized module or method produce typed errors; denied system mutation does not perform side effect.
- Bounded queues and latest-wins telemetry behave under burst load; capability revocation and errors handled without panic.
- Negative dependency test: `kira-engine` не импортирует `kira-desktop` / Niri / `kira-core` / `gtk4-layer-shell`.

**Никаких обещаний работающей реализации до проверки native GTK integration.**
