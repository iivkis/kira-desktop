//! Reusable GTK4 notification presentation: one output-targeted shaped,
//! bottom-right Wayland surface. No D-Bus, IPC, WebKit or access to external application commands.
//! A frame is acknowledged only after two real mapped GTK frame ticks.
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use gtk::cairo::{RectangleInt, Region};
use gtk::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use crate::motion::MotionToken;

pub const MAX_TOASTS: usize = 3;
pub const PREFERRED_WIDTH: i32 = 360;
pub const MIN_HOST_WIDTH: i32 = 240;
pub const OUTER_MARGIN: i32 = 12;
pub const INNER_MARGIN: i32 = 8;
pub const GAP: i32 = 8;
pub const MIN_CARD_HEIGHT: i32 = 58;
pub const TOAST_ENTRY_MS: u64 = 180;
pub const TOAST_EXIT_MS: u64 = 180;

/// Pinned main source: y=12→0, alpha=0→1 in 180ms power2.out.
/// The actual paint-gated dwell starts independently of this decoration.
pub fn toast_entry_frame(progress: f64) -> (f64, f64) {
    let time = if progress.is_finite() {
        progress.clamp(0.0, 1.0)
    } else {
        1.0
    };
    let remaining = (1.0 - time).powi(2);
    (12.0 * remaining, 1.0 - remaining)
}

/// Inverted *same-path* departure. It is the exact reverse of the
/// 12px/alpha arrival, not an unrelated fade or generic GTK transition.
pub fn toast_exit_frame(progress: f64) -> (f64, f64) {
    toast_exit_from_opacity(progress, 1.0)
}

/// The actual GTK content may be halfway through its entrance when it is
/// dismissed. Reverse from its observed opacity/contact point, instead of
/// snapping to a fictional fully opaque start frame.
pub fn toast_exit_from_opacity(progress: f64, opacity: f64) -> (f64, f64) {
    let time = if progress.is_finite() {
        progress.clamp(0.0, 1.0)
    } else {
        1.0
    };
    let alpha = if opacity.is_finite() {
        opacity.clamp(0.0, 1.0)
    } else {
        1.0
    };
    let cursor = (1.0 - alpha).sqrt();
    let remaining = (cursor + (1.0 - cursor) * time).powi(2);
    (12.0 * remaining, 1.0 - remaining)
}

/// Only a genuinely displayed ID that leaves the new queue snapshot may
/// animate out. A replaced card, hidden/unmapped host or changed output
/// geometry must not create a fake notification or keep intercepting input.
pub fn departing_toasts(
    previous: &[NotificationView],
    next: &[NotificationView],
    painted: &HashSet<u32>,
    geometry_unchanged: bool,
) -> Vec<(u32, usize)> {
    if !geometry_unchanged {
        return Vec::new();
    }
    previous
        .iter()
        .enumerate()
        .filter(|(_, item)| painted.contains(&item.id) && !next.iter().any(|new| new.id == item.id))
        .map(|(index, item)| (item.id, index))
        .collect()
}

struct CardEntrance {
    id: u32,
    frame: gtk::Fixed,
    card: gtk::Widget,
    motion: MotionToken,
}

/// An old GTK button can finish dispatching after its card was detached.
/// Leases bind every button to the exact rendered snapshot which created
/// it, so A→B→A output switches never resurrect stale actions.
#[derive(Clone, Default)]
struct ToastActionGate(Rc<Cell<u64>>);

impl ToastActionGate {
    fn renew(&self) -> u64 {
        let next = self.0.get().wrapping_add(1);
        self.0.set(next);
        next
    }

