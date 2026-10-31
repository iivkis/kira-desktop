//! Native layer-shell launcher. It is a separate non-exclusive overlay,
//! not a resized top bar and not a command-capable WebKit API.
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc;

use gtk::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use kira_ipc::{DesktopApplication, PROTOCOL_VERSION, RpcRequest};

use crate::ShellDelivery;

const CSS: &str = r#"
window.kira-launcher { background: transparent; }
.kira-launcher-panel {
  background: #49111c; color: #f2f4f3;
  border: 1px solid #a9927d; border-radius: 12px;
  padding: 16px;
}
.kira-launcher-title { color: #f2f4f3; font-weight: 800; font-size: 17px; }
.kira-launcher-hint { color: #a9927d; font-size: 12px; }
.kira-launcher-search {
  background: #0a0908; color: #f2f4f3;
  border: 1px solid #a9927d; border-radius: 8px;
  padding: 10px; caret-color: #a9927d;
}
.kira-launcher-search:focus { outline: 2px solid #a9927d; }
.kira-launcher-list { background: transparent; color: #f2f4f3; }
.kira-launcher-list row { border-radius: 6px; padding: 10px; }
.kira-launcher-list row:selected { background: #a9927d; color: #0a0908; }
.kira-launcher-list row:hover:not(:selected) { background: #5e503f; }
.kira-launcher-message { color: #f2f4f3; font-size: 12px; }
"#;

pub struct LauncherUi {
    pub window: gtk::ApplicationWindow,
    pub search: gtk::SearchEntry,
    list: gtk::ListBox,
    message: gtk::Label,
    catalog: Rc<RefCell<Vec<DesktopApplication>>>,
    visible_ids: Rc<RefCell<Vec<String>>>,
    launching: Rc<Cell<bool>>,
}

impl LauncherUi {
    pub fn show_loading(&self) {
        self.search.set_text("");
        self.catalog.borrow_mut().clear();
        self.rebuild();
        self.message.set_text("Loading installed applications…");
        self.window.present();
        self.search.grab_focus();
    }

    pub fn set_catalog(&self, result: Result<Vec<DesktopApplication>, String>) {
        if !self.window.is_visible() {
            return;
        }
        match result {
            Ok(apps) => {
                *self.catalog.borrow_mut() = apps;
                self.rebuild();
            }
            Err(error) => {
                self.catalog.borrow_mut().clear();
                self.rebuild();
                self.message.set_text(&error);
            }
        }
    }

    pub fn launch_finished(&self, result: Result<(), String>, bar_view: &webkit6::WebView) {
        self.launching.set(false);
        match result {
            Ok(()) => {
                self.window.hide();
                bar_view.grab_focus();
            }
            Err(error) => self.message.set_text(&error),
        }
    }

    fn rebuild(&self) {
        render(
            &self.catalog.borrow(),
            self.search.text().as_str(),
            &self.list,
            &self.visible_ids,
            &self.message,
        );
    }
}

fn matches_search(app: &DesktopApplication, query: &str) -> bool {
    let needle = query.trim().to_lowercase();
    app.name.to_lowercase().contains(&needle) || app.id.to_lowercase().contains(&needle)
}

fn render(
    catalog: &[DesktopApplication],
    query: &str,
    list: &gtk::ListBox,
    visible_ids: &RefCell<Vec<String>>,
    message: &gtk::Label,
) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
    let mut ids = visible_ids.borrow_mut();
    ids.clear();
    for app in catalog.iter().filter(|app| matches_search(app, query)) {
        let row = gtk::ListBoxRow::new();
        row.set_activatable(true);
        row.set_selectable(true);
        let label = gtk::Label::new(Some(&app.name));
        label.set_xalign(0.0);
        label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        label.set_tooltip_text(Some(&app.id));
        row.set_child(Some(&label));
        list.append(&row);
        ids.push(app.id.clone());
    }
    if let Some(first) = list.row_at_index(0) {
        list.select_row(Some(&first));
    }
    message.set_text(if catalog.is_empty() {
        "No installed applications available"
    } else if ids.is_empty() {
        "No matching applications"
    } else {
        "↑ ↓ navigate · Enter launch · Esc close"
    });
}

fn call_core(
    socket: Option<PathBuf>,
    method: &str,
    id: Option<String>,
) -> Result<serde_json::Value, String> {
    let path = socket.ok_or_else(|| "Kira Core unavailable".to_string())?;
    let request = RpcRequest {
        jsonrpc: PROTOCOL_VERSION.into(),
        id: 21,
        method: method.into(),
        params: id.map(|id| serde_json::json!({"id": id})),
    };
    let response = kira_ipc::client::call(&path, &request)
        .map_err(|_| "Cannot connect to Kira Core".to_string())?;
    if let Some(error) = response.error {
        return Err(error.message);
    }
    response
        .result
        .ok_or_else(|| "Empty Core response".to_string())
}

pub fn open_catalog(socket: Option<PathBuf>, sender: mpsc::Sender<ShellDelivery>) {
    std::thread::spawn(move || {
        let result = call_core(socket, "get_applications", None).and_then(|value| {
            serde_json::from_value::<Vec<DesktopApplication>>(value)
                .map_err(|_| "Invalid application catalog".to_string())
        });
        let _ = sender.send(ShellDelivery::LauncherCatalog(result));
    });
}

pub fn create(
    app: &gtk::Application,
    bar_view: &webkit6::WebView,
    socket: Option<PathBuf>,
    sender: mpsc::Sender<ShellDelivery>,
) -> LauncherUi {
    let provider = gtk::CssProvider::new();
    provider.load_from_data(CSS);
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }

    let window = gtk::ApplicationWindow::new(app);
    window.set_title(Some("Kira Application Launcher"));
    window.set_decorated(false);
    window.set_resizable(false);
    window.set_default_size(520, 420);
    window.add_css_class("kira-launcher");
    window.init_layer_shell();
    window.set_namespace(Some("kira-desktop-launcher"));
    window.set_layer(Layer::Overlay);
    window.set_anchor(Edge::Top, true);
    window.set_margin(Edge::Top, 52);
    window.set_exclusive_zone(0);
    window.set_keyboard_mode(KeyboardMode::Exclusive);
    window.set_hide_on_close(true);

    let panel = gtk::Box::new(gtk::Orientation::Vertical, 12);
    panel.add_css_class("kira-launcher-panel");
    let header = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let title = gtk::Label::new(Some("APPLICATIONS"));
    title.add_css_class("kira-launcher-title");
    title.set_xalign(0.0);
    title.set_hexpand(true);
    header.append(&title);
    let close = gtk::Button::with_label("Close · Esc");
    close.set_tooltip_text(Some("Close launcher and return to the top bar"));
    header.append(&close);
    panel.append(&header);

    let search = gtk::SearchEntry::new();
    search.add_css_class("kira-launcher-search");
    search.set_placeholder_text(Some("Search applications by name"));
    search.set_max_length(100);
    panel.append(&search);

    let list = gtk::ListBox::new();
    list.add_css_class("kira-launcher-list");
    list.set_selection_mode(gtk::SelectionMode::Single);
    list.set_activate_on_single_click(true);
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_min_content_height(270);
    scroll.set_vexpand(true);
    scroll.set_child(Some(&list));
    panel.append(&scroll);

    let message = gtk::Label::new(Some("Search installed applications"));
    message.add_css_class("kira-launcher-message");
    message.set_xalign(0.0);
    panel.append(&message);
    window.set_child(Some(&panel));

    let catalog = Rc::new(RefCell::new(Vec::<DesktopApplication>::new()));
    let visible_ids = Rc::new(RefCell::new(Vec::<String>::new()));
    let launching = Rc::new(Cell::new(false));

    let list_search = list.clone();
    let catalog_search = catalog.clone();
    let ids_search = visible_ids.clone();
    let message_search = message.clone();
    search.connect_search_changed(move |entry| {
        render(
            &catalog_search.borrow(),
            entry.text().as_str(),
            &list_search,
            &ids_search,
            &message_search,
        );
    });

    let ids_activate = visible_ids.clone();
    let launching_activate = launching.clone();
    let socket_activate = socket.clone();
    let sender_activate = sender.clone();
    let message_activate = message.clone();
    list.connect_row_activated(move |_, row| {
        if launching_activate.get() {
            return;
        }
        let Some(id) = ids_activate.borrow().get(row.index() as usize).cloned() else {
            return;
        };
        launching_activate.set(true);
        message_activate.set_text("Launching application…");
        let socket = socket_activate.clone();
        let sender = sender_activate.clone();
        std::thread::spawn(move || {
            let result = call_core(socket, "launch_application", Some(id)).map(|_| ());
            let _ = sender.send(ShellDelivery::LauncherLaunched(result));
        });
    });

    let window_close = window.clone();
    let bar_close = bar_view.clone();
    close.connect_clicked(move |_| {
        window_close.hide();
        bar_close.grab_focus();
    });

    let search_enter = list.clone();
    search.connect_activate(move |_| {
        if let Some(row) = search_enter.selected_row() {
            row.activate();
        }
    });

    let keys = gtk::EventControllerKey::new();
    let window_key = window.clone();
    let bar_key = bar_view.clone();
    let list_key = list.clone();
    keys.connect_key_pressed(move |_, key, _, _| match key {
        gtk::gdk::Key::Escape => {
            window_key.hide();
            bar_key.grab_focus();
            gtk::glib::Propagation::Stop
        }
        gtk::gdk::Key::Down | gtk::gdk::Key::Up => {
            let index = list_key.selected_row().map(|row| row.index()).unwrap_or(0);
            let next = if key == gtk::gdk::Key::Down {
                index + 1
            } else {
                (index - 1).max(0)
            };
            if let Some(row) = list_key.row_at_index(next) {
                list_key.select_row(Some(&row));
            }
            gtk::glib::Propagation::Stop
        }
        _ => gtk::glib::Propagation::Proceed,
    });
    // Capture Escape from every child, including the list and Close button.
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    window.add_controller(keys);

    LauncherUi {
        window,
        search,
        list,
        message,
        catalog,
        visible_ids,
        launching,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn search_is_unicode_safe_and_case_insensitive() {
        let app = DesktopApplication {
            id: "org.example.Terminal.desktop".into(),
            name: "Терминал".into(),
            icon: None,
        };
        assert!(matches_search(&app, "терм"));
        assert!(matches_search(&app, "TERMINAL"));
        assert!(matches_search(&app, "  "));
        assert!(!matches_search(&app, "browser"));
        assert!(!matches_search(&app, &"z".repeat(500)));
    }

    #[test]
    fn native_rpc_requires_core_socket_and_no_arbitrary_command() {
        assert!(call_core(None, "launch_application", Some("ok.desktop".into())).is_err());
        // Only fixed method strings are used by the native UI, never a search term.
    }
}
