//! Native read-only Processes tab. GTK widgets stay on the main loop;
//! /proc scanning and CPU accounting run only on a worker while tab is active.
use std::cell::{Cell, RefCell};
use std::cmp::Ordering;
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
use std::sync::{Arc, mpsc};
use std::time::Duration;

use gtk::prelude::*;

use crate::process_monitor::{ProcessRow, ProcessSampler, ProcessSnapshot};
use crate::system_metrics::bytes_label;

const VISIBLE_ROWS: usize = 36;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Sort {
    Cpu,
    Memory,
    Pid,
}

pub struct ProcessPage {
    pub root: gtk::Box,
    rows: gtk::Box,
    scroll: gtk::ScrolledWindow,
    summary: gtk::Label,
    query: RefCell<String>,
    sort: Cell<Sort>,
    latest: RefCell<Option<ProcessSnapshot>>,
    error: RefCell<Option<String>>,
    cpu_button: gtk::Button,
    memory_button: gtk::Button,
    pid_button: gtk::Button,
    running: Arc<AtomicBool>,
    active: Arc<AtomicBool>,
    receiver: RefCell<mpsc::Receiver<Result<ProcessSnapshot, String>>>,
}

impl ProcessPage {
    pub fn new() -> Rc<Self> {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 12);
        root.add_css_class("kira-process-page");
        let section = gtk::Label::new(Some("PROCESSES  /  HOST"));
        section.set_xalign(0.0);
        section.add_css_class("kira-system-section");
        root.append(&section);

        let filter = gtk::SearchEntry::new();
        filter.set_placeholder_text(Some("Filter by process name or PID"));
        filter.set_tooltip_text(Some("Search currently readable Linux processes"));
        filter.add_css_class("kira-process-search");
        root.append(&filter);

        let sort_controls = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        sort_controls.add_css_class("kira-process-controls");
        let caption = gtk::Label::new(Some("SORT"));
        caption.add_css_class("kira-process-caption");
        caption.set_hexpand(true);
        caption.set_xalign(0.0);
        sort_controls.append(&caption);
        let cpu_button = sort_button("CPU", "Sort by CPU usage, highest first");
        let memory_button = sort_button("RAM", "Sort by resident memory, highest first");
        let pid_button = sort_button("PID", "Sort by process identifier");
        for button in [&cpu_button, &memory_button, &pid_button] {
            sort_controls.append(button);
        }
        root.append(&sort_controls);

        let headings = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        headings.add_css_class("kira-process-headings");
        headings.append(&text("PROCESS", "kira-process-heading", true, 0));
        headings.append(&text("PID", "kira-process-heading", false, 6));
        headings.append(&text("CPU", "kira-process-heading", false, 7));
        headings.append(&text("RAM", "kira-process-heading", false, 8));
        root.append(&headings);

        let scroll = gtk::ScrolledWindow::new();
        scroll.add_css_class("kira-process-scroll");
        scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroll.set_min_content_height(260);
        scroll.set_max_content_height(336);
        scroll.set_propagate_natural_height(false);
        let rows = gtk::Box::new(gtk::Orientation::Vertical, 0);
        rows.add_css_class("kira-process-rows");
        scroll.set_child(Some(&rows));
        root.append(&scroll);
        let summary = gtk::Label::new(Some("WAITING FOR FIRST SAMPLE"));
        summary.set_xalign(0.0);
        summary.add_css_class("kira-process-summary");
        root.append(&summary);
        let hint = gtk::Label::new(Some("READ ONLY  /  CPU % OF ENTIRE HOST  /  RSS MEMORY"));
        hint.set_xalign(0.0);
        hint.set_wrap(true);
        hint.add_css_class("kira-process-hint");
        root.append(&hint);

        let (sender, receiver) = mpsc::channel();
        let active = Arc::new(AtomicBool::new(false));
        let running = Arc::new(AtomicBool::new(true));
        let thread_active = active.clone();
        let thread_running = running.clone();
        std::thread::spawn(move || {
            let mut sampler = ProcessSampler::default();
            let mut was_active = false;
            while thread_running.load(AtomicOrdering::Relaxed) {
                if thread_active.load(AtomicOrdering::Relaxed) {
                    if !was_active {
                        sampler = ProcessSampler::default();
                        was_active = true;
                    }
                    if sender
                        .send(sampler.sample().map_err(|e| e.to_string()))
                        .is_err()
                    {
                        break;
                    }
                } else {
                    was_active = false;
                }
                std::thread::sleep(Duration::from_secs(2));
            }
        });
        let page = Rc::new(Self {
            root,
            rows,
            scroll,
            summary,
            query: RefCell::new(String::new()),
            sort: Cell::new(Sort::Cpu),
            latest: RefCell::new(None),
            error: RefCell::new(None),
            cpu_button,
            memory_button,
            pid_button,
            running,
            active,
            receiver: RefCell::new(receiver),
        });
        page.update_sort_styles();
        page.show_empty("OPEN PROCESSES TO SAMPLE");

