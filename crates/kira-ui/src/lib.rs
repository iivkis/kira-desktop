//! Reusable, compositor-agnostic GTK4 presentation components for Kira.
//! No Core IPC, Niri commands, filesystem mutations or process execution.
pub mod bar;
pub mod calendar;
pub mod calendar_model;
pub mod clock_digits;
pub mod displays;
pub mod launcher;
pub mod motion;
pub mod popup;
pub mod system;
pub mod workspace;

pub use bar::{Bar, action_button};
pub use calendar::Calendar;
pub use calendar_model::{CivilDate, DayState, current_local_date, month_grid};
pub use displays::{
    DisplayChoice, DisplayIntent, DisplayMode, DisplayOutput, DisplayPhase, DisplaySettings,
};
pub use launcher::{
    ApplicationEntry, Launcher, filter_applications, matching_apps_label, next_launcher_selection,
};
pub use popup::{PopupHost, PopupKind};
pub use system::{
    GpuReading, ProcessItem, ProcessList, SystemMonitor, SystemOverview, overview_labels,
    top_processes,
};
pub use workspace::{
    WorkspaceItem, WorkspaceStrip, WorkspaceWindow, visible_workspace_window, visible_workspaces,
};

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

/// A reusable settings composition without binding the UI crate to Core/Niri.
pub fn settings_page(summary: &gtk::Widget, displays: &gtk::Widget) -> gtk::Widget {
    let root = gtk::Box::new(gtk::Orientation::Vertical, 16);
    root.append(summary);
    let divider = gtk::Separator::new(gtk::Orientation::Horizontal);
    root.append(&divider);
    root.append(displays);
    root.upcast()
}
