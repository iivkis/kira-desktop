# Kira Engine и Kira Desktop — граница продукта и платформы

**Статус:** нормативная архитектура будущей V2, **не готовая реализация**. 
**Решение:** [ADR-0003](adr/0003-kira-engine-desktop-split.md); [mlua runtime ADR-0002](adr/0002-embedded-lua-mlua.md).
**Связанные документы:** [V2 architecture](V2_ARCHITECTURE.md), [Lua API](V2_LANGUAGE.md), [Styles](V2_STYLES.md), [Migration](V2_MIGRATION.md).

## Два самостоятельных слоя

**Kira Engine** — **универсальный фреймворк/движок для нативных GTK4-приложений на Lua**. Это самостоятельная Rust-библиотека/рантайм, не привязанная к Niri, Linux desktop shell, workspaces, трей-бару или конкретной оболочке. Встраивает Lua 5.4 через `mlua`, предоставляет `kira.ui`, создаёт native GTK widgets, управляет состоянием, подписками, обработчиками, жизненным циклом, стилями через GTK CSS и диагностикой Lua-ошибок.

**Kira Desktop** — **конкретное рабочее окружение, построенное на Kira Engine**. Предоставляет NixOS/Niri/Wayland layer-shell, top bar, launcher, tray, notifications, calendar, settings, OS-интеграцию, конфигурацию и desktop-specific capabilities. Его Lua-виджеты используют стандартный Engine UI API и разрешённый расширенный API `kira.desktop`.

Связь **односторонняя**: `Kira Desktop → Kira Engine`. Engine **не** импортирует `kira-desktop`, `kira-core`, `kira-ipc`, Niri и `gtk4-layer-shell` в своём базовом runtime. Desktop инжектирует разрешённые системные capabilities в Engine. Один и тот же Engine может работать в обычном GTK-приложении, не являющемся оболочкой рабочего стола.

## Слои

```text
         GENERIC GTK LUA APP                 KIRA DESKTOP (Niri/Wayland)
         Lua app widgets                     Lua shell widgets
                 |                                  |
                 v                                  v
         +--------------------------------------------------+
         |                  KIRA ENGINE                     |
         |  Rust library + mlua Lua 5.4 VM + kira.ui API    |
         |  WidgetSpec -> GTK4 renderer / lifecycle         |
         |  state/bindings, signals, GTK CSS, diagnostics   |
         +--------------------------------------------------+
                 |                                  |
                 v                                  v
           GTK Application                 Desktop Rust host
                                           gtk4-layer-shell, Core/IPC
                                           Niri, D-Bus, Linux, settings
```

Это **Tauri-подобное разделение** в смысле reusable app runtime + app, но **не форк Tauri и не WebView**. Весь GUI отображается нативным GTK4.

## Границы Rust crates и приложений (планируемая структура)

```text
crates/
  kira-engine/              # NEW Rust library: mlua + GTK4 + UI runtime
  kira-desktop/             # NEW Rust application: shell built on kira-engine
  kira-core/                # existing: desktop-specific system services
  kira-ipc/                 # existing: desktop-specific typed IPC
  kira-shell/               # existing V1 migration code; not the Engine
ui/
  desktop/                  # NEW future Lua widgets and theme for Kira Desktop
examples/
  v2/
    counter.lua             # generic Engine widget API fixture
    styles.lua              # generic Engine style fixture
```

**Важно:** директории `crates/kira-engine/`, `crates/kira-desktop/` и `ui/desktop/` **сейчас не созданы**, это архитектурный target layout. Существующий `kira-shell` пока остаётся рабочим наследием V1, а не готовым новым Engine. Создавать пустые crates и редактировать Cargo.lock ради документации нельзя.

При реализации новый бинарник `kira-desktop` зависит от `kira-engine`; `kira-core`/`kira-ipc` остаются desktop-side. Проверить отсутствие обратной зависимости в Cargo graph и модульном API.

## Публичные API

**Engine API: `require("kira.ui")`**

- `ui.box`, `ui.text`, `ui.button` и следующие GTK-виджеты
- `ui.state`, `ui.bind`, widget lifecycle, Lua closures, error reporting
- `ui.theme`, `ui.compose`, безопасный runtime GTK CSS
- приложение и контейнеры без предположений о compositor/workspace

**Desktop API: `require("kira.desktop")` (пока проектируемый)**

- Desktop регистрирует отдельный, проверяемый capabilities module через Engine host registration.
- Получение состояния рабочих столов/мониторов, вызовы Niri и системные действия проходят **через Rust Desktop/Core** с валидацией, разграничением полномочий и типизированными сообщениями.
- Модуль `kira.desktop` **недоступен** обычному GTK Engine-приложению по умолчанию.
- Не предоставлять raw D-Bus, filesystem, spawn или произвольные GTK/OS полномочия в Lua. Модуль `kira.desktop` **не реализован**.

Например, [counter.lua](../examples/v2/counter.lua) использует **только `kira.ui`** и не зависит от Niri, поэтому обязан работать в обычном Engine-host приложении, когда движок появится.

## Ответственность при внедрении

| Вопрос | Kira Engine | Kira Desktop |
| --- | --- | --- |
| Lua VM (`mlua`), module loader | Да | Создаёт/настраивает Engine instance |
| GTK4 widget tree и GTK CSS | Да | Использует API Engine |
| Reactive state, callbacks, teardown | Да | Использует API Engine |
| Generic GTK application hosting | Да | Может работать поверх Engine |
| Wayland `gtk4-layer-shell` и панели | Нет, отдельная интеграция | Да |
| Niri workspaces / focus / IPC | Нет | Да |
| D-Bus, notifications, display safety | Только через injected capabilities | Да, desktop-side |
| Lua desktop widgets | Нет | Да |
| NixOS desktop packaging | Независимый engine library | Да |

GTK и Lua UI callbacks в M1 выполняются в **GTK main thread**; background system workers сообщают об изменениях через typed messages. Lua VM не является security sandbox. Для M1 принимаются лишь доверенные локальные Lua приложения/виджеты.

## Критерии независимости

1. Generic Engine example (`counter.lua`) работает как GTK4 окно **без Niri, kira-core или kira-desktop** и с реальным Lua click callback, state/binding и стилями.
2. Kira Desktop может использовать тот же Engine runtime в layer-shell и получать workspace state через `kira.desktop` provider, не внедряя desktop-specific код в Engine.
3. Cargo dependency graph подтверждает только `kira-desktop -> kira-engine`; `kira-engine` не зависит от `kira-desktop` и Niri-интеграций.
4. Отдельные тесты Engine API и интеграционные тесты Desktop в nested Niri. Legacy V1 CI **не** доказывает ни один из них.

Такое разделение фиксируется **сейчас**, а фактическая реализация, упаковка и проверка независимости выполняются в следующих этапах.
