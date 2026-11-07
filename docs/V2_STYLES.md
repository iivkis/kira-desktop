# Lua Style DSL (V2)

**Статус:** спецификация для ещё не реализованного Rust-транслятора. Для разработчика **единственный исходный язык стилей — Lua**. Никаких Grass, Sass/SCSS, Less, PostCSS, Tailwind, Node или внешних CSS-препроцессоров в нативной V2. `kira-lua-compiler` сам генерирует GTK CSS из Lua AST и Rust-код виджетов, а GTK4 читает готовый CSS через `CssProvider`.

Пример: [styles.lua](../examples/v2/styles.lua), использование: [counter.lua](../examples/v2/counter.lua), [контракт UI](V2_LANGUAGE.md).

## Тема

```lua
return ui.theme {
  tokens = {
    surface = "#14213d", text = "#ffffff",
    accent = "#fca311", ink = "#000000",
    muted = "#e5e5e5", md = 12, lg = 16,
  },
  styles = {
    panel = { background = "$surface", color = "$text", padding = "$lg" },
    control = { color = "$ink", padding = "$md" },
    primary = {
      background = "$accent", radius = 6,
      hover = { background = "$muted" },
      disabled = { background = "$muted" },
    },
  },
}
```

`ui.theme { ... }` — распознаваемая Rust-парсером синтаксическая форма, **не исполняемый Lua-вызов**. Файл темы имеет один `return ui.theme { ... }`, без `require`, `local`, динамических выражений и произвольных функций.

## Применение

```lua
return ui.box {
  id = "root", theme = "styles.lua",
  direction = "column", gap = 12, style = "panel",
  ui.button {
    id = "save", text = "Сохранить",
    style = { "control", "primary", padding = 16 },
  },
}
```

Формы `style = "panel"`, `style = { "control", "primary" }`, `style = { "control", "primary", padding = 16 }` поддерживают reuse, композицию **слева направо** и локальный override в конце. Запись каждого свойства в одном литерале допускается ровно один раз. Составные стили разворачиваются статически. Последний названный стиль побеждает при конфликте базовых полей; state-поля сливаются отдельно, также слева направо.

## Компилируемый контракт M1

- `theme = "styles.lua"` допустим только на корне UI и является буквальным относительным путём от entry-файла, внутри объявленного input root. Запрещены абсолютный путь, `..`, выход через симлинки и незадекларированные источники.
- `tokens` содержат только `#RRGGBB` и неотрицательные целые числа 0–128. Ссылка `"$accent"` подставляется **при сборке** с проверкой типа, а не через runtime CSS `var()`.
- `background` → GTK CSS `background-color`, `color` → `color`, `padding` → `padding: Npx`, `radius` → `border-radius: Npx`. Другие свойства дают диагностику до расширения схемы.
- `hover`, `active`, `disabled`, `focus_visible` с такими же ограниченными полями превращаются в GTK `:hover`, `:active`, `:disabled`, `:focus-visible`. Нет произвольных селекторов, вложенных состояний, CSS-блоков или импорта CSS.
- Компилятор объединяет все стили виджета в один проверенный typed style IR, строит **один детерминированный CSS-класс на нормализованную комбинацию**, переиспользует его для одинаковых стилей и генерирует CSS из валидированных значений. Никакой вставки сырого текста Lua в CSS/Rust.
- `direction` и `gap` — свойства `gtk::Box`, не CSS. GTK `CssProvider` подключается на уровне приложения, а `add_css_class` назначает классы нативным виджетам.
- Изменение Lua темы требует пересборки. Динамическая смена темы и произвольный runtime CSS вне M1.

## Пример ожидаемого GTK CSS

```css
.kira-generated {
  color: #000000;
  background-color: #fca311;
  padding: 16px;
  border-radius: 6px;
}
.kira-generated:hover { background-color: #e5e5e5; }
.kira-generated:disabled { background-color: #e5e5e5; }
```

Название класса иллюстративно; на практике оно выводится детерминированно из нормализованного набора свойств. Rust добавляет `button.add_css_class("...")`.

## Ошибки и тесты

Неизвестный токен/стиль, повтор ключа, неверный тип, недопустимый селектор/путь, конфликт формы таблицы — ошибка `path:line:column`. Нужны golden tests для CSS, precedence, reuse, hover/disabled, диагностики и детерминированности. Дополнительно GTK `CssProvider::parsing-error` должен проверяться в native smoke-тестах, поскольку некорректный CSS GTK может принять частично.

**Это не готовый компилятор и не выполняемый Lua.** Существующий V1 `frontend/` с Tailwind остаётся только временным migration baseline.