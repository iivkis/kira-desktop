//! A read-only system dashboard under the Top Bar's health indicator.
//! Canonical native GTK styling; no arbitrary commands and no new Core RPC.
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Once;
use std::time::Duration;

use gtk::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use crate::system_metrics::{Snapshot, bytes_label, percent_used};

const TITLE: &str = "Kira System";

fn label(text: &str, class: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.set_xalign(0.0);
    label.add_css_class(class);
    label
}

struct Metric {
    value: gtk::Label,
    bar: gtk::ProgressBar,
}

fn metric(parent: &gtk::Box, heading: &str) -> Metric {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    row.add_css_class("kira-system-metric-heading");
    let name = label(heading, "kira-system-metric-name");
    name.set_hexpand(true);
    row.append(&name);
    let value = label("UNAVAILABLE", "kira-system-metric-value");
    value.set_xalign(1.0);
    row.append(&value);
    parent.append(&row);
    let bar = gtk::ProgressBar::new();
    bar.add_css_class("kira-system-meter");
    bar.set_show_text(false);
    bar.set_visible(false);
    parent.append(&bar);
    Metric { value, bar }
}

fn percentage(line: &Metric, percent: Option<u8>, detail: String) {
    line.value.set_text(&detail);
    line.bar.set_visible(percent.is_some());
    if let Some(value) = percent {
        line.bar.set_fraction(f64::from(value) / 100.0);
    }
}

pub fn disk() -> Option<(u64, u64)> {
    let file = gtk::gio::File::for_path("/");
    let info = file
        .query_filesystem_info(
            "filesystem::size,filesystem::free",
            None::<&gtk::gio::Cancellable>,
        )
        .ok()?;
    let total = info.attribute_uint64("filesystem::size") / 1024;
    let free = info.attribute_uint64("filesystem::free") / 1024;
    (total > 0 && free <= total).then_some((total, free))
}

fn uptime_label(seconds: Option<u64>) -> String {
    let Some(seconds) = seconds else {
        return "UNAVAILABLE".into();
    };
    let days = seconds / 86_400;
    let hours = (seconds % 86_400) / 3_600;
    let minutes = (seconds % 3_600) / 60;
    if days > 0 {
        format!("{days}d {hours}h {minutes}m")
    } else {
        format!("{hours}h {minutes}m")
    }
}

fn summary(
    core: bool,
    niri: bool,
    snapshot: &Snapshot,
    root: Option<(u64, u64)>,
) -> (&'static str, &'static str) {
    if !core {
        return ("CORE OFFLINE", "kira-system-error");
    }
    if !niri {
        return ("NIRI DISCONNECTED", "kira-system-error");
    }
    if let Some((total, available)) = root
        && total > 0
        && percent_used(total, available).is_some_and(|used| used >= 90)
    {
        return ("LOW STORAGE", "kira-system-warning");
    }
    if snapshot.memory_percent().is_some_and(|p| p >= 90) {
        return ("MEMORY PRESSURE", "kira-system-warning");
    }
    if snapshot.battery_percent.is_some_and(|b| b <= 15) && snapshot.battery_charging == Some(false)
    {
        return ("LOW BATTERY", "kira-system-warning");
    }
    if snapshot.cpu_percent.is_some_and(|p| p >= 95) {
        return ("SYSTEM BUSY", "kira-system-warning");
    }
    if snapshot.cpu_percent.is_none() || snapshot.memory_percent().is_none() {
        return ("MEASURING", "kira-system-pending");
    }
    if root.is_none() {
        return ("PARTIAL DATA", "kira-system-pending");
    }
    ("ALL SYSTEMS NOMINAL", "kira-system-ready")
}

struct Panel {
    state: gtk::Label,
    core: gtk::Label,
    niri: gtk::Label,
    cpu: Metric,
    memory: Metric,
    swap: Metric,
    storage: Metric,
    battery: gtk::Label,
    load: gtk::Label,
    uptime: gtk::Label,
    updated: gtk::Label,
}

