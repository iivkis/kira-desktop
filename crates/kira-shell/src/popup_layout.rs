//! Transient popups use a per-widget dock. Detached widgets are regular
//! Niri windows and never occupy popup columns or change popup geometry.
use crate::webview_pool::Surface;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PopupDock {
    Right,
    Center,
}

pub const PREFERRED_POPUP_WIDTH: i32 = 420;
// Settings has more information density but remains a fixed-width popup.
pub const SETTINGS_POPUP_WIDTH: i32 = 600;
const OUTER_MARGIN: i32 = 12;

/// Launcher opens at the top center; other transient widgets keep the right
/// dock. A detached Niri xdg-toplevel is positioned by the compositor.
pub fn choose_dock(kind: Surface, _right_occupied: bool) -> PopupDock {
    match kind {
        Surface::Launcher => PopupDock::Center,
        _ => PopupDock::Right,
    }
}

/// Place the Launcher's top edge 20% down from the logical output's top.
/// Integer logical pixels avoid applying physical monitor scaling twice.
pub fn launcher_top_margin(output_height: i32) -> i32 {
    output_height.max(0) / 5
}

/// No active-window columns: normal Niri windows manage their own layout.
pub fn popup_width(output_width: i32, _active_count: usize, _launcher_active: bool) -> i32 {
    PREFERRED_POPUP_WIDTH.min(output_width.saturating_sub(OUTER_MARGIN * 2).max(1))
}

pub fn tray_popup_width(output_width: i32) -> i32 {
    304.min(output_width.saturating_sub(OUTER_MARGIN * 2).max(1))
}

pub fn settings_popup_width(output_width: i32) -> i32 {
    SETTINGS_POPUP_WIDTH.min(output_width.saturating_sub(OUTER_MARGIN * 2).max(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transient_panel_has_widget_specific_anchor() {
        for occupied in [false, true] {
            assert_eq!(choose_dock(Surface::Calendar, occupied), PopupDock::Right);
            assert_eq!(choose_dock(Surface::System, occupied), PopupDock::Right);
            assert_eq!(choose_dock(Surface::Launcher, occupied), PopupDock::Center);
            assert_eq!(choose_dock(Surface::Settings, occupied), PopupDock::Right);
        }
    }

    #[test]
    fn launcher_top_margin_is_one_fifth_of_logical_output_height() {
        assert_eq!(launcher_top_margin(1080), 216);
        assert_eq!(launcher_top_margin(1440), 288);
        assert_eq!(launcher_top_margin(720), 144);
        assert_eq!(launcher_top_margin(0), 0);
    }

    #[test]
    fn detached_windows_do_not_shrink_popup_width() {
        assert_eq!(popup_width(1264, 1, false), 420);
        assert_eq!(popup_width(1264, 3, true), 420);
        assert_eq!(popup_width(800, 4, true), 420);
        assert_eq!(popup_width(400, 3, true), 376);
        assert_eq!(tray_popup_width(1264), 304);
        assert_eq!(tray_popup_width(300), 276);
        assert_eq!(settings_popup_width(1264), 600);
        assert_eq!(settings_popup_width(800), 600);
        assert_eq!(settings_popup_width(480), 456);
    }
}
