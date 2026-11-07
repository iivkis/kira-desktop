-- Kira Desktop V2: ordinary Lua 5.4 for future mlua GTK host.
-- API contract example. Requires Rust-provided "kira.ui" module, not wired up yet.
local ui = require("kira.ui")
local theme = require("styles")
local count = ui.state(0)

return ui.box {
  id = "counter",
  orientation = "vertical",
  spacing = 12,
  style = theme.panel,

  ui.text {
    id = "counter-value",
    text = ui.bind(count),
    style = theme.counter_value,
  },

  ui.button {
    id = "increment",
    text = "Добавить",
    style = ui.compose(theme.control, theme.primary, { padding = 16 }),

    on_click = function()
      count:set(count:get() + 1)
    end,
  },
}
