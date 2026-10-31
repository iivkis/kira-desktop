//! Strict, deliberately small KDL configuration profile for the first MVP.
//! No arbitrary commands, includes, environment interpolation or KDL actions.
//! A rejected update leaves the last successfully parsed configuration active.

use std::collections::HashSet;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::Duration;

use kira_ipc::{ConfigState, UiConfig};
use tokio::sync::watch;

const MAX_CONFIG_BYTES: u64 = 64 * 1024;
const MAX_LINES: usize = 128;

/// No user configuration file is created or overwritten by the daemon.
/// KIRA_CONFIG_FILE must be absolute, including in the isolated preview.
pub fn config_path() -> Result<PathBuf, String> {
    if let Some(override_path) = std::env::var_os("KIRA_CONFIG_FILE") {
        let path = PathBuf::from(override_path);
        return if path.is_absolute() {
            Ok(path)
        } else {
            Err("KIRA_CONFIG_FILE must be an absolute path".into())
        };
    }
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        let path = PathBuf::from(xdg);
        return if path.is_absolute() {
            Ok(path.join("kira-desktop/config.kdl"))
        } else {
            Err("XDG_CONFIG_HOME must be an absolute path".into())
        };
    }
    let home = std::env::var_os("HOME").ok_or("HOME is not set")?;
    let home = PathBuf::from(home);
    if !home.is_absolute() {
        return Err("HOME must be an absolute path".into());
    }
    Ok(home.join(".config/kira-desktop/config.kdl"))
}

/// Truncate comments only outside quoted strings; values may contain "//".
fn strip_comment(line: &str) -> &str {
    let mut quoted = false;
    let mut escaped = false;
    let mut previous_slash = None;
    for (index, ch) in line.char_indices() {
        if quoted {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                quoted = false;
            }
        } else if ch == '"' {
            quoted = true;
        } else if ch == '/' {
            if let Some(start) = previous_slash {
                return &line[..start];
            }
            previous_slash = Some(index);
            continue;
        }
        previous_slash = None;
    }
    line
}

/// Tokens are node names and key=value properties. Quoted JSON-style strings
/// may contain spaces, backslashes and "=", but nodes remain one per line.
fn line_tokens(line: &str) -> Result<Vec<String>, String> {
    let mut result = Vec::new();
    let mut token = String::new();
    let mut quoted = false;
    let mut escaped = false;
    for ch in line.chars() {
        if quoted {
            token.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                quoted = false;
            }
        } else if ch == '"' {
            quoted = true;
            token.push(ch);
        } else if ch.is_whitespace() {
            if !token.is_empty() {
                result.push(std::mem::take(&mut token));
            }
        } else {
            token.push(ch);
        }
    }
    if quoted {
        return Err("unterminated quoted string".into());
    }
    if !token.is_empty() {
        result.push(token);
    }
    Ok(result)
}

fn boolean(value: &str, key: &str) -> Result<bool, String> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(format!("{key} must be true or false")),
    }
}

