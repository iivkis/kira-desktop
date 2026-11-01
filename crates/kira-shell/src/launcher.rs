//! Trusted GIO desktop-app adapter for the Svelte Launcher. No GTK widgets,
//! command strings, executable paths, terminal spawning or Core RPC.
//! GIO application objects stay in one worker; only owned Rust data crosses
//! into the GTK thread. Icon file I/O and base64 encoding never block GTK.
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};

use gtk::gio;
use gtk::gio::prelude::*;

const MAX_ICON_BYTES: usize = 128 * 1024;
const ICON_BATCH_LIMIT: usize = 16;
const MAX_APPS: usize = 1024;

#[derive(Clone)]
struct AppEntry {
    id: usize,
    name: String,
    description: String,
    // GIcon's standard string serialization, not a Gio object or user input.
    icon: Option<String>,
}

enum WorkerRequest {
    Icons(Vec<(usize, Option<PathBuf>, Option<PathBuf>)>),
    Launch(usize),
}

enum WorkerResponse {
    Apps(Vec<AppEntry>),
    Icons(Vec<(usize, Option<String>)>),
    Launched(Result<(), String>),
}

pub enum LauncherMessage {
    Apps(serde_json::Value),
    Icons(serde_json::Value),
    Launched(Result<(), String>),
    Error(String),
}

/// Catalog is a GTK-thread handle to an isolated GIO worker. The Gio AppInfo
/// objects are created, used, and dropped on that same worker thread because
/// GObject wrappers are not Send.
pub struct Catalog {
    commands: Sender<WorkerRequest>,
    results: Receiver<WorkerResponse>,
    apps: Option<Vec<AppEntry>>,
    disconnected: bool,
}

impl Catalog {
    /// Nonblocking for GTK: enumeration/sorting happen in a dedicated thread.
    pub fn discover() -> Result<Self, String> {
        let (commands, requests) = mpsc::channel();
        let (results_tx, results) = mpsc::channel();
        std::thread::Builder::new()
            .name("kira-launcher".into())
            .spawn(move || run_worker(requests, results_tx))
            .map_err(|error| format!("Cannot start application catalog: {error}"))?;
        Ok(Self {
            commands,
            results,
            apps: None,
            disconnected: false,
        })
    }

    pub fn poll(&mut self) -> Option<LauncherMessage> {
        match self.results.try_recv() {
            Ok(WorkerResponse::Apps(apps)) => {
                self.apps = Some(apps);
                Some(LauncherMessage::Apps(self.as_json()))
            }
            Ok(WorkerResponse::Icons(icons)) => Some(LauncherMessage::Icons(serde_json::json!({
                "icons": icons.into_iter().map(|(id, data_url)| {
                    serde_json::json!({"id": id, "data_url": data_url})
                }).collect::<Vec<_>>()
            }))),
            Ok(WorkerResponse::Launched(result)) => Some(LauncherMessage::Launched(result)),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) if !self.disconnected => {
                self.disconnected = true;
                Some(LauncherMessage::Error(
                    "Application service disconnected".into(),
                ))
            }
            Err(mpsc::TryRecvError::Disconnected) => None,
        }
    }

    fn as_json(&self) -> serde_json::Value {
        let Some(apps) = self.apps.as_ref() else {
            return serde_json::json!({"apps": []});
        };
        serde_json::json!({
            "apps": apps.iter().map(|app| {
                serde_json::json!({
                    "id": app.id,
                    "name": app.name,
                    "description": app.description,
                })
            }).collect::<Vec<_>>()
        })
    }

    pub fn ready_json(&self) -> Option<serde_json::Value> {
        self.apps.as_ref().map(|_| self.as_json())
    }

    /// GTK-owned IconTheme resolves paths, but never reads image data. Actual
    /// file opening, validation, and encoding all happen in run_worker().
    pub fn request_icons(&self, ids: &[usize]) -> Result<(), String> {
        let apps = self.apps.as_ref().ok_or("Application catalog not ready")?;
        let theme =
            gtk::gdk::Display::default().map(|display| gtk::IconTheme::for_display(&display));
        let generic = theme.as_ref().and_then(generic_icon_path);
        let requested = ids
            .iter()
            .take(ICON_BATCH_LIMIT)
            .filter_map(|id| {
                let app = apps.get(*id)?;
                Some((
                    *id,
                    resolve_icon_path(app.icon.as_deref(), theme.as_ref()),
                    generic.clone(),
                ))
            })
            .collect();
        self.commands
            .send(WorkerRequest::Icons(requested))
            .map_err(|_| "Application service unavailable".into())
    }

    /// IDs remain indices into the worker's trusted application snapshot.
    /// Never accept an executable path or command from WebKit.
    pub fn launch(&self, id: usize) -> Result<(), String> {
        if self.apps.as_ref().and_then(|apps| apps.get(id)).is_none() {
            return Err("Unknown application index".into());
        }
        self.commands
            .send(WorkerRequest::Launch(id))
            .map_err(|_| "Application service unavailable".into())
    }
}

