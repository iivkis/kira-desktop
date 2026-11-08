//! GTK-only process explorer. Sampling and clipboard authority stay outside kira-ui.
use std::cell::{Cell, RefCell};
use std::cmp::Ordering;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk::prelude::*;

use crate::system::{ProcessItem, ProcessList};

const PROCESS_PAGE: usize = 36;
const MAX_VISIBLE_PROCESSES: usize = 180;

/// Source-compatible progressive disclosure, without allocating thousands
/// of GTK rows when /proc yields more than the visible budget.
pub fn next_process_limit(current: usize, available: usize) -> usize {
    current
        .saturating_add(PROCESS_PAGE)
        .min(MAX_VISIBLE_PROCESSES)
        .min(available)
}
const MAX_SNAPSHOT_AGE: Duration = Duration::from_secs(8);

/// A received snapshot expires after eight seconds without fresh data.
pub fn process_snapshot_fresh(received: Option<Instant>, now: Instant) -> bool {
    received.is_some_and(|time| now.saturating_duration_since(time) < MAX_SNAPSHOT_AGE)
}

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
    fn heading(self) -> &'static str {
        match self {
            Self::Name => "ПРОЦЕСС",
            Self::Pid => "PID",
            Self::Cpu => "CPU",
            Self::Rss => "RSS",
            Self::Pss => "PSS",
        }
    }

    fn default_descending(self) -> bool {
        matches!(self, Self::Cpu | Self::Rss | Self::Pss)
    }
}

/// Repeated clicks toggle the same column, switching columns restores that
/// metric's source default. Unknown measurements always remain last.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProcessSortState {
    pub field: ProcessSort,
    pub descending: bool,
}

impl Default for ProcessSortState {
    fn default() -> Self {
        Self {
            field: ProcessSort::Cpu,
            descending: true,
        }
    }
}

impl ProcessSortState {
    pub fn for_field(field: ProcessSort) -> Self {
        Self {
            field,
            descending: field.default_descending(),
        }
    }

    pub fn toggle(self, field: ProcessSort) -> Self {
        if self.field == field {
            Self {
                field,
                descending: !self.descending,
            }
        } else {
            Self::for_field(field)
        }
    }

    fn label(self) -> String {
        format!(
            "{} ({})",
            self.field.heading(),
            if self.descending {
                "убыв."
            } else {
                "возр."
            },
        )
    }
}

