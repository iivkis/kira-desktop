-- Kira Engine: proposed bidirectional async mlua API (NOT YET RUNNABLE).
-- app.analysis is a Rust module registered inside the same process.
local ui = require("kira.ui")
local tasks = require("kira.tasks")
local events = require("kira.events")
local analysis = require("app.analysis")
local status = ui.state("Готово")
local progress = ui.state(0)

return ui.box {
  id = "async-bridge",
  orientation = "vertical",
  spacing = 12,

  on_mount = function(scope)
    -- Rust -> Lua event.
    scope:track(events.on("analysis.progress", function(update)
      progress:set(update.percent)
    end))

    -- Rust -> Lua async call with a result.
    scope:track(events.handle("analysis.describe", function(request)
      return { title = "Элементов: " .. tostring(request.count) }
    end))
  end,

  ui.text { id = "status", text = ui.bind(status) },
  ui.text { id = "progress", text = ui.bind(progress) },
  ui.button {
    id = "start",
    text = "Посчитать в Rust",
    on_click = function(ctx)
      -- Lua -> Rust async call and return value, via coroutine.
      ctx.scope:track(tasks.spawn(function()
        local result = analysis.compute({ count = 200000 })
        status:set(result.summary)
      end))
    end,
  },
}
