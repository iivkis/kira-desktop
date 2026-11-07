-- Kira Engine V2: Rust extension bridge proposal. NOT YET RUNNABLE.
-- The Engine host must register app.analysis, and Desktop must register
-- kira.desktop.metrics. Compute and system operations run in Rust, not Lua.
local ui = require("kira.ui")
local metrics = require("kira.desktop.metrics")
local analysis = require("app.analysis")

local cpu = ui.state("…")
local summary = ui.state("Готово")

return ui.box {
  id = "rust-bridge-example",
  orientation = "vertical",
  spacing = 12,

  on_mount = function(scope)
    scope:track(metrics.cpu:subscribe(function(value)
      cpu:set(string.format("%.0f%%", value))
    end))
  end,

  ui.text { id = "cpu", text = ui.bind(cpu) },
  ui.text { id = "summary", text = ui.bind(summary) },

  ui.button {
    id = "analyze",
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