        let weak = Rc::downgrade(&page);
        filter.connect_search_changed(move |entry| {
            if let Some(page) = weak.upgrade() {
                *page.query.borrow_mut() = entry.text().to_lowercase();
                page.scroll.vadjustment().set_value(0.0);
                page.render();
            }
        });
        for (button, sort) in [
            (&page.cpu_button, Sort::Cpu),
            (&page.memory_button, Sort::Memory),
            (&page.pid_button, Sort::Pid),
        ] {
            connect_sort(button, sort, Rc::downgrade(&page));
        }
        page
    }

    pub fn set_active(&self, active: bool) {
        self.active.store(active, AtomicOrdering::Relaxed);
        if active && self.latest.borrow().is_none() {
            self.show_empty("SAMPLING / PLEASE WAIT");
        }
    }

    pub fn poll(&self) {
        if !self.active.load(AtomicOrdering::Relaxed) {
            return;
        }
        // Drain any queued snapshots, displaying only the latest sample.
        let mut latest = None;
        while let Ok(result) = self.receiver.borrow().try_recv() {
            latest = Some(result);
        }
        if let Some(result) = latest {
            match result {
                Ok(snapshot) => {
                    *self.latest.borrow_mut() = Some(snapshot);
                    *self.error.borrow_mut() = None;
                }
                Err(message) => {
                    *self.latest.borrow_mut() = None;
                    *self.error.borrow_mut() = Some(message);
                }
            }
            self.render();
        }
    }

    fn show_empty(&self, message: &str) {
        while let Some(child) = self.rows.first_child() {
            self.rows.remove(&child);
        }
        let empty = gtk::Label::new(Some(message));
        empty.add_css_class("kira-process-empty");
        empty.set_margin_top(35);
        self.rows.append(&empty);
    }

    fn update_sort_styles(&self) {
        for (button, sort) in [
            (&self.cpu_button, Sort::Cpu),
            (&self.memory_button, Sort::Memory),
            (&self.pid_button, Sort::Pid),
        ] {
            if self.sort.get() == sort {
                button.add_css_class("kira-process-sort-current");
            } else {
                button.remove_css_class("kira-process-sort-current");
            }
        }
    }

    fn render(&self) {
        self.update_sort_styles();
        if let Some(error) = self.error.borrow().as_ref() {
            self.summary.set_text("PROCESS DATA UNAVAILABLE");
            self.show_empty(&format!("CANNOT READ /proc: {error}"));
            return;
        }
        let borrowed = self.latest.borrow();
        let Some(snapshot) = borrowed.as_ref() else {
            self.summary.set_text("WAITING FOR FIRST SAMPLE");
            self.show_empty("SAMPLING / PLEASE WAIT");
            return;
        };
        // Replacing live rows must not throw the user back to the top of
        // the list every two seconds. Sorting/filtering deliberately resets
        // the viewport, while refreshes preserve the vertical offset.
        let adjustment = self.scroll.vadjustment();
        let previous_position = adjustment.value();
        let query = self.query.borrow();
        let mut rows: Vec<&ProcessRow> = snapshot
            .rows
            .iter()
            .filter(|row| {
                query.is_empty()
                    || row.name.to_lowercase().contains(query.as_str())
                    || row.pid.to_string().contains(query.as_str())
            })
            .collect();
        match self.sort.get() {
            Sort::Cpu => rows.sort_by(|a, b| {
                b.cpu_percent
                    .partial_cmp(&a.cpu_percent)
                    .unwrap_or(Ordering::Equal)
                    .then_with(|| b.rss_kib.cmp(&a.rss_kib))
                    .then(a.pid.cmp(&b.pid))
            }),
            Sort::Memory => {
                rows.sort_by(|a, b| b.rss_kib.cmp(&a.rss_kib).then_with(|| a.pid.cmp(&b.pid)))
            }
            Sort::Pid => rows.sort_by_key(|row| row.pid),
        }
        let matches = rows.len();
        while let Some(child) = self.rows.first_child() {
            self.rows.remove(&child);
        }
        for row in rows.into_iter().take(VISIBLE_ROWS) {
            let horizontal = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            horizontal.add_css_class("kira-process-row");
            let name = text(&row.name, "kira-process-name", true, 0);
            name.set_tooltip_text(Some(&format!(
                "{} / PID {} / STATE {}",
                row.name, row.pid, row.state
            )));
            horizontal.append(&name);
            horizontal.append(&text(&row.pid.to_string(), "kira-process-number", false, 6));
            horizontal.append(&text(
                &row.cpu_percent.map_or("—".into(), |p| format!("{p:.1}%")),
                "kira-process-number",
                false,
                7,
            ));
            horizontal.append(&text(
                &row.rss_kib.map_or("—".into(), bytes_label),
                "kira-process-number",
                false,
                8,
            ));
            self.rows.append(&horizontal);
        }
        if matches == 0 {
            self.show_empty("NO MATCHING PROCESSES");
        }
        self.summary.set_text(&format!(
            "SHOWING {} OF {} MATCHES  /  {} SCANNED{}",
            matches.min(VISIBLE_ROWS),
            matches,
            snapshot.total,
            if snapshot.truncated {
                "  /  LIMIT REACHED"
            } else {
                ""
            }
        ));
        if previous_position > 0.0 {
            gtk::glib::idle_add_local_once(move || {
                let max = (adjustment.upper() - adjustment.page_size()).max(0.0);
                adjustment.set_value(previous_position.min(max));
            });
        }
    }
}

impl Drop for ProcessPage {
    fn drop(&mut self) {
        self.running.store(false, AtomicOrdering::Relaxed);
    }
}

fn text(value: &str, class: &str, flexible: bool, width: i32) -> gtk::Label {
    let label = gtk::Label::new(Some(value));
    label.add_css_class(class);
    label.set_xalign(if flexible { 0.0 } else { 1.0 });
    if flexible {
        label.set_hexpand(true);
        label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    } else {
        label.set_width_chars(width);
    }
    label
}

fn sort_button(name: &str, tooltip: &str) -> gtk::Button {
    let button = gtk::Button::with_label(name);
    button.add_css_class("kira-process-sort");
    button.set_tooltip_text(Some(tooltip));
    button
}

fn connect_sort(button: &gtk::Button, sort: Sort, weak: Weak<ProcessPage>) {
    button.connect_clicked(move |_| {
        if let Some(page) = weak.upgrade() {
            page.sort.set(sort);
            page.scroll.vadjustment().set_value(0.0);
            page.render();
        }
    });
}