fn run_worker(requests: Receiver<WorkerRequest>, results: Sender<WorkerResponse>) {
    // GIO can enumerate apps outside GTK's main loop. Keep every AppInfo on
    // this thread, including the launch call, rather than sending GObjects.
    let mut apps: Vec<_> = gio::AppInfo::all()
        .into_iter()
        .filter(|info| info.should_show())
        .collect();
    apps.sort_by_key(|info| info.display_name().to_string().to_lowercase());
    apps.truncate(MAX_APPS);
    let entries = apps
        .iter()
        .enumerate()
        .map(|(id, info)| AppEntry {
            id,
            name: info
                .display_name()
                .chars()
                .filter(|c| !c.is_control())
                .take(120)
                .collect(),
            description: info
                .description()
                .map(|v| v.to_string())
                .unwrap_or_default()
                .chars()
                .filter(|c| !c.is_control())
                .take(240)
                .collect(),
            icon: info
                .icon()
                .and_then(|icon| icon.to_string().map(|s| s.to_string())),
        })
        .collect();
    if results.send(WorkerResponse::Apps(entries)).is_err() {
        return; // Launcher closed before enumeration completed.
    }

    let mut cache: HashMap<usize, Option<String>> = HashMap::new();
    while let Ok(command) = requests.recv() {
        let reply = match command {
            WorkerRequest::Icons(paths) => {
                let icons = paths
                    .into_iter()
                    .take(ICON_BATCH_LIMIT)
                    .filter(|(id, _, _)| *id < apps.len())
                    .map(|(id, path, fallback)| {
                        let icon = cache
                            .entry(id)
                            .or_insert_with(|| encode_icon(path.as_deref(), fallback.as_deref()))
                            .clone();
                        (id, icon)
                    })
                    .collect();
                WorkerResponse::Icons(icons)
            }
            WorkerRequest::Launch(id) => WorkerResponse::Launched(
                apps.get(id)
                    .ok_or_else(|| "Unknown application index".to_string())
                    .and_then(|info| {
                        info.launch(&[], None::<&gio::AppLaunchContext>)
                            .map_err(|error| {
                                error
                                    .to_string()
                                    .chars()
                                    .filter(|c| !c.is_control())
                                    .take(180)
                                    .collect()
                            })
                    }),
            ),
        };
        if results.send(reply).is_err() {
            break;
        }
    }
}

/// Theme lookup stays with GTK, which owns the display and icon theme. File
/// bytes are not accessed here. Serialized icons originate from trusted GIO.
fn resolve_icon_path(icon: Option<&str>, theme: Option<&gtk::IconTheme>) -> Option<PathBuf> {
    let icon = gio::Icon::for_string(icon?).ok()?;
    if let Some(file_icon) = icon.downcast_ref::<gio::FileIcon>() {
        let file = file_icon.file();
        if file.is_native() {
            return file.path();
        }
    }
    theme?
        .lookup_by_gicon(
            &icon,
            40,
            1,
            gtk::TextDirection::None,
            gtk::IconLookupFlags::empty(),
        )
        .file()
        .and_then(|file| file.is_native().then(|| file.path()).flatten())
}

fn generic_icon_path(theme: &gtk::IconTheme) -> Option<PathBuf> {
    const NAME: &str = "application-x-executable";
    if !theme.has_icon(NAME) {
        return None;
    }
    theme
        .lookup_icon(
            NAME,
            &[],
            40,
            1,
            gtk::TextDirection::None,
            gtk::IconLookupFlags::empty(),
        )
        .file()
        .and_then(|file| file.is_native().then(|| file.path()).flatten())
}

