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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Sort {
    Name,
    Pid,
    Cpu,
    Memory,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct SortState {
    field: Sort,
    descending: bool,
}

impl SortState {
    const fn new(field: Sort) -> Self {
        Self {
            field,
            descending: matches!(field, Sort::Cpu | Sort::Memory),
        }
    }

    fn clicked(self, field: Sort) -> Self {
        if self.field == field {
            Self {
                field,
                descending: !self.descending,
            }
        } else {
            Self::new(field)
        }
    }
}

/// Unknown readings stay at the bottom in either direction.
fn compare_known<T: PartialOrd>(a: Option<T>, b: Option<T>, descending: bool) -> Ordering {
    match (a, b) {
        (Some(a), Some(b)) => {
            let order = a.partial_cmp(&b).unwrap_or(Ordering::Equal);
            if descending { order.reverse() } else { order }
        }
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

fn compare_rows(a: &ProcessRow, b: &ProcessRow, sort: SortState) -> Ordering {
    let result = match sort.field {
        Sort::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        Sort::Pid => a.pid.cmp(&b.pid),
        Sort::Cpu => {
            return compare_known(a.cpu_percent, b.cpu_percent, sort.descending)
                .then_with(|| compare_known(a.rss_kib, b.rss_kib, true))
                .then(a.pid.cmp(&b.pid));
        }
        Sort::Memory => compare_known(a.rss_kib, b.rss_kib, sort.descending),
    };
    let result = if sort.descending {
        result.reverse()
    } else {
        result
    };
    result.then(a.pid.cmp(&b.pid))
}

pub struct ProcessPage {
    pub root: gtk::Box,
    rows: gtk::Box,
    scroll: gtk::ScrolledWindow,
    summary: gtk::Label,
    query: RefCell<String>,
    sort: Cell<SortState>,
    latest: RefCell<Option<ProcessSnapshot>>,
    error: RefCell<Option<String>>,
    headers: [(gtk::Button, gtk::Label); 4],
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

        // The column headings are the sort controls. Clicking the current
        // heading toggles direction; no second toolbar consumes vertical space.
        let headings = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        headings.add_css_class("kira-process-headings");
        let headers = [
            column_header("PROCESS", true, 0),
            column_header("PID", false, 6),
            column_header("CPU", false, 7),
            column_header("RAM", false, 8),
        ];
        for (button, _) in &headers {
            headings.append(button);
        }
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
            sort: Cell::new(SortState::new(Sort::Cpu)),
            latest: RefCell::new(None),
            error: RefCell::new(None),
            headers,
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
        for ((button, _), field) in
            page.headers
                .iter()
                .zip([Sort::Name, Sort::Pid, Sort::Cpu, Sort::Memory])
        {
            connect_sort(button, field, Rc::downgrade(&page));
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
        let sort = self.sort.get();
        for ((button, caption), (field, name)) in self.headers.iter().zip([
            (Sort::Name, "PROCESS"),
            (Sort::Pid, "PID"),
            (Sort::Cpu, "CPU"),
            (Sort::Memory, "RAM"),
        ]) {
            if sort.field == field {
                button.add_css_class("kira-process-header-active");
                caption.set_text(&format!(
                    "{name} {}",
                    if sort.descending { "↓" } else { "↑" }
                ));
                button.set_tooltip_text(Some(if sort.descending {
                    "Sorted descending; click to sort ascending"
                } else {
                    "Sorted ascending; click to sort descending"
                }));
            } else {
                button.remove_css_class("kira-process-header-active");
                caption.set_text(name);
                button.set_tooltip_text(Some("Click to sort by this column"));
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
        let sort = self.sort.get();
        rows.sort_by(|a, b| compare_rows(a, b, sort));
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

fn column_header(name: &str, flexible: bool, width: i32) -> (gtk::Button, gtk::Label) {
    let button = gtk::Button::new();
    button.add_css_class("kira-process-header");
    button.set_hexpand(flexible);
    let caption = text(name, "kira-process-heading", flexible, width);
    button.set_child(Some(&caption));
    (button, caption)
}

fn connect_sort(button: &gtk::Button, field: Sort, weak: Weak<ProcessPage>) {
    button.connect_clicked(move |_| {
        if let Some(page) = weak.upgrade() {
            page.sort.set(page.sort.get().clicked(field));
            page.scroll.vadjustment().set_value(0.0);
            page.render();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn process(pid: u32, name: &str, cpu: Option<f64>, rss: Option<u64>) -> ProcessRow {
        ProcessRow {
            pid,
            name: name.to_string(),
            cpu_percent: cpu,
            rss_kib: rss,
            state: 'S',
        }
    }

    #[test]
    fn clicking_current_header_reverses_direction() {
        let initial = SortState::new(Sort::Cpu);
        assert!(initial.descending);
        // New CPU/RAM selections default to descending, even after
        // switching columns; repeated clicks reverse the direction.
        assert!(SortState::new(Sort::Memory).descending);
        assert!(SortState::new(Sort::Name).clicked(Sort::Cpu).descending);
        assert!(SortState::new(Sort::Pid).clicked(Sort::Memory).descending);
        assert!(!initial.clicked(Sort::Cpu).descending);
        assert!(initial.clicked(Sort::Cpu).clicked(Sort::Cpu).descending);
        assert!(!initial.clicked(Sort::Name).descending);
        assert!(initial.clicked(Sort::Memory).descending);
        assert!(!initial.clicked(Sort::Pid).descending);
    }

    #[test]
    fn each_column_sorts_both_directions() {
        let mut rows = [
            process(3, "zebra", Some(20.0), Some(4000)),
            process(1, "alpha", Some(80.0), Some(1000)),
            process(2, "beta", Some(50.0), Some(2000)),
        ];
        for (field, descending, expected) in [
            (Sort::Name, false, [1, 2, 3]),
            (Sort::Name, true, [3, 2, 1]),
            (Sort::Pid, false, [1, 2, 3]),
            (Sort::Pid, true, [3, 2, 1]),
            (Sort::Cpu, false, [3, 2, 1]),
            (Sort::Cpu, true, [1, 2, 3]),
            (Sort::Memory, false, [1, 2, 3]),
            (Sort::Memory, true, [3, 2, 1]),
        ] {
            rows.sort_by(|a, b| compare_rows(a, b, SortState { field, descending }));
            assert_eq!(
                rows.iter().map(|row| row.pid).collect::<Vec<_>>(),
                expected.to_vec()
            );
        }
    }

    #[test]
    fn missing_metrics_are_last_in_both_directions() {
        let a = process(1, "alpha", None, None);
        let b = process(2, "beta", Some(1.0), Some(1024));
        for descending in [true, false] {
            for field in [Sort::Cpu, Sort::Memory] {
                assert_eq!(
                    compare_rows(&a, &b, SortState { field, descending }),
                    Ordering::Greater
                );
            }
        }
    }
}
