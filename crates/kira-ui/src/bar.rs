//! Reusable 42px Wayland bar surface and generic action buttons.
use gtk::prelude::*;
use gtk4_layer_shell::{Edge, Layer, LayerShell};

use crate::workspace::WorkspaceStrip;

pub const BAR_HEIGHT: i32 = 42;

pub struct Bar {
    pub window: gtk::ApplicationWindow,
    pub workspaces: WorkspaceStrip,
    pub title: gtk::Label,
    pub clock: gtk::Label,
    actions: gtk::Box,
}

impl Bar {
    pub fn new(app: &gtk::Application, on_workspace: impl Fn(String) + 'static) -> Self {
        let window = gtk::ApplicationWindow::new(app);
        window.set_title(Some("Kira Desktop"));
        window.set_decorated(false);
        window.set_default_size(800, BAR_HEIGHT);
        window.add_css_class("kira-bar");
        window.init_layer_shell();
        window.set_namespace(Some("kira-desktop"));
        window.set_layer(Layer::Top);
        window.set_anchor(Edge::Top, true);
        window.set_anchor(Edge::Left, true);
        window.set_anchor(Edge::Right, true);
        window.set_exclusive_zone(BAR_HEIGHT);

        let layout = gtk::CenterBox::new();
        let workspaces = WorkspaceStrip::new(on_workspace);
        workspaces.widget().set_margin_start(12);
        layout.set_start_widget(Some(&workspaces.widget()));

        let title = gtk::Label::new(Some("KIRA DESKTOP"));
        title.add_css_class("bar-title");
        layout.set_center_widget(Some(&title));

        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 3);
        actions.set_margin_end(12);
        let clock = gtk::Label::new(Some("--:--"));
        clock.add_css_class("clock");
        layout.set_end_widget(Some(&actions));
        window.set_child(Some(&layout));
        Self {
            window,
            workspaces,
            title,
            clock,
            actions,
        }
    }

    pub fn add_action(&self, symbol: &str, label: &str, on_click: impl Fn() + 'static) {
        self.actions.append(&action_button(symbol, label, on_click));
    }

    pub fn finish_actions(&self) {
        self.actions.append(&self.clock);
    }

    pub fn set_compact(&self, compact: bool) {
        if compact {
            self.window.add_css_class("compact");
        } else {
            self.window.remove_css_class("compact");
        }
    }
}

pub fn action_button(
    symbol: &str,
    description: &str,
    on_click: impl Fn() + 'static,
) -> gtk::Button {
    let button = gtk::Button::with_label(symbol);
    button.add_css_class("bar-action");
    button.set_tooltip_text(Some(description));
    button.update_property(&[gtk::accessible::Property::Label(description)]);
    button.connect_clicked(move |_| on_click());
    button
}
