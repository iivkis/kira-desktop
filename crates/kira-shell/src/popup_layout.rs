//! A single transient popup has a stable dock. Detached widgets are regular
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

/// Every transient widget uses the same right anchor. A Niri xdg-toplevel
/// is positioned by the compositor, not by the transient popup algorithm.
pub fn choose_dock(_kind: Surface, _right_occupied: bool) -> PopupDock {
    PopupDock::Right
}

/// No active-window columns: normal Niri windows manage their own layout.
pub fn popup_width(output_width: i32, _active_count: usize, _launcher_active: bool) -> i32 {
    PREFERRED_POPUP_WIDTH.min(output_width.saturating_sub(OUTER_MARGIN * 2).max(1))
}

pub fn settings_popup_width(output_width: i32) -> i32 {
    SETTINGS_POPUP_WIDTH.min(output_width.saturating_sub(OUTER_MARGIN * 2).max(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transient_panel_has_one_consistent_anchor() {
        for occupied in [false, true] {
            assert_eq!(choose_dock(Surface::Calendar, occupied), PopupDock::Right);
            assert_eq!(choose_dock(Surface::System, occupied), PopupDock::Right);
            assert_eq!(choose_dock(Surface::Launcher, occupied), PopupDock::Right);
            assert_eq!(choose_dock(Surface::Settings, occupied), PopupDock::Right);
        }
    }

    #[test]
    fn detached_windows_do_not_shrink_popup_width() {
        assert_eq!(popup_width(1264, 1, false), 420);
        assert_eq!(popup_width(1264, 3, true), 420);
        assert_eq!(popup_width(800, 4, true), 420);
        assert_eq!(popup_width(400, 3, true), 376);
        assert_eq!(settings_popup_width(1264), 600);
        assert_eq!(settings_popup_width(800), 600);
        assert_eq!(settings_popup_width(480), 456);
    }
}