/// Prefer the requested app icon; use the system-theme generic icon if
/// a file is missing, too large or has an invalid signature.
fn encode_icon(primary: Option<&Path>, fallback: Option<&Path>) -> Option<String> {
    primary
        .and_then(file_data_url)
        .or_else(|| fallback.and_then(file_data_url))
}

/// Read only small, signature-checked image files into bounded data URIs.
/// A suffix alone is never sufficient, and even a racing file replacement
/// cannot make the worker read more than MAX_ICON_BYTES + 1 bytes.
fn file_data_url(path: &Path) -> Option<String> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    let mime = match extension.as_str() {
        "png" => "image/png",
        "svg" => "image/svg+xml",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        _ => return None,
    };
    let metadata = std::fs::metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_ICON_BYTES as u64 {
        return None;
    }
    let mut content = Vec::with_capacity(metadata.len() as usize);
    File::open(path)
        .ok()?
        .take((MAX_ICON_BYTES + 1) as u64)
        .read_to_end(&mut content)
        .ok()?;
    if content.is_empty() || content.len() > MAX_ICON_BYTES {
        return None;
    }
    let valid = match extension.as_str() {
        "png" => content.starts_with(b"\x89PNG\r\n\x1a\n"),
        "jpg" | "jpeg" => content.starts_with(&[0xff, 0xd8, 0xff]),
        "webp" => content.starts_with(b"RIFF") && content.get(8..12) == Some(b"WEBP".as_slice()),
        "svg" => {
            let prefix = String::from_utf8_lossy(&content[..content.len().min(1024)]);
            prefix.contains("<svg") && !prefix.contains("<!DOCTYPE")
        }
        _ => false,
    };
    valid.then(|| format!("data:{mime};base64,{}", gtk::glib::base64_encode(&content)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn out_of_range_id_cannot_launch_arbitrary_programs() {
        let (tx, _rx) = mpsc::channel();
        let (_result_tx, results) = mpsc::channel();
        let catalog = Catalog {
            commands: tx,
            results,
            apps: Some(vec![]),
            disconnected: false,
        };
        assert_eq!(catalog.launch(0), Err("Unknown application index".into()));
        assert_eq!(
            catalog.launch(usize::MAX),
            Err("Unknown application index".into())
        );
        assert_eq!(catalog.as_json(), serde_json::json!({"apps":[]}));
    }

    #[test]
    fn invalid_icon_data_is_rejected_without_launching_anything() {
        let directory = std::env::temp_dir().join(format!(
            "kira-icons-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let fake = directory.join("bad.png");
        std::fs::write(&fake, b"not actually a png").unwrap();
        assert!(file_data_url(&fake).is_none());

        let huge = directory.join("huge.svg");
        let mut file = File::create(&huge).unwrap();
        file.write_all(b"<svg>").unwrap();
        file.write_all(&vec![b'x'; MAX_ICON_BYTES]).unwrap();
        drop(file);
        assert!(file_data_url(&huge).is_none());
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn invalid_app_icon_falls_back_to_valid_generic_icon() {
        let directory = std::env::temp_dir().join(format!(
            "kira-icon-fallback-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let broken = directory.join("broken.png");
        let generic = directory.join("generic.png");
        std::fs::write(&broken, b"not an image").unwrap();
        std::fs::write(&generic, b"\x89PNG\r\n\x1a\nimage").unwrap();
        assert!(encode_icon(Some(&broken), Some(&generic))
            .unwrap()
            .starts_with("data:image/png;base64,"));
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn small_local_image_is_encoded_as_bounded_data_url() {
        let directory = std::env::temp_dir().join(format!(
            "kira-icon-valid-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let icon = directory.join("valid.png");
        std::fs::write(&icon, b"\x89PNG\r\n\x1a\nimage").unwrap();
        let image = file_data_url(&icon).unwrap();
        assert!(image.starts_with("data:image/png;base64,"));
        assert!(!image.contains(icon.to_str().unwrap()));
        std::fs::remove_dir_all(directory).unwrap();
    }
}
