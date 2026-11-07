//! A GTK-only, independently staged TENET section. Geometry is transformed
//! at allocation time, never by changing the Wayland window or widget model.
use gtk::{glib, prelude::*, subclass::prelude::*};

use crate::motion::{
    CALENDAR_GRID_DURATION_MS, MotionToken, calendar_grid_frame, popup_section_frame,
};

#[derive(Clone, Copy, Default)]
enum MotionProfile {
    #[default]
    Popup,
    CalendarGrid,
}

mod imp {
    use std::cell::{Cell, RefCell};

    use gtk::{glib, prelude::*, subclass::prelude::*};

    use crate::motion::{calendar_grid_frame, popup_section_frame};
    use super::MotionProfile;

    #[derive(Default)]
    pub struct PopupSection {
        pub child: RefCell<Option<gtk::Widget>>,
        pub progress: Cell<f64>,
        pub index: Cell<usize>,
        pub profile: Cell<MotionProfile>,
        pub direction: Cell<i32>,
        pub motion: super::MotionToken,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PopupSection {
        const NAME: &'static str = "KiraTenetPopupSection";
        type Type = super::PopupSection;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for PopupSection {
        fn dispose(&self) {
            if let Some(child) = self.child.borrow_mut().take() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for PopupSection {
        fn measure(
            &self,
            orientation: gtk::Orientation,
            for_size: i32,
        ) -> (i32, i32, i32, i32) {
            self.child
                .borrow()
                .as_ref()
                .map_or((0, 0, -1, -1), |child| child.measure(orientation, for_size))
        }

        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            if let Some(child) = self.child.borrow().as_ref() {
                let (shift_x, shift_y, scale) = match self.profile.get() {
                    MotionProfile::Popup => {
                        let frame = popup_section_frame(self.progress.get(), self.index.get());
                        (0.0, frame.translate_y as f32, frame.scale as f32)
                    }
                    MotionProfile::CalendarGrid => {
                        let frame = calendar_grid_frame(self.progress.get(), self.direction.get());
                        (frame.x as f32, frame.y as f32, frame.scale as f32)
                    }
                };
                // Center scale around the original horizontal contact to
                // avoid an unwanted left-edge shift during the spring.
                let offset_x = shift_x + (width as f32) * (1.0 - scale) * 0.5;
                let transform = gtk::gsk::Transform::new()
                    .translate(&gtk::graphene::Point::new(offset_x, shift_y))
                    .scale(scale, scale);
                child.allocate(width, height, baseline, Some(transform));
            }
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            if let Some(child) = self.child.borrow().as_ref() {
                self.obj().snapshot_child(child, snapshot);
            }
        }
    }
}

glib::wrapper! {
    pub struct PopupSection(ObjectSubclass<imp::PopupSection>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl PopupSection {
    pub fn new(child: &gtk::Widget, index: usize) -> Self {
        let section: Self = glib::Object::new();
        section.imp().index.set(index.min(2));
        section.imp().child.replace(Some(child.clone()));
        child.set_parent(&section);
        section.set_progress(0.0);
        section
    }

    /// The same reusable GSK surface with the source calendar's separate
    /// 440ms 8px/2px spring. It never duplicates the GTK grid buttons.
    pub fn calendar_grid(child: &gtk::Widget) -> Self {
        let surface = Self::new(child, 0);
        surface.imp().profile.set(MotionProfile::CalendarGrid);
        surface.set_progress(1.0);
        surface
    }

    pub fn play_calendar_grid(&self, direction: i32) {
        if !matches!(self.imp().profile.get(), MotionProfile::CalendarGrid) {
            return;
        }
        self.imp().motion.cancel();
        self.imp().direction.set(direction);
        if !self.is_mapped() {
            self.set_progress(1.0);
            return;
        }
        self.set_progress(0.0);
        let step_view = self.downgrade();
        let done_view = self.downgrade();
        self.imp().motion.run(
            &self.clone().upcast(),
            CALENDAR_GRID_DURATION_MS,
            move |cursor| {
                if let Some(surface) = step_view.upgrade() {
                    surface.set_progress(cursor);
                }
            },
            move || {
                if let Some(surface) = done_view.upgrade() {
                    surface.set_progress(1.0);
                }
            },
        );
    }

    pub fn set_progress(&self, progress: f64) {
        let normalized = if progress.is_finite() {
            progress.clamp(0.0, 1.0)
        } else {
            1.0
        };
        self.imp().progress.set(normalized);
        if let Some(child) = self.imp().child.borrow().as_ref() {
            let opacity = match self.imp().profile.get() {
                MotionProfile::Popup => {
                    popup_section_frame(normalized, self.imp().index.get()).opacity
                }
                MotionProfile::CalendarGrid => {
                    calendar_grid_frame(normalized, self.imp().direction.get()).opacity
                }
            };
            child.set_opacity(opacity);
        }
        self.queue_allocate();
    }
}
