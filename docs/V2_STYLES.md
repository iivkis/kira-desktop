# Kira Lua Style API — runtime mlua + GTK CSS

**Статус:** контракт будущего Rust GTK-хоста; **реализация пока отсутствует**.
[Engine/Desktop boundary](ENGINE_DESKTOP.md) · [UI API](V2_LANGUAGE.md) · [пример Lua-темы](../examples/v2/styles.lua) · [пример виджета](../examples/v2/counter.lua).

**Владелец стилей: Kira Engine.** Все GTK CSS style bridge, кеширование классов, темы и `ui.compose` являются универсальными для обычных GTK-приложений. Desktop предоставляет только собственные Lua theme files; Engine не зависит от дизайна, Niri или компоновки Kira Desktop.

## Концепция

Весь стиль пишется **на Lua**. `ui.theme { ... }` — обычная Lua-функция Rust-хоста, которая возвращает объект Theme с переиспользуемыми стилями `theme.panel`, `theme.primary`. `ui.compose(...)` объединяет правила в указанном порядке. Дополнительные SCSS/Grass/PostCSS/Tailwind не нужны.

Rust UI bridge **во время работы программы** проверяет значения и превращает правила в GTK CSS для `gtk::CssProvider`; не генерирует Rust-код и не требует сборки UI при каждом изменении темы. Безопасная перезагрузка темы требует пересоздания/обновления её runtime-объектов.

## Lua-пример

```lua
local ui = require("kira.ui")

return ui.theme {
  tokens = {
    surface = "#14213d",
    accent = "#fca311",
    text = "#ffffff",
    ink = "#000000",
    muted = "#e5e5e5",
    spacing = 12,
  },
  styles = {
    panel = { background = "$surface", color = "$text", padding = 16 },
    control = { color = "$ink", padding = "$spacing" },
    primary = {
      background = "$accent",
      radius = 6,
      hover = { background = "$muted" },
      disabled = { background = "$muted" },
    },
  },
}
```

Использование:

```lua
local theme = require("styles")
ui.button {
  text = "Сохранить",
  style = ui.compose(theme.control, theme.primary, { padding = 16 }),
}
```

## Правила M1

- `ui.theme` accepts `tokens` (named color strings and nonnegative pixel numbers) and `styles` (named tables). `"$accent"` resolves against tokens at runtime when theme is built, with type checking.
- `ui.compose(a,b,{padding=16})` merges **left-to-right**, last property wins, with state tables merged separately. Lua can use ordinary variables and factory functions to reuse styles; no custom AST restrictions.
- Supported style keys in M1: `background` → `background-color`, `color` → `color`, `padding` → `padding: Npx`, `radius` → `border-radius: Npx`; CSS values validated against bounds/colors. Unknown GTK property is a runtime Lua error.
- Supported GTK states: `hover`, `active`, `disabled`, `focus_visible` → `:hover`, `:active`, `:disabled`, `:focus-visible`. Neither selectors nor raw CSS strings are accepted as arbitrary injection.
- Only the GTK main thread may update CSS providers or widget classes. Style hashing/caching should deduplicate identical normalized compositions; provider setup occurs once per display, with controlled updates on reload.
- Use `GtkCssProvider` parsing-error handling in tests. GTK CSS is not web CSS; `direction` and `spacing` are GtkBox widget properties.
- On M1 external themes are trusted local Lua scripts. Lua VM isolation alone does not make downloaded themes safe.

## Не делаем

No Lua-to-Rust transpiler, `generated_ui.rs`, arbitrary native GTK APIs exposed to Lua, or external style processors. Existing Tailwind/WebKit files are legacy V1 components pending migration.
