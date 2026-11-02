//! Optional, keyboard-first application shortcuts embedded in the existing
//! 42px Top Bar. Niri owns window navigation; this only launches explicitly
//! configured desktop entries through trusted native GIO AppInfo objects.
use gtk::prelude::*;
use std::collections::HashSet;

use crate::launcher;

const MAX_SHORTCUTS: usize = 3;
const MAX_COMMAND_BYTES: usize = 48;

#[derive(Debug, PartialEq, Eq)]
enum Command {
    Catalog,
    Icons(Vec<usize>),
    Launch(usize),
}

fn index(text: &str) -> Option<usize> {
    if text.is_empty() || !text.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let id = text.parse::<usize>().ok()?;
    (id < 1024).then_some(id)
}

fn parse_command(message: &str) -> Option<Command> {
    if message.len() > MAX_COMMAND_BYTES {
        return None;
    }
    if message == "catalog" {
        return Some(Command::Catalog);
    }
    if let Some(raw) = message.strip_prefix("launch:") {
        return Some(Command::Launch(index(raw)?));
    }
    if let Some(raw) = message.strip_prefix("icons:") {
        let mut ids = Vec::new();
        for part in raw.split(',') {
            let next = index(part)?;
            if ids.len() == MAX_SHORTCUTS || ids.contains(&next) {
                return None;
            }
            ids.push(next);
        }
        if !ids.is_empty() {
            return Some(Command::Icons(ids));
        }
    }
    None
}

fn configured_names(raw: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    raw.split(',')
        .map(str::trim)
        .filter(|name| {
            !name.is_empty()
                && name.len() <= 120
                && !name.chars().any(char::is_control)
                && seen.insert(name.to_lowercase())
        })
        .take(MAX_SHORTCUTS)
        .map(str::to_owned)
        .collect()
}

fn selected_entries(
    full: &serde_json::Value,
    names: &[String],
) -> (Vec<usize>, serde_json::Value) {
    let apps = full["apps"].as_array();
    let mut selected = Vec::new();
    let mut allowed = Vec::new();
    for name in names {
        let entry = apps.and_then(|all| {
            all.iter().find(|app| {
                app["name"]
                    .as_str()
                    .is_some_and(|display| display.to_lowercase() == name.to_lowercase())
            })
        });
        if let Some(app) = entry {
            let Some(id) = app["id"].as_u64().and_then(|id| usize::try_from(id).ok()) else {
                continue;
            };
            if id >= 1024 || allowed.contains(&id) {
                continue;
            }
            allowed.push(id);
            selected.push(app.clone());
        }
    }
    (allowed, serde_json::json!({ "apps": selected }))
}

pub enum Event {
    Apps(serde_json::Value),
    Icons(serde_json::Value),
    Error(String),
}

pub fn deliver(view: &webkit6::WebView, event: Event) {
    use webkit6::prelude::*;
    let (name, payload) = match event {
        Event::Apps(value) => ("kira:quick:apps", value),
        Event::Icons(value) => ("kira:quick:icons", value),
        Event::Error(message) => (
            "kira:quick:error",
            serde_json::json!({ "message": message }),
        ),
    };
    let Ok(json) = serde_json::to_string(&payload) else {
        return;
    };
    let Ok(quoted) = serde_json::to_string(&json) else {
        return;
    };
    let script = format!(
        "window.dispatchEvent(new CustomEvent('{name}', {{ detail: JSON.parse({quoted}) }}));"
    );
    view.evaluate_javascript(
        &script,
        None,
        None,
        None::<&gtk::gio::Cancellable>,
        |result| {
            if let Err(error) = result {
                eprintln!("kira-quick-launch: WebKit event delivery failed: {error}");
            }
        },
    );
}

pub struct QuickLaunch {
    catalog: launcher::Catalog,
    names: Vec<String>,
    allowed: Vec<usize>,
    ready: Option<serde_json::Value>,
}

impl QuickLaunch {
    /// No configured shortcuts means no catalog worker or UI controls.
    /// The old KIRA_DOCK_APPS environment variable remains a migration alias.
    pub fn discover() -> Result<Option<Self>, String> {
        let configured = std::env::var("KIRA_QUICK_APPS")
            .or_else(|_| std::env::var("KIRA_DOCK_APPS"))
            .unwrap_or_default();
        let names = configured_names(&configured);
        if names.is_empty() {
            return Ok(None);
        }
        Ok(Some(Self {
            catalog: launcher::Catalog::discover()?,
            names,
            allowed: Vec::new(),
            ready: None,
        }))
    }

    pub fn handle(&self, message: &str) -> Option<Event> {
        match parse_command(message)? {
            Command::Catalog => self.ready.clone().map(Event::Apps),
            Command::Launch(id) if self.allowed.contains(&id) => {
                self.catalog.launch(id).err().map(Event::Error)
            }
            Command::Icons(ids) if ids.iter().all(|id| self.allowed.contains(id)) => {
                self.catalog.request_icons(&ids).err().map(Event::Error)
            }
            _ => None,
        }
    }

    pub fn poll(&mut self) -> Option<Event> {
        match self.catalog.poll()? {
            launcher::LauncherMessage::Apps(full) => {
                let (allowed, payload) = selected_entries(&full, &self.names);
                self.allowed = allowed;
                self.ready = Some(payload.clone());
                Some(Event::Apps(payload))
            }
            launcher::LauncherMessage::Icons(data) => Some(Event::Icons(data)),
            launcher::LauncherMessage::Launched(Err(error))
            | launcher::LauncherMessage::Error(error) => Some(Event::Error(error)),
            launcher::LauncherMessage::Launched(Ok(())) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_a_bounded_explicit_launch_contract() {
        assert_eq!(parse_command("catalog"), Some(Command::Catalog));
        assert_eq!(parse_command("launch:7"), Some(Command::Launch(7)));
        assert_eq!(
            parse_command("icons:2,9"),
            Some(Command::Icons(vec![2, 9]))
        );
        for invalid in [
            "",
            "open",
            "run:ls",
            "launch:-1",
            "launch:1024",
            "launch:1:2",
            "icons:",
            "icons:1,1",
            "icons:1,2,3,4",
        ] {
            assert_eq!(parse_command(invalid), None, "{invalid}");
        }
        assert_eq!(parse_command(&"x".repeat(64)), None);
    }

    #[test]
    fn configured_shortcuts_are_explicit_unique_and_limited() {
        assert!(configured_names("").is_empty());
        assert_eq!(
            configured_names("Firefox,Terminal,firefox,Files,Other"),
            vec!["Firefox", "Terminal", "Files"]
        );
        let input = serde_json::json!({
            "apps": [
                {"id": 7, "name": "Files", "description": ""},
                {"id": 2, "name": "Firefox", "description": ""},
                {"id": 8, "name": "Terminal", "description": ""}
            ]
        });
        let names = configured_names("Terminal,Missing,Firefox");
        let (allowed, payload) = selected_entries(&input, &names);
        assert_eq!(allowed, vec![8, 2]);
        assert_eq!(payload["apps"][0]["name"], "Terminal");
        assert_eq!(payload["apps"][1]["name"], "Firefox");
        assert_eq!(payload["apps"].as_array().unwrap().len(), 2);
    }
}
