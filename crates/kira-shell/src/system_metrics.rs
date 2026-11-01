//! Read-only Linux system telemetry. No external commands, privileged APIs or
//! additional dependencies. Optional fields are absent when the kernel does
//! not provide a metric; lack of data must never be displayed as zero.
use std::fs;
use std::time::Duration;

#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub cpu_percent: Option<u8>,
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

fn cpu_ticks(text: &str) -> Option<(u64, u64)> {
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
