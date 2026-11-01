//! Read-only Linux system telemetry. No external commands, privileged APIs or
//! additional dependencies. Optional fields are absent when the kernel does
//! not provide a metric; lack of data must never be displayed as zero.
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct GpuSnapshot {
    pub label: String,
    pub utilization_percent: Option<u8>,
    pub temperature_c: Option<f64>,
}

#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub cpu_percent: Option<u8>,
    pub cpu_temperature_c: Option<f64>,
    pub gpus: Vec<GpuSnapshot>,
    pub memory_total_kib: Option<u64>,
    pub memory_available_kib: Option<u64>,
    pub swap_total_kib: Option<u64>,
    pub swap_free_kib: Option<u64>,
    pub uptime_seconds: Option<u64>,
    pub load: Option<[f64; 3]>,
    pub battery_percent: Option<u8>,
    pub battery_charging: Option<bool>,
    pub root_total_kib: Option<u64>,
    pub root_available_kib: Option<u64>,
}

impl Snapshot {
    pub fn as_json(&self) -> serde_json::Value {
        serde_json::json!({
            "cpu_percent": self.cpu_percent,
            "cpu_temperature_c": self.cpu_temperature_c,
            "gpus": self.gpus.iter().map(|gpu| serde_json::json!({
                "label": gpu.label,
                "utilization_percent": gpu.utilization_percent,
                "temperature_c": gpu.temperature_c,
            })).collect::<Vec<_>>(),
            "memory_total_kib": self.memory_total_kib,
            "memory_available_kib": self.memory_available_kib,
            "swap_total_kib": self.swap_total_kib,
            "swap_free_kib": self.swap_free_kib,
            "uptime_seconds": self.uptime_seconds,
            "load": self.load,
            "battery_percent": self.battery_percent,
            "battery_charging": self.battery_charging,
            "root_total_kib": self.root_total_kib,
            "root_available_kib": self.root_available_kib,
        })
    }

    pub fn memory_percent(&self) -> Option<u8> {
        percent_used(self.memory_total_kib?, self.memory_available_kib?)
    }
}

pub fn percent_used(total: u64, available: u64) -> Option<u8> {
    if total == 0 || available > total {
        return None;
    }
    let used = total - available;
    Some(((u128::from(used) * 100 / u128::from(total)).min(100)) as u8)
}

fn mem_value(text: &str, key: &str) -> Option<u64> {
    let line = text
        .lines()
        .find(|line| line.starts_with(key) && line.as_bytes().get(key.len()) == Some(&b':'))?;
    line.split_whitespace().nth(1)?.parse().ok()
}

fn parse_memory(text: &str, snapshot: &mut Snapshot) {
    snapshot.memory_total_kib = mem_value(text, "MemTotal");
    snapshot.memory_available_kib = mem_value(text, "MemAvailable");
    snapshot.swap_total_kib = mem_value(text, "SwapTotal");
    snapshot.swap_free_kib = mem_value(text, "SwapFree");
}

pub(crate) fn cpu_ticks(text: &str) -> Option<(u64, u64)> {
    let line = text.lines().next()?;
    let mut tokens = line.split_whitespace();
    if tokens.next()? != "cpu" {
        return None;
    }
    let ticks = tokens
        .map(str::parse::<u64>)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    if ticks.len() < 4 {
        return None;
    }
    let total = ticks.iter().copied().sum::<u64>();
    let idle = ticks[3].checked_add(*ticks.get(4).unwrap_or(&0))?;
    Some((total, idle))
}

fn cpu_usage(previous: (u64, u64), next: (u64, u64)) -> Option<u8> {
    let total = next.0.checked_sub(previous.0)?;
    let idle = next.1.checked_sub(previous.1)?;
    if total == 0 || idle > total {
        return None;
    }
    Some(
        (((total - idle) as f64 / total as f64) * 100.0)
            .round()
            .clamp(0.0, 100.0) as u8,
    )
}

fn parse_load(text: &str) -> Option<[f64; 3]> {
    let mut words = text.split_whitespace();
    let vals = [
        words.next()?.parse::<f64>().ok()?,
        words.next()?.parse::<f64>().ok()?,
        words.next()?.parse::<f64>().ok()?,
    ];
    vals.iter()
        .all(|v| v.is_finite() && *v >= 0.0)
        .then_some(vals)
}

