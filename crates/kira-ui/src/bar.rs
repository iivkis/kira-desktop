//! Reusable 42px Wayland bar composition. Layout follows main's left
//! launcher brand, workspace strip, optional title and right controls/clock.
use std::cell::Cell;

use gtk::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use crate::warnings::WarningInspector;
use crate::workspace::WorkspaceStrip;

pub const BAR_HEIGHT: i32 = 42;

pub struct Bar {
    pub window: gtk::ApplicationWindow,
    pub workspaces: WorkspaceStrip,
    pub warnings: WarningInspector,
    pub title: gtk::Label,
    pub clock: gtk::Label,
    brand: gtk::Button,
    clock_button: gtk::Button,
    actions: gtk::Box,
    compact: Cell<bool>,
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
        // On-demand focus allows the in-bar inspector to retain Tab/Escape
        // without claiming exclusive keyboard ownership over ordinary apps.
        window.set_keyboard_mode(KeyboardMode::OnDemand);
        window.set_anchor(Edge::Top, true);
        window.set_anchor(Edge::Left, true);
        window.set_anchor(Edge::Right, true);
        window.set_exclusive_zone(BAR_HEIGHT);

        let layout = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        layout.add_css_class("bar-layout");

        let brand = gtk::Button::with_label("KIRA");
        brand.set_width_request(96);
        brand.add_css_class("brand-button");
        brand.set_tooltip_text(Some("Открыть приложения"));
        brand.update_property(&[gtk::accessible::Property::Label("Открыть меню приложений")]);
        layout.append(&brand);

        let center = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        center.add_css_class("bar-center");
        center.set_hexpand(true);
        let workspaces = WorkspaceStrip::new(on_workspace);
        center.append(&workspaces.widget());
        let title = gtk::Label::new(Some("KIRA DESKTOP"));
        title.add_css_class("bar-title");
        title.set_visible(false);
        center.append(&title);
        layout.append(&center);

        let right = gtk::Box::new(gtk::Orientation::Horizontal, 5);
        right.add_css_class("bar-right");
        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        right.append(&actions);

        let clock_button = gtk::Button::new();
        clock_button.add_css_class("clock-button");
        clock_button.set_tooltip_text(Some("Открыть календарь"));
        clock_button.update_property(&[gtk::accessible::Property::Label("Открыть календарь")]);
        let clock = gtk::Label::new(Some("--:--"));
        clock.add_css_class("clock");
        clock_button.set_child(Some(&clock));
        right.append(&clock_button);
        layout.append(&right);

        let warnings =
            WarningInspector::new(&layout.clone().upcast(), &clock_button.clone().upcast());
        let overlay = gtk::Overlay::new();
        overlay.set_child(Some(&layout));
        overlay.add_overlay(&warnings.inspector_widget());
        window.set_child(Some(&overlay));
        Self {
            window,
            workspaces,
            warnings,
            title,
            clock,
            brand,
            clock_button,
            actions,
            compact: Cell::new(false),
        }
    }

    pub fn set_launcher_action(&self, on_click: impl Fn() + 'static) {
        self.brand.connect_clicked(move |_| on_click());
    }

    pub fn set_calendar_action(&self, on_click: impl Fn() + 'static) {
        self.clock_button.connect_clicked(move |_| on_click());
    }

    pub fn add_action(&self, symbol: &str, label: &str, on_click: impl Fn() + 'static) {
        self.actions.append(&action_button(symbol, label, on_click));
    }

    /// Add a reusable button/widget with its own model and status lifecycle.
    pub fn add_action_widget(&self, widget: &gtk::Widget) {
        self.actions.append(widget);
    }

    /// Place source order: Tray, SystemHealth, Warning, Settings.
    /// Kept separate from ordinary actions so the inspector is not first.
    pub fn add_warning_slot(&self) {
        self.actions.append(&self.warnings.button_widget());
    }

    pub fn set_compact(&self, compact: bool) {
        self.compact.set(compact);
        self.title
            .set_visible(!compact && self.title.text() != "KIRA DESKTOP");
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
