//! XDG desktop-entry discovery and deliberate launch by catalog ID.
//! No paths, Exec strings or shell commands are accepted from the UI.
use std::collections::HashSet;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use kira_ipc::DesktopApplication;
use serde_json::Value;

const MAX_APPS: usize = 180;
const MAX_FILES: usize = 4096;
const MAX_FILE_SIZE: u64 = 64 * 1024;
const MAX_DEPTH: usize = 6;

pub fn data_dirs() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let user_data = std::env::var_os("XDG_DATA_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| home.map(|path| path.join(".local/share")));
    if let Some(dir) = user_data.filter(|p| p.is_absolute()) {
        roots.push(dir.join("applications"));
    }
    let data_dirs = std::env::var_os("XDG_DATA_DIRS")
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| OsString::from("/usr/local/share:/usr/share"));
    for dir in std::env::split_paths(&data_dirs) {
        if dir.is_absolute() {
            roots.push(dir.join("applications"));
        }
    }
    roots
}

fn language_preferences() -> Vec<String> {
    let locales = ["LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|key| std::env::var(key).ok())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    let mut preferred = Vec::new();
    for locale in locales.join(":").split(':') {
        let locale = locale.split('.').next().unwrap_or("");
        let locale = locale.split('@').next().unwrap_or("");
        if locale == "C" || locale == "POSIX" || locale.is_empty() {
            continue;
        }
        let locale = locale.replace('-', "_");
        if !preferred.contains(&locale) {
            preferred.push(locale.clone());
        }
        if let Some((language, _)) = locale.split_once('_') {
            if !preferred.contains(&language.to_string()) {
                preferred.push(language.to_string());
            }
        }
    }
    preferred
}

fn desktop_id(root: &Path, file: &Path) -> Option<String> {
    let relative = file.strip_prefix(root).ok()?;
    let id = relative.to_str()?.replace('/', "-");
    if id.len() > 100 || !id.ends_with(".desktop") || id.chars().any(char::is_control) {
        return None;
    }
    Some(id)
}

fn bool_field(value: Option<&str>) -> bool {
    value.is_some_and(|value| value.eq_ignore_ascii_case("true"))
}

fn unescape_name(input: &str) -> String {
    let mut result = String::new();
    let mut chars = input.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('s') => result.push(' '),
                Some('n') => result.push(' '),
                Some('t') => result.push(' '),
                Some('\\') => result.push('\\'),
                Some(other) => {
                    result.push('\\');
                    result.push(other);
                }
                None => result.push('\\'),
            }
        } else if !c.is_control() {
            result.push(c);
        }
    }
    result.trim().to_owned()
}

fn limited_text(input: &str, bytes: usize) -> String {
    let end = input
        .char_indices()
        .map(|(i, c)| i + c.len_utf8())
        .take_while(|end| *end <= bytes)
        .last()
        .unwrap_or(0);
    input[..end].to_string()
}

fn executable_exists(command: &str, path_env: Option<&OsStr>) -> bool {
    if command.is_empty() || command.chars().any(char::is_control) {
        return false;
    }
    let executable = Path::new(command);
    let candidates = if executable.is_absolute() {
        vec![executable.to_path_buf()]
    } else if command.contains('/') {
        return false;
    } else {
        path_env
            .map(std::env::split_paths)
            .into_iter()
            .flatten()
            .filter(|dir| dir.is_absolute())
            .map(|dir| dir.join(command))
            .collect()
    };
    candidates.iter().any(|path| {
        fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    })
}

#[derive(Debug)]
struct DesktopEntry {
    hidden: bool,
    visible: bool,
    name: String,
    icon: Option<String>,
}