fn battery() -> (Option<u8>, Option<bool>) {
    let Ok(entries) = fs::read_dir("/sys/class/power_supply") else {
        return (None, None);
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let kind = fs::read_to_string(path.join("type")).unwrap_or_default();
        if kind.trim() != "Battery" {
            continue;
        }
        let percentage = fs::read_to_string(path.join("capacity"))
            .ok()
            .and_then(|s| s.trim().parse::<u8>().ok())
            .filter(|v| *v <= 100);
        let charging = fs::read_to_string(path.join("status"))
            .ok()
            .and_then(|status| match status.trim() {
                "Charging" | "Full" => Some(true),
                "Not charging" => None,
                "Discharging" => Some(false),
                _ => None,
            });
        return (percentage, charging);
    }
    (None, None)
}

// sysfs is kernel-provided; optional sensors must never become zero by default.
// GPU utilization is driver-dependent (e.g. AMD gpu_busy_percent).
const MAX_GPUS: usize = 8;
const MAX_HWMON_SENSORS: usize = 32;

fn read_small(path: &Path) -> Option<String> {
    use std::io::Read;
    let mut bytes = Vec::new();
    fs::File::open(path)
        .ok()?
        .take(4096)
        .read_to_end(&mut bytes)
        .ok()?;
    String::from_utf8(bytes).ok()
}

fn temperature_c(text: &str) -> Option<f64> {
    let millidegrees = text.trim().parse::<i64>().ok()?;
    (-40_000..=150_000)
        .contains(&millidegrees)
        .then_some(millidegrees as f64 / 1000.0)
}

fn utilization(text: &str) -> Option<u8> {
    text.trim().parse::<u8>().ok().filter(|p| *p <= 100)
}

fn hwmon_temperature(root: &Path, cpu: bool) -> Option<f64> {
    let entries = fs::read_dir(root).ok()?;
    for directory in entries.flatten() {
        if directory.file_name().to_string_lossy().starts_with("hwmon")
            && let Some(value) = hwmon_temperature_for_device(&directory.path(), cpu)
        {
            return Some(value);
        }
    }
    None
}

fn hwmon_temperature_for_device(path: &Path, cpu: bool) -> Option<f64> {
    let mut sensors: Vec<(u8, f64)> = Vec::new();
    for index in 1..=MAX_HWMON_SENSORS {
        let input = path.join(format!("temp{index}_input"));
        let Some(value) = read_small(&input).and_then(|text| temperature_c(&text)) else {
            continue;
        };
        let label = read_small(&path.join(format!("temp{index}_label")))
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        // CPU: prefer package/die readings. GPU: prefer the edge sensor
        // over hotspot/junction/VRAM readings on multi-sensor cards.
        let priority = if cpu {
            if label.starts_with("package") || label == "tdie" || label == "cpu" {
                0
            } else if label == "tctl" || label.starts_with("core ") {
                1
            } else {
                2
            }
        } else if label == "edge" || label == "gpu" {
            0
        } else if label.contains("junction") || label.contains("hotspot") {
            1
        } else if label.contains("mem") {
            3
        } else {
            2
        };
        sensors.push((priority, value));
    }
    sensors.sort_by_key(|sensor| sensor.0);
    sensors.first().map(|sensor| sensor.1)
}

fn cpu_temperature(sys: &Path) -> Option<f64> {
    if let Ok(entries) = fs::read_dir(sys.join("class/hwmon")) {
        for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = read_small(&path.join("name")) else {
            continue;
        };
            if matches!(
                name.trim(),
                "coretemp" | "k10temp" | "zenpower" | "cpu_thermal" | "cpu-thermal"
            ) && let Some(temp) = hwmon_temperature_for_device(&path, true)
            {
                return Some(temp);
            }
        }
    }
    // Names of thermal zones matter. A generic ACPI/SoC temperature should
    // not be mislabeled as CPU package temperature.
    let zones = fs::read_dir(sys.join("class/thermal")).ok()?;
    for entry in zones.flatten() {
        if !entry
            .file_name()
            .to_string_lossy()
            .starts_with("thermal_zone")
        {
            continue;
        }
        let path = entry.path();
        let name = read_small(&path.join("type")).unwrap_or_default();
        if matches!(name.trim(), "x86_pkg_temp" | "cpu-thermal" | "cpu_thermal")
            && let Some(temp) = read_small(&path.join("temp")).and_then(|s| temperature_c(&s))
        {
            return Some(temp);
        }
    }
    None
}

