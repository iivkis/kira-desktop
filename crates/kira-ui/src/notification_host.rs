//! Reusable GTK4 notification presentation: one shaped, bottom-right Wayland
//! surface. No D-Bus, IPC, WebKit or access to external application commands.
//! A frame is acknowledged only after two real mapped GTK frame ticks.
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
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

struct CardEntrance {
    id: u32,
    frame: gtk::Fixed,
    card: gtk::Widget,
    motion: MotionToken,
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
    layout: Cell<ToastLayout>,
    current: RefCell<Vec<NotificationView>>,
    revision: Rc<Cell<u64>>,
    pending_entrances: RefCell<Vec<CardEntrance>>,
    active_motion: RefCell<Vec<MotionToken>>,
    on_intent: Rc<dyn Fn(NotificationIntent)>,
    on_painted: Rc<dyn Fn(Vec<u32>)>,
}

impl NotificationHost {
    pub fn new(
        app: &gtk::Application,
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
        window.set_child(Some(&root));
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
            layout: Cell::new(layout),
            current: RefCell::new(Vec::new()),
            revision: Rc::new(Cell::new(0)),
            pending_entrances: RefCell::new(Vec::new()),
            active_motion: RefCell::new(Vec::new()),
            on_intent: Rc::new(on_intent),
            on_painted: Rc::new(on_painted),
        })
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
        self.layout.set(layout);
        let old_ids: HashSet<_> = self.current.borrow().iter().map(|notice| notice.id).collect();
        for token in self.active_motion.borrow_mut().drain(..) {
            token.cancel();
        }
        self.pending_entrances.borrow_mut().clear();
        *self.current.borrow_mut() = cards.to_vec();
        self.window.set_opacity(0.0);
        self.clear_input();
        self.window.set_default_size(layout.width, layout.height);
        while let Some(child) = self.root.first_child() {
            self.root.remove(&child);
        }
        if cards.is_empty() {
            return;
        }

        let spacer = gtk::Box::new(gtk::Orientation::Vertical, 0);
        spacer.set_vexpand(true);
        self.root.append(&spacer);
        // The queue is oldest first. GTK children are top to bottom.
        for notice in cards.iter().rev() {
            let content = card(notice, layout.card_height, self.on_intent.clone());
            let frame = gtk::Fixed::new();
            frame.set_hexpand(true);
            frame.set_overflow(gtk::Overflow::Hidden);
            frame.set_size_request(layout.width - INNER_MARGIN * 2, layout.card_height);
            content.set_size_request(layout.width - INNER_MARGIN * 2, layout.card_height);
            frame.put(&content, 0.0, 0.0);
            self.root.append(&frame);
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
        self.set_cards(self.layout.get(), &[]);
    }

    pub fn is_renderable(&self) -> bool {
        self.window.is_mapped() && self.window.surface().is_some()
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
}
