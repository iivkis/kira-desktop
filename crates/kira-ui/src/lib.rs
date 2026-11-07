//! Reusable, compositor-agnostic GTK4 presentation components for Kira.
//! No Core IPC, Niri commands, filesystem mutations or process execution.
pub mod bar;
pub mod launcher;
pub mod popup;
pub mod workspace;

pub use bar::{Bar, action_button};
pub use launcher::{ApplicationEntry, Launcher, filter_applications};
pub use popup::{PopupHost, PopupKind};
pub use workspace::{WorkspaceItem, WorkspaceStrip, visible_workspaces};

use gtk::prelude::*;

pub fn install_style() {
    let provider = gtk::CssProvider::new();
    provider.load_from_data(include_str!("style.css"));
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

pub fn calendar_view() -> gtk::Widget {
    gtk::Calendar::new().upcast()
}

/// A reusable read-only information panel. Its text can be updated independently
/// from the window that currently presents it.
pub struct InfoPanel {
    root: gtk::Box,
    details: gtk::Label,
}

impl InfoPanel {
    pub fn new(heading: &str, initial: &str) -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 12);
        let title = gtk::Label::new(Some(heading));
        title.add_css_class("section-title");
        title.set_xalign(0.0);
        root.append(&title);
        let details = gtk::Label::new(Some(initial));
        details.set_xalign(0.0);
        details.set_wrap(true);
        root.append(&details);
        Self { root, details }
    }

    pub fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }

    pub fn set_details(&self, content: &str) {
        self.details.set_text(content);
    }
}
