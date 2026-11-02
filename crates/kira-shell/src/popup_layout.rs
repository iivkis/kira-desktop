//! Pure popup geometry policy, independent from WebKit leases and GTK
//! events. Only *requested/visible* windows consume a position.
use crate::webview_pool::Surface;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PopupDock {
    Left,
    Right,
    Center,
}

pub const PREFERRED_POPUP_WIDTH: i32 = 420;
const OUTER_MARGIN: i32 = 12;
const INNER_GAP: i32 = 8;

/// Existing windows keep their dock while another surface opens; the next
/// floating popup gets a vacant side. Launcher is always centered.
pub fn choose_dock(kind: Surface, right_occupied: bool) -> PopupDock {
    if kind == Surface::Launcher {
        PopupDock::Center
    } else if right_occupied {
        PopupDock::Left
    } else {
        PopupDock::Right
    }
}

/// Keep windows disjoint, including when centered Launcher is present.
/// Side-by-side Calendar/System uses two columns; Launcher requires a
/// reserved center column even when only two popups are visible.
/// On narrow outputs the widgets get narrower rather than overlap.
/// Width is measured in logical monitor pixels, not WebKit CSS pixels.
pub fn popup_width(output_width: i32, active_count: usize, launcher_active: bool) -> i32 {
    let columns = if active_count >= 3 || (active_count >= 2 && launcher_active) {
        3
    } else if active_count >= 2 {
        2
    } else {
        1
    };
    let available = output_width
        .saturating_sub(OUTER_MARGIN * 2)
        .saturating_sub(INNER_GAP * (columns - 1));
    PREFERRED_POPUP_WIDTH.min((available / columns).max(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dormant_calendar_never_reserves_position_for_system() {
        assert_eq!(choose_dock(Surface::System, false), PopupDock::Right);
        assert_eq!(choose_dock(Surface::System, true), PopupDock::Left);
        assert_eq!(choose_dock(Surface::Calendar, false), PopupDock::Right);
        assert_eq!(choose_dock(Surface::Calendar, true), PopupDock::Left);
        assert_eq!(choose_dock(Surface::Launcher, true), PopupDock::Center);
    }

    #[test]
    fn three_popups_fit_side_by_side_on_screenshot_sized_output() {
        assert_eq!(popup_width(1264, 1, false), 420);
        assert_eq!(popup_width(1264, 2, false), 420);
        assert_eq!(popup_width(1264, 2, true), 408);
        assert_eq!(popup_width(1264, 3, true), 408);
        let w = popup_width(1264, 3, true);
        let left_end = 12 + w;
        let center_start = (1264 - w) / 2;
        let center_end = center_start + w;
        let right_start = 1264 - 12 - w;
        assert!(left_end + 8 <= center_start);
        assert!(center_end + 8 <= right_start);
    }

    #[test]
    fn narrow_output_constrains_width_before_overlap() {
        assert_eq!(popup_width(800, 1, false), 420);
        assert_eq!(popup_width(800, 2, false), 384);
        assert_eq!(popup_width(800, 2, true), 253);
        assert_eq!(popup_width(800, 3, true), 253);
        assert!(popup_width(480, 3, true) > 0);
    }

    #[test]
    fn last_open_widget_returns_to_original_width() {
        assert_eq!(popup_width(1264, 3, true), 408);
        assert_eq!(popup_width(1264, 1, false), PREFERRED_POPUP_WIDTH);
    }
}
