//! Interruptible GTK scroll to the nearest visible row, ported from main's
//! tenetScrollIntoView. User wheel/touch changes cancel the decorative track.
use std::cell::Cell;
use std::rc::Rc;

use gtk::prelude::*;

use crate::motion::MotionToken;

pub const SCROLL_DURATION_MS: u64 = 340;

pub fn nearest_visible_offset(
    current: f64,
    viewport: f64,
    start: f64,
    end: f64,
    lower: f64,
    upper: f64,
) -> f64 {
    if ![current, viewport, start, end, lower, upper]
        .iter()
        .all(|value| value.is_finite())
        || viewport <= 0.0
    {
        return current;
    }
    let destination = if start < current {
        start
    } else if end > current + viewport {
        end - viewport
    } else {
        current
    };
    destination.clamp(lower, (upper - viewport).max(lower))
}

pub struct TenetScroll {
    window: gtk::ScrolledWindow,
    motion: MotionToken,
    internal: Rc<Cell<bool>>,
    handler: Option<gtk::glib::SignalHandlerId>,
}

impl TenetScroll {
    pub fn new(window: &gtk::ScrolledWindow) -> Self {
        let motion = MotionToken::default();
        let internal = Rc::new(Cell::new(false));
        let token = motion.clone();
        let writing = internal.clone();
        let handler = window.vadjustment().connect_value_changed(move |_| {
            if !writing.get() {
                token.cancel();
            }
        });
        Self {
            window: window.clone(),
            motion,
            internal,
            handler: Some(handler),
        }
    }

    pub fn ensure_visible(&self, target: &gtk::Widget, animate: bool) {
        let adjustment = self.window.vadjustment();
        let viewport = adjustment.page_size();
        let Some(bounds) = target.compute_bounds(&self.window) else {
            return;
        };
        let current = adjustment.value();
        let from = current + f64::from(bounds.y());
        let end = from + f64::from(bounds.height());
        let destination = nearest_visible_offset(
            current,
            viewport,
            from,
            end,
            adjustment.lower(),
            adjustment.upper(),
        );
        self.motion.cancel();
        if (destination - current).abs() < 1.0 {
            return;
        }
        let weak_adjustment = adjustment.downgrade();
        let writing = self.internal.clone();
        let widget: gtk::Widget = self.window.clone().upcast();
        self.motion.run(
            &widget,
            if animate && widget.is_mapped() {
                SCROLL_DURATION_MS
            } else {
                0
            },
            move |progress| {
                if let Some(adjustment) = weak_adjustment.upgrade() {
                    let eased = 1.0 - (1.0 - progress).powi(3);
                    writing.set(true);
                    adjustment.set_value(current + (destination - current) * eased);
                    writing.set(false);
                }
            },
            || {},
        );
    }

    pub fn cancel(&self) {
        self.motion.cancel();
    }
}

impl Drop for TenetScroll {
    fn drop(&mut self) {
        self.cancel();
        if let Some(handler) = self.handler.take() {
            self.window.vadjustment().disconnect(handler);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_visible_does_not_scroll_if_row_is_already_visible() {
        assert_eq!(
            nearest_visible_offset(20.0, 100.0, 25.0, 80.0, 0.0, 500.0),
            20.0
        );
    }

    #[test]
    fn nearest_visible_scrolls_minimally_in_both_directions() {
        assert_eq!(
            nearest_visible_offset(80.0, 100.0, 20.0, 40.0, 0.0, 500.0),
            20.0
        );
        assert_eq!(
            nearest_visible_offset(80.0, 100.0, 170.0, 215.0, 0.0, 500.0),
            115.0
        );
    }

    #[test]
    fn nearest_visible_clamps_to_real_adjustment_limits() {
        assert_eq!(
            nearest_visible_offset(0.0, 100.0, -20.0, 0.0, 0.0, 500.0),
            0.0
        );
        assert_eq!(
            nearest_visible_offset(370.0, 100.0, 490.0, 535.0, 0.0, 500.0),
            400.0
        );
        assert_eq!(
            nearest_visible_offset(13.0, 0.0, 100.0, 140.0, 0.0, 500.0),
            13.0
        );
        assert_eq!(
            nearest_visible_offset(13.0, 100.0, f64::NAN, 140.0, 0.0, 500.0),
            13.0
        );
    }
}
