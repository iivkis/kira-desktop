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
    adjustment: gtk::Adjustment,
    motion: MotionToken,
    internal: Rc<Cell<bool>>,
    pending: Rc<Cell<Option<f64>>>,
    wheel_controller: Option<gtk::EventControllerScroll>,
    handler: Option<gtk::glib::SignalHandlerId>,
    unmap_handler: Option<gtk::glib::SignalHandlerId>,
}

impl TenetScroll {
    pub fn new(window: &gtk::ScrolledWindow) -> Self {
        let motion = MotionToken::default();
        let internal = Rc::new(Cell::new(false));
        let token = motion.clone();
        let writing = internal.clone();
        let pending = Rc::new(Cell::new(None));
        let pending_manual = pending.clone();
        let adjustment = window.vadjustment();
        let handler = adjustment.connect_value_changed(move |_| {
            if !writing.get() {
                // The user's actual adjustment wins immediately over the
                // decorative destination, including before the next tick.
                token.cancel();
                pending_manual.set(None);
            }
        });
        // Cancel on the actual wheel gesture, including at a scroll limit
        // where GTK may not emit adjustment::value-changed at all.
        // Proceed so the ScrolledWindow still owns the physical scrolling.
        let wheel_controller =
            gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
        wheel_controller.set_propagation_phase(gtk::PropagationPhase::Capture);
        let wheel_motion = motion.clone();
        let wheel_pending = pending.clone();
        wheel_controller.connect_scroll(move |_, _, _| {
            wheel_motion.cancel();
            wheel_pending.set(None);
            gtk::glib::Propagation::Proceed
        });
        window.add_controller(wheel_controller.clone());
        let unmap_motion = motion.clone();
        let unmap_pending = pending.clone();
        let unmap_writing = internal.clone();
        let weak_adjustment = adjustment.downgrade();
        let unmap_handler = window.connect_unmap(move |_| {
            // Hidden GTK windows do not tick. Keep the requested factual
            // scroll target and clear its in-flight animation on unmap.
            unmap_motion.cancel();
            if let Some(destination) = unmap_pending.replace(None)
                && let Some(adjustment) = weak_adjustment.upgrade()
            {
                unmap_writing.set(true);
                adjustment.set_value(destination);
                unmap_writing.set(false);
            }
        });
        Self {
            window: window.clone(),
            adjustment,
            motion,
            internal,
            pending,
            wheel_controller: Some(wheel_controller),
            handler: Some(handler),
            unmap_handler: Some(unmap_handler),
        }
    }

    pub fn ensure_visible(&self, target: &gtk::Widget, animate: bool) {
        // The old destination must stop even if this target has no layout yet.
        self.motion.cancel();
        self.pending.set(None);
        let adjustment = self.adjustment.clone();
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
        if (destination - current).abs() < 1.0 {
            return;
        }
        self.pending.set(Some(destination));
        let weak_adjustment = adjustment.downgrade();
        let writing = self.internal.clone();
        let pending_complete = self.pending.clone();
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
            move || pending_complete.set(None),
        );
    }

    /// Search resets selection to row zero immediately. Do not use the
    /// previous query's scrolling state for the next filtered list.
    pub fn scroll_to_start(&self) {
        self.motion.cancel();
        self.pending.set(None);
        let adjustment = self.adjustment.clone();
        self.internal.set(true);
        adjustment.set_value(adjustment.lower());
        self.internal.set(false);
    }

    pub fn cancel(&self) {
        self.motion.cancel();
        self.pending.set(None);
    }
}

impl Drop for TenetScroll {
    fn drop(&mut self) {
        self.cancel();
        if let Some(controller) = self.wheel_controller.take() {
            self.window.remove_controller(&controller);
        }
        if let Some(handler) = self.handler.take() {
            // The ScrolledWindow can replace its adjustment; always disconnect
            // from the exact GObject that originally owned this handler.
            self.adjustment.disconnect(handler);
        }
        if let Some(handler) = self.unmap_handler.take() {
            self.window.disconnect(handler);
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
    fn nearest_visible_preserves_scroll_if_everything_fits() {
        assert_eq!(
            nearest_visible_offset(0.0, 480.0, 70.0, 105.0, 0.0, 400.0),
            0.0
        );
        assert_eq!(
            nearest_visible_offset(150.0, 100.0, 160.0, 199.0, 0.0, 500.0),
            150.0
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
