//! Read-only hardware and system disclosure for any GTK host. No /sys,
//! filesystem IO, IPC, process execution or main-thread sensor polling.
use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use gtk::prelude::*;

use crate::system::{GpuReading, SystemOverview};

pub const DETAILS_FRESHNESS: Duration = Duration::from_secs(6);

pub fn cpu_temperature(value: Option<f64>) -> String {
    value
        .filter(|value| value.is_finite())
        .map_or_else(|| "НЕДОСТУПНО".to_owned(), |value| format!("{value:.1} °C"))
}

pub fn gpu_utilization(value: Option<u8>) -> String {
    value
        .filter(|value| *value <= 100)
        .map_or_else(|| "НАГРУЗКА Н/Д".to_owned(), |value| format!("{value}%"))
}

pub fn gpu_temperature(value: Option<f64>) -> String {
    value
        .filter(|value| value.is_finite())
        .map_or_else(|| "ТЕМП Н/Д".to_owned(), |value| format!("{value:.1} °C"))
}

pub fn uptime_label(seconds: Option<u64>) -> String {
    let Some(seconds) = seconds else {
        return "НЕДОСТУПНО".into();
    };
    let days = seconds / 86_400;
    let hours = seconds % 86_400 / 3600;
    let minutes = seconds % 3600 / 60;
    if days > 0 {
        format!("{days}d {hours}h {minutes}m")
    } else {
        format!("{hours}h {minutes}m")
    }
}

pub fn load_label(load: Option<[f64; 3]>) -> String {
    load.filter(|values| values.iter().all(|n| n.is_finite() && *n >= 0.0))
        .map_or_else(
            || "НЕДОСТУПНО".into(),
            |values| format!("{:.2} / {:.2} / {:.2}", values[0], values[1], values[2]),
        )
}

pub fn power_label(battery: Option<u8>, charging: Option<bool>, measured: bool) -> String {
    if !measured {
        return "НЕДОСТУПНО".into();
    }
    let Some(level) = battery.filter(|v| *v <= 100) else {
        return "НЕ ОБНАРУЖЕНО".into();
    };
    let state = match charging {
        Some(true) => " / ЗАРЯДКА",
        Some(false) => " / РАЗРЯДКА",
        None => "",
    };
    format!("{level}%{state}")
}

fn labeled_row(title: &str) -> (gtk::Box, gtk::Label) {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.add_css_class("system-detail-row");
    let name = gtk::Label::new(Some(title));
    name.add_css_class("system-detail-name");
    name.set_xalign(0.0);
    name.set_hexpand(true);
    row.append(&name);
    let value = gtk::Label::new(Some("НЕДОСТУПНО"));
    value.add_css_class("system-detail-value");
    value.set_xalign(1.0);
    value.set_ellipsize(gtk::pango::EllipsizeMode::End);
    row.append(&value);
    (row, value)
}

struct DetailsState {
    cpu_temp: gtk::Label,
    gpu_list: gtk::Box,
    uptime: gtk::Label,
    load: gtk::Label,
    power: gtk::Label,
    revision: Cell<u64>,
}

impl DetailsState {
    fn reset(&self) {
        self.cpu_temp.set_text("НЕДОСТУПНО");
        self.uptime.set_text("НЕДОСТУПНО");
        self.load.set_text("НЕДОСТУПНО");
        self.power.set_text("НЕДОСТУПНО");
        while let Some(child) = self.gpu_list.first_child() {
            self.gpu_list.remove(&child);
        }
        let message = gtk::Label::new(Some("ДАННЫЕ GPU НЕДОСТУПНЫ"));
        message.add_css_class("muted");
        message.set_xalign(0.0);
        self.gpu_list.append(&message);
    }

    fn invalidate(&self) {
        self.revision.set(self.revision.get().wrapping_add(1));
        self.reset();
    }

    fn show_gpu(&self, readings: &[GpuReading]) {
        while let Some(child) = self.gpu_list.first_child() {
            self.gpu_list.remove(&child);
        }
        if readings.is_empty() {
            let label = gtk::Label::new(Some("GPU НЕ ОБНАРУЖЕН"));
            label.add_css_class("muted");
            label.set_xalign(0.0);
            self.gpu_list.append(&label);
            return;
        }
        for gpu in readings.iter().take(8) {
            let line = gtk::Box::new(gtk::Orientation::Horizontal, 9);
            line.add_css_class("system-gpu-row");
            let name = gtk::Label::new(Some(&gpu.name));
            name.set_xalign(0.0);
            name.set_hexpand(true);
            name.set_ellipsize(gtk::pango::EllipsizeMode::End);
            name.set_tooltip_text(Some(&gpu.name));
            line.append(&name);
            let utilization = gtk::Label::new(Some(&gpu_utilization(gpu.utilization_percent)));
            utilization.add_css_class("muted");
            line.append(&utilization);
            let temp = gtk::Label::new(Some(&gpu_temperature(gpu.temperature_c)));
            temp.add_css_class("system-gpu-temperature");
            line.append(&temp);
            self.gpu_list.append(&line);
        }
    }
}

