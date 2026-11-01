//! Read-only Linux process accounting for the System Monitor.
//! CPU is shown as percentage of the *entire host*, not one logical core.
//! Sampling and /proc enumeration run only while Processes is the active tab.
use std::collections::HashMap;
use std::fs;
use std::io::{self, Read};
use std::path::Path;

use crate::system_metrics::cpu_ticks;

const MAX_SCANNED: usize = 4096;
// smaps_rollup is more expensive than /proc/status. Refresh a bounded,
// rotating slice of processes each 2-second worker sample.
const MAX_PSS_READS_PER_SAMPLE: usize = 96;
const MAX_SMAPS_ROLLUP_BYTES: u64 = 16 * 1024;

#[derive(Debug, Clone)]
pub struct ProcessRow {
    pub pid: u32,
    pub name: String,
    pub state: char,
    pub cpu_percent: Option<f64>,
    pub rss_kib: Option<u64>,
    pub pss_kib: Option<u64>,
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
            .parse::<u64>()
            .ok()
    })
}

fn parse_pss_kib(text: &str) -> Option<u64> {
    text.lines().find_map(|line| {
        let mut parts = line.strip_prefix("Pss:")?.split_whitespace();
        let value = parts.next()?.parse::<u64>().ok()?;
        (parts.next()? == "kB").then_some(value)
    })
}

fn read_pss_kib(path: &Path) -> Option<u64> {
    let mut text = String::new();
    fs::File::open(path)
        .ok()?
        .take(MAX_SMAPS_ROLLUP_BYTES)
        .read_to_string(&mut text)
        .ok()?;
    parse_pss_kib(&text)
}

fn pss_refresh_indices(total: usize, cursor: usize) -> Vec<usize> {
    if total == 0 {
        return Vec::new();
    }
    (0..total.min(MAX_PSS_READS_PER_SAMPLE))
        .map(|offset| (cursor + offset) % total)
        .collect()
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
    pss_cache: HashMap<(u32, u64), Option<u64>>,
    pss_cursor: usize,
}

impl ProcessSampler {
    pub fn sample(&mut self) -> io::Result<ProcessSnapshot> {
        let total = cpu_ticks(&fs::read_to_string("/proc/stat")?).map(|x| x.0);
        let total_delta = self
            .previous_total
            .zip(total)
            .and_then(|(before, after)| after.checked_sub(before));
        self.previous_total = total;
        let mut next = HashMap::new();
        let mut collected = Vec::new();
        let mut scanned = 0;
        let mut truncated = false;
        let entries = fs::read_dir("/proc")?;
        for entry in entries.flatten() {
            let Some(pid) = entry
                .file_name()
                .to_str()
                .and_then(|v| v.parse::<u32>().ok())
            else {
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
            let usage = cpu_percent(
                self.previous.get(&identity).copied(),
                stat.ticks,
                total_delta,
            );
            next.insert(identity, stat.ticks);
            let rss = fs::read_to_string(path.join("status"))
                .ok()
                .and_then(|s| rss_kib(&s));
            collected.push((
                ProcessRow {
                    pid,
                    name: stat.name,
                    state: stat.state,
                    cpu_percent: usage,
                    rss_kib: rss,
                    pss_kib: None,
                },
                identity,
                path,
            ));
        }
        // A process is identified by PID plus kernel start ticks. Never
        // transfer stale PSS measurements across a fast PID reuse.
        self.pss_cache
            .retain(|identity, _| next.contains_key(identity));
        collected.sort_by_key(|(row, _, _)| row.pid);
        for index in pss_refresh_indices(collected.len(), self.pss_cursor) {
            let (_, identity, path) = &collected[index];
            let pss = read_pss_kib(&path.join("smaps_rollup"));
            self.pss_cache.insert(*identity, pss);
        }
        if !collected.is_empty() {
            self.pss_cursor =
                (self.pss_cursor + collected.len().min(MAX_PSS_READS_PER_SAMPLE)) % collected.len();
        }
        let rows = collected
            .into_iter()
            .map(|(mut row, identity, _)| {
                row.pss_kib = self.pss_cache.get(&identity).copied().flatten();
                row
            })
            .collect();
        self.previous = next;
        Ok(ProcessSnapshot {
            rows,
            total: scanned,
            truncated,
        })
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
    fn pss_is_parsed_only_from_exact_rollup_field() {
        assert_eq!(
            parse_pss_kib("Rss: 8000 kB\nPss: 1234 kB\nPss_Anon: 100 kB\n"),
            Some(1234)
        );
        assert_eq!(parse_pss_kib("Pss_Anon: 600 kB\n"), None);
        assert_eq!(parse_pss_kib("Pss: -1 kB\n"), None);
        assert_eq!(parse_pss_kib("Pss: 111 KiB\n"), None);
        assert_eq!(parse_pss_kib("Pss: 0 kB\n"), Some(0));
    }

    #[test]
    fn pss_scanning_is_bounded_and_fair() {
        assert!(pss_refresh_indices(0, 0).is_empty());
        assert_eq!(pss_refresh_indices(3, 0), vec![0, 1, 2]);
        let first = pss_refresh_indices(101, 0);
        let second = pss_refresh_indices(101, first.len());
        assert_eq!(first.len(), MAX_PSS_READS_PER_SAMPLE);
        assert!(second.contains(&96));
        assert!(second.contains(&100));
        assert!(second.contains(&0));
    }

    #[test]
    fn malformed_stats_are_discarded() {
        assert_eq!(parse_stat(""), None);
        assert_eq!(parse_stat("42 () S x"), None);
        assert_eq!(parse_stat(&stat_line("test", u64::MAX, 1, 9)), None);
    }
}
