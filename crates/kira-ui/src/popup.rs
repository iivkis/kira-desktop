//! Reusable one-transient native layer-shell popup host.
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use gtk::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use crate::bar::BAR_HEIGHT;

const GAP: i32 = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PopupKind {
    Launcher,
    Calendar,
    System,
    Settings,
}

pub struct PopupHost {
    app: gtk::Application,
    windows: RefCell<HashMap<PopupKind, gtk::ApplicationWindow>>,
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

    /// Hidden views retain their GTK state. There is never more than one visible popup.
    pub fn toggle(&self, kind: PopupKind, make_view: impl FnOnce() -> gtk::Widget) {
        if let Some(previous) = self.active.replace(None) {
            if let Some(window) = self.windows.borrow().get(&previous) {
                window.set_visible(false);
            }
            if previous == kind {
                return;
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
            scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
            scroll.set_max_content_height(max_height);
            scroll.set_propagate_natural_height(true);
            scroll.set_child(Some(&frame));
            window.set_child(Some(&scroll));

            let active = self.active.clone();
            window.connect_close_request(move |window| {
                window.set_visible(false);
                active.set(None);
                gtk::glib::Propagation::Stop
            });
            let active = self.active.clone();
            let escape_window = window.clone();
            let key = gtk::EventControllerKey::new();
            key.connect_key_pressed(move |_, keyval, _, _| {
                if keyval == gtk::gdk::Key::Escape {
                    escape_window.set_visible(false);
                    active.set(None);
                    gtk::glib::Propagation::Stop
                } else {
                    gtk::glib::Propagation::Proceed
                }
            });
            window.add_controller(key);
            self.windows.borrow_mut().insert(kind, window);
        }
        if let Some(window) = self.windows.borrow().get(&kind) {
            self.active.set(Some(kind));
            window.present();
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
