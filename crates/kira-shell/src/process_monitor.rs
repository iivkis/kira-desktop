//! Read-only Linux process accounting for the System Monitor.
//! CPU is shown as percentage of the *entire host*, not one logical core.
//! Sampling and /proc enumeration run only while Processes is the active tab.
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::Path;

use crate::system_metrics::cpu_ticks;

const MAX_SCANNED: usize = 4096;

#[derive(Debug, Clone)]
pub struct ProcessRow {
    pub pid: u32,
    pub name: String,
    pub state: char,
    pub cpu_percent: Option<f64>,
    pub rss_kib: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct ProcessSnapshot {
    pub rows: Vec<ProcessRow>,
    pub total: usize,
    pub truncated: bool,
}

#[derive(Debug, PartialEq)]
struct Stat {
    name: String,
    state: char,
    ticks: u64,
    start_ticks: u64,
}

/// /proc/[pid]/stat has a parenthesized comm field, potentially containing
/// spaces or parentheses. The final ')' marks its end, not split_whitespace.
fn parse_stat(input: &str) -> Option<Stat> {
    let begin = input.find('(')?;
    let end = input.rfind(')')?;
    if end <= begin {
        return None;
    }
    let name: String = input[begin + 1..end]
        .chars()
        .filter(|c| !c.is_control())
        .take(64)
        .collect();
    let fields: Vec<&str> = input[end + 1..].split_whitespace().collect();
    // After comm, the first field is field 3: state.
    if fields.len() < 20 {
        return None;
    }
    let state = fields[0].chars().next()?;
    let user: u64 = fields[11].parse().ok()?;
    let kernel: u64 = fields[12].parse().ok()?;
    Some(Stat {
        name,
        state,
        ticks: user.checked_add(kernel)?,
        start_ticks: fields[19].parse().ok()?,
    })
}

fn rss_kib(input: &str) -> Option<u64> {
    input.lines().find_map(|line| {
        line.strip_prefix("VmRSS:")?
            .split_whitespace()
            .next()?
            .parse::<u64>().ok()
    })
}

fn cpu_percent(prev: Option<u64>, current: u64, total_delta: Option<u64>) -> Option<f64> {
    let delta = current.checked_sub(prev?)?;
    let total = total_delta?;
    if total == 0 || delta > total {
        return None;
    }
    Some(100.0 * delta as f64 / total as f64)
}

/// Identity is (PID, kernel start time) so fast PID reuse cannot inherit
/// another process's past CPU ticks.
#[derive(Default)]
pub struct ProcessSampler {
    previous: HashMap<(u32, u64), u64>,
    previous_total: Option<u64>,
}

impl ProcessSampler {
    pub fn sample(&mut self) -> io::Result<ProcessSnapshot> {
        let total = cpu_ticks(&fs::read_to_string("/proc/stat")?)
            .map(|x| x.0);
        let total_delta = self.previous_total.zip(total)
            .and_then(|(before, after)| after.checked_sub(before));
        self.previous_total = total;
        let mut next = HashMap::new();
        let mut rows = Vec::new();
        let mut scanned = 0;
        let mut truncated = false;
        let entries = fs::read_dir("/proc")?;
        for entry in entries.flatten() {
            let Some(pid) = entry.file_name().to_str().and_then(|v| v.parse::<u32>().ok()) else {
                continue;
            };
            if scanned >= MAX_SCANNED {
                truncated = true;
                break;
            }
            scanned += 1;
            let path = entry.path();
            let Some(stat) = read_stat(&path) else {
                // Processes can exit during enumeration or be unreadable.
                continue;
            };
            let identity = (pid, stat.start_ticks);
            let usage = cpu_percent(self.previous.get(&identity).copied(), stat.ticks, total_delta);
            next.insert(identity, stat.ticks);
            let rss = fs::read_to_string(path.join("status"))
                .ok().and_then(|s| rss_kib(&s));
            rows.push(ProcessRow {
                pid,
                name: stat.name,
                state: stat.state,
                cpu_percent: usage,
                rss_kib: rss,
            });
        }
        self.previous = next;
        Ok(ProcessSnapshot { rows, total: scanned, truncated })
    }
}

fn read_stat(path: &Path) -> Option<Stat> {
    parse_stat(&fs::read_to_string(path.join("stat")).ok()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stat_line(name: &str, utime: u64, stime: u64, start: u64) -> String {
        let mut fields = vec!["0".to_string(); 20];
        fields[0] = "S".into();
        fields[11] = utime.to_string();
        fields[12] = stime.to_string();
        fields[19] = start.to_string();
        format!("42 ({name}) {}", fields.join(" "))
    }

    #[test]
    fn parse_process_names_with_spaces_and_parentheses() {
        let record = parse_stat(&stat_line("renderer (worker) extra)", 20, 5, 902)).unwrap();
        assert_eq!(record.name, "renderer (worker) extra)");
        assert_eq!(record.state, 'S');
        assert_eq!(record.ticks, 25);
        assert_eq!(record.start_ticks, 902);
        assert_eq!(parse_stat("42 (truncated"), None);
    }

    #[test]
    fn cpu_is_delta_of_total_host_and_never_invented_on_first_sample() {
        assert_eq!(cpu_percent(None, 120, Some(100)), None);
        assert_eq!(cpu_percent(Some(100), 150, Some(500)), Some(10.0));
        assert_eq!(cpu_percent(Some(200), 150, Some(500)), None);
        assert_eq!(cpu_percent(Some(100), 150, Some(0)), None);
        assert_eq!(cpu_percent(Some(100), 200, Some(50)), None);
    }

    #[test]
    fn rss_and_pid_identity_work_without_cmdline_access() {
        assert_eq!(rss_kib("Name:\ttest\nVmRSS:\t2048 kB\n"), Some(2048));
        assert_eq!(rss_kib("Name:\ttest\n"), None);
        let mut sampler = ProcessSampler::default();
        sampler.previous.insert((42, 111), 200);
        assert_eq!(sampler.previous.get(&(42, 222)), None);
    }

    #[test]
    fn malformed_stats_are_discarded() {
        assert_eq!(parse_stat(""), None);
        assert_eq!(parse_stat("42 () S x"), None);
        assert_eq!(parse_stat(&stat_line("test", u64::MAX, 1, 9)), None);
    }
}
