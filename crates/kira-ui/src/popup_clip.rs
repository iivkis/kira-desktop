//! GSK clip of an already measured GTK child: never resizes the Wayland
//! popup window while TENET materializes or retraces the same surface.
use gtk::{glib, prelude::*, subclass::prelude::*};

mod imp {
    use std::cell::{Cell, RefCell};

    use gtk::{prelude::*, subclass::prelude::*};

    #[derive(Default)]
    pub struct PopupClip {
        pub child: RefCell<Option<gtk::Widget>>,
        pub fraction: Cell<f64>,
        pub from_bottom: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PopupClip {
        const NAME: &'static str = "KiraTenetPopupClip";
        type Type = super::PopupClip;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for PopupClip {
        fn dispose(&self) {
            if let Some(child) = self.child.borrow_mut().take() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for PopupClip {
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
                child.allocate(width, height, baseline, None);
            }
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let Some(child) = self.child.borrow().clone() else {
                return;
            };
            let progress = self.fraction.get().clamp(0.0, 1.0);
            if progress <= 0.0 {
                return;
            }
            if progress >= 1.0 {
                self.obj().snapshot_child(&child, snapshot);
                return;
            }
            let width = self.obj().width().max(0) as f32;
            let height = self.obj().height().max(0) as f32;
            let visible = height * progress as f32;
            let top = if self.from_bottom.get() {
                height - visible
            } else {
                0.0
            };
            snapshot.push_clip(&gtk::graphene::Rect::new(0.0, top, width, visible));
            self.obj().snapshot_child(&child, snapshot);
            snapshot.pop();
        }
    }
}

glib::wrapper! {
    pub struct PopupClip(ObjectSubclass<imp::PopupClip>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl PopupClip {
    pub fn new(child: &gtk::Widget) -> Self {
        let clip: Self = glib::Object::new();
        clip.imp().child.replace(Some(child.clone()));
        child.set_parent(&clip);
        clip
    }

    pub fn set_direction(&self, direction: i32) {
        self.imp().from_bottom.set(direction < 0);
        self.queue_draw();
    }

    pub fn set_fraction(&self, fraction: f64) {
        self.imp().fraction.set(if fraction.is_finite() {
            fraction.clamp(0.0, 1.0)
        } else {
            0.0
        });
        self.queue_draw();
    }
}
