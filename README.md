# Kira Desktop V2

**Ветка:** `kira-desktop-v2` · **Статус:** новая архитектура принята; интеграция `mlua` и GTK Lua API ещё не реализованы.

Kira Desktop — модульная оболочка для **NixOS / Niri / Wayland**. **Виджеты, стили и UI-логика пишутся на настоящем Lua 5.4, который Rust исполняет через [mlua](https://docs.rs/mlua/)**. Rust отвечает за GTK4/Wayland, Niri IPC, D-Bus, безопасность и системные сервисы. Никакого самописного транслятора Lua → Rust и генерации Rust-кода из Lua.

```text
Lua widgets + Lua theme + callbacks
                  |
                  v
mlua (embedded Lua VM in Rust GTK Shell)
                  |
                  v
Rust ui bridge -> native gtk4-rs widgets + GtkCssProvider
                  |
                  v
Niri / Wayland + Kira Core / IPC / Linux
```

**Статус реализации:** унаследованный V1 Shell пока использует Rust + GTK/WebKitGTK + Svelte. `mlua` ещё не подключён к Cargo Workspace; Lua UI API и GTK runtime не написаны. Существующие Rust/JS исходники и lockfiles оставлены для миграции и регрессионных тестов. Успешная сборка V1 не доказывает работоспособности V2.

## Документы

- [Архитектура V2](docs/V2_ARCHITECTURE.md) — ответственность Lua, mlua, Rust, GTK и жизненный цикл.
- [Runtime Lua UI API](docs/V2_LANGUAGE.md) — функции, виджеты, события, state, модули.
- [Lua Style API](docs/V2_STYLES.md) — темы и переиспользуемые стили без внешних препроцессоров.
- [ADR-0002](docs/adr/0002-embedded-lua-mlua.md) — действующее архитектурное решение; [ADR-0001](docs/adr/0001-lua-rust-gtk.md) отменён.
- [План миграции](docs/V2_MIGRATION.md) · [Текущий статус/ревью](docs/V2_REVIEW.md).
- [Пример виджета](examples/v2/counter.lua) · [пример темы](examples/v2/styles.lua) · [правила разработки](AGENTS.md).

## Пример запуска в будущем

```lua
local ui = require("kira.ui")
local theme = require("styles")
local count = ui.state(0)

return ui.box {
  orientation = "vertical", spacing = 12,
  style = theme.panel,
  ui.text { text = ui.bind(count) },
  ui.button {
    text = "Добавить",
    style = ui.compose(theme.control, theme.primary),
    on_click = function()
      count:set(count:get() + 1)
    end,
  },
}
```

Это **исполняемый синтаксис Lua**, но он требует будущего модуля `kira.ui`, который зарегистрирует Rust-хост через `mlua`. Обычный `lua examples/v2/counter.lua` не запустится без этого хоста.

## Существующие проверки V1

```sh
nix develop --command task check
nix build
nix develop --command task dev:preview
```

Пока они проверяют **унаследованную реализацию V1** и документацию, не Lua/GTK runtime. [Старый гайд разработки](docs/DEVELOPMENT.md), [старое ТЗ](docs/SPECIFICATION.md) и [preview](docs/PREVIEW.md) сохранены как справочники миграции.

`main` не изменяется; работа над V2 ведётся отдельно.
