-- Kira Desktop V2 / ergonomic Lua UI DSL (NOT YET COMPILABLE).
-- Rust translates ui.* static AST forms to GTK, with no Lua VM.
return ui.box {
  id = "counter",
  theme = "styles.lua",
  direction = "column", gap = 12,
  state = { count = 0 },
  style = "panel",

  ui.text {
    id = "value",
    text = ui.bind("count"),
    style = "counter_value",
  },

  ui.button {
    id = "increment",
    text = "Добавить",
    style = { "control", "primary", padding = 16 },
    on_click = function(state)
      state.count = state.count + 1
    end,
  },
}
