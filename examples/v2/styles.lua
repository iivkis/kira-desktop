-- Kira Desktop V2: Lua runtime theme for future Rust mlua host.
-- This is real Lua syntax; requires host-provided "kira.ui" module (not yet wired).
local ui = require("kira.ui")

return ui.theme {
  tokens = {
    surface = "#14213d",
    text = "#ffffff",
    ink = "#000000",
    accent = "#fca311",
    muted = "#e5e5e5",
    spacing = 12,
  },
  styles = {
    panel = {
      background = "$surface",
      color = "$text",
      padding = 16,
    },
    counter_value = {
      color = "$text",
    },
    control = {
      color = "$ink",
      padding = "$spacing",
    },
    primary = {
      background = "$accent",
      radius = 6,
      hover = { background = "$muted" },
      disabled = { background = "$muted" },
    },
  },
}
