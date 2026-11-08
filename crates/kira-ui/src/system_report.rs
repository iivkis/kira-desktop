//! Source-aligned read-only System Overview report and clipboard intent.
//! No /proc, Core, Niri, file writes, or system commands in the GTK library.
//! A fresh sample is required at the exact moment of export.
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk::prelude::*;

use crate::system::SystemOverview;

pub const OVERVIEW_REPORT_FRESHNESS: Duration = Duration::from_secs(6);

pub fn overview_snapshot_fresh(received: Option<Instant>, now: Instant) -> bool {
    received.is_some_and(|at| now >= at && now.duration_since(at) < OVERVIEW_REPORT_FRESHNESS)
}

fn safe_text(value: &str) -> String {
    value
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(100)
        .collect()
}

fn percent(value: Option<u8>) -> String {
    value
        .filter(|value| *value <= 100)
        .map_or_else(|| "НЕДОСТУПНО".to_owned(), |value| format!("{value}%"))
}

fn temperature(value: Option<f64>) -> String {
    value
        .filter(|value| value.is_finite())
        .map_or_else(|| "НЕДОСТУПНО".to_owned(), |value| format!("{value:.1} °C"))
}

fn fraction(total: Option<u64>, available: Option<u64>) -> String {
    let (Some(total), Some(available)) = (total, available) else {
        return "НЕДОСТУПНО".to_owned();
    };
    if available > total {
        return "НЕДОСТУПНО".to_owned();
    }
    if total == 0 {
        return "НЕ НАСТРОЕНО".to_owned();
    }
    let mib = 1024.0;
    format!(
        "{:.1} / {:.1} MiB (занято / всего); свободно {:.1} MiB",
        (total - available) as f64 / mib,
        total as f64 / mib,
        available as f64 / mib
    )
}

/// A truthful point-in-time report. Unknown values remain unknown, not zero.
/// GPU output is bounded and names cannot inject extra clipboard rows.
pub fn format_overview_report(snapshot: &SystemOverview) -> String {
    let load = snapshot
        .load
        .filter(|values| values.iter().all(|value| value.is_finite() && *value >= 0.0))
        .map_or_else(
            || "НЕДОСТУПНО".to_owned(),
            |values| format!("{:.2} / {:.2} / {:.2}", values[0], values[1], values[2]),
        );
    let uptime = snapshot.uptime_seconds.map_or_else(
        || "НЕДОСТУПНО".to_owned(),
        |seconds| {
            format!(
                "{}d {}h {}m",
                seconds / 86_400,
                seconds % 86_400 / 3_600,
                seconds % 3_600 / 60
            )
        },
    );
    let power = match snapshot.battery_percent.filter(|level| *level <= 100) {
        None => "НЕДОСТУПНО ИЛИ НЕ ОБНАРУЖЕНО".to_owned(),
        Some(level) => format!(
            "{level}%{}",
            match snapshot.battery_charging {
                Some(true) => " / ЗАРЯДКА",
                Some(false) => " / РАЗРЯДКА",
                None => "",
            }
        ),
    };

    let mut report = format!(
        "Kira System Monitor · Обзор (полученный снимок)\nCPU: {}\nТемпература CPU: {}\nRAM: {}\nSWAP: {}\nДиск /: {}\nНагрузка (1/5/15): {}\nВремя работы: {}\nПитание: {}\n",
        percent(snapshot.cpu_percent),
        temperature(snapshot.cpu_temperature_c),
        fraction(snapshot.memory_total_kib, snapshot.memory_available_kib),
        fraction(snapshot.swap_total_kib, snapshot.swap_free_kib),
        fraction(snapshot.root_total_kib, snapshot.root_available_kib),
        load,
        uptime,
        power,
    );

    if snapshot.gpu.is_empty() {
        report.push_str("GPU: НЕ ОБНАРУЖЕНЫ ИЛИ НЕДОСТУПНЫ\n");
    } else {
        report.push_str(&format!("GPU (показано до 8 из {}):\n", snapshot.gpu.len()));
        for gpu in snapshot.gpu.iter().take(8) {
            report.push_str(&format!(
                "{} | нагрузка {} | температура {}\n",
                safe_text(&gpu.name),
                percent(gpu.utilization_percent),
                temperature(gpu.temperature_c),
            ));
        }
    }
    report
}

type CopyHandler = Box<dyn Fn(String) -> Result<(), String>>;

struct ReportState {
    copy: gtk::Button,
    feedback: gtk::Label,
    snapshot: RefCell<Option<SystemOverview>>,
    received: Cell<Option<Instant>>,
    revision: Cell<u64>,
    on_copy: RefCell<Option<CopyHandler>>,
}

impl ReportState {
    fn render(&self) {
        self.copy.set_sensitive(
            self.snapshot.borrow().is_some()
                && overview_snapshot_fresh(self.received.get(), Instant::now())
                && self.on_copy.borrow().is_some(),
        );
    }

    fn invalidate(&self) {
        self.revision.set(self.revision.get().wrapping_add(1));
        self.received.set(None);
        self.snapshot.replace(None);
        self.feedback
            .set_text("Данные обзора устарели. Дождитесь нового измерения.");
        self.render();
    }

