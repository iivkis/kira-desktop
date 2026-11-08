//! Reusable, compositor-agnostic GTK4 presentation components for Kira.
//! No Core IPC, Niri commands, filesystem mutations or process execution.
pub mod bar;
pub mod calendar;
pub mod calendar_model;
pub mod capacity;
pub mod clock_digits;
pub mod display_map;
pub mod displays;
pub mod flow_indicator;
pub mod launcher;
pub mod motion;
pub mod notification_host;
pub mod popup;
mod popup_clip;
mod popup_section;
pub mod process_table;
pub mod reflow;
pub mod sky_catalog;
pub mod sky_map;
pub mod sky_model;
pub mod system;
pub mod system_charts;
pub mod system_details;
pub mod system_health;
pub mod system_report;
pub mod tenet_scroll;
pub mod tray;
pub mod workspace;

pub use bar::{Bar, action_button};
pub use calendar::Calendar;
pub use calendar_model::{CivilDate, DayState, current_local_date, month_grid};
pub use capacity::{CapacityCards, CapacityReading, capacity_reading};
pub use display_map::{
    DisplayMap, MapIntent, MapRect, layout_rectangles, logical_size, snap_position,
};
pub use displays::{
    DisplayChoice, DisplayIntent, DisplayMode, DisplayOutput, DisplayPhase, DisplaySettings,
};
pub use flow_indicator::{FlowAxis, FlowIndicator, FlowRect, FlowStyle};
pub use launcher::{
    ApplicationEntry, Launcher, filter_applications, matching_apps_label, next_launcher_selection,
};
pub use notification_host::{
    NotificationHost, NotificationIntent, NotificationView, ToastLayout, bounded_notification,
    toast_hit_region, toast_layout,
};
pub use popup::{PopupHost, PopupKind};
pub use process_table::{
    ProcessSort, ProcessSortState, ProcessTable, filter_sort_processes,
    filter_sort_processes_with_state, next_process_limit, process_report,
    process_report_with_state,
};
pub use reflow::{ReflowOffset, ReflowRow, ReflowScope, ReflowSnapshot};
pub use sky_map::SkyMap;
pub use system::{
    GpuReading, ProcessItem, ProcessList, SystemMonitor, SystemOverview, overview_labels,
    top_processes,
};
pub use system_charts::{
    ActivityAxis, ActivityLane, ChartPoint, ChartSample, GpuPulseCoupler, PulseHistory,
    SystemCharts, activity_axis, measured_history_caption,
};
pub use system_details::{
    SystemDetails, cpu_temperature, gpu_temperature, gpu_utilization, load_label, power_label,
    uptime_label,
};
pub use system_health::{
    CoreStatus, HealthMetrics, HealthPhase, HealthSummary, SystemHealth, classify_health,
};
pub use system_report::{
    OVERVIEW_REPORT_FRESHNESS, SystemReport, format_overview_report, overview_snapshot_fresh,
};
pub use tenet_scroll::TenetScroll;
pub use tray::{
    MenuNavigation, TrayBarModel, TrayIntent, TrayItem, TrayMenu, TrayMenuEntry, TrayPixmap,
    TrayState, TrayStatus, TrayUi, menu_focus_target, tray_bar_model,
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
