//! Searchable application list: GTK presentation from caller-provided metadata.
use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApplicationEntry {
    pub id: String,
    pub name: String,
}

pub fn filter_applications(apps: &[ApplicationEntry], query: &str, limit: usize) -> Vec<ApplicationEntry> {
    let needle = query.trim().to_lowercase();
    apps.iter()
        .filter(|app| app.name.to_lowercase().contains(&needle))
        .take(limit)
        .cloned()
        .collect()
}

pub struct Launcher {
    root: gtk::Box,
    list: gtk::Box,
    search: gtk::SearchEntry,
    status: gtk::Label,
    applications: Rc<RefCell<Vec<ApplicationEntry>>>,
}

fn render(
    list: &gtk::Box,
    applications: &[ApplicationEntry],
    query: &str,
    on_launch: &Rc<dyn Fn(String)>,
) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
    let filtered = filter_applications(applications, query, 100);
    if filtered.is_empty() && !applications.is_empty() {
        let empty = gtk::Label::new(Some("Приложений не найдено"));
        empty.add_css_class("muted");
        list.append(&empty);
    }
    for app in filtered {
        let button = gtk::Button::with_label(&app.name);
        button.add_css_class("app-row");
        button.set_halign(gtk::Align::Fill);
        let callback = on_launch.clone();
        button.connect_clicked(move |_| callback(app.id.clone()));
        list.append(&button);
    }
}

impl Launcher {
    pub fn new(on_launch: impl Fn(String) + 'static) -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 12);
        let search = gtk::SearchEntry::new();
        search.set_placeholder_text(Some("Найти приложение"));
        search.update_property(&[gtk::accessible::Property::Label("Поиск приложений")]);
        root.append(&search);

        let status = gtk::Label::new(Some("Загрузка приложений"));
        status.set_xalign(0.0);
        status.set_wrap(true);
        status.add_css_class("muted");
        root.append(&status);

        let scroll = gtk::ScrolledWindow::new();
        scroll.set_min_content_height(240);
        scroll.set_max_content_height(480);
        scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        let list = gtk::Box::new(gtk::Orientation::Vertical, 4);
        scroll.set_child(Some(&list));
        root.append(&scroll);

        let applications = Rc::new(RefCell::new(Vec::new()));
        let callback: Rc<dyn Fn(String)> = Rc::new(on_launch);
        search.connect_search_changed({
            let list = list.clone();
            let applications = applications.clone();
            let callback = callback.clone();
            move |entry| render(&list, &applications.borrow(), &entry.text(), &callback)
        });
        // Store callback on the GTK widget as the actual item buttons' click connection
        // by rebuilding the results when the search model changes.
        // A dedicated immutable callback is retained in the search signal closure.
        Self { root, list, search, status, applications }
    }

    pub fn set_applications(&self, apps: Vec<ApplicationEntry>) {
        *self.applications.borrow_mut() = apps;
        // Triggers the pre-registered result renderer, including after a fresh catalogue.
        self.search.emit_by_name::<()>("search-changed", &[]);
        self.status.set_text("");
    }

    pub fn set_status(&self, status: &str) {
        self.status.set_text(status);
    }

    pub fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_matches_and_bounds_results() {
        let apps = vec![
            ApplicationEntry { id: "one".into(), name: "Terminal".into() },
            ApplicationEntry { id: "two".into(), name: "Text Editor".into() },
            ApplicationEntry { id: "three".into(), name: "Terminal 2".into() },
        ];
        assert_eq!(filter_applications(&apps, "TERMINAL", 1)[0].id, "one");
        assert_eq!(filter_applications(&apps, "terminal", 10).len(), 2);
        assert!(filter_applications(&apps, "browser", 10).is_empty());
    }
}
