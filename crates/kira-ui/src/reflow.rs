//! Retained-key GTK FLIP: record the *painted* location of existing rows,
//! mutate their real layout, then animate surviving identities from that saved
//! location to the new one. Decorative movement never delays user intent.
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use gtk::{glib, prelude::*, subclass::prelude::*};

use crate::motion::{animations_enabled, back_out};

pub const REFLOW_DURATION_MS: u64 = 340;
pub const REFLOW_MAX_ROWS: usize = 24;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ReflowOffset {
    pub x: f32,
    pub y: f32,
}

pub fn reflow_offset(start: ReflowOffset, end: ReflowOffset, progress: f64) -> ReflowOffset {
    let remainder = (1.0 - back_out(progress, 1.14)) as f32;
    ReflowOffset {
        x: (start.x - end.x) * remainder,
        y: (start.y - end.y) * remainder,
    }
}

mod imp {
    use std::cell::{Cell, RefCell};

    use gtk::{glib, prelude::*, subclass::prelude::*};

    use super::ReflowOffset;
    use crate::motion::MotionToken;

    #[derive(Default)]
    pub struct ReflowRow {
        pub child: RefCell<Option<gtk::Widget>>,
        pub offset: Cell<ReflowOffset>,
        pub motion: MotionToken,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ReflowRow {
        const NAME: &'static str = "KiraTenetReflowRow";
        type Type = super::ReflowRow;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for ReflowRow {
        fn dispose(&self) {
            self.motion.cancel();
            if let Some(child) = self.child.borrow_mut().take() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for ReflowRow {
        fn measure(&self, orientation: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            self.child
                .borrow()
                .as_ref()
                .map_or((0, 0, -1, -1), |child| child.measure(orientation, for_size))
        }

        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            if let Some(child) = self.child.borrow().as_ref() {
                let offset = self.offset.get();
                let transform = gtk::gsk::Transform::new()
                    .translate(&gtk::graphene::Point::new(offset.x, offset.y));
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
    pub struct ReflowRow(ObjectSubclass<imp::ReflowRow>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl ReflowRow {
    pub fn new(child: &gtk::Widget) -> Self {
        let row: Self = glib::Object::new();
        row.imp().child.replace(Some(child.clone()));
        row.set_hexpand(child.compute_expand(gtk::Orientation::Horizontal));
        row.set_vexpand(child.compute_expand(gtk::Orientation::Vertical));
        row.set_halign(child.halign());
        row.set_valign(child.valign());
        child.set_parent(&row);
        row
    }

    pub fn painted_origin(&self, relative_to: &gtk::Widget) -> Option<ReflowOffset> {
        let child = self.imp().child.borrow().clone()?;
        let bounds = child.compute_bounds(relative_to)?;
        ReflowOffset::from_bounds(&bounds)
    }

    pub fn layout_origin(&self, relative_to: &gtk::Widget) -> Option<ReflowOffset> {
        let bounds = self.compute_bounds(relative_to)?;
        ReflowOffset::from_bounds(&bounds)
    }

    fn move_offset(&self, offset: ReflowOffset) {
        self.imp().offset.set(offset);
        self.queue_allocate();
    }

    /// Freeze at the current rendered transform before a second user action.
    /// The old tick callback must not move the row while GTK reorders it.
    pub fn freeze(&self) {
        self.imp().motion.cancel();
    }

    pub fn settle(&self) {
        self.freeze();
        self.move_offset(ReflowOffset { x: 0.0, y: 0.0 });
    }

    pub fn play_from(&self, previous: ReflowOffset, new: ReflowOffset) {
        let start = reflow_offset(previous, new, 0.0);
        if start.x.abs() < 0.5 && start.y.abs() < 0.5 {
            self.settle();
            return;
        }
        let weak = self.downgrade();
        let final_row = self.downgrade();
        self.imp().motion.run(
            &self.clone().upcast(),
            REFLOW_DURATION_MS,
            move |progress| {
                if let Some(row) = weak.upgrade() {
                    row.move_offset(reflow_offset(previous, new, progress));
                }
            },
            move || {
                if let Some(row) = final_row.upgrade() {
                    row.move_offset(ReflowOffset { x: 0.0, y: 0.0 });
                }
            },
        );
    }
}

impl ReflowOffset {
    fn from_bounds(bounds: &gtk::graphene::Rect) -> Option<Self> {
        let (x, y) = (bounds.x(), bounds.y());
        if x.is_finite() && y.is_finite() {
            Some(Self { x, y })
        } else {
            None
        }
    }
}

#[derive(Default)]
pub struct ReflowSnapshot {
    positions: HashMap<String, ReflowOffset>,
}

pub struct ReflowScope {
    container: gtk::Widget,
    generation: Rc<Cell<u64>>,
    active_rows: RefCell<Vec<glib::WeakRef<ReflowRow>>>,
}

impl ReflowScope {
    pub fn new(container: &gtk::Widget) -> Self {
        Self {
            container: container.clone(),
            generation: Rc::new(Cell::new(0)),
            active_rows: RefCell::new(Vec::new()),
        }
    }

    pub fn capture(&self, ordered: &[(String, ReflowRow)]) -> ReflowSnapshot {
        // A second input supersedes any pending layout tick, but surviving
        // rows must keep their last painted transform for continuous FLIP.
        self.invalidate_pending();
        if !animations_enabled() {
            self.cancel();
            for (_, row) in ordered {
                row.settle();
            }
            return ReflowSnapshot::default();
        }
        let mut positions = HashMap::new();
        if self.container.is_mapped() {
            for (key, row) in ordered.iter().take(REFLOW_MAX_ROWS) {
                if let Some(origin) = row.painted_origin(&self.container) {
                    positions.insert(key.clone(), origin);
                    row.freeze();
                }
            }
        }
        ReflowSnapshot { positions }
    }

    /// Called after applying a user-driven reorder/filter. Two bounded GTK
    /// frame ticks allow the new box allocation to be measured once, without
    /// running an idle polling loop or blocking the GTK main context.
    pub fn play(&self, snapshot: ReflowSnapshot, current: &[(String, ReflowRow)]) {
        self.invalidate_pending();
        let generation = self.generation.get();
        self.active_rows
            .replace(current.iter().map(|(_, row)| row.downgrade()).collect());
        if snapshot.positions.is_empty() || !self.container.is_mapped() || !animations_enabled() {
            self.cancel();
            return;
        }
        let rows: Vec<_> = current
            .iter()
            .filter_map(|(key, row)| {
                snapshot
                    .positions
                    .get(key)
                    .map(|old| (row.downgrade(), *old))
            })
            .collect();
        if rows.is_empty() {
            self.cancel();
            return;
        }
        let weak_container = self.container.downgrade();
        let token = self.generation.clone();
        let ticks = Cell::new(0);
        self.container.add_tick_callback(move |_, _| {
            if token.get() != generation {
                return gtk::glib::ControlFlow::Break;
            }
            ticks.set(ticks.get() + 1);
            if ticks.get() < 2 {
                return gtk::glib::ControlFlow::Continue;
            }
            if let Some(container) = weak_container.upgrade() {
                for (weak_row, old) in &rows {
                    if let Some(row) = weak_row.upgrade()
                        && let Some(new) = row.layout_origin(&container)
                    {
                        row.play_from(*old, new);
                    }
                }
            }
            gtk::glib::ControlFlow::Break
        });
    }

    fn invalidate_pending(&self) {
        self.generation.set(self.generation.get().wrapping_add(1));
    }

    /// Cancel both pending layout callbacks and active physical transforms.
    /// A hidden/destroyed widget must never reappear with a stale displacement.
    pub fn cancel(&self) {
        self.invalidate_pending();
        for row in self.active_rows.borrow_mut().drain(..) {
            if let Some(row) = row.upgrade() {
                row.settle();
            }
        }
    }
}

impl Drop for ReflowScope {
    fn drop(&mut self) {
        self.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recorded_path_starts_at_old_painted_coordinates_and_lands_exactly() {
        let before = ReflowOffset { x: 25.0, y: 150.0 };
        let after = ReflowOffset { x: 90.0, y: 35.0 };
        assert_eq!(
            reflow_offset(before, after, 0.0),
            ReflowOffset { x: -65.0, y: 115.0 }
        );
        assert_eq!(
            reflow_offset(before, after, 1.0),
            ReflowOffset { x: 0.0, y: 0.0 }
        );
    }

    #[test]
    fn source_back_out_spring_overshoots_without_changing_data_position() {
        let before = ReflowOffset { x: 0.0, y: 0.0 };
        let after = ReflowOffset { x: 0.0, y: 100.0 };
        let near_end = reflow_offset(before, after, 0.8);
        assert!(near_end.y > 0.0);
    }

    #[test]
    fn interruption_continues_from_current_painted_offset() {
        let old = ReflowOffset { x: 0.0, y: 0.0 };
        let first = ReflowOffset { x: 100.0, y: 0.0 };
        let next = ReflowOffset { x: 30.0, y: 0.0 };
        let visible = reflow_offset(old, first, 0.25);
        let painted = ReflowOffset {
            x: first.x + visible.x,
            y: first.y + visible.y,
        };
        assert_eq!(reflow_offset(painted, next, 0.0).x, painted.x - next.x,);
    }
}