/// Contains a native, keyboard-operable GTK Expander and stable readouts.
/// Its source TENET disclosure curve is still subject to graphical acceptance.
pub struct SystemDetails {
    root: gtk::Box,
    state: Rc<DetailsState>,
}

impl SystemDetails {
    pub fn new() -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.add_css_class("system-details");

        let hardware = gtk::Box::new(gtk::Orientation::Vertical, 8);
        hardware.add_css_class("system-hardware");
        let header = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        let title = gtk::Label::new(Some("ОБОРУДОВАНИЕ"));
        title.add_css_class("system-detail-heading");
        title.set_xalign(0.0);
        title.set_hexpand(true);
        header.append(&title);
        let label = gtk::Label::new(Some("CPU · ТЕМП"));
        label.add_css_class("system-detail-name");
        header.append(&label);
        let cpu_temp = gtk::Label::new(Some("НЕДОСТУПНО"));
        cpu_temp.add_css_class("system-detail-temperature");
        header.append(&cpu_temp);
        hardware.append(&header);

        let gpu_list = gtk::Box::new(gtk::Orientation::Vertical, 4);
        hardware.append(&gpu_list);
        root.append(&hardware);

        let disclosure = gtk::Expander::new(Some("СВЕДЕНИЯ О СИСТЕМЕ"));
        disclosure.add_css_class("system-disclosure");
        disclosure.set_expanded(false);
        let fields = gtk::Box::new(gtk::Orientation::Vertical, 4);
        fields.add_css_class("system-detail-fields");
        let (uptime_row, uptime) = labeled_row("ВРЕМЯ РАБОТЫ");
        let (load_row, load) = labeled_row("НАГРУЗКА / 1 5 15");
        let (power_row, power) = labeled_row("ПИТАНИЕ");
        fields.append(&uptime_row);
        fields.append(&load_row);
        fields.append(&power_row);
        disclosure.set_child(Some(&fields));
        root.append(&disclosure);

        let state = Rc::new(DetailsState {
            cpu_temp,
            gpu_list,
            uptime,
            load,
            power,
            revision: Cell::new(0),
        });
        state.reset();
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

    pub fn update(&self, overview: &SystemOverview) {
        let revision = self.state.revision.get().wrapping_add(1);
        self.state.revision.set(revision);
        self.state
            .cpu_temp
            .set_text(&cpu_temperature(overview.cpu_temperature_c));
        self.state.show_gpu(&overview.gpu);
        self.state
            .uptime
            .set_text(&uptime_label(overview.uptime_seconds));
        self.state.load.set_text(&load_label(overview.load));
        self.state.power.set_text(&power_label(
            overview.battery_percent,
            overview.battery_charging,
            true,
        ));

        let weak = Rc::downgrade(&self.state);
        gtk::glib::timeout_add_local_once(DETAILS_FRESHNESS, move || {
            if let Some(state) = weak.upgrade()
                && state.revision.get() == revision
            {
                state.invalidate();
            }
        });
    }
}

impl Default for SystemDetails {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temperature_and_gpu_missingness_are_explicit() {
        assert_eq!(cpu_temperature(None), "НЕДОСТУПНО");
        assert_eq!(cpu_temperature(Some(f64::NAN)), "НЕДОСТУПНО");
        assert_eq!(cpu_temperature(Some(48.25)), "48.2 °C");
        assert_eq!(gpu_utilization(Some(25)), "25%");
        assert_eq!(gpu_utilization(Some(101)), "НАГРУЗКА Н/Д");
        assert_eq!(gpu_utilization(None), "НАГРУЗКА Н/Д");
        assert_eq!(gpu_temperature(None), "ТЕМП Н/Д");
        assert_eq!(gpu_temperature(Some(55.375)), "55.4 °C");
    }

    #[test]
    fn uptime_load_and_battery_are_source_accurate() {
        assert_eq!(uptime_label(None), "НЕДОСТУПНО");
        assert_eq!(uptime_label(Some(0)), "0h 0m");
        assert_eq!(uptime_label(Some(90_061)), "1d 1h 1m");
        assert_eq!(load_label(Some([0.2, 1.2, 3.0])), "0.20 / 1.20 / 3.00");
        assert_eq!(load_label(Some([0.2, f64::NAN, 3.0])), "НЕДОСТУПНО");
        assert_eq!(power_label(None, None, false), "НЕДОСТУПНО");
        assert_eq!(power_label(None, None, true), "НЕ ОБНАРУЖЕНО");
        assert_eq!(power_label(Some(15), Some(false), true), "15% / РАЗРЯДКА");
        assert_eq!(power_label(Some(88), Some(true), true), "88% / ЗАРЯДКА");
        assert_eq!(power_label(Some(101), None, true), "НЕ ОБНАРУЖЕНО");
    }
}
