//! Trusted GIO desktop-app adapter for the Svelte Launcher. No GTK widgets,
//! command strings, executable paths, terminal spawning or Core RPC.
use std::cell::RefCell;
use std::collections::HashMap;

use gtk::gio;
use gtk::gio::prelude::*;

const MAX_ICON_BYTES: usize = 128 * 1024;
const ICON_BATCH_LIMIT: usize = 16;

/// Browser images receive only bounded data URIs from the GTK icon theme,
/// never local paths, arbitrary file URLs or executable commands.
fn icon_data_url(icon: &gio::Icon, theme: &gtk::IconTheme) -> Option<String> {
    let paintable = theme.lookup_by_gicon(
        icon,
        40,
        1,
        gtk::TextDirection::None,
        gtk::IconLookupFlags::empty(),
    );
    let file = paintable.file()?;
    if !file.is_native() {
        return None;
    }
    let path = file.path()?;
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    let mime = match extension.as_str() {
        "png" => "image/png",
        "svg" => "image/svg+xml",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        _ => return None,
    };
    let metadata = file
        .query_info(
            "standard::size,standard::type",
            gio::FileQueryInfoFlags::NONE,
            None::<&gio::Cancellable>,
        )
        .ok()?;
    if metadata.file_type() != gio::FileType::Regular
        || metadata.size() <= 0
        || metadata.size() > MAX_ICON_BYTES as i64
    {
        return None;
    }
    let (bytes, _) = file.load_bytes(None::<&gio::Cancellable>).ok()?;
    let content: &[u8] = bytes.as_ref();
    if content.is_empty() || content.len() > MAX_ICON_BYTES {
        return None;
    }
    // Do not turn an arbitrary text file into an image via a .png filename.
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
    if !valid {
        return None;
    }
    Some(format!(
        "data:{mime};base64,{}",
        gtk::glib::base64_encode(content)
    ))
}

/// Only an index into this server-side snapshot is accepted from Svelte.
/// Never use a web-supplied executable path, desktop ID or shell command.
pub struct Catalog {
    apps: Vec<gio::AppInfo>,
    icons: RefCell<HashMap<usize, Option<String>>>,
}

impl Catalog {
    pub fn discover() -> Self {
        let mut apps: Vec<_> = gio::AppInfo::all()
            .into_iter()
            .filter(|info| info.should_show())
            .collect();
        apps.sort_by_key(|info| info.display_name().to_string().to_lowercase());
        apps.truncate(1024);
        Self {
            apps,
            icons: RefCell::new(HashMap::new()),
        }
    }

    pub fn as_json(&self) -> serde_json::Value {
        let apps: Vec<_> = self
            .apps
            .iter()
            .enumerate()
            .map(|(id, info)| {
                let name: String = info
                    .display_name()
                    .chars()
                    .filter(|c| !c.is_control())
                    .take(120)
                    .collect();
                let description: String = info
                    .description()
                    .map(|v| v.to_string())
                    .unwrap_or_default()
                    .chars()
                    .filter(|c| !c.is_control())
                    .take(240)
                    .collect();
                serde_json::json!({
                    "id": id,
                    "name": name,
                    "description": description,
                })
            })
            .collect();
        serde_json::json!({ "apps": apps })
    }

    /// Resolve only icons for rows that Svelte currently displays; caching
    /// avoids repeated file I/O when the search filter changes. All data is
    /// freed when the launcher is closed along with its native catalog.
    pub fn icons_json(&self, ids: &[usize]) -> serde_json::Value {
        let theme =
            gtk::gdk::Display::default().map(|display| gtk::IconTheme::for_display(&display));
        let icons: Vec<_> = ids
            .iter()
            .take(ICON_BATCH_LIMIT)
            .filter_map(|id| {
                let info = self.apps.get(*id)?;
                let icon = {
                    let mut cache = self.icons.borrow_mut();
                    cache
                        .entry(*id)
                        .or_insert_with(|| {
                            info.icon().and_then(|gicon| {
                                theme
                                    .as_ref()
                                    .and_then(|theme| icon_data_url(&gicon, theme))
                            })
                        })
                        .clone()
                };
                Some(serde_json::json!({ "id": id, "data_url": icon }))
            })
            .collect();
        serde_json::json!({ "icons": icons })
    }

    pub fn launch(&self, id: usize) -> Result<(), String> {
        let info = self.apps.get(id).ok_or("Unknown application index")?;
        info.launch(&[], None::<&gio::AppLaunchContext>)
            .map_err(|error| {
                error
                    .to_string()
                    .chars()
                    .filter(|c| !c.is_control())
                    .take(180)
                    .collect()
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn out_of_range_id_cannot_launch_arbitrary_programs() {
        let catalog = Catalog {
            apps: vec![],
            icons: RefCell::new(HashMap::new()),
        };
        assert_eq!(catalog.launch(0), Err("Unknown application index".into()));
        assert_eq!(
            catalog.launch(usize::MAX),
            Err("Unknown application index".into())
        );
        assert_eq!(catalog.as_json(), serde_json::json!({"apps":[]}));
    }
}