impl Panel {
    fn refresh(&self, snapshot: &Snapshot, core: bool, niri: bool) -> String {
        let root = snapshot.root_total_kib.zip(snapshot.root_available_kib);
        let (headline, class) = summary(core, niri, snapshot, root);
        self.state.set_text(headline);
        for state in [
            "kira-system-ready",
            "kira-system-error",
            "kira-system-warning",
            "kira-system-pending",
        ] {
            self.state.remove_css_class(state);
        }
        self.state.add_css_class(class);
        self.core
            .set_text(if core { "CONNECTED" } else { "OFFLINE" });
        self.niri
            .set_text(if niri { "CONNECTED" } else { "DISCONNECTED" });

        let cpu = snapshot.cpu_percent;
        percentage(
            &self.cpu,
            cpu,
            cpu.map_or("MEASURING".into(), |v| format!("{v}%")),
        );
        let mem = snapshot.memory_total_kib.zip(snapshot.memory_available_kib);
        percentage(
            &self.memory,
            snapshot.memory_percent(),
            mem.map_or("UNAVAILABLE".into(), |(total, avail)| {
                format!(
                    "{} / {}",
                    bytes_label(total.saturating_sub(avail)),
                    bytes_label(total),
                )
            }),
        );
        let swap = snapshot.swap_total_kib.zip(snapshot.swap_free_kib);
        let swap_percent = swap.and_then(|(total, free)| percent_used(total, free));
        percentage(
            &self.swap,
            swap_percent,
            swap.map_or("UNAVAILABLE".into(), |(total, free)| {
                if total == 0 {
                    "NOT CONFIGURED".into()
                } else {
                    format!(
                        "{} / {}",
                        bytes_label(total.saturating_sub(free)),
                        bytes_label(total)
                    )
                }
            }),
        );
        let disk_percent = root.and_then(|(total, free)| percent_used(total, free));
        percentage(
            &self.storage,
            disk_percent,
            root.map_or("UNAVAILABLE".into(), |(total, free)| {
                format!(
                    "{} / {}",
                    bytes_label(total.saturating_sub(free)),
                    bytes_label(total)
                )
            }),
        );
        self.load
            .set_text(&snapshot.load.map_or("UNAVAILABLE".into(), |v| {
                format!("{:.2} / {:.2} / {:.2}", v[0], v[1], v[2])
            }));
        self.uptime.set_text(&uptime_label(snapshot.uptime_seconds));
        self.battery.set_text(
            &snapshot
                .battery_percent
                .map_or("NOT DETECTED".into(), |percent| {
                    let state = match snapshot.battery_charging {
                        Some(true) => " / AC OR CHARGING",
                        Some(false) => " / DISCHARGING",
                        None => "",
                    };
                    format!("{percent}%{state}")
                }),
        );
        if let Ok(now) = gtk::glib::DateTime::now_local() {
            self.updated.set_text(&format!(
                "LIVE / {}",
                now.format("%H:%M:%S")
                    .map(|v| v.to_string())
                    .unwrap_or_default()
            ));
        }
        format!(
            "KIRA SYSTEM / {headline}\nCore: {}\nNiri: {}\nCPU: {}\nMemory: {}\nSwap: {}\nStorage /: {}\nLoad 1/5/15: {}\nUptime: {}\nBattery: {}",
            self.core.text(),
            self.niri.text(),
            self.cpu.value.text(),
            self.memory.value.text(),
            self.swap.value.text(),
            self.storage.value.text(),
            self.load.text(),
            self.uptime.text(),
            self.battery.text(),
        )
    }
}

fn install_style() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| crate::native_ui::install_styles(include_str!("system_panel.css")));
}

pub fn dismiss(app: &gtk::Application) -> bool {
    crate::native_ui::dismiss_overlay(app, TITLE)
}