fn gpu_label(card: &str, vendor: &str, uevent: &str) -> String {
    let manufacturer = match vendor.trim() {
        "0x8086" => "Intel",
        "0x1002" => "AMD",
        "0x10de" => "NVIDIA",
        "0x1af4" => "Virtio",
        _ => "Unknown",
    };
    // PCI 00:02.0 is the standard integrated Intel graphics device.
    // For other GPUs do not guess integrated vs discrete from vendor.
    let integrated = vendor.trim() == "0x8086"
        && uevent
            .lines()
            .any(|line| line == "PCI_SLOT_NAME=0000:00:02.0");
    let kind = if integrated { "iGPU" } else { "GPU" };
    format!("{kind} / {manufacturer} ({card})")
}

fn sample_gpus(sys: &Path) -> Vec<GpuSnapshot> {
    let Ok(entries) = fs::read_dir(sys.join("class/drm")) else {
        return Vec::new();
    };
    let mut cards: Vec<(String, PathBuf)> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_str()?.to_owned();
            name.strip_prefix("card")?.parse::<u8>().ok()?;
            let device = entry.path().join("device");
            device.exists().then_some((name, device))
        })
        .collect();
    cards.sort_by(|a, b| a.0.cmp(&b.0));
    cards
        .into_iter()
        .take(MAX_GPUS)
        .map(|(card, device)| {
            let vendor = read_small(&device.join("vendor")).unwrap_or_default();
            let uevent = read_small(&device.join("uevent")).unwrap_or_default();
            let utilization_percent = [
                device.join("gpu_busy_percent"),
                device.join("gt_busy_percent"),
                device.join("gt/gt0/gt_busy_percent"),
            ]
            .iter()
            .find_map(|path| read_small(path).and_then(|s| utilization(&s)));
            GpuSnapshot {
                label: gpu_label(&card, &vendor, &uevent),
                utilization_percent,
                temperature_c: hwmon_temperature(&device.join("hwmon"), false),
            }
        })
        .collect()
}

#[derive(Default)]
pub struct Sampler {
    previous_cpu: Option<(u64, u64)>,
}

impl Sampler {
    pub fn sample(&mut self) -> Snapshot {
        let mut s = Snapshot::default();
        if let Ok(text) = fs::read_to_string("/proc/stat")
            && let Some(next) = cpu_ticks(&text)
        {
            s.cpu_percent = self.previous_cpu.and_then(|prev| cpu_usage(prev, next));
            self.previous_cpu = Some(next);
        }
        if let Ok(text) = fs::read_to_string("/proc/meminfo") {
            parse_memory(&text, &mut s);
        }
        if let Ok(text) = fs::read_to_string("/proc/loadavg") {
            s.load = parse_load(&text);
        }
        if let Ok(text) = fs::read_to_string("/proc/uptime") {
            s.uptime_seconds = text
                .split_whitespace()
                .next()
                .and_then(|value| value.parse::<f64>().ok())
                .filter(|v| v.is_finite() && *v >= 0.0)
                .map(|v| Duration::from_secs_f64(v).as_secs());
        }
        (s.battery_percent, s.battery_charging) = battery();
        s.cpu_temperature_c = cpu_temperature(Path::new("/sys"));
        s.gpus = sample_gpus(Path::new("/sys"));
        s
    }
}

