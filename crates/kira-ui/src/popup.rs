//! Reusable one-transient native layer-shell popup host.
//! One retained GTK tree and one reversible TENET cursor per surface.
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use gtk::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use crate::bar::BAR_HEIGHT;
use crate::motion::{MotionToken, POPUP_MASTER_DURATION_MS, PopupTimeline, popup_trace_frame};
use crate::popup_clip::PopupClip;
use crate::popup_section::PopupSection;

const GAP: i32 = 12;

fn supports_detach(kind: PopupKind) -> bool {
    kind != PopupKind::Tray
}

fn detached_title(kind: PopupKind) -> &'static str {
    match kind {
        PopupKind::Launcher => "Kira · Запуск приложений",
        PopupKind::Calendar => "Kira · Календарь",
        PopupKind::System => "Kira · Система",
        PopupKind::Settings => "Kira · Настройки",
        PopupKind::Tray => "Kira · Трей",
    }
}

fn screen_geometry(monitor: Option<&gtk::gdk::Monitor>) -> (i32, i32) {
    // When the bar surface is not mapped, let the compositor choose an output
    // rather than falsely pinning the transient to monitor zero.
    monitor
        .map(|monitor| (monitor.geometry().width(), monitor.geometry().height()))
        .unwrap_or((1280, 800))
}