    fn accepts(&self, lease: u64) -> bool {
        self.0.get() == lease
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ToastLayout {
    pub width: i32,
    pub height: i32,
    pub card_height: i32,
}

/// The height never exceeds half the logical output. Tiny surfaces fail
/// closed instead of reserving an unclickable region.
pub fn toast_layout(
    output_width: i32,
    output_height: i32,
    bottom_clearance: i32,
) -> Option<ToastLayout> {
    if output_width <= OUTER_MARGIN * 2 || output_height <= OUTER_MARGIN * 2 {
        return None;
    }
    let width = PREFERRED_WIDTH.min(output_width - OUTER_MARGIN * 2);
    if width < MIN_HOST_WIDTH {
        return None;
    }
    let height = (output_height / 2)
        .min(bottom_clearance - OUTER_MARGIN)
        .max(0);
    let card_height = (height - INNER_MARGIN * 2 - GAP * 2) / 3;
    (card_height >= MIN_CARD_HEIGHT).then_some(ToastLayout {
        width,
        height,
        card_height,
    })
}

/// Choose a *real* output rather than silently using monitor index zero.
/// Prefer the bar's current monitor when it has enough logical space; if it
/// cannot host 240px safely, choose the first other usable output. No output
/// means no renderable toast, not imaginary geometry. The Shell maps these
/// indices to live GDK Monitor objects and never passes Niri commands here.
pub fn toast_output_plan(
    outputs: &[(i32, i32)],
    preferred: Option<usize>,
) -> Option<(usize, ToastLayout)> {
    toast_output_plan_unobstructed(outputs, preferred, &[])
}

/// Choose a safe, *unobstructed* live output. A pooled transient covers
/// only its own output, not the whole desktop. The caller constructs the
/// blocker mask from real compositor monitor identities. Missing mask
/// entries are not blocked; unknown popup affinity must be rejected by the
/// trusted caller before invoking this plan.
pub fn toast_output_plan_unobstructed(
    outputs: &[(i32, i32)],
    preferred: Option<usize>,
    blocked: &[bool],
) -> Option<(usize, ToastLayout)> {
    toast_output_plan_stable(outputs, preferred, None, blocked)
}

/// A changing GDK enumeration must not shuffle an already safe fallback
/// output. The mapped bar is authoritative when usable; otherwise retain
/// the last safe actual monitor before selecting another real output.
/// The Shell converts both indices from GDK object identity, never Niri
/// numbering, and refuses unknown transient affinity.
pub fn toast_output_plan_stable(
    outputs: &[(i32, i32)],
    preferred: Option<usize>,
    retained: Option<usize>,
    blocked: &[bool],
) -> Option<(usize, ToastLayout)> {
    let candidate = |index: usize| {
        if blocked.get(index).copied().unwrap_or(false) {
            return None;
        }
        outputs.get(index).and_then(|&(width, height)| {
            toast_layout(width, height, height).map(|layout| (index, layout))
        })
    };
    preferred
        .and_then(candidate)
        .or_else(|| retained.and_then(candidate))
        .or_else(|| (0..outputs.len()).find_map(candidate))
}

pub fn toast_hit_region(layout: ToastLayout, painted_count: usize) -> Region {
    let region = Region::create();
    for index in 0..painted_count.min(MAX_TOASTS) {
        let y = layout.height
            - INNER_MARGIN
            - layout.card_height
            - index as i32 * (layout.card_height + GAP);
        let rect = RectangleInt::new(
            INNER_MARGIN,
            y,
            layout.width - INNER_MARGIN * 2,
            layout.card_height,
        );
        let _ = region.union_rectangle(&rect);
    }
    region
}

/// Plain, bounded view-model. External markup is always rendered as GTK
/// Label text, never as Pango markup or application-controlled actions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotificationView {
    pub id: u32,
    pub app: String,
    pub summary: String,
    pub body: String,
    pub critical: bool,
    pub actions: Vec<(String, String)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NotificationIntent {
    Close(u32),
    Action(u32, String),
}

pub fn bounded_notification(view: &NotificationView) -> bool {
    view.id != 0
        && view.app.chars().count() <= 128
        && view.summary.chars().count() <= 256
        && view.body.chars().count() <= 2048
        && view.actions.len() <= 8
        && view
            .actions
            .iter()
            .all(|(key, label)| key.chars().count() <= 128 && label.chars().count() <= 128)
}

fn text_label(value: &str, class: &str, xalign: f32) -> gtk::Label {
    let label = gtk::Label::new(Some(value));
    label.set_xalign(xalign);
    label.add_css_class(class);
    label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    label
}

fn card(
    notice: &NotificationView,
    height: i32,
    intent: Rc<dyn Fn(NotificationIntent)>,
) -> gtk::Widget {
    let root = gtk::Box::new(gtk::Orientation::Vertical, 3);
    root.add_css_class("notification-card");
    if notice.critical {
        root.add_css_class("critical");
    }
    root.set_height_request(height);
    root.set_overflow(gtk::Overflow::Hidden);
    root.set_hexpand(true);

    let top = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let app = text_label(&notice.app, "notification-sender", 0.0);
    app.set_hexpand(true);
    top.append(&app);
    if notice.critical {
        let important = text_label("ВАЖНО", "notification-critical", 0.0);
        top.append(&important);
    }
    let close = gtk::Button::with_label("×");
    close.add_css_class("notification-close");
    close.update_property(&[gtk::accessible::Property::Label(&format!(
        "Закрыть уведомление от {}",
        notice.app
    ))]);
    let id = notice.id;
    let action = intent.clone();
    close.connect_clicked(move |_| action(NotificationIntent::Close(id)));
    top.append(&close);
    root.append(&top);

    let default_action = notice.actions.iter().any(|(key, _)| key == "default");
    let summary = text_label(&notice.summary, "notification-title", 0.0);
    summary.set_hexpand(true);
    if default_action {
        let open = gtk::Button::new();
        open.add_css_class("notification-open");
        open.set_child(Some(&summary));
        open.update_property(&[gtk::accessible::Property::Label(&format!(
            "Открыть уведомление {} от {}",
            notice.summary, notice.app
        ))]);
        let intent = intent.clone();
        let id = notice.id;
        open.connect_clicked(move |_| {
            intent(NotificationIntent::Action(id, "default".to_owned()));
        });
        root.append(&open);
    } else {
        root.append(&summary);
    }
    let extra_count = notice
        .actions
        .iter()
        .filter(|(key, _)| key != "default")
        .count();
    let show_body = height >= (if extra_count > 0 { 94 } else { 74 });
    if !notice.body.is_empty() && show_body {
        let body = text_label(&notice.body, "notification-body", 0.0);
        body.set_wrap(true);
        body.set_wrap_mode(gtk::pango::WrapMode::WordChar);
        body.set_lines(2);
        body.set_tooltip_text(Some(&notice.body));
        root.append(&body);
    }
    if extra_count > 0 {
        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        actions.add_css_class("notification-actions");
        for (key, label) in notice
            .actions
            .iter()
            .filter(|(key, _)| key != "default")
            .take(2)
        {
            let button = gtk::Button::with_label(label);
            button.add_css_class("notification-action");
            let id = notice.id;
            let key = key.clone();
            let intent = intent.clone();
            button.connect_clicked(move |_| {
                intent(NotificationIntent::Action(id, key.clone()));
            });
            actions.append(&button);
        }
        root.append(&actions);
    }
    root.upcast()
}

/// Native notification host is a reusable GTK component. The Shell supplies
/// the declared notification actions and a frame acknowledgement callback.
pub struct NotificationHost {
    pub window: gtk::ApplicationWindow,
    root: gtk::Box,
    leaving: gtk::Fixed,
    visible_frames: RefCell<HashMap<u32, gtk::Fixed>>,
    painted_ids: RefCell<HashSet<u32>>,
    suppress_departure: Cell<bool>,
    layout: Cell<ToastLayout>,
    current: RefCell<Vec<NotificationView>>,
    revision: Rc<Cell<u64>>,
    pending_entrances: RefCell<Vec<CardEntrance>>,
    active_motion: RefCell<Vec<MotionToken>>,
    on_intent: Rc<dyn Fn(NotificationIntent)>,
    on_painted: Rc<dyn Fn(Vec<u32>)>,
    action_gate: ToastActionGate,
}

impl NotificationHost {
    pub fn new(
        app: &gtk::Application,
        monitor: &gtk::gdk::Monitor,
        layout: ToastLayout,
        on_intent: impl Fn(NotificationIntent) + 'static,
        on_painted: impl Fn(Vec<u32>) + 'static,
    ) -> Option<Self> {
        let window = gtk::ApplicationWindow::new(app);
        window.set_title(Some("Kira Notifications"));
        window.set_decorated(false);
        window.add_css_class("kira-notification-host");
        window.init_layer_shell();
        window.set_namespace(Some("kira-notifications"));
        window.set_layer(Layer::Overlay);
        window.set_anchor(Edge::Left, false);
        window.set_anchor(Edge::Top, false);
        window.set_anchor(Edge::Right, true);
        window.set_anchor(Edge::Bottom, true);
        // A layer surface without an explicit monitor may land on the wrong
        // output even when its geometry was measured from another monitor.
        window.set_monitor(Some(monitor));
        window.set_margin(Edge::Right, OUTER_MARGIN);
        window.set_margin(Edge::Bottom, OUTER_MARGIN);
        window.set_exclusive_zone(0);
        window.set_keyboard_mode(KeyboardMode::OnDemand);
        window.set_default_size(layout.width, layout.height);

        let root = gtk::Box::new(gtk::Orientation::Vertical, GAP);
        root.add_css_class("notification-stack");
        root.set_margin_start(INNER_MARGIN);
        root.set_margin_end(INNER_MARGIN);
        root.set_margin_top(INNER_MARGIN);
        root.set_margin_bottom(INNER_MARGIN);
        let layers = gtk::Overlay::new();
        layers.set_child(Some(&root));
        let leaving = gtk::Fixed::new();
        leaving.set_halign(gtk::Align::Fill);
        leaving.set_valign(gtk::Align::Fill);
        leaving.set_hexpand(true);
        leaving.set_vexpand(true);
        leaving.set_can_target(false);
        leaving.set_overflow(gtk::Overflow::Hidden);
        layers.add_overlay(&leaving);
        window.set_child(Some(&layers));
        window.set_opacity(0.0);
        gtk::prelude::WidgetExt::realize(&window);
        let surface = window.surface()?;
        // Before any rendered frame, not even a single input pixel belongs
        // to an invisible host. Otherwise the 360px column hijacks clicks.
        surface.set_input_region(Some(&Region::create()));
        window.set_visible(true);
        Some(Self {
            window,
            root,
            leaving,
            visible_frames: RefCell::new(HashMap::new()),
            painted_ids: RefCell::new(HashSet::new()),
            suppress_departure: Cell::new(false),
            layout: Cell::new(layout),
            current: RefCell::new(Vec::new()),
            revision: Rc::new(Cell::new(0)),
            pending_entrances: RefCell::new(Vec::new()),
            active_motion: RefCell::new(Vec::new()),
            on_intent: Rc::new(on_intent),
            on_painted: Rc::new(on_painted),
            action_gate: ToastActionGate::default(),
        })
    }

    /// Hotplug or a moved Top Bar changes the destination output. Release
    /// old shaped-input ownership and old animation frames *before* retargeting.
    /// The queued D-Bus notification IDs remain owned by the trusted Shell.
    pub fn retarget_monitor(&self, monitor: &gtk::gdk::Monitor) {
        if self.window.monitor().as_ref() != Some(monitor) {
            self.hide();
            self.window.set_monitor(Some(monitor));
        }
    }

    pub fn has_snapshot(&self, cards: &[NotificationView], layout: ToastLayout) -> bool {
        self.layout.get() == layout && self.current.borrow().as_slice() == cards
    }

    pub fn set_cards(&self, layout: ToastLayout, cards: &[NotificationView]) {
        assert!(cards.len() <= MAX_TOASTS && cards.iter().all(bounded_notification));
        if self.has_snapshot(cards, layout) {
            return;
        }
        self.revision.set(self.revision.get().wrapping_add(1));
        let lease = self.action_gate.renew();
        let action_gate = self.action_gate.clone();
        let dispatch = self.on_intent.clone();
        let on_intent: Rc<dyn Fn(NotificationIntent)> = Rc::new(move |intent| {
            if action_gate.accepts(lease) {
                dispatch(intent);
            }
        });
        let old_layout = self.layout.replace(layout);
        let previous = self.current.borrow().clone();
        let painted = self.painted_ids.replace(HashSet::new());
        let departures = departing_toasts(
            &previous,
            cards,
            &painted,
            old_layout == layout && !self.suppress_departure.get(),
        );
        let old_ids: HashSet<_> = previous.iter().map(|notice| notice.id).collect();
        for token in self.active_motion.borrow_mut().drain(..) {
            token.cancel();
        }
        self.pending_entrances.borrow_mut().clear();

        // A superseded exit never survives into the next snapshot. Reuse
        // actual old GTK frames for departure, not a copied image/ghost.
        while let Some(child) = self.leaving.first_child() {
            self.leaving.remove(&child);
        }
        let mut available = std::mem::take(&mut *self.visible_frames.borrow_mut());
        let exiting = departures
            .into_iter()
            .filter_map(|(id, index)| available.remove(&id).map(|frame| (index, frame)))
            .collect::<Vec<_>>();

        *self.current.borrow_mut() = cards.to_vec();
        self.root.set_opacity(0.0);
        self.window
            .set_opacity(if exiting.is_empty() { 0.0 } else { 1.0 });
        self.clear_input();
        self.window.set_default_size(layout.width, layout.height);
        while let Some(child) = self.root.first_child() {
            self.root.remove(&child);
        }

        // The old surface's *already-painted* cards leave along the
        // reversed entrance path. They are not targetable and do not
        // hold the Freedesktop notification ID or its input region.
        for (index, frame) in exiting {
            frame.set_can_target(false);
            // An interrupted entrance left this child's *real* opacity
            // and position at an intermediate contact point. Transfer that
            // pose to the outgoing container before reversing the path.
            let start_alpha = frame
                .first_child()
                .map(|child| {
                    let opacity = child.opacity();
                    frame.move_(&child, 0.0, 0.0);
                    child.set_opacity(1.0);
                    opacity
                })
                .unwrap_or(1.0);
            let y = old_layout.height
                - INNER_MARGIN
                - old_layout.card_height
                - index as i32 * (old_layout.card_height + GAP);
            self.leaving
                .put(&frame, f64::from(INNER_MARGIN), f64::from(y));
            let layer = self.leaving.clone();
            let moving = frame.clone();
            let completed_layer = self.leaving.clone();
            let completed_frame = frame.clone();
            let window = self.window.clone();
            let root = self.root.clone();
            let motion = MotionToken::default();
            motion.run(
                &self.leaving.clone().upcast(),
                TOAST_EXIT_MS,
                move |progress| {
                    let (offset, alpha) = toast_exit_from_opacity(progress, start_alpha);
                    layer.move_(&moving, f64::from(INNER_MARGIN), f64::from(y) + offset);
                    moving.set_opacity(alpha);
                },
                move || {
                    completed_layer.remove(&completed_frame);
                    if completed_layer.first_child().is_none() && root.opacity() == 0.0 {
                        window.set_opacity(0.0);
                    }
                },
            );
            self.active_motion.borrow_mut().push(motion);
        }

        if cards.is_empty() {
            return;
        }

        let spacer = gtk::Box::new(gtk::Orientation::Vertical, 0);
        spacer.set_vexpand(true);
        self.root.append(&spacer);
        // The queue is oldest first. GTK children are top to bottom.
        for notice in cards.iter().rev() {
            let content = card(notice, layout.card_height, on_intent.clone());
            let frame = gtk::Fixed::new();
            frame.set_hexpand(true);
            frame.set_overflow(gtk::Overflow::Hidden);
            frame.set_size_request(layout.width - INNER_MARGIN * 2, layout.card_height);
            content.set_size_request(layout.width - INNER_MARGIN * 2, layout.card_height);
            frame.put(&content, 0.0, 0.0);
            self.root.append(&frame);
            self.visible_frames
                .borrow_mut()
                .insert(notice.id, frame.clone());
            // Existing IDs do not replay their entrance when a different
            // notification arrives or a slow application replaces its text.
            if !old_ids.contains(&notice.id) {
                let motion = MotionToken::default();
                self.pending_entrances.borrow_mut().push(CardEntrance {
                    id: notice.id,
                    frame,
                    card: content,
                    motion: motion.clone(),
                });
                self.active_motion.borrow_mut().push(motion);
            }
        }

        let wanted = self.revision.get();
        let revision = self.revision.clone();
        let ids = cards.iter().map(|notice| notice.id).collect::<Vec<_>>();
        let on_painted = self.on_painted.clone();
        let count = Rc::new(Cell::new(0u8));
        let root = self.root.clone();
        self.window.add_tick_callback(move |widget, _| {
            if revision.get() != wanted {
                return gtk::glib::ControlFlow::Break;
            }
            if !widget.is_mapped() || root.allocated_height() <= 0 {
                return gtk::glib::ControlFlow::Continue;
            }
            let next = count.get().saturating_add(1);
            count.set(next);
            if next < 2 {
                return gtk::glib::ControlFlow::Continue;
            }
            on_painted(ids.clone());
            gtk::glib::ControlFlow::Break
        });
    }

    pub fn reveal(&self, ids: &[u32]) -> bool {
        if !self.window.is_mapped()
            || self
                .current
                .borrow()
                .iter()
                .map(|view| view.id)
                .ne(ids.iter().copied())
        {
            return false;
        }
        if let Some(surface) = self.window.surface() {
            surface.set_input_region(Some(&toast_hit_region(self.layout.get(), ids.len())));
            self.painted_ids.replace(ids.iter().copied().collect());
            self.root.set_opacity(1.0);
            self.window.set_opacity(1.0);
            for entrance in self.pending_entrances.borrow_mut().drain(..) {
                // All input is already scoped to painted toast rectangles.
                // Move the *real* GTK card, never a fake ghost or timer.
                if !ids.contains(&entrance.id) {
                    continue;
                }
                let position = entrance.frame.clone();
                let card = entrance.card.clone();
                let completed_position = entrance.frame.clone();
                let completed_card = entrance.card.clone();
                entrance.motion.run(
                    &entrance.frame.clone().upcast(),
                    TOAST_ENTRY_MS,
                    move |progress| {
                        let (y, alpha) = toast_entry_frame(progress);
                        position.move_(&card, 0.0, y);
                        card.set_opacity(alpha);
                    },
                    move || {
                        completed_position.move_(&completed_card, 0.0, 0.0);
                        completed_card.set_opacity(1.0);
                    },
                );
            }
            true
        } else {
            false
        }
    }

    pub fn clear_input(&self) {
        if let Some(surface) = self.window.surface() {
            surface.set_input_region(Some(&Region::create()));
        }
    }

    pub fn hide(&self) {
        // A modal popup or lost monitor requires immediate, input-free
        // suppression, not a departing toast drawn on top of that popup.
        self.suppress_departure.set(true);
        self.set_cards(self.layout.get(), &[]);
        self.suppress_departure.set(false);
        for token in self.active_motion.borrow_mut().drain(..) {
            token.cancel();
        }
        while let Some(child) = self.leaving.first_child() {
            self.leaving.remove(&child);
        }
        self.root.set_opacity(0.0);
        self.window.set_opacity(0.0);
        self.clear_input();
    }

    pub fn is_renderable(&self) -> bool {
        self.window.is_mapped() && self.window.surface().is_some()
    }

    /// Only actually paint-acknowledged IDs may dispatch user actions.
    pub fn is_painted(&self, id: u32) -> bool {
        self.is_renderable() && self.painted_ids.borrow().contains(&id)
    }
}

/// Output identity is the live GDK monitor object, never a numeric
/// compositor index or a client-provided string. At most eight transparent
/// hosts are retained, one per recently used output; only the active host
/// may dispatch actions or paint acknowledgements.
pub const MAX_RETAINED_TOAST_OUTPUTS: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastHostSelection {
    Unchanged,
    Changed,
    Unavailable,
}

struct OutputToastHost {
    id: u64,
    monitor: gtk::gdk::Monitor,
    scale_factor: i32,
    host: NotificationHost,
}

/// Reallocation on an unchanged GDK monitor still changes the actual
/// painted contact region. A scale change must revoke old input and
/// frame acknowledgements even when logical width/height stay identical.
fn toast_geometry_changed(
    previous: ToastLayout,
    next: ToastLayout,
    previous_scale: i32,
    next_scale: i32,
) -> bool {
    previous != next || previous_scale != next_scale
}

/// Reusable multi-output GTK presentation controller. The trusted Shell
/// chooses one physical destination for each snapshot: no duplicated D-Bus
/// notification, painted lifetime or action across monitors. A rendered host
/// retains its GTK window across repeat selections. Disappeared monitors
/// are immediately closed with empty pointer regions.
pub struct NotificationHostSet {
    app: gtk::Application,
    hosts: RefCell<Vec<OutputToastHost>>,
    active: Rc<Cell<Option<u64>>>,
    next_id: Cell<u64>,
    on_intent: Rc<dyn Fn(NotificationIntent)>,
    on_painted: Rc<dyn Fn(Vec<u32>)>,
    on_ready: Rc<dyn Fn()>,
}

impl NotificationHostSet {
    pub fn new(
        app: &gtk::Application,
        monitor: &gtk::gdk::Monitor,
        layout: ToastLayout,
        on_intent: impl Fn(NotificationIntent) + 'static,
        on_painted: impl Fn(Vec<u32>) + 'static,
        on_ready: impl Fn() + 'static,
    ) -> Option<Self> {
        let set = Self {
            app: app.clone(),
            hosts: RefCell::new(Vec::new()),
            active: Rc::new(Cell::new(None)),
            next_id: Cell::new(0),
            on_intent: Rc::new(on_intent),
            on_painted: Rc::new(on_painted),
            on_ready: Rc::new(on_ready),
        };
        (set.select(monitor, layout) != ToastHostSelection::Unavailable).then_some(set)
    }

    pub fn select(&self, monitor: &gtk::gdk::Monitor, layout: ToastLayout) -> ToastHostSelection {
        let mut hosts = self.hosts.borrow_mut();
        if let Some(active) = self.active.get()
            && let Some(entry) = hosts
                .iter_mut()
                .find(|entry| entry.id == active && entry.monitor == *monitor)
        {
            let scale = monitor.scale_factor();
            if toast_geometry_changed(entry.host.layout.get(), layout, entry.scale_factor, scale) {
                // The compositor can change fractional/physical scale
                // without changing the monitor object. Invalidate input,
                // animations and old paint ACKs before Shell resets FIFO.
                entry.host.hide();
                entry.scale_factor = scale;
                return ToastHostSelection::Changed;
            }
            return ToastHostSelection::Unchanged;
        }
        // Drop the old output's cards and input region before the next one
        // can accept a pointer or acknowledge a frame.
        if let Some(active) = self.active.replace(None)
            && let Some(entry) = hosts.iter().find(|entry| entry.id == active)
        {
            entry.host.hide();
        }
        if let Some(entry) = hosts.iter().find(|entry| entry.monitor == *monitor) {
            self.active.set(Some(entry.id));
            return ToastHostSelection::Changed;
        }
        // Any output is eligible, but the number of live Wayland surfaces is
        // capped. Eviction always clears input before closing the window.
        if hosts.len() >= MAX_RETAINED_TOAST_OUTPUTS {
            let evicted = hosts.remove(0);
            evicted.host.hide();
            evicted.host.window.close();
        }
        let id = self.next_id.get().wrapping_add(1).max(1);
        self.next_id.set(id);
        let active = self.active.clone();
        let on_intent = self.on_intent.clone();
        let intent = move |action| {
            if active.get() == Some(id) {
                on_intent(action);
            }
        };
        let active = self.active.clone();
        let on_painted = self.on_painted.clone();
        let painted = move |ids| {
            if active.get() == Some(id) {
                on_painted(ids);
            }
        };
        let Some(host) = NotificationHost::new(&self.app, monitor, layout, intent, painted) else {
            return ToastHostSelection::Unavailable;
        };
        let active = self.active.clone();
        let on_ready = self.on_ready.clone();
        host.window.connect_map(move |_| {
            if active.get() == Some(id) {
                let callback = on_ready.clone();
                let active_at_dispatch = active.clone();
                // A stale map event must not reschedule an old output
                // after its host was already replaced or hot-unplugged.
                // Never re-enter Shell::sync while a host is borrowed.
                gtk::glib::idle_add_local_once(move || {
                    if active_at_dispatch.get() == Some(id) {
                        callback();
                    }
                });
            }
        });
        hosts.push(OutputToastHost {
            id,
            monitor: monitor.clone(),
            scale_factor: monitor.scale_factor(),
            host,
        });
        self.active.set(Some(id));
        ToastHostSelection::Changed
    }

    /// Remove orphan Wayland surfaces immediately on GDK hotplug. Do not
    /// preserve a detached input shape against a disconnected monitor.
    /// Returns whether the selected painted destination was invalidated.
    pub fn reconcile(&self, monitors: &[gtk::gdk::Monitor]) -> bool {
        let mut hosts = self.hosts.borrow_mut();
        let old_active = self.active.get();
        hosts.retain(|entry| {
            if monitors.contains(&entry.monitor) {
                true
            } else {
                entry.host.hide();
                entry.host.window.close();
                false
            }
        });
        let lost = old_active.is_some_and(|id| !hosts.iter().any(|entry| entry.id == id));
        if lost {
            self.active.set(None);
        }
        lost
    }

    fn with_active<T>(&self, f: impl FnOnce(&NotificationHost) -> T) -> Option<T> {
        let active = self.active.get()?;
        self.hosts
            .borrow()
            .iter()
            .find(|entry| entry.id == active)
            .map(|entry| f(&entry.host))
    }

    pub fn is_renderable(&self) -> bool {
        self.with_active(NotificationHost::is_renderable)
            .unwrap_or(false)
    }

    /// GDK object identity survives reorder of the monitor model.
    pub fn active_monitor(&self) -> Option<gtk::gdk::Monitor> {
        let id = self.active.get()?;
        self.hosts
            .borrow()
            .iter()
            .find(|host| host.id == id)
            .map(|host| host.monitor.clone())
    }

    pub fn is_painted(&self, id: u32) -> bool {
        self.with_active(|host| host.is_painted(id))
            .unwrap_or(false)
    }

    pub fn reveal(&self, ids: &[u32]) -> bool {
        self.with_active(|host| host.reveal(ids)).unwrap_or(false)
    }

    pub fn set_cards(&self, layout: ToastLayout, cards: &[NotificationView]) {
        self.with_active(|host| host.set_cards(layout, cards));
    }

    pub fn hide(&self) {
        self.with_active(NotificationHost::hide);
    }

    pub fn hide_all(&self) {
        let mut hosts = self.hosts.borrow_mut();
        self.active.set(None);
        for entry in hosts.iter_mut() {
            entry.host.hide();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notice(id: u32) -> NotificationView {
        NotificationView {
            id,
            app: "App".into(),
            summary: "Message".into(),
            body: "Details".into(),
            critical: false,
            actions: vec![("default".into(), "Open".into())],
        }
    }

    #[test]
    fn output_host_pool_is_bounded_and_switching_has_explicit_reset_states() {
        assert_eq!(MAX_RETAINED_TOAST_OUTPUTS, 8);
        assert_ne!(ToastHostSelection::Changed, ToastHostSelection::Unchanged);
        assert_ne!(ToastHostSelection::Unavailable, ToastHostSelection::Changed);
        // A host cannot be constructed from made-up logical geometry.
        assert!(toast_layout(239, 640, 640).is_none());
    }

    #[test]
    fn inverted_departure_uses_the_same_contact_path() {
        assert_eq!(toast_exit_frame(0.0), (0.0, 1.0));
        assert_eq!(toast_exit_frame(0.5), (3.0, 0.75));
        assert_eq!(toast_exit_frame(1.0), (12.0, 0.0));
        assert_eq!(toast_exit_frame(f64::NAN), (12.0, 0.0));
        let (offset, alpha) = toast_exit_from_opacity(0.0, 0.75);
        assert_eq!((offset, alpha), (3.0, 0.75));
        assert_eq!(toast_exit_from_opacity(1.0, 0.75), (12.0, 0.0));
    }

    #[test]
    fn only_painted_removed_ids_qualify_for_inverse_motion() {
        let previous = [notice(1), notice(2), notice(3)];
        let next = [notice(1), notice(3)];
        let painted = HashSet::from([1, 2]);
        assert_eq!(
            departing_toasts(&previous, &next, &painted, true),
            vec![(2, 1)]
        );
        assert!(departing_toasts(&previous, &next, &painted, false).is_empty());
        assert!(departing_toasts(&previous, &previous, &painted, true).is_empty());
        assert!(departing_toasts(&previous, &[], &HashSet::new(), true).is_empty());
    }

    #[test]
    fn entry_is_exact_source_power_two_out_and_settles() {
        assert_eq!(toast_entry_frame(0.0), (12.0, 0.0));
        assert_eq!(toast_entry_frame(0.5), (3.0, 0.75));
        assert_eq!(toast_entry_frame(1.0), (0.0, 1.0));
        assert_eq!(toast_entry_frame(1.5), (0.0, 1.0));
        assert_eq!(toast_entry_frame(f64::NAN), (0.0, 1.0));
    }

    #[test]
    fn native_geometry_obeys_half_output_and_no_tiny_click_regions() {
        for (width, height) in [(360, 640), (600, 480), (1920, 1080)] {
            let layout = toast_layout(width, height, height).unwrap();
            assert!(layout.height * 2 <= height);
            assert!(layout.card_height * 3 <= layout.height);
            assert!(toast_hit_region(layout, 3).num_rectangles() >= 1);
        }
        assert!(toast_layout(100, 480, 480).is_none());
        assert!(toast_layout(500, 480, 95).is_none());
    }

    #[test]
    fn input_is_only_the_actually_visible_cards() {
        let layout = toast_layout(900, 700, 700).unwrap();
        let one = toast_hit_region(layout, 1);
        assert!(one.contains_point(100, layout.height - 20));
        assert!(!one.contains_point(100, 20));
        assert!(!one.contains_point(1, layout.height - 20));
        assert!(!toast_hit_region(layout, 0).contains_point(100, layout.height - 20));
    }

    #[test]
    fn bounded_plain_text_model_rejects_untrusted_sizes() {
        assert!(bounded_notification(&notice(1)));
        let mut item = notice(0);
        assert!(!bounded_notification(&item));
        item.id = 1;
        item.body = "x".repeat(2049);
        assert!(!bounded_notification(&item));
        item.body = String::new();
        item.actions = vec![("k".into(), "v".into()); 9];
        assert!(!bounded_notification(&item));
    }
    #[test]
    fn output_plan_prefers_bar_monitor_not_first_gdk_item() {
        let outputs = [(1920, 1080), (800, 600), (360, 640)];
        let (index, layout) = toast_output_plan(&outputs, Some(2)).unwrap();
        assert_eq!(index, 2);
        assert_eq!(layout.width, 336);
        assert_eq!(toast_output_plan(&outputs, Some(1)).unwrap().0, 1);
        assert_eq!(toast_output_plan(&outputs, None).unwrap().0, 0);
    }

    #[test]
    fn preferred_and_retained_outputs_remain_stable_through_reorder_and_hotplug() {
        let outputs = [(120, 700), (800, 600), (1024, 768)];
        assert_eq!(
            toast_output_plan_stable(&outputs, Some(0), Some(2), &[])
                .unwrap()
                .0,
            2
        );
        assert_eq!(
            toast_output_plan_stable(&outputs, Some(1), Some(2), &[])
                .unwrap()
                .0,
            1
        );
        // The previous real monitor is still available, but GDK reordered
        // it to index zero. It must not hop to a different valid fallback.
        let shuffled = [(1024, 768), (120, 700), (800, 600)];
        assert_eq!(
            toast_output_plan_stable(&shuffled, Some(1), Some(0), &[])
                .unwrap()
                .0,
            0
        );
        assert_eq!(
            toast_output_plan_stable(&shuffled, Some(1), Some(0), &[true, false, false])
                .unwrap()
                .0,
            2
        );
        assert_eq!(
            toast_output_plan_stable(&shuffled, Some(1), Some(0), &[true, true, true]),
            None
        );
        assert_eq!(
            toast_output_plan_stable(&shuffled, Some(999), Some(888), &[])
                .unwrap()
                .0,
            0
        );
    }

    #[test]
    fn geometry_or_scale_updates_revoke_a_painted_output_without_monitor_replacement() {
        let base = toast_layout(1920, 1080, 1080).unwrap();
        let resized = toast_layout(800, 600, 600).unwrap();
        assert!(!toast_geometry_changed(base, base, 1, 1));
        assert!(toast_geometry_changed(base, resized, 1, 1));
        assert!(toast_geometry_changed(base, base, 1, 2));
        assert!(toast_geometry_changed(base, base, 2, 1));
        assert!(!toast_geometry_changed(resized, resized, 2, 2));
    }

    #[test]
    fn action_leases_revoke_buttons_on_every_snapshot_even_after_reselection() {
        let gate = ToastActionGate::default();
        let a = gate.renew();
        assert!(gate.accepts(a));
        let b = gate.renew();
        assert!(!gate.accepts(a));
        assert!(gate.accepts(b));
        let third = gate.renew();
        assert!(!gate.accepts(a));
        assert!(!gate.accepts(b));
        assert!(gate.accepts(third));
        // Wrapping a revision never reuses the immediately previous lease.
        gate.0.set(u64::MAX);
        let wrapped = gate.renew();
        assert_eq!(wrapped, 0);
        assert!(!gate.accepts(u64::MAX));
    }

    #[test]
    fn output_plan_routes_around_an_actual_pooled_popup() {
        let outputs = [(1920, 1080), (800, 600), (360, 640)];
        assert_eq!(
            toast_output_plan_unobstructed(&outputs, Some(1), &[false, true, false])
                .unwrap()
                .0,
            0
        );
        assert_eq!(
            toast_output_plan_unobstructed(&outputs, Some(0), &[true, true, false])
                .unwrap()
                .0,
            2
        );
        assert_eq!(
            toast_output_plan_unobstructed(&outputs, Some(0), &[true, true, true]),
            None
        );
        assert_eq!(
            toast_output_plan_unobstructed(&[(120, 500), (640, 480)], Some(0), &[true, false])
                .unwrap()
                .0,
            1
        );
        assert_eq!(
            toast_output_plan_unobstructed(&[(120, 500), (640, 480)], Some(1), &[true, false])
                .unwrap()
                .0,
            1
        );
    }

    #[test]
    fn unusable_or_disappeared_output_fails_over_without_fake_geometry() {
        let outputs = [(120, 600), (800, 600), (264, 640)];
        assert_eq!(toast_output_plan(&outputs, Some(0)).unwrap().0, 1);
        assert_eq!(toast_output_plan(&outputs, Some(5)).unwrap().0, 1);
        assert_eq!(toast_output_plan(&outputs, Some(2)).unwrap().1.width, 240);
        assert_eq!(toast_output_plan(&[(120, 600), (500, 100)], None), None);
        assert_eq!(toast_output_plan(&[], Some(0)), None);
    }
}
