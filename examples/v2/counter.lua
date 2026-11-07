-- Kira Desktop V2: proposed Lua UI syntax (NOT YET COMPILABLE).
-- The future Rust translator will statically parse this limited subset;
-- it must NOT run arbitrary Lua or embed a Lua VM in the GTK shell.

return Widget {
  id = "counter",
  kind = "Box",
  orientation = "vertical",
  spacing = 12,

  style = Style {
    background = "#14213d",
    color = "#ffffff",
    padding = 16,
  },

  state = State {
    count = 0,
  },

  children = {
    Widget {
      id = "count",
      kind = "Label",
      text = Bind("count"),
    },
    Widget {
      id = "increment",
      kind = "Button",
      text = "Добавить",
      on_click = function(state)
        state.count = state.count + 1
      end,
    },
  },
}