fn known_f64_order(a: Option<f64>, b: Option<f64>, descending: bool) -> Ordering {
    match (a.filter(|v| v.is_finite()), b.filter(|v| v.is_finite())) {
        (Some(a), Some(b)) => {
            let order = a.partial_cmp(&b).unwrap_or(Ordering::Equal);
            if descending { order.reverse() } else { order }
        }
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

fn known_u64_order(a: Option<u64>, b: Option<u64>, descending: bool) -> Ordering {
    match (a, b) {
        (Some(a), Some(b)) => {
            if descending {
                b.cmp(&a)
            } else {
                a.cmp(&b)
            }
        }
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

/// All tokens match the observed name or PID. Missing metrics sort last.
pub fn filter_sort_processes<'a>(
    rows: &'a [ProcessItem],
    query: &str,
    sort: ProcessSort,
    limit: usize,
) -> Vec<&'a ProcessItem> {
    filter_sort_processes_with_state(rows, query, ProcessSortState::for_field(sort), limit)
}

pub fn filter_sort_processes_with_state<'a>(
    rows: &'a [ProcessItem],
    query: &str,
    sort: ProcessSortState,
    limit: usize,
) -> Vec<&'a ProcessItem> {
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
        let order = match sort.field {
            ProcessSort::Cpu => known_f64_order(a.cpu_percent, b.cpu_percent, sort.descending),
            ProcessSort::Name => {
                let order = a.name.to_lowercase().cmp(&b.name.to_lowercase());
                if sort.descending {
                    order.reverse()
                } else {
                    order
                }
            }
            ProcessSort::Pid => {
                if sort.descending {
                    b.pid.cmp(&a.pid)
                } else {
                    a.pid.cmp(&b.pid)
                }
            }
            ProcessSort::Rss => known_u64_order(a.rss_kib, b.rss_kib, sort.descending),
            ProcessSort::Pss => known_u64_order(a.pss_kib, b.pss_kib, sort.descending),
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

/// Clipboard export is explicit and bounded independently of /proc size.
/// Missing PSS is never silently replaced by RSS or real zero.
pub fn process_report(snapshot: &ProcessList, query: &str, sort: ProcessSort) -> String {
    process_report_with_state(snapshot, query, ProcessSortState::for_field(sort))
}

pub fn process_report_with_state(
    snapshot: &ProcessList,
    query: &str,
    sort: ProcessSortState,
) -> String {
    let found = filter_sort_processes_with_state(&snapshot.rows, query, sort, usize::MAX);
    let shown = found.len().min(MAX_VISIBLE_PROCESSES);
    let mut report = format!(
        "Процессы Kira (снимок)\nВсего обнаружено: {}\nПолучено строк: {}{}\nФильтр: {}\nСортировка: {}\nПоказано: {} из {}\nPID | Имя | Состояние | CPU | RSS | PSS\n",
        snapshot.total,
        snapshot.rows.len(),
        if snapshot.truncated {
            " (сканирование ограничено)"
        } else {
            ""
        },
        clean_text(query),
        sort.label(),
        shown,
        found.len()
    );
    for item in found.into_iter().take(MAX_VISIBLE_PROCESSES) {
        report.push_str(&format!(
            "{} | {} | {} | {} | {} | {}\n",
            item.pid,
            clean_text(&item.name),
            if item.state.is_ascii_graphic() {
                item.state
            } else {
                '?'
            },
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
    sort: Cell<ProcessSortState>,
    sort_buttons: Vec<(ProcessSort, gtk::Button)>,
    visible_count: Cell<usize>,
    show_more: gtk::Button,
    summary: gtk::Label,
    rows: gtk::Box,
    feedback: gtk::Label,
    copy: gtk::Button,
    snapshot: RefCell<Option<ProcessList>>,
    error: RefCell<Option<String>>,
    received_at: RefCell<Option<String>>,
    received_instant: Cell<Option<Instant>>,
    revision: Cell<u64>,
    on_copy: RefCell<Option<CopyReport>>,
}

impl ProcessTableState {
    fn render(&self) {
        while let Some(child) = self.rows.first_child() {
            self.rows.remove(&child);
        }
        self.show_more.set_visible(false);
        for (field, button) in &self.sort_buttons {
            if *field == self.sort.get().field {
                button.add_css_class("selected");
                let arrow = if self.sort.get().descending {
                    " ↓"
                } else {
                    " ↑"
                };
                button.set_label(&format!("{}{}", field.heading(), arrow));
            } else {
                button.remove_css_class("selected");
                button.set_label(field.heading());
            }
        }
        self.copy.set_sensitive(
            self.snapshot.borrow().is_some()
                && process_snapshot_fresh(self.received_instant.get(), Instant::now())
                && self.on_copy.borrow().is_some(),
        );
        if let Some(error) = self.error.borrow().as_ref() {
            self.summary.set_text(error);
            return;
        }
        if self.snapshot.borrow().is_some()
            && !process_snapshot_fresh(self.received_instant.get(), Instant::now())
        {
            self.summary
                .set_text("Данные процессов устарели, ожидается новый снимок");
            return;
        }
        let snapshot = self.snapshot.borrow();
        let Some(snapshot) = snapshot.as_ref() else {
            self.summary
                .set_text("Список процессов появится после открытия вкладки");
            return;
        };
        let query = self.search.text();
        let found = filter_sort_processes_with_state(
            &snapshot.rows,
            query.as_str(),
            self.sort.get(),
            usize::MAX,
        );
        let shown = found.len().min(self.visible_count.get());
        self.show_more
            .set_visible(shown < found.len() && shown < MAX_VISIBLE_PROCESSES);
        self.show_more.set_sensitive(shown < MAX_VISIBLE_PROCESSES);
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
        for item in found.into_iter().take(shown) {
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 3);
            row.add_css_class("process-row");
            let name = gtk::Label::new(Some(&item.name));
            name.set_xalign(0.0);
            name.set_hexpand(true);
            name.set_ellipsize(gtk::pango::EllipsizeMode::End);
            name.set_tooltip_text(Some(&format!(
                "{} / PID {} / СОСТОЯНИЕ {}",
                item.name, item.pid, item.state
            )));
            row.append(&name);
            for (heading, value, width) in [
                ("PID", item.pid.to_string(), 46),
                ("CPU", cpu_text(item.cpu_percent), 54),
                ("RSS", memory_text(item.rss_kib), 70),
                ("PSS", memory_text(item.pss_kib), 70),
            ] {
                let cell = gtk::Label::new(Some(&value));
                cell.set_width_request(width);
                cell.set_xalign(1.0);
                cell.set_ellipsize(gtk::pango::EllipsizeMode::End);
                cell.set_tooltip_text(Some(heading));
                cell.add_css_class("process-value");
                row.append(&cell);
            }
            self.rows.append(&row);
        }
    }

    fn copy_report(&self) {
        if !process_snapshot_fresh(self.received_instant.get(), Instant::now()) {
            self.feedback
                .set_text("Данные устарели, отчёт не скопирован");
            self.render();
            return;
        }
        let snapshot = self.snapshot.borrow();
        let Some(snapshot) = snapshot.as_ref() else {
            self.feedback.set_text("Нет данных для отчёта");
            return;
        };
        let report =
            process_report_with_state(snapshot, self.search.text().as_str(), self.sort.get());
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
        let search = gtk::SearchEntry::new();
        search.set_hexpand(true);
        search.set_placeholder_text(Some("Фильтр по имени или PID"));
        search.update_property(&[gtk::accessible::Property::Label(
            "Фильтр по названию процесса или PID",
        )]);
        root.append(&search);
        let headers = gtk::Box::new(gtk::Orientation::Horizontal, 3);
        headers.add_css_class("process-sort-header");
        let mut sort_buttons = Vec::new();
        for field in [
            ProcessSort::Name,
            ProcessSort::Pid,
            ProcessSort::Cpu,
            ProcessSort::Rss,
            ProcessSort::Pss,
        ] {
            let button = gtk::Button::with_label(field.heading());
            button.add_css_class("process-sort-button");
            button.set_tooltip_text(Some(&format!("Сортировать по {}", field.heading())));
            button.update_property(&[gtk::accessible::Property::Label(&format!(
                "Сортировать по {}",
                field.heading()
            ))]);
            if field == ProcessSort::Name {
                button.set_hexpand(true);
                button.set_halign(gtk::Align::Fill);
            } else {
                button.set_width_request(match field {
                    ProcessSort::Pid => 46,
                    ProcessSort::Cpu => 54,
                    ProcessSort::Rss | ProcessSort::Pss => 70,
                    ProcessSort::Name => 0,
                });
            }
            headers.append(&button);
            sort_buttons.push((field, button));
        }
        root.append(&headers);
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
        let list = gtk::Box::new(gtk::Orientation::Vertical, 5);
        list.append(&rows);
        let show_more = gtk::Button::with_label("ПОКАЗАТЬ ЕЩЁ");
        show_more.add_css_class("process-show-more");
        show_more.set_visible(false);
        list.append(&show_more);
        scroll.set_child(Some(&list));
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
            sort: Cell::new(ProcessSortState::default()),
            sort_buttons,
            visible_count: Cell::new(PROCESS_PAGE),
            show_more,
            summary,
            rows,
            feedback,
            copy,
            snapshot: RefCell::new(None),
            error: RefCell::new(None),
            received_at: RefCell::new(None),
            received_instant: Cell::new(None),
            revision: Cell::new(0),
            on_copy: RefCell::new(None),
        });
        state.search.connect_search_changed({
            let weak = Rc::downgrade(&state);
            move |_| {
                if let Some(state) = weak.upgrade() {
                    state.visible_count.set(PROCESS_PAGE);
                    state.render();
                }
            }
        });
        for (field, button) in &state.sort_buttons {
            let weak = Rc::downgrade(&state);
            let field = *field;
            button.connect_clicked(move |_| {
                if let Some(state) = weak.upgrade() {
                    state.sort.set(state.sort.get().toggle(field));
                    state.visible_count.set(PROCESS_PAGE);
                    state.render();
                }
            });
        }
        state.show_more.connect_clicked({
            let weak = Rc::downgrade(&state);
            move |_| {
                if let Some(state) = weak.upgrade() {
                    state
                        .visible_count
                        .set(next_process_limit(state.visible_count.get(), usize::MAX));
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

    /// Leaving and reopening the processes page must not reuse stale rows.
    /// Search, sort direction and scroll are retained, sample identity is not.
    pub fn reset_for_tab(&self) {
        self.state
            .revision
            .set(self.state.revision.get().wrapping_add(1));
        self.state.snapshot.replace(None);
        self.state.received_instant.set(None);
        self.state.received_at.replace(None);
        self.state.error.replace(None);
        self.state.visible_count.set(PROCESS_PAGE);
        self.state.feedback.set_text("");
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
        self.state.received_instant.set(Some(Instant::now()));
        let revision = self.state.revision.get().wrapping_add(1);
        self.state.revision.set(revision);
        self.state.feedback.set_text("");
        self.state.render();
        // One bounded timeout per received snapshot; no background polling.
        // An old callback cannot expire a newer, freshly sampled process list.
        let weak = Rc::downgrade(&self.state);
        gtk::glib::timeout_add_local_once(MAX_SNAPSHOT_AGE, move || {
            if let Some(state) = weak.upgrade()
                && state.revision.get() == revision
            {
                state.snapshot.replace(None);
                state.received_instant.set(None);
                state.error.replace(Some(
                    "Данные процессов устарели, ожидается новый снимок".to_owned(),
                ));
                state.render();
            }
        });
    }

    pub fn set_error(&self, error: &str) {
        self.state
            .revision
            .set(self.state.revision.get().wrapping_add(1));
        self.state.received_instant.set(None);
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
            state: 'S',
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
        let ids: Vec<_> = filter_sort_processes(&rows, "kIrA 201", ProcessSort::Cpu, 80)
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
    fn freshness_expires_at_eight_seconds_and_never_invents_a_snapshot() {
        let now = Instant::now();
        assert!(!process_snapshot_fresh(None, now));
        assert!(process_snapshot_fresh(Some(now), now));
        assert!(process_snapshot_fresh(
            Some(now),
            now + Duration::from_millis(7999)
        ));
        assert!(!process_snapshot_fresh(Some(now), now + MAX_SNAPSHOT_AGE));
    }

    #[test]
    fn bidirectional_source_sort_and_progressive_limit_are_deterministic() {
        let rows = [
            item(3, "zebra", Some(20.0), Some(4000), Some(1200)),
            item(1, "alpha", Some(80.0), Some(1000), Some(800)),
            item(2, "beta", Some(50.0), Some(2000), Some(1300)),
            item(4, "missing", None, None, None),
        ];
        let ids = |sort: ProcessSortState| {
            filter_sort_processes_with_state(&rows, "", sort, 180)
                .into_iter()
                .map(|p| p.pid)
                .collect::<Vec<_>>()
        };
        for (field, ascending, descending) in [
            (ProcessSort::Name, vec![1, 2, 4, 3], vec![3, 4, 2, 1]),
            (ProcessSort::Pid, vec![1, 2, 3, 4], vec![4, 3, 2, 1]),
            (ProcessSort::Cpu, vec![3, 2, 1, 4], vec![1, 2, 3, 4]),
            (ProcessSort::Rss, vec![1, 2, 3, 4], vec![3, 2, 1, 4]),
            (ProcessSort::Pss, vec![1, 3, 2, 4], vec![2, 3, 1, 4]),
        ] {
            let current = ProcessSortState::for_field(field);
            assert_eq!(
                ids(ProcessSortState {
                    descending: false,
                    ..current
                }),
                ascending
            );
            assert_eq!(
                ids(ProcessSortState {
                    descending: true,
                    ..current
                }),
                descending
            );
        }
        assert_eq!(
            ProcessSortState::default().toggle(ProcessSort::Cpu),
            ProcessSortState {
                field: ProcessSort::Cpu,
                descending: false
            }
        );
        assert_eq!(
            ProcessSortState::default().toggle(ProcessSort::Rss),
            ProcessSortState {
                field: ProcessSort::Rss,
                descending: true
            }
        );
        assert_eq!(next_process_limit(0, 400), 36);
        assert_eq!(next_process_limit(36, 400), 72);
        assert_eq!(next_process_limit(144, 400), 180);
        assert_eq!(next_process_limit(180, 400), 180);
        assert_eq!(next_process_limit(36, 50), 50);
        assert_eq!(next_process_limit(usize::MAX, 400), 180);
    }

    #[test]
    fn report_includes_real_process_state_and_selected_sort_direction() {
        let mut snapshot = ProcessList {
            rows: (0..500)
                .map(|pid| item(pid, "worker", Some(f64::from(pid)), None, None))
                .collect(),
            total: 500,
            truncated: false,
        };
        snapshot.rows[0].state = 'Z';
        let report = process_report_with_state(
            &snapshot,
            "",
            ProcessSortState {
                field: ProcessSort::Pid,
                descending: false,
            },
        );
        assert!(report.contains("Показано: 180 из 500"));
        assert!(report.contains("PID | Имя | Состояние | CPU | RSS | PSS"));
        assert!(report.contains("0 | worker | Z"));
        assert!(!report.contains("499 | worker"));
        assert!(report.contains("Сортировка: PID (возр.)"));
        let back = process_report_with_state(
            &snapshot,
            "",
            ProcessSortState {
                field: ProcessSort::Pid,
                descending: true,
            },
        );
        assert!(back.contains("499 | worker"));
        assert!(!back.contains("0 | worker | Z"));
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
        assert!(report.contains("41 | host spoof | S | — | 2.0 MiB | —"));
        assert!(!report.contains("host\nspoof"));
        assert!(!report.contains("0.0%"));
    }
}
