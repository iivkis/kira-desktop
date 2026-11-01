//! Trusted GIO desktop-app adapter for the Svelte Launcher. No GTK widgets,
//! command strings, executable paths, terminal spawning or Core RPC.
use gtk::gio;
use gtk::gio::prelude::*;

/// Only an index into this server-side snapshot is accepted from Svelte.
/// Never use a web-supplied executable path, desktop ID or shell command.
pub struct Catalog {
    apps: Vec<gio::AppInfo>,
}

impl Catalog {
    pub fn discover() -> Self {
        let mut apps: Vec<_> = gio::AppInfo::all()
            .into_iter()
            .filter(|info| info.should_show())
            .collect();
        apps.sort_by_key(|info| info.display_name().to_string().to_lowercase());
        apps.truncate(1024);
        Self { apps }
    }

    pub fn as_json(&self) -> serde_json::Value {
        let apps: Vec<_> = self.apps.iter().enumerate().map(|(id, info)| {
            let name: String = info.display_name().chars().filter(|c| !c.is_control()).take(120).collect();
            let description: String = info.description().map(|v| v.to_string()).unwrap_or_default()
                .chars().filter(|c| !c.is_control()).take(240).collect();
            serde_json::json!({
                "id": id,
                "name": name,
                "description": description,
            })
        }).collect();
        serde_json::json!({ "apps": apps })
    }

    pub fn launch(&self, id: usize) -> Result<(), String> {
        let info = self.apps.get(id).ok_or("Unknown application index")?;
        info.launch(&[], None::<&gio::AppLaunchContext>())
            .map_err(|error| error.to_string().chars().filter(|c| !c.is_control()).take(180).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn out_of_range_id_cannot_launch_arbitrary_programs() {
        let catalog = Catalog { apps: vec![] };
        assert_eq!(catalog.launch(0), Err("Unknown application index".into()));
        assert_eq!(catalog.launch(usize::MAX), Err("Unknown application index".into()));
        assert_eq!(catalog.as_json(), serde_json::json!({"apps":[]}));
    }
}