fn parse_entry(
    content: &str,
    locales: &[String],
    path_env: Option<&OsStr>,
) -> Option<DesktopEntry> {
    let mut section = false;
    let mut keys = std::collections::HashMap::<&str, &str>::new();
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = line == "[Desktop Entry]";
            continue;
        }
        if !section {
            continue;
        }
        let (key, value) = line.split_once('=')?;
        keys.insert(key.trim(), value.trim());
    }
    // A minimal Hidden=true entry is a deletion marker per the XDG spec.
    let hidden = bool_field(keys.get("Hidden").copied());
    if hidden {
        return Some(DesktopEntry {
            hidden: true,
            visible: false,
            name: String::new(),
            icon: None,
        });
    }
    if keys.get("Type") != Some(&"Application") {
        return None;
    }
    let visible = !bool_field(keys.get("NoDisplay").copied());
    let has_exec = keys.get("Exec").is_some_and(|value| !value.is_empty());
    let dbus = bool_field(keys.get("DBusActivatable").copied());
    if !hidden && !has_exec && !dbus {
        return None;
    }
    let try_exec = keys.get("TryExec").copied();
    if let Some(command) = try_exec {
        if !executable_exists(command, path_env) {
            return Some(DesktopEntry {
                hidden,
                visible: false,
                name: String::new(),
                icon: None,
            });
        }
    }
    let name = locales
        .iter()
        .find_map(|lang| keys.get(format!("Name[{lang}]").as_str()).copied())
        .or_else(|| keys.get("Name").copied())
        .map(unescape_name)
        .unwrap_or_default();
    if name.is_empty() && !hidden {
        return None;
    }
    let icon = keys
        .get("Icon")
        .map(|value| limited_text(value, 100))
        .filter(|value| !value.is_empty() && !value.chars().any(char::is_control));
    Some(DesktopEntry {
        hidden,
        visible,
        name: limited_text(&name, 120),
        icon,
    })
}

#[derive(Debug)]
struct Discovered {
    app: DesktopApplication,
    path: PathBuf,
}

fn scan(roots: &[PathBuf], locales: &[String], path_env: Option<&OsStr>) -> Vec<Discovered> {
    let mut seen = HashSet::<String>::new();
    let mut all = Vec::new();
    let mut visited = 0;
    for root in roots {
        let mut files = Vec::<PathBuf>::new();
        let mut directories = vec![(root.clone(), 0)];
        while let Some((dir, depth)) = directories.pop() {
            if depth > MAX_DEPTH || visited >= MAX_FILES {
                break;
            }
            let Ok(items) = fs::read_dir(&dir) else {
                continue;
            };
            for item in items.flatten() {
                if visited >= MAX_FILES {
                    break;
                }
                visited += 1;
                let path = item.path();
                let Ok(kind) = item.file_type() else {
                    continue;
                };
                if kind.is_dir() && depth < MAX_DEPTH {
                    directories.push((path, depth + 1));
                } else if path.extension().is_some_and(|ext| ext == "desktop")
                    && (kind.is_file() || kind.is_symlink())
                {
                    files.push(path);
                }
            }
        }
        files.sort();
        for file in files {
            let Some(id) = desktop_id(root, &file) else {
                continue;
            };
            if seen.contains(&id) {
                continue;
            }
            let Ok(metadata) = fs::metadata(&file) else {
                continue;
            };
            if metadata.len() > MAX_FILE_SIZE || !metadata.is_file() {
                continue;
            }
            let Ok(contents) = fs::read_to_string(&file) else {
                continue;
            };
            let Some(entry) = parse_entry(&contents, locales, path_env) else {
                continue;
            };
            // Even Hidden / NoDisplay / missing TryExec entries shadow lower-priority IDs.
            seen.insert(id.clone());
            if entry.visible && !entry.hidden {
                all.push(Discovered {
                    app: DesktopApplication {
                        id,
                        name: entry.name,
                        icon: entry.icon,
                    },
                    path: file,
                });
            }
        }
    }
    all.sort_by(|a, b| {
        a.app
            .name
            .to_lowercase()
            .cmp(&b.app.name.to_lowercase())
            .then(a.app.id.cmp(&b.app.id))
    });
    all.truncate(MAX_APPS);
    all
}

pub fn catalog() -> Vec<DesktopApplication> {
    scan(
        &data_dirs(),
        &language_preferences(),
        std::env::var_os("PATH").as_deref(),
    )
    .into_iter()
    .map(|item| item.app)
    .collect()
}

/// The client supplies only an exact desktop ID; the current catalog resolves
/// its path again. Never accept a client-provided path or Exec command.
pub fn selected_path(params: Option<&Value>) -> Result<PathBuf, &'static str> {
    let params = params
        .and_then(Value::as_object)
        .ok_or("Invalid application selection")?;
    if params.len() != 1 {
        return Err("Invalid application selection");
    }
    let id = params
        .get("id")
        .and_then(Value::as_str)
        .ok_or("Invalid application selection")?;
    if id.is_empty()
        || id.len() > 100
        || !id.ends_with(".desktop")
        || id.chars().any(char::is_control)
        || id.contains('/')
        || id.contains('\\')
    {
        return Err("Invalid application selection");
    }
    scan(
        &data_dirs(),
        &language_preferences(),
        std::env::var_os("PATH").as_deref(),
    )
    .into_iter()
    .find(|item| item.app.id == id)
    .map(|item| item.path)
    .ok_or("Application not in catalog")
}

