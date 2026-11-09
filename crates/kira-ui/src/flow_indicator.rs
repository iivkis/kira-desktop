//! One retained GTK selection indicator, with source TENET 420ms geometry
//! and 120ms monotonic visibility. No compositor, Core or shell dependencies.
use gtk::{glib, prelude::*, subclass::prelude::*};

use crate::motion::back_out;

pub const FLOW_DURATION_MS: u64 = 420;
pub const FLOW_FADE_MS: u64 = 120;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlowRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl FlowRect {
    pub fn interpolate(self, end: Self, progress: f64, axis: FlowAxis) -> Self {
        let amount = back_out(progress, 1.18) as f32;
        let mix = |start: f32, finish: f32| start + (finish - start) * amount;
        Self {
            x: mix(self.x, end.x),
            y: if axis == FlowAxis::Both {
                mix(self.y, end.y)
            } else {
                end.y
            },
            width: mix(self.width, end.width),
            height: if axis == FlowAxis::Both {
                mix(self.height, end.height)
            } else {
                end.height
            },
        }
    }

    fn from_bounds(bounds: gtk::graphene::Rect) -> Option<Self> {
        let values = [bounds.x(), bounds.y(), bounds.width(), bounds.height()];
        if values.iter().all(|value| value.is_finite())
            && bounds.width() > 0.0
            && bounds.height() > 0.0
        {
            Some(Self {
                x: bounds.x(),
                y: bounds.y(),
                width: bounds.width(),
                height: bounds.height(),
            })
        } else {
            None
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FlowAxis {
    #[default]
    Both,
    Horizontal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FlowStyle {
    #[default]
    Fill,
    Rail,
    /// Source system-monitor sortable columns use a single sliding baseline.
    Underline,
}

mod imp {
    use std::cell::{Cell, RefCell};

    use gtk::{glib, prelude::*, subclass::prelude::*};

    use super::{FlowAxis, FlowRect, FlowStyle};
    use crate::motion::MotionToken;

    #[derive(Default)]
    pub struct FlowIndicator {
        pub child: RefCell<Option<gtk::Widget>>,
        pub current: Cell<Option<FlowRect>>,
        pub destination: Cell<Option<FlowRect>>,
        pub opacity: Cell<f32>,
        pub target: RefCell<Option<glib::WeakRef<gtk::Widget>>>,
        pub axis: Cell<FlowAxis>,
        pub style: Cell<FlowStyle>,
        pub motion: MotionToken,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for FlowIndicator {
        const NAME: &'static str = "KiraTenetFlowIndicator";
        type Type = super::FlowIndicator;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for FlowIndicator {
        fn dispose(&self) {
            self.motion.cancel();
            self.target.borrow_mut().take();
            if let Some(child) = self.child.borrow_mut().take() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for FlowIndicator {
        fn unmap(&self) {
            // GTK does not tick hidden surfaces. Forget the last frame while
            // retaining the selected target, so a remap measures fresh bounds
            // and paints the factual selection without a zombie spring.
            self.motion.cancel();
            self.current.set(None);
            self.destination.set(None);
            self.opacity.set(0.0);
            self.obj().remove_css_class("flow-moving");
            self.parent_unmap();
        }

        fn measure(&self, orientation: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            self.child
                .borrow()
                .as_ref()
                .map_or((0, 0, -1, -1), |child| child.measure(orientation, for_size))
        }

        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            if let Some(child) = self.child.borrow().as_ref() {
                child.allocate(width, height, baseline, None);
            }
            // Reconcile real physical geometry after GTK recalculates
            // allocations, including keyed reorders and window scaling.
            self.obj().reconcile(true);
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let marker = self.current.get();
            let opacity = f64::from(self.opacity.get()).clamp(0.0, 1.0) as f32;
            let style = self.style.get();
            if style == FlowStyle::Fill
                && let Some(rect) = marker
            {
                snapshot.append_color(
                    &gtk::gdk::RGBA::new(0.988, 0.639, 0.067, opacity),
                    &gtk::graphene::Rect::new(
                        rect.x,
                        rect.y,
                        rect.width.max(0.0),
                        rect.height.max(0.0),
                    ),
                );
            }
            if let Some(child) = self.child.borrow().as_ref() {
                self.obj().snapshot_child(child, snapshot);
            }
            if let Some(rect) = marker {
                if style == FlowStyle::Rail {
                    snapshot.append_color(
                        &gtk::gdk::RGBA::new(0.988, 0.639, 0.067, opacity),
                        &gtk::graphene::Rect::new(rect.x, rect.y, 3.0, rect.height.max(0.0)),
                    );
                } else if style == FlowStyle::Underline {
                    snapshot.append_color(
                        &gtk::gdk::RGBA::new(0.988, 0.639, 0.067, opacity),
                        &gtk::graphene::Rect::new(
                            rect.x,
                            rect.y + (rect.height - 2.0).max(0.0),
                            rect.width.max(0.0),
                            2.0,
                        ),
                    );
                }
            }
        }
    }
}

glib::wrapper! {
    pub struct FlowIndicator(ObjectSubclass<imp::FlowIndicator>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl FlowIndicator {
    pub fn new(child: &gtk::Widget, axis: FlowAxis, style: FlowStyle) -> Self {
        let indicator: Self = glib::Object::new();
        indicator.imp().axis.set(axis);
        indicator.imp().style.set(style);
        indicator.imp().child.replace(Some(child.clone()));
        // A GTK wrapper must preserve the contained grid/list's expansion and
        // alignment contract, not silently turn Fill into a narrow requisition.
        indicator.set_hexpand(child.compute_expand(gtk::Orientation::Horizontal));
        indicator.set_vexpand(child.compute_expand(gtk::Orientation::Vertical));
        indicator.set_halign(child.halign());
        indicator.set_valign(child.valign());
        child.set_parent(&indicator);
        indicator
    }

    pub fn retarget(&self, target: &gtk::Widget) {
        self.imp().target.replace(Some(target.downgrade()));
        self.reconcile(true);
        self.queue_allocate();
    }

    pub fn clear(&self) {
        self.imp().motion.cancel();
        self.imp().target.borrow_mut().take();
        self.imp().destination.set(None);
        self.imp().current.set(None);
        self.imp().opacity.set(0.0);
        self.remove_css_class("flow-moving");
        self.queue_draw();
    }

    fn reconcile(&self, animate: bool) {
        let Some(target) = self
            .imp()
            .target
            .borrow()
            .as_ref()
            .and_then(glib::WeakRef::upgrade)
        else {
            return;
        };
        let Some(rect) = target.compute_bounds(self).and_then(FlowRect::from_bounds) else {
            // On the first unmapped frame the real layout does not exist.
            // Keep the target reference and retry at the next allocation.
            return;
        };
        if self.imp().destination.get() == Some(rect) {
            return;
        }
        self.imp().destination.set(Some(rect));
        let Some(start) = self.imp().current.get() else {
            self.imp().motion.cancel();
            self.imp().current.set(Some(rect));
            self.imp().opacity.set(1.0);
            self.remove_css_class("flow-moving");
            self.queue_draw();
            return;
        };
        if start == rect {
            self.imp().motion.cancel();
            self.remove_css_class("flow-moving");
            return;
        }
        if !animate || !self.is_mapped() {
            self.imp().motion.cancel();
            self.imp().current.set(Some(rect));
            self.imp().opacity.set(1.0);
            self.remove_css_class("flow-moving");
            self.queue_draw();
            return;
        }
        self.add_css_class("flow-moving");
        let from_opacity = self.imp().opacity.get();
        let step_view = self.downgrade();
        let finished_view = self.downgrade();
        let axis = self.imp().axis.get();
        self.imp().motion.run(
            &self.clone().upcast(),
            FLOW_DURATION_MS,
            move |progress| {
                if let Some(indicator) = step_view.upgrade() {
                    indicator
                        .imp()
                        .current
                        .set(Some(start.interpolate(rect, progress, axis)));
                    let fade =
                        (progress * FLOW_DURATION_MS as f64 / FLOW_FADE_MS as f64).clamp(0.0, 1.0);
                    let opacity = f64::from(from_opacity)
                        + (1.0 - f64::from(from_opacity)) * (1.0 - (1.0 - fade).powi(2));
                    indicator.imp().opacity.set(opacity as f32);
                    indicator.queue_draw();
                }
            },
            move || {
                if let Some(indicator) = finished_view.upgrade() {
                    indicator.imp().current.set(Some(rect));
                    indicator.imp().opacity.set(1.0);
                    indicator.remove_css_class("flow-moving");
                    indicator.queue_draw();
                }
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f32, y: f32, width: f32, height: f32) -> FlowRect {
        FlowRect {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn source_spring_ends_at_exact_target_without_overshooting_visibility() {
        let a = rect(0.0, 1.0, 32.0, 24.0);
        let b = rect(90.0, 80.0, 48.0, 30.0);
        assert_eq!(a.interpolate(b, 0.0, FlowAxis::Both), a);
        assert_eq!(a.interpolate(b, 1.0, FlowAxis::Both), b);
        assert!(a.interpolate(b, 0.9, FlowAxis::Both).x > b.x);
    }

    #[test]
    fn horizontal_marker_tracks_only_x_and_width() {
        let a = rect(0.0, 5.0, 31.0, 29.0);
        let b = rect(70.0, 10.0, 44.0, 34.0);
        let sample = a.interpolate(b, 0.5, FlowAxis::Horizontal);
        assert_eq!(sample.y, b.y);
        assert_eq!(sample.height, b.height);
        assert!(sample.x > b.x);
    }

    #[test]
    fn interruption_samples_live_position_not_prior_destination() {
        let a = rect(0.0, 0.0, 30.0, 30.0);
        let b = rect(80.0, 0.0, 40.0, 30.0);
        let c = rect(20.0, 0.0, 36.0, 30.0);
        let running = a.interpolate(b, 0.32, FlowAxis::Both);
        assert_eq!(running.interpolate(c, 0.0, FlowAxis::Both), running);
        assert_eq!(running.interpolate(c, 1.0, FlowAxis::Both), c);
    }
}