fn detached_size(kind: PopupKind, screen_width: i32, screen_height: i32) -> (i32, i32) {
    let (width, height) = match kind {
        PopupKind::Settings => (680, 720),
        PopupKind::System => (520, 700),
        PopupKind::Launcher => (520, 600),
        PopupKind::Calendar => (1040, 720),
        PopupKind::Tray => (420, 520),
    };
    (
        width.min((screen_width - GAP * 2).max(160)),
        height.min((screen_height - GAP * 2).max(160)),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PopupKind {
    Launcher,
    Calendar,
    System,
    Settings,
    Tray,
}

struct PopupView {
    window: gtk::ApplicationWindow,
    clip: PopupClip,
    seam: gtk::DrawingArea,
    sections: Vec<PopupSection>,
    contact: Rc<Cell<Option<(f64, f64)>>>,
    timeline: Rc<RefCell<PopupTimeline>>,
    motion: MotionToken,
    // Move one retained GTK clip tree between two distinct Wayland roles.
    detached: RefCell<Option<gtk::ApplicationWindow>>,
    detach_button: Option<gtk::Button>,
    scroll: gtk::ScrolledWindow,
    content: gtk::Widget,
    last_content_focus: RefCell<Option<gtk::Widget>>,
    on_detached: Option<Rc<dyn Fn(bool)>>,
}

impl PopupView {
    /// Remember focus only inside the actual widget content. Detach/attach
    /// controls must not erase the last focused application entry or day.
    fn remember_content_focus(&self, window: &gtk::ApplicationWindow) {
        if let Some(focused) = gtk::prelude::GtkWindowExt::focus(window)
            && focused.is_ancestor(&self.content)
        {
            self.last_content_focus.replace(Some(focused));
        }
    }

    /// Retain keyboard focus without trying to focus an unparented or hidden
    /// widget. The idle guard prevents stealing focus after a rapid switch.
    fn restore_content_focus(&self, window: &gtk::ApplicationWindow) {
        let Some(target) = self.last_content_focus.borrow().clone() else {
            return;
        };
        let weak = window.downgrade();
        gtk::glib::idle_add_local_once(move || {
            if let Some(window) = weak.upgrade()
                && window.is_visible()
                && window.is_active()
                && target.is_ancestor(&window)
                && target.is_sensitive()
                && target.is_visible()
            {
                target.grab_focus();
            }
        });
    }

    fn retarget_monitor(&self, kind: PopupKind, monitor: Option<&gtk::gdk::Monitor>) {
        if self.window.monitor() != monitor.cloned() {
            self.window.set_monitor(monitor);
        }
        let (width, height) = screen_geometry(monitor);
        let (popup_width, top, max_height) = layout_for(kind, width, height);
        self.window.set_margin(Edge::Top, top);
        self.window.set_default_size(popup_width, -1);
        self.scroll.set_max_content_height(max_height);
        self.contact.set(None);
    }

    fn render(&self) {
        let frame = self.timeline.borrow().frame(1);
        self.clip.set_fraction(frame.clip);
        for section in &self.sections {
            section.set_progress(self.timeline.borrow().progress());
        }
        self.seam.queue_draw();
    }

    /// Always reverse the same master progress, even if dismissed during
    /// arrival or reopened during dismissal. The window does not resize.
    fn transition(&self, opening: bool) {
        let start = {
            let mut timeline = self.timeline.borrow_mut();
            if opening {
                timeline.open();
            } else {
                timeline.close();
            }
            timeline.progress()
        };
        if opening {
            if start <= 0.0 {
                self.contact.set(None);
            }
            self.window.present();
        }
        self.render();
        let remaining = if opening { 1.0 - start } else { start };
        let duration = (remaining * POPUP_MASTER_DURATION_MS).ceil() as u64;
        let previous = Rc::new(Cell::new(0.0));
        let timeline = self.timeline.clone();
        let weak_clip = self.clip.downgrade();
        let weak_seam = self.seam.downgrade();
        let sections = self.sections.clone();
        let completed_timeline = self.timeline.clone();
        let completed_clip = self.clip.downgrade();
        let completed_seam = self.seam.downgrade();
        let completed_sections = self.sections.clone();
        let completed_window = self.window.downgrade();
        let widget: gtk::Widget = self.clip.clone().upcast();
        self.motion.run(
            &widget,
            duration,
            move |cursor| {
                let previous_cursor = previous.replace(cursor);
                let elapsed = (cursor - previous_cursor).max(0.0) * duration as f64;
                let frame = {
                    let mut timeline = timeline.borrow_mut();
                    timeline.advance_ms(elapsed);
                    timeline.frame(1)
                };
                if let Some(clip) = weak_clip.upgrade() {
                    clip.set_fraction(frame.clip);
                }
                for section in &sections {
                    section.set_progress(timeline.borrow().progress());
                }
                if let Some(seam) = weak_seam.upgrade() {
                    seam.queue_draw();
                }
            },
            move || {
                let frame = {
                    let mut timeline = completed_timeline.borrow_mut();
                    timeline.settle();
                    timeline.frame(1)
                };
                if let Some(clip) = completed_clip.upgrade() {
                    clip.set_fraction(frame.clip);
                }
                for section in &completed_sections {
                    section.set_progress(completed_timeline.borrow().progress());
                }
                if let Some(seam) = completed_seam.upgrade() {
                    seam.queue_draw();
                }
                if !opening && let Some(window) = completed_window.upgrade() {
                    window.set_visible(false);
                }
            },
        );
    }

    fn close(&self) {
        self.transition(false);
    }

    /// Reparent the exact retained widget tree, without creating controllers.
    fn reattach(&self, reopen: bool) {
        let Some(detached) = self.detached.borrow_mut().take() else {
            return;
        };
        detached.set_child(None::<&gtk::Widget>);
        if let Some(callback) = &self.on_detached {
            callback(false);
        }
        detached.destroy();
        self.window.set_child(Some(&self.clip));
        if let Some(button) = &self.detach_button {
            button.set_label("Отделить");
            button.set_tooltip_text(Some("Перенести виджет в отдельное окно"));
            button.update_property(&[gtk::accessible::Property::Label(
                "Отделить виджет в отдельное окно",
            )]);
        }
        self.motion.cancel();
        {
            let mut timeline = self.timeline.borrow_mut();
            timeline.close();
            timeline.settle();
        }
        self.render();
        self.contact.set(None);
        self.window.set_visible(false);
        if reopen {
            self.transition(true);
            self.restore_content_focus(&self.window);
        }
    }

    fn detach(
        self: &Rc<Self>,
        app: &gtk::Application,
        kind: PopupKind,
        active: &Rc<Cell<Option<PopupKind>>>,
    ) {
        if self.detached.borrow().is_some() || !supports_detach(kind) {
            return;
        }
        // A layer-shell window itself cannot become an xdg-toplevel.
        // Settle the old reveal before changing the clip's GTK parent.
        self.motion.cancel();
        {
            let mut timeline = self.timeline.borrow_mut();
            timeline.open();
            timeline.settle();
        }
        self.render();
        self.clip.set_fraction(1.0);
        self.window.set_child(None::<&gtk::Widget>);
        self.window.set_visible(false);
        if active.get() == Some(kind) {
            active.set(None);
        }

        let detached = gtk::ApplicationWindow::new(app);
        detached.set_title(Some(detached_title(kind)));
        detached.set_decorated(true);
        detached.set_resizable(true);
        detached.add_css_class("kira-popup");
        detached.add_css_class("kira-detached");
        let monitor = self.window.monitor();
        let (screen_width, screen_height) = screen_geometry(monitor.as_ref());
        let (width, height) = detached_size(kind, screen_width, screen_height);
        detached.set_default_size(width, height);
        detached.set_child(Some(&self.clip));
        if let Some(callback) = &self.on_detached {
            callback(true);
        }
        if let Some(button) = &self.detach_button {
            button.set_label("Прикрепить");
            button.set_tooltip_text(Some("Вернуть виджет на панель"));
            button.update_property(&[gtk::accessible::Property::Label(
                "Прикрепить виджет к панели",
            )]);
        }

        let weak = Rc::downgrade(self);
        detached.connect_notify_local(Some("focus-widget"), move |window, _| {
            if let Some(view) = weak.upgrade() {
                view.remember_content_focus(window);
            }
        });
        let weak = Rc::downgrade(self);
        detached.connect_notify_local(Some("is-active"), move |window, _| {
            if window.is_active()
                && let Some(view) = weak.upgrade()
            {
                view.restore_content_focus(window);
            }
        });

        let weak = Rc::downgrade(self);
        detached.connect_close_request(move |_| {
            // A window-manager close retains the original controller for
            // the next Top Bar toggle. Nothing is reconstructed.
            if let Some(view) = weak.upgrade() {
                view.reattach(false);
            }
            gtk::glib::Propagation::Stop
        });
        let weak = Rc::downgrade(self);
        let key = gtk::EventControllerKey::new();
        key.connect_key_pressed(move |_, keyval, _, _| {
            if keyval == gtk::gdk::Key::Escape {
                if let Some(view) = weak.upgrade() {
                    view.reattach(false);
                }
                gtk::glib::Propagation::Stop
            } else {
                gtk::glib::Propagation::Proceed
            }
        });
        detached.add_controller(key);
        self.detached.replace(Some(detached.clone()));
        detached.present();
        self.restore_content_focus(&detached);
    }

    /// Switching to a different popup cannot leave two active layer-shell
    /// windows. State is kept, while the superseded visual scene is cancelled.
    fn abort_hidden(&self) {
        self.motion.cancel();
        {
            let mut timeline = self.timeline.borrow_mut();
            timeline.close();
            timeline.settle();
        }
        self.render();
        self.window.set_visible(false);
    }
}

pub struct PopupHost {
    detach_handlers: RefCell<HashMap<PopupKind, Rc<dyn Fn(bool)>>>,
    app: gtk::Application,
    bar: gtk::glib::WeakRef<gtk::ApplicationWindow>,
    windows: RefCell<HashMap<PopupKind, Rc<PopupView>>>,
    active: Rc<Cell<Option<PopupKind>>>,
}

fn layout_for(kind: PopupKind, width: i32, height: i32) -> (i32, i32, i32) {
    let top = match kind {
        PopupKind::Launcher => (height / 5).max(BAR_HEIGHT + GAP),
        _ => BAR_HEIGHT + GAP,
    };
    let nominal_width = if kind == PopupKind::Settings {
        600
    } else {
        420
    };
    let width = nominal_width.min((width - GAP * 2).max(160));
    let usable_height = (height - top - GAP).max(160);
    (width, top, usable_height)
}

/// Decorate, but never recreate, up to three original view sections.
/// The original widgets and their controllers survive opening, closing
/// and subsequent state updates. Siblings keep their original order.
fn stage_sections(view: &gtk::Widget) -> Vec<PopupSection> {
    let root = if view.has_css_class("kira-calendar") {
        // Stage the same three source sections inside the responsive rail,
        // not the enclosing FlowBox or its detached-only atlas sibling.
        view.first_child()
            .and_then(|flow| flow.first_child())
            .and_then(|child| child.first_child())
            .and_then(|rail| rail.downcast::<gtk::Box>().ok())
    } else {
        view.downcast_ref::<gtk::Box>().cloned()
    };
    let Some(root) = root else {
        return Vec::new();
    };
    let mut children = Vec::new();
    let mut current = root.first_child();
    while let Some(widget) = current {
        current = widget.next_sibling();
        children.push(widget);
    }
    for child in &children {
        root.remove(child);
    }
    let mut staged = Vec::new();
    for (index, child) in children.into_iter().enumerate() {
        if index < 3 {
            let section = PopupSection::new(&child, index);
            root.append(&section);
            staged.push(section);
        } else {
            root.append(&child);
        }
    }
    staged
}

/// Stable editorial contact: clock for Calendar, search for Launcher,
/// heading for other views. Never derive a new contact during a reversal.
fn contact_widget(root: &gtk::Widget) -> gtk::Widget {
    fn find_class(root: &gtk::Widget, class: &str) -> Option<gtk::Widget> {
        if root.has_css_class(class) {
            return Some(root.clone());
        }
        let mut current = root.first_child();
        while let Some(child) = current {
            current = child.next_sibling();
            if let Some(found) = find_class(&child, class) {
                return Some(found);
            }
        }
        None
    }
    ["calendar-clock-play", "launcher-contact", "section-title"]
        .into_iter()
        .find_map(|class| find_class(root, class))
        .unwrap_or_else(|| root.clone())
}

impl PopupHost {
    pub fn new(app: &gtk::Application, bar: &gtk::ApplicationWindow) -> Self {
        Self {
            detach_handlers: RefCell::new(HashMap::new()),
            app: app.clone(),
            bar: bar.downgrade(),
            windows: RefCell::new(HashMap::new()),
            active: Rc::new(Cell::new(None)),
        }
    }

    /// Optional lifecycle callback to show detached-only content without
    /// introducing shell policy into reusable GTK view constructors.
    pub fn set_detach_handler(&self, kind: PopupKind, callback: impl Fn(bool) + 'static) {
        self.detach_handlers.borrow_mut().insert(kind, Rc::new(callback));
    }

    fn bar_monitor(&self) -> Option<gtk::gdk::Monitor> {
        let bar = self.bar.upgrade()?;
        let surface = bar.surface()?;
        gtk::prelude::WidgetExt::display(&bar).monitor_at_surface(&surface)
    }

    pub fn is_transient_active(&self, kind: PopupKind) -> bool {
        self.active.get() == Some(kind)
    }

    /// Detaching a view does not pause its live model or pending Settings
    /// confirmation. Only the one shared transient counts for overlays.
    pub fn is_active(&self, kind: PopupKind) -> bool {
        self.is_transient_active(kind)
            || self.windows.borrow().get(&kind).is_some_and(|view| {
                view.detached
                    .borrow()
                    .as_ref()
                    .is_some_and(|window| window.is_visible())
            })
    }

    /// Hidden views retain their GTK state. Only one transient is visible;
    /// Escape and same-button toggles retrace the actual saved timeline.
    pub fn toggle(&self, kind: PopupKind, make_view: impl FnOnce() -> gtk::Widget) {
        if let Some(view) = self.windows.borrow().get(&kind)
            && let Some(window) = view.detached.borrow().as_ref()
        {
            // The existing detached window is raised, not duplicated.
            window.present();
            view.restore_content_focus(window);
            return;
        }
        let previous = self.active.replace(None);
        if previous == Some(kind) {
            if let Some(view) = self.windows.borrow().get(&kind) {
                view.close();
            }
            return;
        }
        // Any already-closing surface is also cancelled before a different
        // surface arrives. It remains owned and can be reopened in place.
        for (other, view) in self.windows.borrow().iter() {
            if *other != kind && view.window.is_visible() {
                view.abort_hidden();
            }
        }
        if !self.windows.borrow().contains_key(&kind) {
            let window = gtk::ApplicationWindow::new(&self.app);
            window.set_title(Some("Kira Desktop"));
            window.set_decorated(false);
            window.set_resizable(false);
            window.add_css_class("kira-popup");
            window.init_layer_shell();
            window.set_namespace(Some("kira-desktop-popup"));
            window.set_layer(Layer::Top);
            window.set_anchor(Edge::Top, true);
            window.set_exclusive_zone(0);
            window.set_keyboard_mode(KeyboardMode::OnDemand);

            let monitor = self.bar_monitor();
            window.set_monitor(monitor.as_ref());
            let (screen_width, screen_height) = screen_geometry(monitor.as_ref());
            let (width, top, max_height) = layout_for(kind, screen_width, screen_height);
            window.set_margin(Edge::Top, top);
            if kind == PopupKind::Launcher {
                window.set_anchor(Edge::Left, false);
                window.set_anchor(Edge::Right, false);
            } else {
                window.set_anchor(Edge::Right, true);
                window.set_margin(Edge::Right, GAP);
            }
            window.set_default_size(width, -1);

            let frame = gtk::Box::new(gtk::Orientation::Vertical, 14);
            frame.set_margin_top(18);
            frame.set_margin_bottom(18);
            frame.set_margin_start(18);
            frame.set_margin_end(18);
            let detach_button = if supports_detach(kind) {
                let button = gtk::Button::with_label("Отделить");
                button.add_css_class("popup-window-control");
                button.set_halign(gtk::Align::End);
                button.set_tooltip_text(Some("Перенести виджет в отдельное окно"));
                button.update_property(&[gtk::accessible::Property::Label(
                    "Отделить виджет в отдельное окно",
                )]);
                frame.append(&button);
                Some(button)
            } else {
                None
            };
            let view_widget = make_view();
            let sections = stage_sections(&view_widget);
            let contact_widget = contact_widget(&view_widget);
            frame.append(&view_widget);
            let scroll = gtk::ScrolledWindow::new();
            scroll.add_css_class("kira-popup-surface");
            scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
            scroll.set_max_content_height(max_height);
            scroll.set_propagate_natural_height(true);
            scroll.set_child(Some(&frame));

            let timeline = Rc::new(RefCell::new(PopupTimeline::default()));
            let overlay = gtk::Overlay::new();
            overlay.set_child(Some(&scroll));
            let seam = gtk::DrawingArea::new();
            seam.set_can_target(false);
            seam.set_halign(gtk::Align::Fill);
            seam.set_valign(gtk::Align::Fill);
            let contact = Rc::new(Cell::new(None::<(f64, f64)>));
            seam.set_draw_func({
                let timeline = timeline.clone();
                let contact = contact.clone();
                let target = contact_widget.downgrade();
                move |area, cr, width, height| {
                    if width <= 0 || height <= 0 {
                        return;
                    }
                    let frame = timeline.borrow().frame(1);
                    if frame.seam_opacity > 0.0 {
                        let y = frame.seam * f64::from((height - 2).max(0));
                        cr.set_source_rgba(0.988, 0.639, 0.067, frame.seam_opacity);
                        cr.rectangle(0.0, y, f64::from(width), 1.0);
                        let _ = cr.fill();
                    }
                    let origin = contact.get().or_else(|| {
                        target.upgrade().and_then(|widget| {
                            widget.compute_bounds(area).and_then(|bounds| {
                                if bounds.width() > 0.0 && bounds.height() > 0.0 {
                                    let point = (
                                        f64::from(bounds.x() + bounds.width() / 2.0),
                                        f64::from(bounds.y() + bounds.height() / 2.0),
                                    );
                                    contact.set(Some(point));
                                    Some(point)
                                } else {
                                    None
                                }
                            })
                        })
                    });
                    let Some((x, y)) = origin else {
                        return;
                    };
                    for index in 0..4 {
                        let trace = popup_trace_frame(timeline.borrow().progress(), 1, index);
                        if trace.opacity <= 0.0 {
                            continue;
                        }
                        let _ = cr.save();
                        cr.translate(x + trace.x, y + trace.y);
                        cr.rotate(trace.rotation.to_radians());
                        if index % 2 == 0 {
                            cr.set_source_rgba(0.988, 0.639, 0.067, trace.opacity);
                        } else {
                            cr.set_source_rgba(0.898, 0.898, 0.898, trace.opacity);
                        }
                        cr.rectangle(-6.0, -0.5, 12.0, 1.0);
                        let _ = cr.fill();
                        let _ = cr.restore();
                    }
                }
            });
            overlay.add_overlay(&seam);
            overlay.set_measure_overlay(&seam, false);
            let clip = PopupClip::new(&overlay.upcast());
            clip.set_direction(1);
            clip.set_fraction(0.0);
            window.set_child(Some(&clip));
            let view = Rc::new(PopupView {
                window: window.clone(),
                clip,
                seam,
                sections,
                contact,
                timeline,
                motion: MotionToken::default(),
                detached: RefCell::new(None),
                detach_button: detach_button.clone(),
                scroll,
                content: view_widget,
                last_content_focus: RefCell::new(None),
                on_detached: self.detach_handlers.borrow().get(&kind).cloned(),
            });

            if let Some(button) = detach_button {
                let weak = Rc::downgrade(&view);
                let active = self.active.clone();
                let app = self.app.clone();
                button.connect_clicked(move |_| {
                    if let Some(view) = weak.upgrade() {
                        let detached = view.detached.borrow().is_some();
                        if detached {
                            // Reattach to the hidden pool. If another popup
                            // is active, showing this one here would violate
                            // the single-transient contract. The bar opens it.
                            view.reattach(false);
                        } else {
                            view.detach(&app, kind, &active);
                        }
                    }
                });
            }

            let weak = Rc::downgrade(&view);
            window.connect_notify_local(Some("focus-widget"), move |window, _| {
                if let Some(view) = weak.upgrade() {
                    view.remember_content_focus(window);
                }
            });
            let weak = Rc::downgrade(&view);
            window.connect_notify_local(Some("is-active"), move |window, _| {
                if window.is_active()
                    && let Some(view) = weak.upgrade()
                {
                    view.restore_content_focus(window);
                }
            });
            let active = self.active.clone();
            let weak = Rc::downgrade(&view);
            window.connect_close_request(move |_| {
                if let Some(view) = weak.upgrade() {
                    view.close();
                    active.set(None);
                }
                gtk::glib::Propagation::Stop
            });
            let active = self.active.clone();
            let weak = Rc::downgrade(&view);
            let key = gtk::EventControllerKey::new();
            key.connect_key_pressed(move |_, keyval, _, _| {
                if keyval == gtk::gdk::Key::Escape {
                    if let Some(view) = weak.upgrade() {
                        view.close();
                        active.set(None);
                    }
                    gtk::glib::Propagation::Stop
                } else {
                    gtk::glib::Propagation::Proceed
                }
            });
            window.add_controller(key);
            self.windows.borrow_mut().insert(kind, view);
        }
        if let Some(view) = self.windows.borrow().get(&kind) {
            let monitor = self.bar_monitor();
            view.retarget_monitor(kind, monitor.as_ref());
            self.active.set(Some(kind));
            view.transition(true);
            view.restore_content_focus(&view.window);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn popup_geometry_is_bounded() {
        assert_eq!(layout_for(PopupKind::Calendar, 1920, 1080), (420, 54, 1014));
        assert_eq!(layout_for(PopupKind::Settings, 500, 600).0, 476);
        assert!(layout_for(PopupKind::Launcher, 640, 480).2 > 0);
        assert_eq!(screen_geometry(None), (1280, 800));
    }

    #[test]
    fn detachable_native_windows_are_bounded_and_tray_stays_pooled() {
        for kind in [
            PopupKind::Launcher,
            PopupKind::Calendar,
            PopupKind::System,
            PopupKind::Settings,
        ] {
            assert!(supports_detach(kind));
            let (width, height) = detached_size(kind, 600, 480);
            assert!((160..=576).contains(&width));
            assert!((160..=456).contains(&height));
            assert!(detached_title(kind).starts_with("Kira"));
        }
        assert!(!supports_detach(PopupKind::Tray));
    }
}