/// Parse only the documented, valid KDL subset. Reject unknown nodes,
/// duplicate properties and unsafe or unsupported syntax instead of guessing.
pub fn parse(input: &str) -> Result<UiConfig, String> {
    if input.len() as u64 > MAX_CONFIG_BYTES {
        return Err("config exceeds 64 KiB".into());
    }
    if input.lines().count() > MAX_LINES {
        return Err("config exceeds 128 lines".into());
    }

    let mut config = UiConfig::default();
    let mut root_open = false;
    let mut root_closed = false;
    let mut seen_nodes = HashSet::new();

    for (number, raw_line) in input.lines().enumerate() {
        let line = strip_comment(raw_line).trim();
        if line.is_empty() {
            continue;
        }
        if !root_open {
            if line != "kira {" {
                return Err(format!("line {}: expected 'kira {{'", number + 1));
            }
            root_open = true;
            continue;
        }
        if root_closed {
            return Err(format!("line {}: unexpected content after kira block", number + 1));
        }
        if line == "}" {
            root_closed = true;
            continue;
        }

        let tokens = line_tokens(line).map_err(|e| format!("line {}: {e}", number + 1))?;
        let node = tokens.first().map(String::as_str).unwrap_or("");
        if !matches!(node, "bar" | "clock" | "workspaces") {
            return Err(format!("line {}: unknown KDL node '{node}'", number + 1));
        }
        if !seen_nodes.insert(node.to_owned()) {
            return Err(format!("line {}: duplicate node '{node}'", number + 1));
        }
        let mut seen_properties = HashSet::new();
        for prop in tokens.iter().skip(1) {
            let Some((key, raw_value)) = prop.split_once('=') else {
                return Err(format!("line {}: expected key=value property", number + 1));
            };
            if key.is_empty() || raw_value.is_empty() {
                return Err(format!("line {}: empty property name or value", number + 1));
            }
            if !seen_properties.insert(key) {
                return Err(format!("line {}: duplicate property '{key}'", number + 1));
            }
            let key_path = format!("{node}.{key}");
            match (node, key) {
                ("bar", "compact") => config.compact = boolean(raw_value, &key_path)?,
                ("bar", "title") => {
                    let title: String = serde_json::from_str(raw_value)
                        .map_err(|_| format!("line {}: bar.title must be a quoted string", number + 1))?;
                    if title.is_empty() || title.chars().count() > 32 ||
                        title.chars().any(char::is_control)
                    {
                        return Err(format!("line {}: bar.title must contain 1–32 printable characters", number + 1));
                    }
                    config.title = title;
                }
                ("clock", "seconds") => config.show_seconds = boolean(raw_value, &key_path)?,
                ("workspaces", "show-names") => config.show_names = boolean(raw_value, &key_path)?,
                ("workspaces", "limit") => {
                    let limit = raw_value.parse::<u8>()
                        .map_err(|_| format!("line {}: workspaces.limit must be an integer", number + 1))?;
                    if !(1..=32).contains(&limit) {
                        return Err(format!("line {}: workspaces.limit must be 1–32", number + 1));
                    }
                    config.max_workspaces = limit;
                }
                _ => return Err(format!("line {}: unsupported property '{key_path}'", number + 1)),
            }
        }
    }
    if !root_open || !root_closed {
        return Err("expected one closed kira { ... } block".into());
    }
    Ok(config)
}

pub fn load(path: &Path) -> Result<UiConfig, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("cannot read config: {e}"))?;
    let mut bytes = Vec::new();
    file.take(MAX_CONFIG_BYTES + 1).read_to_end(&mut bytes)
        .map_err(|e| format!("cannot read config: {e}"))?;
    if bytes.len() as u64 > MAX_CONFIG_BYTES {
        return Err("config exceeds 64 KiB".into());
    }
    let text = std::str::from_utf8(&bytes).map_err(|_| "config must be UTF-8".to_string())?;
    parse(text)
}

fn apply(current: &ConfigState, latest: Result<UiConfig, String>) -> ConfigState {
    match latest {
        Ok(config) => ConfigState { config, error: None },
        Err(error) => ConfigState {
            config: current.config.clone(),
            error: Some(error),
        },
    }
}