pub fn toggle(
    app: &gtk::Application,
    current: Rc<RefCell<Snapshot>>,
    core_connected: Rc<Cell<bool>>,
    niri_connected: Rc<Cell<bool>>,
) {
    if dismiss(app) {
        return;
    }
    crate::calendar::dismiss(app);
    crate::launcher::dismiss(app);
    install_style();

    let window = gtk::ApplicationWindow::new(app);
    window.set_title(Some(TITLE));
    window.set_decorated(false);
    window.set_default_size(408, 600);
    window.add_css_class("kira-system-window");
    window.init_layer_shell();
    window.set_namespace(Some("kira-system"));
    window.set_layer(Layer::Overlay);
    window.set_anchor(Edge::Top, true);
    window.set_anchor(Edge::Right, true);
    window.set_margin(Edge::Top, 50);
    window.set_margin(Edge::Right, 12);
    window.set_exclusive_zone(0);
    window.set_keyboard_mode(KeyboardMode::OnDemand);

    let scroll = gtk::ScrolledWindow::new();
    scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroll.set_propagate_natural_height(true);
    scroll.set_min_content_height(200);
    window.set_child(Some(&scroll));

    let card = gtk::Box::new(gtk::Orientation::Vertical, 0);
    card.add_css_class("kira-system-card");
    scroll.set_child(Some(&card));

    let hero = gtk::Box::new(gtk::Orientation::Vertical, 8);
    hero.add_css_class("kira-system-hero");
    let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let eyebrow = label("KIRA  /  SYSTEM MONITOR", "kira-system-eyebrow");
    eyebrow.set_hexpand(true);
    header.append(&eyebrow);
    let close = gtk::Button::with_label("×");
    close.add_css_class("kira-system-close");
    close.set_tooltip_text(Some("Close (Esc)"));
    let weak_window = window.downgrade();
    close.connect_clicked(move |_| {
        if let Some(window) = weak_window.upgrade() {
            window.close();
        }
    });
    header.append(&close);
    hero.append(&header);
    hero.append(&label("System overview", "kira-system-heading"));
    let state = label("MEASURING", "kira-system-state");
    hero.append(&state);
    card.append(&hero);

    // These are Kira's flat editorial tabs, not a themed GtkNotebook.
    // Each page owns its behavior and only the active Processes page scans
    // /proc. Overview remains the original system diagnostics surface.
    let navigation = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    navigation.add_css_class("kira-system-tabs");
    let overview_tab = gtk::Button::with_label("01  OVERVIEW");
    overview_tab.add_css_class("kira-system-tab");
    overview_tab.add_css_class("kira-system-tab-active");
    let processes_tab = gtk::Button::with_label("02  PROCESSES");
    processes_tab.add_css_class("kira-system-tab");
    navigation.append(&overview_tab);
    navigation.append(&processes_tab);
    card.append(&navigation);

    let stack = gtk::Stack::new();
    stack.set_transition_type(gtk::StackTransitionType::None);
    let overview = gtk::Box::new(gtk::Orientation::Vertical, 0);
    stack.add_named(&overview, Some("overview"));
    let process_page = crate::process_page::ProcessPage::new();
    stack.add_named(&process_page.root, Some("processes"));
    card.append(&stack);
    stack.set_visible_child_name("overview");

    let overview_weak = overview_tab.downgrade();
    let processes_weak = processes_tab.downgrade();
    let stack_overview = stack.clone();
    let page_overview = process_page.clone();
    overview_tab.connect_clicked(move |_| {
        stack_overview.set_visible_child_name("overview");
        page_overview.set_active(false);
        if let Some(button) = overview_weak.upgrade() {
            button.add_css_class("kira-system-tab-active");
        }
        if let Some(button) = processes_weak.upgrade() {
            button.remove_css_class("kira-system-tab-active");
        }
    });
    let overview_weak = overview_tab.downgrade();
    let processes_weak = processes_tab.downgrade();
    let stack_processes = stack.clone();
    let page_processes = process_page.clone();
    processes_tab.connect_clicked(move |_| {
        stack_processes.set_visible_child_name("processes");
        page_processes.set_active(true);
        if let Some(button) = overview_weak.upgrade() {
            button.remove_css_class("kira-system-tab-active");
        }
        if let Some(button) = processes_weak.upgrade() {
            button.add_css_class("kira-system-tab-active");
        }
    });

    let services = gtk::Box::new(gtk::Orientation::Vertical, 7);
    services.add_css_class("kira-system-services");
    services.append(&label("CONNECTIONS  /  REAL-TIME", "kira-system-eyebrow"));
    let core_label = label("", "kira-system-value");
    let niri_label = label("", "kira-system-value");
    for (name, value) in [("KIRA CORE", &core_label), ("NIRI IPC", &niri_label)] {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        row.append(&label(name, "kira-system-key"));
        value.set_xalign(1.0);
        value.set_hexpand(true);
        row.append(value);
        services.append(&row);
    }
    overview.append(&services);

    let workbench = gtk::Box::new(gtk::Orientation::Vertical, 10);
    workbench.add_css_class("kira-system-workbench");
    workbench.append(&label(
        "RESOURCES  /  LIVE TELEMETRY",
        "kira-system-section",
    ));
    let cpu = metric(&workbench, "CPU / ALL CORES");
    let memory = metric(&workbench, "MEMORY / RAM");
    let swap = metric(&workbench, "SWAP");
    let storage = metric(&workbench, "STORAGE / ROOT");
    overview.append(&workbench);

    let details = gtk::Box::new(gtk::Orientation::Vertical, 8);
    details.add_css_class("kira-system-details");
    details.append(&label(
        "OPERATING SYSTEM  /  CONTEXT",
        "kira-system-eyebrow",
    ));
    let uptime = label("", "kira-system-value");
    let load = label("", "kira-system-value");
    let battery = label("", "kira-system-value");
    for (name, value) in [
        ("UPTIME", &uptime),
        ("LOAD / 1 5 15", &load),
        ("POWER", &battery),
    ] {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let heading = label(name, "kira-system-key");
        heading.set_hexpand(true);
        row.append(&heading);
        value.set_xalign(1.0);
        row.append(value);
        details.append(&row);
    }
    let footer = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    footer.add_css_class("kira-system-footer");
    let updated = label("LIVE", "kira-system-updated");
    updated.set_hexpand(true);
    footer.append(&updated);
    let copy = gtk::Button::with_label("COPY REPORT");
    copy.add_css_class("kira-system-copy");
    footer.append(&copy);
    details.append(&footer);
    overview.append(&details);

    let panel = Rc::new(Panel {
        state,
        core: core_label,
        niri: niri_label,
        cpu,
        memory,
        swap,
        storage,
        battery,
        load,
        uptime,
        updated,
    });
    let report = Rc::new(RefCell::new(String::new()));
    *report.borrow_mut() = panel.refresh(
        &current.borrow(),
        core_connected.get(),
        niri_connected.get(),
    );
    let copy_report = report.clone();
    let copy_button = copy.clone();
    copy.connect_clicked(move |_| {
        if let Some(display) = gtk::gdk::Display::default() {
            display.clipboard().set_text(&copy_report.borrow());
            copy_button.set_label("COPIED");
        }
    });

    let weak_window = window.downgrade();
    // Retain the view-model while the popup is open. Only the weak window
    // reference is captured, so closing the popup stops this source and
    // releases the labels without a reference cycle.
    gtk::glib::timeout_add_local(Duration::from_secs(2), move || {
        if weak_window.upgrade().is_none() {
            return gtk::glib::ControlFlow::Break;
        }
        *report.borrow_mut() = panel.refresh(
            &current.borrow(),
            core_connected.get(),
            niri_connected.get(),
        );
        copy.set_label("COPY REPORT");
        gtk::glib::ControlFlow::Continue
    });

    // Keep process updates on the GTK main loop. Polling stops when the
    // window is destroyed, releasing the view-model and its scan worker.
    let weak_window = window.downgrade();
    gtk::glib::timeout_add_local(Duration::from_millis(250), move || {
        if weak_window.upgrade().is_none() {
            return gtk::glib::ControlFlow::Break;
        }
        process_page.poll();
        gtk::glib::ControlFlow::Continue
    });

    let keys = gtk::EventControllerKey::new();
    let weak_window = window.downgrade();
    keys.connect_key_pressed(move |_, key, _, _| {
        if key == gtk::gdk::Key::Escape {
            if let Some(window) = weak_window.upgrade() {
                window.close();
            }
            gtk::glib::Propagation::Stop
        } else {
            gtk::glib::Propagation::Proceed
        }
    });
    window.add_controller(keys);

    let was_active = Rc::new(Cell::new(false));
    window.connect_is_active_notify(move |window| {
        if window.is_active() {
            was_active.set(true);
        } else if was_active.get() {
            window.close();
        }
    });
    window.present();
}
