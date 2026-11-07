-- Kira Desktop V2 / Lua style DSL (NOT YET COMPILABLE).
-- Rust parser reads static AST; no Lua execution.
return ui.theme {
  tokens = {
    surface = "#14213d", text = "#ffffff",
    ink = "#000000", accent = "#fca311",
    muted = "#e5e5e5", md = 12, lg = 16,
  },
  styles = {
    panel = { background = "$surface", color = "$text", padding = "$lg" },
    counter_value = { color = "$text" },
    control = { color = "$ink", padding = "$md" },
    primary = {
      background = "$accent", radius = 6,
      hover = { background = "$muted" },
      disabled = { background = "$muted" },
    },
  },
}
