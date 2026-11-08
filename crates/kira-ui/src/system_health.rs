//! Source-derived, compositor-independent system health classification and
//! native GTK status button. This crate never probes Core, Niri or sensors.
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk::prelude::*;

pub const HEALTH_SAMPLE_FRESHNESS: Duration = Duration::from_secs(6);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreStatus {
    Connecting,
    Connected,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthPhase {
    Ready,
    Pending,
    Warning,
    Error,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HealthMetrics {
    pub cpu_percent: Option<u8>,
    pub memory_total_kib: Option<u64>,
    pub memory_available_kib: Option<u64>,
    pub root_total_kib: Option<u64>,
    pub root_available_kib: Option<u64>,
    pub battery_percent: Option<u8>,
    pub battery_charging: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HealthSummary {
    pub label: &'static str,
    pub localized: &'static str,
    pub phase: HealthPhase,
    pub details: String,
    pub quiet_refresh: bool,
}

pub fn used_percent(total: Option<u64>, available: Option<u64>) -> Option<u8> {
    let (Some(total), Some(available)) = (total, available) else {
        return None;
    };
    if total == 0 || available > total {
        return None;
    }
    let used = total - available;
    Some(((u128::from(used) * 100 / u128::from(total)).min(100)) as u8)
}

fn status_label(code: &'static str) -> &'static str {
    match code {
        "CORE OFF" => "НЕТ CORE",
        "STARTING" => "ЗАПУСК",
        "NIRI OFF" => "НЕТ NIRI",
        "WAITING" => "ОЖИДАНИЕ",
        "CHECKING" => "ПРОВЕРКА",
        "LOW DISK" => "МАЛО МЕСТА",
        "LOW MEM" => "МАЛО ПАМЯТИ",
        "LOW BAT" => "НИЗКИЙ ЗАРЯД",
        "BUSY" => "НАГРУЗКА",
        "PARTIAL" => "НЕТ ЧАСТИ ДАННЫХ",
        "READY" => "ГОТОВО",
        _ => "НЕИЗВЕСТНО",
    }
}

/// Reproduces main's priority of actual Core and Niri failures, stale sampling,
/// measured resource warnings, and partial-data state. A stale stream may be
/// displayed quietly but cannot ever become READY or retain an old warning.
pub fn classify_health(
    core: CoreStatus,
    version: &str,
    niri: Option<bool>,
    metrics: Option<&HealthMetrics>,
    stale: bool,
) -> HealthSummary {
    let core_detail = match core {
        CoreStatus::Connected => {
            if version.is_empty() {
                "Доступен".to_owned()
            } else {
                format!("Доступен (v{version})")
            }
        }
        CoreStatus::Connecting => "Подключение".to_owned(),
        CoreStatus::Unavailable => "Недоступен".to_owned(),
    };
    let niri_detail = match niri {
        None => "Ожидание первого снимка",
        Some(true) => "Подключён",
        Some(false) => "Отключён",
    };
    let live = if stale { None } else { metrics };
    let mem = live.and_then(|m| used_percent(m.memory_total_kib, m.memory_available_kib));
    let disk = live.and_then(|m| used_percent(m.root_total_kib, m.root_available_kib));
    let cpu = live.and_then(|m| m.cpu_percent.filter(|v| *v <= 100));
    let battery = live.and_then(|m| m.battery_percent.filter(|v| *v <= 100));
    let details = format!(
        "Kira Core: {core_detail}\nNiri IPC: {niri_detail}\nCPU: {}\nПамять: {}\nДиск: {}\nБатарея: {}",
        if live.is_none() {
            "Нет данных".to_owned()
        } else {
            cpu.map_or_else(|| "Измеряется".to_owned(), |p| format!("{p}%"))
        },
        if live.is_none() {
            "Нет данных".to_owned()
        } else {
            mem.map_or_else(|| "Недоступно".to_owned(), |p| format!("{p}% занято"))
        },
        if live.is_none() {
            "Нет данных".to_owned()
        } else {
            disk.map_or_else(|| "Недоступно".to_owned(), |p| format!("{p}% занято"))
        },
        if live.is_none() {
            "Нет данных".to_owned()
        } else {
            battery.map_or_else(|| "Не обнаружена".to_owned(), |p| format!("{p}%"))
        }
    );
    let (label, phase) = if core == CoreStatus::Unavailable {
        ("CORE OFF", HealthPhase::Error)
    } else if core == CoreStatus::Connecting {
        ("STARTING", HealthPhase::Pending)
    } else if niri == Some(false) {
        ("NIRI OFF", HealthPhase::Error)
    } else if niri.is_none() {
        ("WAITING", HealthPhase::Pending)
    } else if stale || live.is_none() || mem.is_none() || cpu.is_none() {
        ("CHECKING", HealthPhase::Pending)
    } else if disk.is_some_and(|p| p >= 90) {
        ("LOW DISK", HealthPhase::Warning)
    } else if mem.is_some_and(|p| p >= 90) {
        ("LOW MEM", HealthPhase::Warning)
    } else if live.is_some_and(|m| {
        m.battery_percent.is_some_and(|p| p <= 15) && m.battery_charging == Some(false)
    }) {
        ("LOW BAT", HealthPhase::Warning)
    } else if cpu.is_some_and(|p| p >= 95) {
        ("BUSY", HealthPhase::Warning)
    } else if disk.is_none() {
        ("PARTIAL", HealthPhase::Pending)
    } else {
        ("READY", HealthPhase::Ready)
    };
    HealthSummary {
        label,
        localized: status_label(label),
        phase,
        details,
        quiet_refresh: stale && core == CoreStatus::Connected && niri == Some(true),
    }
}

struct HealthState {
    button: gtk::Button,
    marker: gtk::Box,
    readout: gtk::Label,
    core: Cell<CoreStatus>,
    version: RefCell<String>,
    niri: Cell<Option<bool>>,
    metrics: RefCell<Option<HealthMetrics>>,
    received_at: Cell<Option<Instant>>,
    revision: Cell<u64>,
}

impl HealthState {
    fn summary(&self) -> HealthSummary {
        let now = Instant::now();
        let at = self.received_at.get();
        let stale = at.is_some_and(|at| {
            now < at || now.saturating_duration_since(at) > HEALTH_SAMPLE_FRESHNESS
        });
        classify_health(
            self.core.get(),
            &self.version.borrow(),
            self.niri.get(),
            self.metrics.borrow().as_ref(),
            stale,
        )
    }

    fn render(&self) {
        let health = self.summary();
        for class in ["ready", "pending", "warning", "error"] {
            self.button.remove_css_class(class);
            self.marker.remove_css_class(class);
        }
        let class = match health.phase {
            HealthPhase::Ready => "ready",
            HealthPhase::Pending => "pending",
            HealthPhase::Warning => "warning",
            HealthPhase::Error => "error",
        };
        self.button.add_css_class(class);
        self.marker.add_css_class(class);
        self.readout.set_text(health.localized);
        self.readout
            .set_visible(health.phase != HealthPhase::Ready && !health.quiet_refresh);
        let accessible_name = if health.quiet_refresh {
            "Открыть монитор системы".to_owned()
        } else {
            format!("Открыть монитор системы: {}", health.localized)
        };
        self.button.update_property(&[gtk::accessible::Property::Label(&accessible_name)]);
        self.button.set_tooltip_text(Some(&format!(
            "{}\nНажмите для подробных сведений",
            health.details
        )));
    }
}

/// Reusable status indicator; Shell owns Core/Niri transport and sampling.
/// Action may be set independently of attaching this component to a bar.
pub struct SystemHealth {
    root: gtk::Button,
    state: Rc<HealthState>,
}

impl SystemHealth {
    pub fn new() -> Self {
        let root = gtk::Button::new();
        root.add_css_class("system-health");
        let content = gtk::Box::new(gtk::Orientation::Horizontal, 7);
        let marker = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        marker.add_css_class("system-health-dot");
        content.append(&marker);
        let readout = gtk::Label::new(Some("ЗАПУСК"));
        readout.add_css_class("system-health-readout");
        content.append(&readout);
        root.set_child(Some(&content));
        let state = Rc::new(HealthState {
            button: root.clone(),
            marker,
            readout,
            core: Cell::new(CoreStatus::Connecting),
            version: RefCell::new(String::new()),
            niri: Cell::new(None),
            metrics: RefCell::new(None),
            received_at: Cell::new(None),
            revision: Cell::new(0),
        });
        state.render();
        Self { root, state }
    }

    pub fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }

    pub fn set_open_action(&self, on_open: impl Fn() + 'static) {
        self.root.connect_clicked(move |_| on_open());
    }

    pub fn set_core(&self, status: CoreStatus, version: &str) {
        self.state.core.set(status);
        *self.state.version.borrow_mut() = version.to_owned();
        self.state.render();
    }

    pub fn set_niri(&self, connected: Option<bool>) {
        self.state.niri.set(connected);
        self.state.render();
    }

    pub fn set_metrics(&self, metrics: HealthMetrics, measured_at: Instant) {
        // Discard delayed worker samples which would revive an old state.
        if self.state.received_at.get().is_some_and(|at| measured_at <= at) {
            return;
        }
        self.state.received_at.set(Some(measured_at));
        *self.state.metrics.borrow_mut() = Some(metrics);
        let revision = self.state.revision.get().wrapping_add(1);
        self.state.revision.set(revision);
        self.state.render();
        let weak = Rc::downgrade(&self.state);
        gtk::glib::timeout_add_local_once(
            HEALTH_SAMPLE_FRESHNESS + Duration::from_millis(1),
            move || {
                if let Some(state) = weak.upgrade()
                    && state.revision.get() == revision
                {
                    state.render();
                }
            },
        );
    }

    pub fn summary(&self) -> HealthSummary {
        self.state.summary()
    }
}

impl Default for SystemHealth {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn healthy() -> HealthMetrics {
        HealthMetrics {
            cpu_percent: Some(40),
            memory_total_kib: Some(1000),
            memory_available_kib: Some(500),
            root_total_kib: Some(1000),
            root_available_kib: Some(800),
            battery_percent: Some(82),
            battery_charging: Some(false),
        }
    }

    fn classify(
        core: CoreStatus,
        niri: Option<bool>,
        data: Option<&HealthMetrics>,
        stale: bool,
    ) -> HealthSummary {
        classify_health(core, "0.1.0", niri, data, stale)
    }

    #[test]
    fn service_outages_take_precedence_over_metrics() {
        let good = healthy();
        let unavailable = classify(CoreStatus::Unavailable, Some(true), Some(&good), false);
        assert_eq!(unavailable.label, "CORE OFF");
        assert_eq!(unavailable.phase, HealthPhase::Error);
        assert!(unavailable.details.contains("Kira Core: Недоступен"));
        assert_eq!(
            classify(CoreStatus::Connecting, Some(false), Some(&good), false).label,
            "STARTING"
        );
        assert_eq!(
            classify(CoreStatus::Connected, Some(false), Some(&good), false).label,
            "NIRI OFF"
        );
        assert_eq!(
            classify(CoreStatus::Connected, None, Some(&good), false).label,
            "WAITING"
        );
    }

    #[test]
    fn warnings_follow_source_priority() {
        let mut m = healthy();
        assert_eq!(
            classify(CoreStatus::Connected, Some(true), Some(&m), false).label,
            "READY"
        );
        m.root_available_kib = Some(90);
        m.memory_available_kib = Some(20);
        m.cpu_percent = Some(99);
        m.battery_percent = Some(5);
        assert_eq!(
            classify(CoreStatus::Connected, Some(true), Some(&m), false).label,
            "LOW DISK"
        );
        m.root_available_kib = Some(900);
        assert_eq!(
            classify(CoreStatus::Connected, Some(true), Some(&m), false).label,
            "LOW MEM"
        );
        m.memory_available_kib = Some(500);
        assert_eq!(
            classify(CoreStatus::Connected, Some(true), Some(&m), false).label,
            "LOW BAT"
        );
        m.battery_charging = Some(true);
        assert_eq!(
            classify(CoreStatus::Connected, Some(true), Some(&m), false).label,
            "BUSY"
        );
    }

    #[test]
    fn stale_and_missing_samples_never_claim_ready_or_warning() {
        let m = healthy();
        let stale = classify(CoreStatus::Connected, Some(true), Some(&m), true);
        assert_eq!(stale.label, "CHECKING");
        assert!(stale.quiet_refresh);
        assert!(!stale.details.contains("40%"));
        assert_eq!(
            classify(CoreStatus::Connected, Some(true), None, false).label,
            "CHECKING"
        );
        let mut partial = healthy();
        partial.root_available_kib = None;
        assert_eq!(
            classify(CoreStatus::Connected, Some(true), Some(&partial), false).label,
            "PARTIAL"
        );
        partial.cpu_percent = None;
        assert_eq!(
            classify(CoreStatus::Connected, Some(true), Some(&partial), false).label,
            "CHECKING"
        );
    }

    #[test]
    fn invalid_measurements_are_unavailable_not_zero() {
        assert_eq!(used_percent(Some(1000), Some(500)), Some(50));
        assert_eq!(used_percent(Some(0), Some(0)), None);
        assert_eq!(used_percent(Some(100), Some(101)), None);
        assert_eq!(used_percent(Some(u64::MAX), Some(0)), Some(100));
        let mut m = healthy();
        m.memory_available_kib = Some(1001);
        assert_eq!(
            classify(CoreStatus::Connected, Some(true), Some(&m), false).label,
            "CHECKING"
        );
    }
}
