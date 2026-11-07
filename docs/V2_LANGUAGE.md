# Kira Lua UI Language v0.2

**Статус:** будущий контракт M1, **компилятор пока не реализован**. [Стили только на Lua](V2_STYLES.md) · [архитектура](V2_ARCHITECTURE.md).

Lua — язык исходников виджетов, state, событий и оформления. Rust читает **статическое подмножество Lua 5.4** как AST, проверяет типы и генерирует native gtk4-rs + GTK CSS. \`ui.box\`, \`ui.text\`, \`ui.button\`, \`ui.bind\`, \`ui.theme\` являются **зарезервированными AST-формами**, не исполняемыми функциями Lua. Ни во время сборки, ни в готовом Shell нет Lua VM.

## Канонический UI-файл M1

\`\`\`lua
return ui.box {
  id = "counter", theme = "styles.lua",
  direction = "column", gap = 12,
  state = { count = 0 },
  style = "panel",

  ui.text { id = "count", text = ui.bind("count") },
  ui.button {
    id = "add", text = "Добавить",
    style = { "control", "primary", padding = 16 },
    on_click = function(state)
      state.count = state.count + 1
    end,
  },
}
\`\`\`

- Весь файл: комментарии и **ровно один** \`return ui.box { ... }\`; никакого динамического вычисления дерева. Дочерние \`ui.*\` виджеты задаются **позиционными** элементами Lua-таблицы в исходном порядке, без \`children = {...}\`.
- \`ui.box\` → \`gtk::Box\`, \`direction = "row" | "column"\`, \`gap\` целое 0–128; \`ui.text\` → \`gtk::Label\`, \`ui.button\` → \`gtk::Button\`. Каждый виджет требует уникальный строковый \`id\`.
- \`text\` принимает буквальную строку или \`ui.bind("field")\`; \`on_click\` допустим только для button. Любой неизвестный виджет, prop, token, вызов или тип даёт ошибку, не fallback в Lua runtime.
- \`state = { count = 0 }\` определяет локальные типизированные поля на widget. Потомки видят state предка, затенение одноимённых state-полей запрещено. Тип выводится из literals: \`i64/f64/bool/String\`. \`nil\`, динамические таблицы и Option вне M1.
- \`ui.bind("count")\` создаёт native dependency и обновление GTK property. Для \`text\` разрешены \`String\` и десятичное форматирование \`i64\`. Обновления UI выполняются в GTK main context.
- \`on_click = function(state) ... end\` статически транслируется в GTK \`clicked\` callback. Поддержаны обращения к известным полям, ограниченная \`i64\` арифметика/присваивания и \`if\` с bool. Для целых используется wrapping в debug/release; нет циклов, произвольных функций, upvalues и прямых системных вызовов.
- \`theme = "styles.lua"\` — буквальный путь в корне UI на второй Lua AST-файл \`return ui.theme { ... }\`. Правила \`style = "panel"\` или \`style = { "control", "primary", padding = 16 }\` описаны в [V2_STYLES.md](V2_STYLES.md). Токены и GTK states компилируются напрямую в GTK CSS **без Grass/Sass/SCSS/PostCSS/Tailwind**.

## Безопасность и сборка

- Parse without executing Lua → AST with spans → schema/name/type validation → typed UI/style IR → Rust codegen + GTK CSS in Cargo \`OUT_DIR\`.
- Никакого \`require\`, \`dofile\`, \`load\`, \`os/io/debug\`, \`local\`, metatables, globals, FFI, динамического table indexing и внешних CSS processors.
- Файл темы ограничен input root с проверкой symlinks, сборка офлайн, генерация детерминирована при фиксированных исходниках и версии компилятора, ошибки \`path:line:column\`.
- Сгенерированный Rust выполняется с правами Shell: проверка Lua не равна runtime sandbox. Все системные операции остаются за безопасным Rust API.
- GTK CSS проверяется собственным \`GtkCssProvider\` в native тестах; все UI state updates выполняются в GTK main context.
- Пример [counter.lua](../examples/v2/counter.lua) вместе с [styles.lua](../examples/v2/styles.lua) — M1 fixture. Native acceptance включает binding, click, tokens, composition, hover/disabled и отсутствие Lua VM.

**Примеры пока не компилируются, это спецификация для реализации Rust-транслятора.**