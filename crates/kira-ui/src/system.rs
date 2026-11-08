//! Portable GTK4 system-monitor UI. No /proc reads, Core IPC or process actions.
use std::cmp::Ordering;
use std::time::Instant;

use gtk::prelude::*;

use crate::process_table::ProcessTable;
use crate::system_charts::{ChartSample, SystemCharts};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SystemOverview {
    pub cpu_percent: Option<u8>,
    pub cpu_temperature_c: Option<f64>,
    pub memory_total_kib: Option<u64>,
    pub memory_available_kib: Option<u64>,
    pub load: Option<[f64; 3]>,
    pub uptime_seconds: Option<u64>,
    pub battery_percent: Option<u8>,
    pub battery_charging: Option<bool>,
    pub gpu: Vec<GpuReading>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GpuReading {
    pub name: String,
    pub utilization_percent: Option<u8>,
    pub temperature_c: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProcessItem {
    pub pid: u32,
    pub name: String,
    pub cpu_percent: Option<f64>,
    pub rss_kib: Option<u64>,
    pub pss_kib: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProcessList {
    pub rows: Vec<ProcessItem>,
    pub total: usize,
    pub truncated: bool,
}

/// CPU ordering is stable when measurements are missing, and the bounded
/// output prevents a process explosion from creating thousands of GTK nodes.
pub fn top_processes(rows: &[ProcessItem], limit: usize) -> Vec<&ProcessItem> {
    let mut selected: Vec<_> = rows.iter().collect();
    selected.sort_by(|a, b| {
        match (
            a.cpu_percent.filter(|v| v.is_finite()),
            b.cpu_percent.filter(|v| v.is_finite()),
        ) {
            (Some(a), Some(b)) => b.partial_cmp(&a).unwrap_or(Ordering::Equal),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => Ordering::Equal,
        }
        .then_with(|| a.pid.cmp(&b.pid))
    });
    selected.truncate(limit);
    selected
}

fn gib(kib: u64) -> String {
    format!("{:.1} GiB", kib as f64 / 1024.0 / 1024.0)
}

fn percent(value: Option<u8>) -> String {
    value.map_or_else(|| "недоступно".into(), |value| format!("{value}%"))
}

fn temperature(value: Option<f64>) -> String {
    value
        .filter(|value| value.is_finite())
        .map_or_else(|| "недоступно".into(), |value| format!("{value:.1} °C"))
}

pub fn overview_labels(value: &SystemOverview) -> Vec<String> {
    let memory = match (value.memory_total_kib, value.memory_available_kib) {
        (Some(total), Some(available)) if available <= total => {
            format!("{} свободно из {}", gib(available), gib(total))
        }
        _ => "недоступно".into(),
    };
    let load = value
        .load
        .filter(|v| v.iter().all(|n| n.is_finite() && *n >= 0.0))
        .map_or_else(
            || "недоступно".into(),
            |v| format!("{:.2} / {:.2} / {:.2}", v[0], v[1], v[2]),
        );
    let uptime = value.uptime_seconds.map_or_else(
        || "недоступно".into(),
        |seconds| format!("{} ч {} мин", seconds / 3600, (seconds % 3600) / 60),
    );
    let battery = value.battery_percent.map_or_else(
        || "недоступно".into(),
        |level| {
            format!(
                "{level}%{}",
                if value.battery_charging == Some(true) {
                    " · зарядка"
                } else {
                    ""
                }
            )
        },
    );
    let gpu = if value.gpu.is_empty() {
        "недоступно".into()
    } else {
        value
            .gpu
            .iter()
            .take(3)
            .map(|gpu| {
                format!(
                    "{}: {} / {}",
                    gpu.name,
                    percent(gpu.utilization_percent),
                    temperature(gpu.temperature_c)
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    vec![
        percent(value.cpu_percent),
        temperature(value.cpu_temperature_c),
        memory,
        load,
        uptime,
        battery,
        gpu,
    ]
}

pub struct SystemMonitor {
    root: gtk::Box,
    pages: gtk::Stack,
    readings: Vec<gtk::Label>,
    charts: SystemCharts,
    process_table: ProcessTable,
}

impl SystemMonitor {
    pub fn new() -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 12);
        root.add_css_class("system-monitor");
        let title = gtk::Label::new(Some("Система"));
        title.add_css_class("section-title");
        title.set_xalign(0.0);
        root.append(&title);

        let pages = gtk::Stack::new();
        pages.set_transition_type(gtk::StackTransitionType::None);
        let selector = gtk::StackSwitcher::new();
        selector.set_stack(Some(&pages));
        root.append(&selector);

        let overview = gtk::Box::new(gtk::Orientation::Vertical, 5);
        let charts = SystemCharts::new();
        overview.append(&charts.widget());
        let mut readings = Vec::new();
        for label in [
            "CPU",
            "Температура CPU",
            "Оперативная память",
            "Нагрузка (1/5/15)",
            "Аптайм",
            "Батарея",
            "GPU",
        ] {
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            row.add_css_class("system-metric");
            let name = gtk::Label::new(Some(label));
            name.set_xalign(0.0);
            name.set_hexpand(true);
            name.add_css_class("muted");
            row.append(&name);
            let reading = gtk::Label::new(Some("недоступно"));
            reading.set_xalign(1.0);
            reading.set_wrap(true);
            reading.set_max_width_chars(36);
            reading.add_css_class("system-value");
            row.append(&reading);
            overview.append(&row);
            readings.push(reading);
        }
        pages.add_titled(&overview, Some("overview"), "Обзор");

        let process_table = ProcessTable::new();
        pages.add_titled(&process_table.widget(), Some("processes"), "Процессы");

        root.append(&pages);
        Self {
            root,
            pages,
            readings,
            charts,
            process_table,
        }
    }

    pub fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }

    pub fn processes_selected(&self) -> bool {
        self.pages.visible_child_name().as_deref() == Some("processes")
    }

    pub fn set_overview(&self, overview: &SystemOverview) {
        for (label, text) in self.readings.iter().zip(overview_labels(overview)) {
            label.set_text(&text);
        }
    }

    pub fn set_pulse(&self, sample: ChartSample) {
        self.charts.push_sample(sample);
    }

    /// Peak is the maximum of actually measured GPU utilizations.
    /// A missing device reading must not be interpreted as 0%.
    pub fn observe_gpu(&self, overview: &SystemOverview, measured_at: Instant) {
        let peak = overview
            .gpu
            .iter()
            .filter_map(|gpu| gpu.utilization_percent)
            .filter(|value| *value <= 100)
            .max();
        self.charts.observe_gpu(measured_at, peak);
    }

    pub fn set_processes(&self, snapshot: &ProcessList) {
        self.process_table.set_snapshot(snapshot);
    }

    pub fn set_process_error(&self, message: &str) {
        self.process_table.set_error(message);
    }

    pub fn set_report_copy_handler(
        &self,
        on_copy: impl Fn(String) -> Result<(), String> + 'static,
    ) {
        self.process_table.set_copy_handler(on_copy);
    }
}

impl Default for SystemMonitor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(pid: u32, cpu: Option<f64>) -> ProcessItem {
        ProcessItem {
            pid,
            name: "sample".into(),
            cpu_percent: cpu,
            rss_kib: None,
            pss_kib: None,
        }
    }

    #[test]
    fn top_processes_sort_known_cpu_before_missing_and_bound_output() {
        let rows = [
            row(1, None),
            row(2, Some(2.0)),
            row(3, Some(8.0)),
            row(4, Some(f64::NAN)),
        ];
        assert_eq!(
            top_processes(&rows, 2)
                .iter()
                .map(|p| p.pid)
                .collect::<Vec<_>>(),
            [3, 2]
        );
        assert_eq!(top_processes(&rows, 0).len(), 0);
    }

    #[test]
    fn overview_never_invents_missing_values() {
        let empty = overview_labels(&SystemOverview::default());
        assert_eq!(empty.len(), 7);
        assert!(empty.iter().all(|text| text == "недоступно"));
        let known = SystemOverview {
            memory_total_kib: Some(2 * 1024 * 1024),
            memory_available_kib: Some(1024 * 1024),
            load: Some([0.5, 1.25, 2.0]),
            ..Default::default()
        };
        let formatted = overview_labels(&known);
        assert_eq!(formatted[2], "1.0 GiB свободно из 2.0 GiB");
        assert_eq!(formatted[3], "0.50 / 1.25 / 2.00");
    }
}
