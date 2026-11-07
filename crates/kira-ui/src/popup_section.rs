//! A GTK-only, independently staged TENET section. Geometry is transformed
//! at allocation time, never by changing the Wayland window or widget model.
use gtk::{glib, prelude::*, subclass::prelude::*};

use crate::motion::popup_section_frame;

mod imp {
    use std::cell::{Cell, RefCell};

    use gtk::{glib, prelude::*, subclass::prelude::*};

    use crate::motion::popup_section_frame;

    #[derive(Default)]
    pub struct PopupSection {
        pub child: RefCell<Option<gtk::Widget>>,
        pub progress: Cell<f64>,
        pub index: Cell<usize>,
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
                let frame = popup_section_frame(self.progress.get(), self.index.get());
                let shift_y = frame.translate_y as f32;
                let scale = frame.scale as f32;
                // Center scale around the original horizontal contact to
                // avoid an unwanted left-edge shift during the spring.
                let offset_x = (width as f32) * (1.0 - scale) * 0.5;
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

    pub fn set_progress(&self, progress: f64) {
        let normalized = if progress.is_finite() {
            progress.clamp(0.0, 1.0)
        } else {
            1.0
        };
        self.imp().progress.set(normalized);
        if let Some(child) = self.imp().child.borrow().as_ref() {
            child.set_opacity(popup_section_frame(normalized, self.imp().index.get()).opacity);
        }
        self.queue_allocate();
    }
}
