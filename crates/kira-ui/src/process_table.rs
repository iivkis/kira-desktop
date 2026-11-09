//! GTK-only process explorer. Sampling and clipboard authority stay outside kira-ui.
use std::cell::{Cell, RefCell};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::rc::{Rc, Weak};
use std::time::{Duration, Instant};

use gtk::prelude::*;

use crate::flow_indicator::{FlowAxis, FlowIndicator, FlowStyle};
use crate::reflow::{REFLOW_MAX_ROWS, ReflowRow, ReflowScope};
use crate::system::{ProcessItem, ProcessList};
use crate::tenet_scroll::TenetScroll;

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
    // A malformed producer must not cause one GTK row to be parented
    // twice under the same PID. Stable sorted order decides the survivor.
    let mut seen_pids = HashSet::new();
    selected.retain(|item| seen_pids.insert(item.pid));
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

/// Only a small user-driven list reflows; a sensor pulse is never an
/// animation instruction and broad sorting must remain responsive.
pub fn animate_process_reflow(user_action: bool, visible_before: usize) -> bool {
    user_action && (1..=REFLOW_MAX_ROWS).contains(&visible_before)
}

/// A PID alone can be reused by Linux. Selection is explicitly tied to the
/// current snapshot's name as well, never used as a process-control authority.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessSelection {
    pub pid: u32,
    pub name: String,
}

pub fn process_selected<'a>(
    snapshot: &'a ProcessList,
    selection: &ProcessSelection,
) -> Option<&'a ProcessItem> {
    snapshot
        .rows
        .iter()
        .find(|item| item.pid == selection.pid && item.name == selection.name)
}