    fn copy_report(&self) {
        if !overview_snapshot_fresh(self.received.get(), Instant::now()) {
            self.invalidate();
            return;
        }
        let snapshot = self.snapshot.borrow();
        let Some(snapshot) = snapshot.as_ref() else {
            self.feedback.set_text("Нет измерений для отчёта");
            self.render();
            return;
        };
        let text = format_overview_report(snapshot);
        let handler = self.on_copy.borrow();
        let result = handler
            .as_ref()
            .map_or_else(|| Err("Буфер обмена недоступен".to_owned()), |copy| copy(text));
        self.feedback.set_text(match result {
            Ok(()) => "Отчёт обзора скопирован",
            Err(ref error) => error,
        });
    }
}

/// Retained GTK report action; the Shell supplies the native GDK clipboard.
pub struct SystemReport {
    root: gtk::Box,
    state: Rc<ReportState>,
}

impl SystemReport {
    pub fn new() -> Self {
        let root = gtk::Box::new(gtk::Orientation::Horizontal, 9);
        root.add_css_class("system-report-actions");
        let copy = gtk::Button::with_label("КОПИРОВАТЬ ОТЧЁТ");
        copy.add_css_class("system-report-copy");
        copy.set_sensitive(false);
        copy.update_property(&[gtk::accessible::Property::Label(
            "Скопировать текущий отчёт об измерениях системы",
        )]);
        let feedback = gtk::Label::new(None);
        feedback.set_hexpand(true);
        feedback.set_xalign(1.0);
        feedback.set_wrap(true);
        feedback.add_css_class("muted");
        root.append(&feedback);
        root.append(&copy);

        let state = Rc::new(ReportState {
            copy,
            feedback,
            snapshot: RefCell::new(None),
            received: Cell::new(None),
            revision: Cell::new(0),
            on_copy: RefCell::new(None),
        });
        let weak = Rc::downgrade(&state);
        state.copy.connect_clicked(move |_| {
            if let Some(state) = weak.upgrade() {
                state.copy_report();
            }
        });
        let weak = Rc::downgrade(&state);
        root.connect_unmap(move |_| {
            if let Some(state) = weak.upgrade() {
                state.invalidate();
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

    pub fn update(&self, snapshot: &SystemOverview) {
        self.state.snapshot.replace(Some(snapshot.clone()));
        self.state.received.set(Some(Instant::now()));
        let revision = self.state.revision.get().wrapping_add(1);
        self.state.revision.set(revision);
        self.state.feedback.set_text("");
        self.state.render();

        let weak = Rc::downgrade(&self.state);
        gtk::glib::timeout_add_local_once(OVERVIEW_REPORT_FRESHNESS, move || {
            if let Some(state) = weak.upgrade()
                && state.revision.get() == revision
            {
                state.invalidate();
            }
        });
    }
}

impl Default for SystemReport {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::GpuReading;

    #[test]
    fn report_freshness_fails_closed_on_expiry_or_future() {
        let now = Instant::now();
        assert!(!overview_snapshot_fresh(None, now));
        assert!(overview_snapshot_fresh(Some(now), now));
        assert!(overview_snapshot_fresh(
            Some(now),
            now + Duration::from_millis(5_999)
        ));
        assert!(!overview_snapshot_fresh(
            Some(now),
            now + OVERVIEW_REPORT_FRESHNESS
        ));
        assert!(!overview_snapshot_fresh(
            Some(now + Duration::from_secs(1)),
            now
        ));
    }

    #[test]
    fn overview_report_is_bounded_and_does_not_fabricate_missing_values() {
        let blank = format_overview_report(&SystemOverview::default());
        assert!(blank.contains("CPU: НЕДОСТУПНО"));
        assert!(blank.contains("RAM: НЕДОСТУПНО"));
        assert!(blank.contains("Питание: НЕДОСТУПНО ИЛИ НЕ ОБНАРУЖЕНО"));
        assert!(!blank.contains("CPU: 0%"));

        let sample = SystemOverview {
            cpu_percent: Some(20),
            cpu_temperature_c: Some(f64::NAN),
            memory_total_kib: Some(4096),
            memory_available_kib: Some(1024),
            swap_total_kib: Some(0),
            swap_free_kib: Some(0),
            root_total_kib: Some(100),
            root_available_kib: Some(200),
            battery_percent: Some(8),
            battery_charging: Some(false),
            load: Some([0.25, 1.5, 3.0]),
            gpu: (0..20)
                .map(|i| GpuReading {
                    name: format!("GPU {i}\nINJECT"),
                    utilization_percent: Some(90),
                    temperature_c: None,
                })
                .collect(),
            ..SystemOverview::default()
        };
        let text = format_overview_report(&sample);
        assert!(text.contains("CPU: 20%"));
        assert!(text.contains("Температура CPU: НЕДОСТУПНО"));
        assert!(text.contains("RAM: 3.0 / 4.0 MiB"));
        assert!(text.contains("SWAP: НЕ НАСТРОЕНО"));
        assert!(text.contains("Диск /: НЕДОСТУПНО"));
        assert!(text.contains("Питание: 8% / РАЗРЯДКА"));
        assert!(text.contains("GPU (показано до 8 из 20)"));
        assert!(text.contains("GPU 0 INJECT"));
        assert!(!text.contains("GPU 19"));
        assert!(!text.contains("GPU 0\nINJECT"));
    }
}
