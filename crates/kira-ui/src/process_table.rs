//! GTK-only process explorer. Sampling and clipboard authority stay outside kira-ui.
use std::cell::RefCell;
use std::cmp::Ordering;
use std::rc::Rc;

use gtk::prelude::*;

use crate::system::{ProcessItem, ProcessList};

const MAX_VISIBLE_PROCESSES: usize = 80;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ProcessSort {
    #[default]
    Cpu,
    Name,
    Pid,
    Rss,
    Pss,
}

impl ProcessSort {
    fn from_selection(index: u32) -> Self {
        match index {
            1 => Self::Name,
            2 => Self::Pid,
            3 => Self::Rss,
            4 => Self::Pss,
            _ => Self::Cpu,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Cpu => "CPU (убыв.)",
            Self::Name => "Имя (возр.)",
            Self::Pid => "PID (возр.)",
            Self::Rss => "RSS (убыв.)",
            Self::Pss => "PSS (убыв.)",
        }
    }
}

fn known_f64_desc(a: Option<f64>, b: Option<f64>) -> Ordering {
    match (a.filter(|v| v.is_finite()), b.filter(|v| v.is_finite())) {
        (Some(a), Some(b)) => b.partial_cmp(&a).unwrap_or(Ordering::Equal),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

fn known_u64_desc(a: Option<u64>, b: Option<u64>) -> Ordering {
    match (a, b) {
        (Some(a), Some(b)) => b.cmp(&a),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

/// All tokens match the observed name or PID. Missing metrics sort last.
pub fn filter_sort_processes(
    rows: &[ProcessItem],
    query: &str,
    sort: ProcessSort,
    limit: usize,
) -> Vec<&ProcessItem> {
    let terms: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    let mut selected: Vec<_> = rows
        .iter()
        .filter(|item| {
            let name = item.name.to_lowercase();
            let pid = item.pid.to_string();
            terms
                .iter()
                .all(|term| name.contains(term) || pid.contains(term))
        })
        .collect();
    selected.sort_by(|a, b| {
        let order = match sort {
            ProcessSort::Cpu => known_f64_desc(a.cpu_percent, b.cpu_percent),
            ProcessSort::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            ProcessSort::Pid => a.pid.cmp(&b.pid),
            ProcessSort::Rss => known_u64_desc(a.rss_kib, b.rss_kib),
            ProcessSort::Pss => known_u64_desc(a.pss_kib, b.pss_kib),
        };
        order.then_with(|| a.pid.cmp(&b.pid))
    });
    selected.truncate(limit);
    selected
}

fn clean_text(text: &str) -> String {
    text.chars()
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .take(128)
        .collect()
}

fn cpu_text(value: Option<f64>) -> String {
    value
        .filter(|v| v.is_finite())
        .map_or_else(|| "—".into(), |v| format!("{v:.1}%"))
}

fn memory_text(value: Option<u64>) -> String {
    value.map_or_else(
        || "—".into(),
        |kib| format!("{:.1} MiB", kib as f64 / 1024.0),
    )
}

/// Explicit copy intent uses the filtered real snapshot, capped at 80 rows.
/// Missing PSS is never silently replaced with RSS.
pub fn process_report(snapshot: &ProcessList, query: &str, sort: ProcessSort) -> String {
    let found = filter_sort_processes(&snapshot.rows, query, sort, usize::MAX);
    let mut report = format!(
        "Процессы Kira (снимок)\nВсего обнаружено: {}\nПолучено строк: {}{}\nФильтр: {}\nСортировка: {}\nПоказано: {} из {}\nPID | Имя | CPU | RSS | PSS\n",
        snapshot.total,
        snapshot.rows.len(),
        if snapshot.truncated {
            " (сканирование ограничено)"
        } else {
            ""
        },
        clean_text(query),
        sort.label(),
        found.len().min(MAX_VISIBLE_PROCESSES),
        found.len()
    );
    for item in found.into_iter().take(MAX_VISIBLE_PROCESSES) {
        report.push_str(&format!(
            "{} | {} | {} | {} | {}\n",
            item.pid,
            clean_text(&item.name),
            cpu_text(item.cpu_percent),
            memory_text(item.rss_kib),
            memory_text(item.pss_kib)
        ));
    }
    report
}

type CopyReport = Box<dyn Fn(String) -> Result<(), String>>;

struct ProcessTableState {
    search: gtk::SearchEntry,
    sort: gtk::DropDown,
    summary: gtk::Label,
    rows: gtk::Box,
    feedback: gtk::Label,
    copy: gtk::Button,
    snapshot: RefCell<Option<ProcessList>>,
    error: RefCell<Option<String>>,
    received_at: RefCell<Option<String>>,
    on_copy: RefCell<Option<CopyReport>>,
}

impl ProcessTableState {
    fn render(&self) {
        while let Some(child) = self.rows.first_child() {
            self.rows.remove(&child);
        }
        self.copy
            .set_sensitive(self.snapshot.borrow().is_some() && self.on_copy.borrow().is_some());
        if let Some(error) = self.error.borrow().as_ref() {
            self.summary.set_text(error);
            return;
        }
        let snapshot = self.snapshot.borrow();
        let Some(snapshot) = snapshot.as_ref() else {
            self.summary
                .set_text("Список процессов появится после открытия вкладки");
            return;
        };
        let query = self.search.text();
        let sort = ProcessSort::from_selection(self.sort.selected());
        let found = filter_sort_processes(&snapshot.rows, query.as_str(), sort, usize::MAX);
        let shown = found.len().min(MAX_VISIBLE_PROCESSES);
        let truncated = if snapshot.truncated {
            " · сканирование ограничено"
        } else {
            ""
        };
        let receipt = self
            .received_at
            .borrow()
            .as_ref()
            .map_or_else(String::new, |time| format!(" · получено {time}"));
        self.summary.set_text(&format!(
            "Найдено: {} · показано: {} · просмотрено: {} из {}{}{}",
            found.len(),
            shown,
            snapshot.rows.len(),
            snapshot.total,
            truncated,
            receipt
        ));
        if found.is_empty() {
            let empty = gtk::Label::new(Some("Совпадений нет"));
            empty.add_css_class("muted");
            self.rows.append(&empty);
        }
        for item in found.into_iter().take(MAX_VISIBLE_PROCESSES) {
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            row.add_css_class("process-row");
            let name = gtk::Label::new(Some(&format!("{} ({})", item.name, item.pid)));
            name.set_xalign(0.0);
            name.set_hexpand(true);
            name.set_max_width_chars(24);
            name.set_tooltip_text(Some(&item.name));
            row.append(&name);
            for (heading, value) in [
                ("CPU", cpu_text(item.cpu_percent)),
                ("RSS", memory_text(item.rss_kib)),
                ("PSS", memory_text(item.pss_kib)),
            ] {
                let cell = gtk::Label::new(Some(&value));
                cell.set_xalign(1.0);
                cell.set_tooltip_text(Some(heading));
                cell.add_css_class("process-value");
                row.append(&cell);
            }
            self.rows.append(&row);
        }
    }

    fn copy_report(&self) {
        let snapshot = self.snapshot.borrow();
        let Some(snapshot) = snapshot.as_ref() else {
            self.feedback.set_text("Нет данных для отчёта");
            return;
        };
        let report = process_report(
            snapshot,
            self.search.text().as_str(),
            ProcessSort::from_selection(self.sort.selected()),
        );
        let handler = self.on_copy.borrow();
        let result = handler
            .as_ref()
            .map_or_else(|| Err("Копирование недоступно".into()), |copy| copy(report));
        self.feedback.set_text(match result {
            Ok(()) => "Отчёт скопирован",
            Err(ref error) => error,
        });
    }
}

/// Standalone view: plain snapshots and a typed copy intent, no Core or Niri.
pub struct ProcessTable {
    root: gtk::Box,
    state: Rc<ProcessTableState>,
}

impl ProcessTable {
    pub fn new() -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 8);
        root.add_css_class("process-table");
        let controls = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let search = gtk::SearchEntry::new();
        search.set_hexpand(true);
        search.set_placeholder_text(Some("Поиск по имени или PID"));
        controls.append(&search);
        let sort = gtk::DropDown::from_strings(&["CPU ↓", "Имя ↑", "PID ↑", "RSS ↓", "PSS ↓"]);
        sort.update_property(&[gtk::accessible::Property::Label("Сортировка процессов")]);
        controls.append(&sort);
        root.append(&controls);
        let summary = gtk::Label::new(Some("Список процессов появится после открытия вкладки"));
        summary.add_css_class("muted");
        summary.set_xalign(0.0);
        summary.set_wrap(true);
        root.append(&summary);
        let scroll = gtk::ScrolledWindow::new();
        scroll.set_min_content_height(180);
        scroll.set_max_content_height(360);
        scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        let rows = gtk::Box::new(gtk::Orientation::Vertical, 2);
        scroll.set_child(Some(&rows));
        root.append(&scroll);
        let footer = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let copy = gtk::Button::with_label("Скопировать отчёт");
        copy.set_sensitive(false);
        footer.append(&copy);
        let feedback = gtk::Label::new(None);
        feedback.add_css_class("muted");
        feedback.set_wrap(true);
        footer.append(&feedback);
        root.append(&footer);

        let state = Rc::new(ProcessTableState {
            search,
            sort,
            summary,
            rows,
            feedback,
            copy,
            snapshot: RefCell::new(None),
            error: RefCell::new(None),
            received_at: RefCell::new(None),
            on_copy: RefCell::new(None),
        });
        state.search.connect_search_changed({
            let weak = Rc::downgrade(&state);
            move |_| {
                if let Some(state) = weak.upgrade() {
                    state.render();
                }
            }
        });
        state.sort.connect_selected_notify({
            let weak = Rc::downgrade(&state);
            move |_| {
                if let Some(state) = weak.upgrade() {
                    state.render();
                }
            }
        });
        state.copy.connect_clicked({
            let weak = Rc::downgrade(&state);
            move |_| {
                if let Some(state) = weak.upgrade() {
                    state.copy_report();
                }
            }
        });
        Self { root, state }
    }

    pub fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }

    pub fn set_copy_handler(&self, on_copy: impl Fn(String) -> Result<(), String> + 'static) {
        self.state.on_copy.replace(Some(Box::new(on_copy)));
        self.state.render();
    }

    pub fn set_snapshot(&self, snapshot: &ProcessList) {
        self.state.snapshot.replace(Some(snapshot.clone()));
        self.state.error.replace(None);
        let received = gtk::glib::DateTime::now_local()
            .ok()
            .and_then(|time| time.format("%H:%M:%S").ok())
            .map(|time| time.to_string());
        self.state.received_at.replace(received);
        self.state.feedback.set_text("");
        self.state.render();
    }

    pub fn set_error(&self, error: &str) {
        self.state.snapshot.replace(None);
        self.state.error.replace(Some(error.to_owned()));
        self.state.received_at.replace(None);
        self.state.feedback.set_text("");
        self.state.render();
    }
}

impl Default for ProcessTable {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(
        pid: u32,
        name: &str,
        cpu: Option<f64>,
        rss: Option<u64>,
        pss: Option<u64>,
    ) -> ProcessItem {
        ProcessItem {
            pid,
            name: name.into(),
            cpu_percent: cpu,
            rss_kib: rss,
            pss_kib: pss,
        }
    }

    #[test]
    fn search_matches_all_tokens_across_name_and_pid() {
        let rows = [
            item(120, "Kira Worker", Some(5.0), None, None),
            item(201, "Kira Shell", Some(3.0), None, None),
            item(300, "Renderer", Some(1.0), None, None),
        ];
        let ids: Vec<_> = filter_sort_processes(&rows, "kIrA 20", ProcessSort::Cpu, 80)
            .iter()
            .map(|p| p.pid)
            .collect();
        assert_eq!(ids, [201]);
        assert!(filter_sort_processes(&rows, "no-such", ProcessSort::Pid, 80).is_empty());
        assert!(filter_sort_processes(&rows, "", ProcessSort::Cpu, 0).is_empty());
    }

    #[test]
    fn sort_respects_selected_metric_and_missing_values() {
        let rows = [
            item(8, "Zulu", None, Some(4096), None),
            item(5, "Alpha", Some(2.0), None, Some(123)),
            item(2, "Alpha", Some(f64::NAN), Some(8192), Some(600)),
        ];
        let ids = |sort| {
            filter_sort_processes(&rows, "", sort, 80)
                .iter()
                .map(|p| p.pid)
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(ProcessSort::Cpu), [5, 2, 8]);
        assert_eq!(ids(ProcessSort::Name), [2, 5, 8]);
        assert_eq!(ids(ProcessSort::Pid), [2, 5, 8]);
        assert_eq!(ids(ProcessSort::Rss), [2, 8, 5]);
        assert_eq!(ids(ProcessSort::Pss), [2, 5, 8]);
    }

    #[test]
    fn report_never_invents_missing_pss_and_escapes_record_breaks() {
        let snapshot = ProcessList {
            rows: vec![item(41, "host\nspoof", None, Some(2048), None)],
            total: 3,
            truncated: true,
        };
        let report = process_report(&snapshot, "host", ProcessSort::Pss);
        assert!(report.contains("Получено строк: 1 (сканирование ограничено)"));
        assert!(report.contains("41 | host spoof | — | 2.0 MiB | —"));
        assert!(!report.contains("host\nspoof"));
        assert!(!report.contains("0.0%"));
    }
}