/// Poll the small user-owned file at most four times per second.
/// Compare contents/state and publish only changes; atomic rename is detected.
/// Missing file on first startup means defaults. Removing a previously valid
/// file is treated as an error and retains the last valid state.
pub async fn monitor(path: PathBuf, sender: watch::Sender<ConfigState>) {
    let mut initialized = false;
    let mut previous_contents: Option<Result<Vec<u8>, io::ErrorKind>> = None;
    loop {
        let current = std::fs::read(&path)
            .map_err(|error| error.kind())
            .and_then(|bytes| {
                if bytes.len() as u64 > MAX_CONFIG_BYTES {
                    Err(io::ErrorKind::InvalidData)
                } else {
                    Ok(bytes)
                }
            });

        // Don't reparse unchanged contents, but still catch atomic replacements.
        let changed = match (&previous_contents, &current) {
            (Some(Ok(old)), Ok(new)) => old != new,
            (Some(Err(old)), Err(new)) => old != new,
            _ => true,
        };
        if changed {
            let state = if !initialized && matches!(&current, Err(io::ErrorKind::NotFound)) {
                ConfigState::default()
            } else {
                let parsed = match &current {
                    Ok(bytes) => std::str::from_utf8(bytes)
                        .map_err(|_| "config must be UTF-8".to_owned())
                        .and_then(parse),
                    Err(io::ErrorKind::InvalidData) => Err("config exceeds 64 KiB".into()),
                    Err(error) => Err(format!("cannot read config: {error}")),
                };
                apply(&sender.borrow().clone(), parsed)
            };
            if *sender.borrow() != state {
                if let Some(error) = &state.error {
                    eprintln!("kira-core config: {error}; keeping previous valid settings");
                }
                sender.send_replace(state);
            }
            previous_contents = Some(current);
            initialized = true;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const EXAMPLE: &str = include_str!("../../../config/kira.kdl.example");

    #[test]
    fn example_parses_and_uses_defaults() {
        assert_eq!(parse(EXAMPLE).unwrap(), UiConfig::default());
        assert_eq!(parse("kira {\n  bar compact=true title=\"KIRA // TEST\"\n  clock seconds=true\n  workspaces show-names=false limit=3\n}\n").unwrap(),
            UiConfig { compact: true, title: "KIRA // TEST".into(), show_seconds: true,
                show_names: false, max_workspaces: 3 });
    }

    #[test]
    fn invalid_config_cannot_override_previous_valid_state() {
        let initial = ConfigState { config: parse("kira {\n  bar compact=true\n}\n").unwrap(), error: None };
        for broken in [
            "kira {\n  bar compact=false\n  bar compact=true\n}",
            "kira {\n  workspaces limit=0\n}",
            "kira {\n  clock seconds=yes\n}",
            "kira {\n  bar title=\"\"\n}",
            "kira {\n  bar title=\"bad\\nnewline\"\n}",
            "kira {\n  exec command=\"rm -rf\"\n}",
            "kira {\n  bar compact=true compact=false\n}",
            "kira {\n  workspaces limit=99\n}",
            "kira {\n  clock seconds=true",
            "kira { }\n",
        ] {
            assert!(parse(broken).is_err(), "{broken}");
            let next = apply(&initial, parse(broken));
            assert_eq!(next.config, initial.config);
            assert!(next.error.is_some());
        }
        assert!(apply(&initial, parse(EXAMPLE)).error.is_none());
    }

    #[tokio::test]
    async fn hot_reload_rejects_invalid_and_recovers_from_atomic_replace() {
        let directory = std::env::temp_dir().join(format!("kira-kdl-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("config.kdl");
        let (tx, mut rx) = watch::channel(ConfigState::default());
        let task = tokio::spawn(monitor(path.clone(), tx));
        std::fs::write(&path, "kira {\n  clock seconds=true\n}\n").unwrap();
        tokio::time::timeout(Duration::from_secs(2), rx.changed()).await.unwrap().unwrap();
        assert!(rx.borrow().config.show_seconds);
        std::fs::write(&path, "kira {\n  clock seconds=bad\n}\n").unwrap();
        tokio::time::timeout(Duration::from_secs(2), rx.changed()).await.unwrap().unwrap();
        assert!(rx.borrow().config.show_seconds);
        assert!(rx.borrow().error.is_some());
        let staged = directory.join("pending.kdl");
        std::fs::write(&staged, "kira {\n  clock seconds=false\n}\n").unwrap();
        std::fs::rename(staged, &path).unwrap();
        tokio::time::timeout(Duration::from_secs(2), rx.changed()).await.unwrap().unwrap();
        assert!(!rx.borrow().config.show_seconds);
        assert!(rx.borrow().error.is_none());
        task.abort();
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }

    #[test]
    fn relative_path_never_falls_back_to_host_config() {
        assert!(Path::new("config.kdl").is_relative());
    }
}
