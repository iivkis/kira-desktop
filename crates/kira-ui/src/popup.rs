//! Reusable one-transient native layer-shell popup host.
//! One retained GTK tree and one reversible TENET cursor per surface.
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use gtk::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use crate::bar::BAR_HEIGHT;
use crate::motion::{MotionToken, POPUP_MASTER_DURATION_MS, PopupTimeline};
use crate::popup_clip::PopupClip;

const GAP: i32 = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PopupKind {
    Launcher,
    Calendar,
    System,
    Settings,
}

struct PopupView {
    window: gtk::ApplicationWindow,
    clip: PopupClip,
    seam: gtk::DrawingArea,
    timeline: Rc<RefCell<PopupTimeline>>,
    motion: MotionToken,
}

impl PopupView {
    fn render(&self) {
        let frame = self.timeline.borrow().frame(1);
        self.clip.set_fraction(frame.clip);
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
            self.window.present();
        }
        self.render();
        let remaining = if opening { 1.0 - start } else { start };
        let duration = (remaining * POPUP_MASTER_DURATION_MS).ceil() as u64;
        let previous = Rc::new(Cell::new(0.0));
        let timeline = self.timeline.clone();
        let weak_clip = self.clip.downgrade();
        let weak_seam = self.seam.downgrade();
        let completed_timeline = self.timeline.clone();
        let completed_clip = self.clip.downgrade();
        let completed_seam = self.seam.downgrade();
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
    app: gtk::Application,
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

impl PopupHost {
    pub fn new(app: &gtk::Application) -> Self {
        Self {
            app: app.clone(),
            windows: RefCell::new(HashMap::new()),
            active: Rc::new(Cell::new(None)),
        }
    }

    pub fn is_active(&self, kind: PopupKind) -> bool {
        self.active.get() == Some(kind)
    }

    /// Hidden views retain their GTK state. Only one transient is visible;
    /// Escape and same-button toggles retrace the actual saved timeline.
    pub fn toggle(&self, kind: PopupKind, make_view: impl FnOnce() -> gtk::Widget) {
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

            let (screen_width, screen_height) = gtk::gdk::Display::default()
                .and_then(|display| display.monitors().item(0))
                .and_then(|item| item.downcast::<gtk::gdk::Monitor>().ok())
                .map(|monitor| (monitor.geometry().width(), monitor.geometry().height()))
                .unwrap_or((1280, 800));
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
            frame.append(&make_view());
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
            seam.set_draw_func({
                let timeline = timeline.clone();
                move |_, cr, width, height| {
                    let frame = timeline.borrow().frame(1);
                    if frame.seam_opacity <= 0.0 {
                        return;
                    }
                    let y = frame.seam * f64::from((height - 2).max(0));
                    cr.set_source_rgba(0.988, 0.639, 0.067, frame.seam_opacity);
                    cr.rectangle(0.0, y, f64::from(width), 1.0);
                    let _ = cr.fill();
                }
            });
            overlay.add_overlay(&seam);
            let clip = PopupClip::new(&overlay.upcast());
            clip.set_fraction(0.0);
            window.set_child(Some(&clip));
            let view = Rc::new(PopupView {
                window: window.clone(),
                clip,
                seam,
                timeline,
                motion: MotionToken::default(),
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
            self.active.set(Some(kind));
            view.transition(true);
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
    }
}
