//! Bounded, read-only Linux host identity for Settings / About System.
//! No shell commands, external processes, privileged files or browser input.
use std::io::Read;

fn read_text(path: &str, limit: u64) -> Option<String> {
    let file = std::fs::File::open(path).ok()?;
    let mut text = String::new();
    file.take(limit).read_to_string(&mut text).ok()?;
    Some(text)
}

fn os_name(text: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let value = line.strip_prefix("PRETTY_NAME=")?;
        let name = value.trim().trim_matches('"').trim();
        (!name.is_empty()).then(|| name.chars().take(120).collect())
    })
}

fn cpu_name(text: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        if key.trim() != "model name" && key.trim() != "Hardware" && key.trim() != "Processor" {
            return None;
        }
        let name = value.trim();
        (!name.is_empty()).then(|| name.chars().take(140).collect())
    })
}

fn memory_total(text: &str) -> Option<u64> {
    let line = text.lines().find(|line| line.starts_with("MemTotal:"))?;
    line.split_whitespace().nth(1)?.parse::<u64>().ok()
}

fn single_line(value: Option<String>, limit: usize) -> Option<String> {
    value
        .map(|s| s.lines().next().unwrap_or_default().trim().chars().take(limit).collect::<String>())
        .filter(|s| !s.is_empty())
}

/// Capture once when Settings opens. A missing kernel field stays null,
/// never a fabricated operating system, CPU, memory amount or version.
pub fn about_snapshot() -> serde_json::Value {
    let os = read_text("/etc/os-release", 8 * 1024).and_then(|s| os_name(&s));
    let cpu = read_text("/proc/cpuinfo", 128 * 1024).and_then(|s| cpu_name(&s));
    let memory = read_text("/proc/meminfo", 16 * 1024).and_then(|s| memory_total(&s));
    let kernel = single_line(read_text("/proc/sys/kernel/osrelease", 256), 120);
    let hostname = single_line(read_text("/proc/sys/kernel/hostname", 256), 120);
    let session = single_line(std::env::var("XDG_SESSION_TYPE").ok(), 24);

    serde_json::json!({
        "os_name": os,
        "kernel": kernel,
        "architecture": std::env::consts::ARCH,
        "hostname": hostname,
        "cpu_model": cpu,
        "cpu_threads": std::thread::available_parallelism().ok().map(|n| n.get()),
        "memory_total_kib": memory,
        "session": session,
        "shell_version": env!("CARGO_PKG_VERSION"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn os_release_parses_only_explicit_pretty_name() {
        assert_eq!(os_name("NAME=Example\nPRETTY_NAME=\"Example Linux 1\"\n"),
                   Some("Example Linux 1".to_string()));
        assert_eq!(os_name("NAME=Example\n"), None);
        assert_eq!(os_name("PRETTY_NAME=\"\"\n"), None);
    }

    #[test]
    fn cpu_and_memory_missingness_are_truthful() {
        assert_eq!(cpu_name("processor: 0\nmodel name : Sample CPU\n"),
                   Some("Sample CPU".to_string()));
        assert_eq!(cpu_name("processor: 0\n"), None);
        assert_eq!(memory_total("MemTotal:       16384 kB\nMemFree: 123 kB\n"), Some(16384));
        assert_eq!(memory_total("MemFree: 123 kB\n"), None);
        assert_eq!(memory_total("MemTotal: unknown kB\n"), None);
    }

    #[test]
    fn snapshot_has_only_read_only_bounded_fields() {
        let value = about_snapshot();
        assert!(value["shell_version"].is_string());
        assert!(value["architecture"].is_string());
        for field in ["hostname", "os_name", "kernel", "cpu_model", "session"] {
            assert!(value[field].is_null() || value[field].as_str().is_some_and(|s| s.len() <= 600));
        }
    }
}