pub fn bytes_label(kib: u64) -> String {
    let gib = kib as f64 / 1_048_576.0;
    if gib >= 1.0 {
        format!("{gib:.1} GiB")
    } else {
        format!("{:.0} MiB", kib as f64 / 1024.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_ratio_is_bounded_and_not_fabricated() {
        assert_eq!(percent_used(1000, 90), Some(91));
        assert_eq!(percent_used(0, 0), None);
        assert_eq!(percent_used(100, 101), None);
        let mut s = Snapshot::default();
        parse_memory(
            "MemTotal: 16384 kB\nMemAvailable: 4096 kB\nSwapTotal: 1024 kB\nSwapFree: 512 kB\n",
            &mut s,
        );
        assert_eq!(s.memory_percent(), Some(75));
        assert_eq!(s.swap_free_kib, Some(512));
    }

    #[test]
    fn cpu_uses_differences_not_lifetime_average() {
        assert_eq!(cpu_ticks("cpu  10 10 10 70 0 0 0 0\n"), Some((100, 70)));
        assert_eq!(cpu_usage((100, 70), (200, 130)), Some(40));
        assert_eq!(cpu_usage((100, 70), (100, 70)), None);
        assert_eq!(cpu_usage((100, 70), (90, 60)), None);
    }

    #[test]
    fn sensor_parsers_reject_invalid_or_missing_measurements() {
        assert_eq!(temperature_c("50250\n"), Some(50.25));
        assert_eq!(temperature_c("151000"), None);
        assert_eq!(temperature_c("n/a"), None);
        assert_eq!(utilization("83\n"), Some(83));
        assert_eq!(utilization("101"), None);
        assert_eq!(utilization("-1"), None);
        assert_eq!(
            gpu_label("card0", "0x8086", "PCI_SLOT_NAME=0000:00:02.0\n"),
            "iGPU / Intel (card0)"
        );
        assert_eq!(
            gpu_label("card1", "0x1002", "PCI_SLOT_NAME=0000:03:00.0\n"),
            "GPU / AMD (card1)"
        );
    }

    #[test]
    fn cpu_thermal_zone_is_used_without_hwmon() {
        let root = std::env::temp_dir().join(format!(
            "kira-cpu-thermal-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let zone = root.join("class/thermal/thermal_zone0");
        fs::create_dir_all(&zone).unwrap();
        fs::write(zone.join("type"), "x86_pkg_temp\n").unwrap();
        fs::write(zone.join("temp"), "48250\n").unwrap();
        assert_eq!(cpu_temperature(&root), Some(48.25));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn gpu_and_cpu_sensors_are_optional_and_device_scoped() {
        let root = std::env::temp_dir().join(format!(
            "kira-sys-hwmon-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let hwmon = root.join("class/hwmon/hwmon0");
        let gpu = root.join("class/drm/card1/device");
        fs::create_dir_all(&hwmon).unwrap();
        fs::create_dir_all(gpu.join("hwmon/hwmon2")).unwrap();
        fs::write(hwmon.join("name"), "k10temp\n").unwrap();
        fs::write(hwmon.join("temp1_label"), "Tctl\n").unwrap();
        fs::write(hwmon.join("temp1_input"), "61125\n").unwrap();
        fs::write(gpu.join("vendor"), "0x1002\n").unwrap();
        fs::write(gpu.join("gpu_busy_percent"), "47\n").unwrap();
        fs::write(gpu.join("hwmon/hwmon2/temp1_label"), "edge\n").unwrap();
        fs::write(gpu.join("hwmon/hwmon2/temp1_input"), "55375\n").unwrap();
        fs::write(gpu.join("hwmon/hwmon2/temp2_label"), "junction\n").unwrap();
        fs::write(gpu.join("hwmon/hwmon2/temp2_input"), "79900\n").unwrap();
        assert_eq!(cpu_temperature(&root), Some(61.125));
        let cards = sample_gpus(&root);
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].label, "GPU / AMD (card1)");
        assert_eq!(cards[0].utilization_percent, Some(47));
        assert_eq!(cards[0].temperature_c, Some(55.375));
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn unavailable_or_malformed_data_is_unknown() {
        assert_eq!(parse_load("bad data"), None);
        assert_eq!(
            parse_load("0.23 1.02 2.33 4/5 111"),
            Some([0.23, 1.02, 2.33])
        );
        assert_eq!(cpu_ticks("cpu 10 foo"), None);
        assert_eq!(mem_value("MemTotal: x kB", "MemTotal"), None);
    }
}