/// This is a *snapshot*, not a live process descriptor. Keep clipboard text
/// bounded and do not invent missing measurements or allow control chars.
pub fn selected_process_report(item: &ProcessItem) -> String {
    format!(
        "Процесс Kira (снимок)\nPID: {}\nИмя: {}\nСостояние: {}\nCPU: {}\nRSS: {}\nPSS: {}\n",
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
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessNavigation {
    Previous,
    Next,
    First,
    Last,
    PagePrevious,
    PageNext,
}

/// Page moves are bounded to the *rendered* list, never hidden /proc rows.
pub fn process_selection_after(current: usize, visible: usize, action: ProcessNavigation) -> usize {
    if visible == 0 {
        return 0;
    }
    let current = current.min(visible - 1);
    match action {
        ProcessNavigation::Previous => current.saturating_sub(1),
        ProcessNavigation::Next => current.saturating_add(1).min(visible - 1),
        ProcessNavigation::First => 0,
        ProcessNavigation::Last => visible - 1,
        ProcessNavigation::PagePrevious => current.saturating_sub(8),
        ProcessNavigation::PageNext => current.saturating_add(8).min(visible - 1),
    }
}

type CopyReport = Box<dyn Fn(String) -> Result<(), String>>;

/// The GTK instance survives /proc refreshes. ReflowRow owns exactly
/// one focusable process button; only its factual labels are updated.
#[derive(Clone)]
struct ProcessVisualRow {
    identity: ProcessSelection,
    wrapper: ReflowRow,
    button: gtk::Button,
    name: gtk::Label,
    pid: gtk::Label,
    cpu: gtk::Label,
    rss: gtk::Label,
    pss: gtk::Label,
}

fn numeric_cell(width: i32) -> gtk::Label {
    let label = gtk::Label::new(None);
    label.set_width_request(width);
    label.set_xalign(1.0);
    label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    label.add_css_class("process-value");
    label
}

impl ProcessVisualRow {
    fn new(item: &ProcessItem, weak: Weak<ProcessTableState>) -> Self {
        let button = gtk::Button::new();
        button.add_css_class("process-row");
        button.set_hexpand(true);
        button.set_halign(gtk::Align::Fill);
        let layout = gtk::Box::new(gtk::Orientation::Horizontal, 3);
        let name = gtk::Label::new(None);
        name.set_xalign(0.0);
        name.set_hexpand(true);
        name.set_ellipsize(gtk::pango::EllipsizeMode::End);
        layout.append(&name);
        let pid = numeric_cell(46);
        let cpu = numeric_cell(54);
        let rss = numeric_cell(70);
        let pss = numeric_cell(70);
        for label in [&pid, &cpu, &rss, &pss] {
            layout.append(label);
        }
        button.set_child(Some(&layout));
        let key = gtk::EventControllerKey::new();
        let key_weak = weak.clone();
        let process_id = item.pid;
        key.set_propagation_phase(gtk::PropagationPhase::Capture);
        key.connect_key_pressed(move |_, pressed, _, modifiers| {
            if modifiers.intersects(
                gtk::gdk::ModifierType::CONTROL_MASK
                    | gtk::gdk::ModifierType::ALT_MASK
                    | gtk::gdk::ModifierType::SUPER_MASK,
            ) {
                return gtk::glib::Propagation::Proceed;
            }
            let action = match pressed {
                gtk::gdk::Key::Up => ProcessNavigation::Previous,
                gtk::gdk::Key::Down => ProcessNavigation::Next,
                gtk::gdk::Key::Home => ProcessNavigation::First,
                gtk::gdk::Key::End => ProcessNavigation::Last,
                gtk::gdk::Key::Page_Up => ProcessNavigation::PagePrevious,
                gtk::gdk::Key::Page_Down => ProcessNavigation::PageNext,
                _ => return gtk::glib::Propagation::Proceed,
            };
            if let Some(state) = key_weak.upgrade()
                && let Some(index) = state
                    .visible_ids
                    .borrow()
                    .iter()
                    .position(|id| *id == process_id)
            {
                let next = process_selection_after(index, state.visible_ids.borrow().len(), action);
                state.select_index(next, true);
            }
            gtk::glib::Propagation::Stop
        });
        button.add_controller(key);
        button.connect_clicked(move |_| {
            if let Some(state) = weak.upgrade()
                && let Some(index) = state
                    .visible_ids
                    .borrow()
                    .iter()
                    .position(|id| *id == process_id)
            {
                state.select_index(index, false);
            }
        });
        let wrapper = ReflowRow::new(&button.clone().upcast());
        let row = Self {
            identity: ProcessSelection {
                pid: item.pid,
                name: item.name.clone(),
            },
            wrapper,
            button,
            name,
            pid,
            cpu,
            rss,
            pss,
        };
        row.update(item);
        row
    }

    fn update(&self, item: &ProcessItem) {
        let name = clean_text(&item.name);
        self.name.set_text(&name);
        self.pid.set_text(&item.pid.to_string());
        self.cpu.set_text(&cpu_text(item.cpu_percent));
        self.rss.set_text(&memory_text(item.rss_kib));
        self.pss.set_text(&memory_text(item.pss_kib));
        let description = format!(
            "{} · PID {} · состояние {} · CPU {} · RSS {} · PSS {}",
            name,
            item.pid,
            if item.state.is_ascii_graphic() {
                item.state
            } else {
                '?'
            },
            cpu_text(item.cpu_percent),
            memory_text(item.rss_kib),
            memory_text(item.pss_kib),
        );
        self.button.set_tooltip_text(Some(&description));
        self.button
            .update_property(&[gtk::accessible::Property::Label(&description)]);
    }
}

struct ProcessTableState {
    weak_self: Weak<ProcessTableState>,
    search: gtk::SearchEntry,
    clear: gtk::Button,
    sort: Cell<ProcessSortState>,
    sort_buttons: Vec<(ProcessSort, gtk::Button)>,
    sort_flow: FlowIndicator,
    visible_count: Cell<usize>,
    show_more: gtk::Button,
    summary: gtk::Label,
    rows: gtk::Box,
    visible_ids: RefCell<Vec<u32>>,
    cache: RefCell<HashMap<u32, ProcessVisualRow>>,
    reflow: ReflowScope,
    scroll: TenetScroll,
    selected: RefCell<Option<ProcessSelection>>,
    selected_info: gtk::Label,
    selected_copy: gtk::Button,
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
    fn fresh(&self) -> bool {
        process_snapshot_fresh(self.received_instant.get(), Instant::now())
    }

    fn clear_rows(&self) {
        self.reflow.cancel();
        self.scroll.cancel();
        self.visible_ids.borrow_mut().clear();
        self.cache.borrow_mut().clear();
        while let Some(child) = self.rows.first_child() {
            self.rows.remove(&child);
        }
        self.selected.replace(None);
        self.selected_info.set_visible(false);
        self.selected_copy.set_visible(false);
    }

    fn select_index(&self, index: usize, focus: bool) {
        let pid = self.visible_ids.borrow().get(index).copied();
        let Some(pid) = pid else {
            return;
        };
        let row = self.cache.borrow().get(&pid).cloned();
        let Some(row) = row else {
            return;
        };
        self.selected.replace(Some(row.identity.clone()));
        if focus {
            row.button.grab_focus();
            self.scroll
                .ensure_visible(&row.wrapper.clone().upcast(), true);
        }
        self.update_selected();
    }

    fn update_selected(&self) {
        let current = self.selected.borrow().clone();
        let selected = current.as_ref().and_then(|identity| {
            self.snapshot
                .borrow()
                .as_ref()
                .and_then(|snapshot| process_selected(snapshot, identity))
                .cloned()
        });
        let selected = selected.filter(|item| self.visible_ids.borrow().contains(&item.pid));
        let fresh = self.fresh();
        if !fresh || selected.is_none() {
            self.selected.replace(None);
            self.selected_info.set_visible(false);
            self.selected_copy.set_visible(false);
        } else if let Some(item) = selected {
            self.selected_info.set_label(&format!(
                "ВЫБРАН: {} · PID {} · состояние {} · CPU {} · RSS {} · PSS {}",
                clean_text(&item.name),
                item.pid,
                if item.state.is_ascii_graphic() {
                    item.state
                } else {
                    '?'
                },
                cpu_text(item.cpu_percent),
                memory_text(item.rss_kib),
                memory_text(item.pss_kib),
            ));
            self.selected_info.set_visible(true);
            self.selected_copy
                .set_sensitive(self.on_copy.borrow().is_some());
            self.selected_copy.set_visible(true);
        }
        let selected = self.selected.borrow();
        for row in self.cache.borrow().values() {
            if selected.as_ref() == Some(&row.identity) {
                row.button.add_css_class("selected");
            } else {
                row.button.remove_css_class("selected");
            }
        }
    }

    /// Only user search/sort requests may animate keyed row reordering.
    /// Telemetry refreshes update retained GTK labels in place, without FLIP.
    fn render_rows(&self, shown: &[&ProcessItem], animate: bool) {
        let prior = self.visible_ids.borrow().clone();
        let previous = if animate_process_reflow(animate, prior.len()) {
            let cache = self.cache.borrow();
            let ordered: Vec<_> = prior
                .iter()
                .filter_map(|id| {
                    cache
                        .get(id)
                        .map(|row| (id.to_string(), row.wrapper.clone()))
                })
                .collect();
            Some(self.reflow.capture(&ordered))
        } else {
            self.reflow.cancel();
            for row in self.cache.borrow().values() {
                row.wrapper.settle();
            }
            None
        };

        // Non-row placeholders are rebuildable; genuine process buttons
        // remain stable across repeated 2-second producer snapshots.
        let mut child = self.rows.first_child();
        while let Some(widget) = child {
            child = widget.next_sibling();
            if !widget.is::<ReflowRow>() {
                self.rows.remove(&widget);
            }
        }
        let wanted: HashSet<_> = shown.iter().map(|item| item.pid).collect();
        let stale: Vec<_> = self
            .cache
            .borrow()
            .keys()
            .copied()
            .filter(|pid| !wanted.contains(pid))
            .collect();
        for pid in stale {
            if let Some(row) = self.cache.borrow_mut().remove(&pid) {
                self.rows.remove(&row.wrapper);
            }
        }

        let mut ordered = Vec::new();
        let mut predecessor: Option<gtk::Widget> = None;
        let mut ids = Vec::new();
        for item in shown {
            let replace = self
                .cache
                .borrow()
                .get(&item.pid)
                .is_some_and(|row| row.identity.name != item.name);
            if replace
                && let Some(old) = self.cache.borrow_mut().remove(&item.pid)
            {
                self.rows.remove(&old.wrapper);
            }
            // Finish the immutable borrow before creating a missing row.
            // RefCell temporaries in an if-let condition otherwise live into
            // the else arm and panic on borrow_mut during first population.
            let cached = { self.cache.borrow().get(&item.pid).cloned() };
            let row = if let Some(row) = cached {
                row
            } else {
                let row = ProcessVisualRow::new(item, self.weak_self.clone());
                self.rows.append(&row.wrapper);
                self.cache.borrow_mut().insert(item.pid, row.clone());
                row
            };
            row.update(item);
            let in_position = match &predecessor {
                Some(before) => before.next_sibling().as_ref() == Some(row.wrapper.upcast_ref()),
                None => self.rows.first_child().as_ref() == Some(row.wrapper.upcast_ref()),
            };
            if !in_position {
                self.rows
                    .reorder_child_after(&row.wrapper, predecessor.as_ref());
            }
            predecessor = Some(row.wrapper.clone().upcast());
            ids.push(item.pid);
            ordered.push((item.pid.to_string(), row.wrapper));
        }
        self.visible_ids.replace(ids);

        if shown.is_empty() {
            let empty = gtk::Box::new(gtk::Orientation::Vertical, 8);
            empty.set_margin_top(22);
            empty.set_margin_bottom(20);
            empty.set_halign(gtk::Align::Center);
            let message = gtk::Label::new(Some("ПРОЦЕССЫ НЕ НАЙДЕНЫ"));
            message.add_css_class("muted");
            empty.append(&message);
            if !self.search.text().trim().is_empty() {
                let clear = gtk::Button::with_label("ОЧИСТИТЬ ФИЛЬТР");
                clear.add_css_class("process-clear-empty");
                let weak = self.weak_self.clone();
                clear.connect_clicked(move |_| {
                    if let Some(state) = weak.upgrade() {
                        state.search.set_text("");
                        state.search.grab_focus();
                    }
                });
                empty.append(&clear);
            }
            self.rows.append(&empty);
        }
        if let Some(snapshot) = previous {
            self.reflow.play(snapshot, &ordered);
        }
        self.update_selected();
    }

    fn render(&self, animate: bool) {
        self.clear.set_visible(!self.search.text().is_empty());
        for (field, button) in &self.sort_buttons {
            let active = *field == self.sort.get().field;
            if active {
                button.add_css_class("selected");
                button.set_label(&format!(
                    "{} {}",
                    field.heading(),
                    if self.sort.get().descending {
                        "↓"
                    } else {
                        "↑"
                    }
                ));
                self.sort_flow.retarget(&button.clone().upcast());
            } else {
                button.remove_css_class("selected");
                button.set_label(field.heading());
            }
            button.update_property(&[gtk::accessible::Property::Description(if active {
                if self.sort.get().descending {
                    "Сортировка по убыванию, повторное нажатие изменит направление"
                } else {
                    "Сортировка по возрастанию, повторное нажатие изменит направление"
                }
            } else {
                "Сортировать по этому столбцу"
            })]);
        }
        self.copy.set_sensitive(
            self.snapshot.borrow().is_some() && self.fresh() && self.on_copy.borrow().is_some(),
        );
        self.show_more.set_visible(false);
        if let Some(error) = self.error.borrow().as_ref() {
            self.summary.set_text(error);
            self.clear_rows();
            return;
        }
        if self.snapshot.borrow().is_some() && !self.fresh() {
            self.summary
                .set_text("Данные процессов устарели, ожидается новый снимок");
            self.clear_rows();
            return;
        }
        let snapshot = self.snapshot.borrow();
        let Some(snapshot) = snapshot.as_ref() else {
            self.summary
                .set_text("Список процессов появится после открытия вкладки");
            self.clear_rows();
            return;
        };
        let found = filter_sort_processes_with_state(
            &snapshot.rows,
            self.search.text().as_str(),
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
        self.render_rows(&found[..shown], animate);
    }

    fn copy_report(&self) {
        if !self.fresh() {
            self.feedback
                .set_text("Данные устарели, отчёт не скопирован");
            self.render(false);
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

    fn copy_selected(&self) {
        if !self.fresh() {
            self.feedback
                .set_text("Данные устарели, сведения не скопированы");
            self.render(false);
            return;
        }
        let report = self.selected.borrow().as_ref().and_then(|identity| {
            self.snapshot
                .borrow()
                .as_ref()
                .and_then(|snapshot| process_selected(snapshot, identity))
                .map(selected_process_report)
        });
        let result = match report {
            Some(report) => self.on_copy.borrow().as_ref().map_or_else(
                || Err("Копирование недоступно".to_owned()),
                |copy| copy(report),
            ),
            None => Err("Процесс больше не присутствует в текущем снимке".to_owned()),
        };
        self.feedback.set_text(match result {
            Ok(()) => "Сведения о процессе скопированы",
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
        let search_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let search = gtk::SearchEntry::new();
        search.set_hexpand(true);
        search.set_placeholder_text(Some("Фильтр по имени или PID (Ctrl+F)"));
        search.update_property(&[gtk::accessible::Property::Label(
            "Фильтр по названию процесса или PID",
        )]);
        search_row.append(&search);
        let clear = gtk::Button::with_label("×");
        clear.add_css_class("process-search-clear");
        clear.set_visible(false);
        clear.set_tooltip_text(Some("Очистить фильтр"));
        clear.update_property(&[gtk::accessible::Property::Label("Очистить фильтр")]);
        search_row.append(&clear);
        root.append(&search_row);

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
        let sort_flow = FlowIndicator::new(
            &headers.clone().upcast(),
            FlowAxis::Horizontal,
            FlowStyle::Underline,
        );
        root.append(&sort_flow);
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
        let reflow = ReflowScope::new(&rows.clone().upcast());
        let list = gtk::Box::new(gtk::Orientation::Vertical, 5);
        list.append(&rows);
        let show_more = gtk::Button::with_label("ПОКАЗАТЬ ЕЩЁ");
        show_more.add_css_class("process-show-more");
        show_more.set_visible(false);
        list.append(&show_more);
        scroll.set_child(Some(&list));
        let scroll_motion = TenetScroll::new(&scroll);
        root.append(&scroll);

        let selected_info = gtk::Label::new(None);
        selected_info.add_css_class("process-selected-detail");
        selected_info.set_xalign(0.0);
        selected_info.set_wrap(true);
        selected_info.set_selectable(true);
        selected_info.set_visible(false);
        root.append(&selected_info);

        let footer = gtk::Box::new(gtk::Orientation::Vertical, 6);
        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        footer.append(&actions);
        let copy = gtk::Button::with_label("Скопировать отчёт");
        copy.set_sensitive(false);
        actions.append(&copy);
        let selected_copy = gtk::Button::with_label("Скопировать процесс");
        selected_copy.add_css_class("process-selected-copy");
        selected_copy.update_property(&[gtk::accessible::Property::Label(
            "Скопировать сведения о выбранном процессе",
        )]);
        selected_copy.set_visible(false);
        actions.append(&selected_copy);
        let feedback = gtk::Label::new(None);
        feedback.add_css_class("muted");
        feedback.set_wrap(true);
        footer.append(&feedback);
        root.append(&footer);

        let state = Rc::new_cyclic(|weak| ProcessTableState {
            weak_self: weak.clone(),
            search: search.clone(),
            clear: clear.clone(),
            sort: Cell::new(ProcessSortState::default()),
            sort_buttons,
            sort_flow,
            visible_count: Cell::new(PROCESS_PAGE),
            show_more,
            summary,
            rows,
            visible_ids: RefCell::new(Vec::new()),
            cache: RefCell::new(HashMap::new()),
            reflow,
            scroll: scroll_motion,
            selected: RefCell::new(None),
            selected_info,
            selected_copy,
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
                    state.render(true);
                }
            }
        });
        clear.connect_clicked({
            let weak = Rc::downgrade(&state);
            move |_| {
                if let Some(state) = weak.upgrade() {
                    state.search.set_text("");
                    state.search.grab_focus();
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
                    state.render(true);
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
                    state.render(false);
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
        state.selected_copy.connect_clicked({
            let weak = Rc::downgrade(&state);
            move |_| {
                if let Some(state) = weak.upgrade() {
                    state.copy_selected();
                }
            }
        });
        let shortcuts = gtk::EventControllerKey::new();
        shortcuts.set_propagation_phase(gtk::PropagationPhase::Capture);
        let search_weak = search.downgrade();
        shortcuts.connect_key_pressed(move |_, pressed, _, modifiers| {
            if modifiers.contains(gtk::gdk::ModifierType::CONTROL_MASK)
                && matches!(pressed, gtk::gdk::Key::f | gtk::gdk::Key::F)
            {
                if let Some(search) = search_weak.upgrade() {
                    search.grab_focus();
                }
                gtk::glib::Propagation::Stop
            } else {
                gtk::glib::Propagation::Proceed
            }
        });
        root.add_controller(shortcuts);
        state.render(false);
        Self { root, state }
    }

    pub fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }

    pub fn set_copy_handler(&self, on_copy: impl Fn(String) -> Result<(), String> + 'static) {
        self.state.on_copy.replace(Some(Box::new(on_copy)));
        self.state.render(false);
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
        self.state.render(false);
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
        self.state.render(false);
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
                state.render(false);
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
        self.state.render(false);
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

    #[test]
    fn retained_reflow_only_tracks_small_user_driven_changes() {
        assert!(!animate_process_reflow(false, 5));
        assert!(!animate_process_reflow(false, 24));
        assert!(!animate_process_reflow(true, 0));
        assert!(animate_process_reflow(true, 1));
        assert!(animate_process_reflow(true, 24));
        assert!(!animate_process_reflow(true, 25));
        assert!(!animate_process_reflow(true, 180));
    }

    #[test]
    fn keyboard_result_navigation_is_bounded_to_rendered_pages() {
        use ProcessNavigation as Move;
        assert_eq!(process_selection_after(5, 0, Move::Last), 0);
        assert_eq!(process_selection_after(0, 12, Move::Previous), 0);
        assert_eq!(process_selection_after(3, 12, Move::Next), 4);
        assert_eq!(process_selection_after(3, 12, Move::First), 0);
        assert_eq!(process_selection_after(3, 12, Move::Last), 11);
        assert_eq!(process_selection_after(10, 12, Move::PageNext), 11);
        assert_eq!(process_selection_after(10, 12, Move::PagePrevious), 2);
        assert_eq!(process_selection_after(99, 5, Move::Next), 4);
    }

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
    fn selected_process_is_snapshot_scoped_and_report_never_invents_values() {
        let snapshot = ProcessList {
            rows: vec![item(71, "worker\nunknown", None, Some(4096), None)],
            total: 1,
            truncated: false,
        };
        let current = ProcessSelection {
            pid: 71,
            name: "worker\nunknown".into(),
        };
        assert!(process_selected(&snapshot, &current).is_some());
        assert!(
            process_selected(
                &snapshot,
                &ProcessSelection {
                    pid: 71,
                    name: "restarted".into()
                }
            )
            .is_none()
        );
        let report = selected_process_report(&snapshot.rows[0]);
        assert!(report.contains("PID: 71"));
        assert!(report.contains("RSS: 4.0 MiB"));
        assert!(report.contains("PSS: —"));
        assert!(!report.contains("worker\nunknown"));
        assert!(report.contains("worker unknown"));
    }

    #[test]
    fn repeated_pid_from_untrusted_snapshot_never_duplicates_widget_identity() {
        let rows = [
            item(11, "same", Some(70.0), None, None),
            item(12, "other", Some(30.0), None, None),
            item(11, "same", Some(10.0), None, None),
        ];
        let found = filter_sort_processes(&rows, "", ProcessSort::Cpu, 20);
        assert_eq!(
            found.iter().map(|item| item.pid).collect::<Vec<_>>(),
            [11, 12]
        );
        assert_eq!(found[0].cpu_percent, Some(70.0));
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