pub async fn launch(path: &Path) -> std::io::Result<()> {
    // GIO interprets desktop-entry Exec field codes, quoting and activation.
    // No shell and no untrusted argv are involved.
    let binary = std::env::var_os("KIRA_GIO_BIN").unwrap_or_else(|| OsString::from("gio"));
    let result = tokio::time::timeout(
        Duration::from_secs(5),
        tokio::process::Command::new(binary)
            .arg("launch")
            .arg(path)
            .kill_on_drop(true)
            .status(),
    )
    .await
    .map_err(|_| std::io::Error::new(std::io::ErrorKind::TimedOut, "GIO launch timed out"))??;
    if result.success() {
        Ok(())
    } else {
        Err(std::io::Error::other("GIO could not activate application"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    fn fixture() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "kira-apps-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }
    fn write(dir: &Path, name: &str, body: &str) {
        let file = dir.join(name);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, body).unwrap();
    }
    const APP: &str = "[Desktop Entry]\nType=Application\nName=Terminal\nExec=terminal %U\n";

    #[test]
    fn reads_localized_names_nested_ids_and_unicode() {
        let root = fixture();
        write(
            &root,
            "foo/bar.desktop",
            "[Desktop Entry]\nType=Application\nName=Normal\nName[ru]=Терминал\nExec=terminal\n",
        );
        let apps = scan(&[root.clone()], &["ru".into()], None);
        assert_eq!(apps[0].app.id, "foo-bar.desktop");
        assert_eq!(apps[0].app.name, "Терминал");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn hidden_and_nodisplay_shadow_lower_priority() {
        let first = fixture();
        let second = fixture();
        write(
            &first,
            "hidden.desktop",
            "[Desktop Entry]\nType=Application\nHidden=true\n",
        );
        write(
            &first,
            "nodisplay.desktop",
            "[Desktop Entry]\nType=Application\nName=No\nExec=no\nNoDisplay=true\n",
        );
        write(&second, "hidden.desktop", APP);
        write(&second, "nodisplay.desktop", APP);
        write(&second, "present.desktop", APP);
        let apps = scan(&[first.clone(), second.clone()], &[], None);
        assert_eq!(apps.len(), 1);
        assert_eq!(apps[0].app.id, "present.desktop");
        fs::remove_dir_all(first).unwrap();
        fs::remove_dir_all(second).unwrap();
    }

    #[test]
    fn tryexec_and_malformed_entries_are_excluded() {
        let root = fixture();
        write(
            &root,
            "missing.desktop",
            "[Desktop Entry]\nType=Application\nName=Missing\nExec=missing\nTryExec=/this/does/not/exist\n",
        );
        write(
            &root,
            "broken.desktop",
            "[Desktop Entry]\nType=Link\nName=Not an app\nURL=https://example.org\n",
        );
        write(
            &root,
            "empty.desktop",
            "[Desktop Entry]\nType=Application\nName=Empty\n",
        );
        write(&root, "valid.desktop", APP);
        let apps = scan(&[root.clone()], &[], None);
        assert_eq!(apps.len(), 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn invalid_selection_never_reaches_launch() {
        for value in [
            serde_json::json!({"id":"../../bin/sh"}),
            serde_json::json!({"id":"/tmp/run.desktop"}),
            serde_json::json!({"id":"x.desktop","exec":"sh -c whoami"}),
            serde_json::json!({"id":""}),
        ] {
            assert!(selected_path(Some(&value)).is_err());
        }
        assert!(selected_path(None).is_err());
    }

    #[test]
    fn handles_empty_dirs_long_names_and_limits() {
        let root = fixture();
        assert!(scan(&[root.clone()], &[], None).is_empty());
        for i in 0..190 {
            write(
                &root,
                &format!("{i}.desktop"),
                &format!(
                    "[Desktop Entry]\nType=Application\nName={}\nExec=true\n",
                    "🪄".repeat(100)
                ),
            );
        }
        let apps = scan(&[root.clone()], &[], None);
        assert_eq!(apps.len(), MAX_APPS);
        assert!(apps.iter().all(|item| item.app.name.len() <= 120));
        fs::remove_dir_all(root).unwrap();
    }
}
