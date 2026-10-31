//! First launcher slice: a native, keyboard-accessible GTK layer-shell popup.
//! GIO owns XDG desktop-entry semantics and app launching. No user-provided
//! Exec strings are interpreted or sent through the WebKit bridge.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

const MAX_VISIBLE: usize = 64;

type Catalog = Vec<(String, gtk::gio::AppInfo)>;

fn catalog() -> Catalog {
    let mut seen = std::collections::HashSet::new();
    let mut apps = Vec::new();
    for app in gtk::gio::AppInfo::all() {
        if !app.should_show() {
            continue;
        }
        let Some(id) = app.id() else {
            continue;
        };
        if id.len() > 256 || !seen.insert(id.to_string()) {
            continue;
        }
        let name = app.display_name().to_string();
        if name.is_empty() || name.len() > 256 {
            continue;
        }
        apps.push((name, app));
    }
    apps.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
    apps
}

/// Case-insensitive text matching with bounded results; no shell evaluation.
fn matches(name: &str, query: &str) -> bool {
    name.to_lowercase().contains(&query.trim().to_lowercase())
}

fn fill(
    list: &gtk::ListBox,
    apps: &Catalog,
    query: &str,
    selected: &Rc<RefCell<Vec<gtk::gio::AppInfo>>>,
) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    let mut filtered = Vec::new();
    for (name, app) in apps {
        if !matches(name, query) {
            continue;
        }
        let row = gtk::ListBoxRow::new();
        row.add_css_class("kira-launcher-row");
        let label = gtk::Label::new(Some(name));
        label.set_xalign(0.0);
        label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        label.set_margin_start(14);
        label.set_margin_end(14);
        label.set_margin_top(9);
        label.set_margin_bottom(9);
        row.set_child(Some(&label));
        list.append(&row);
        filtered.push(app.clone());
        if filtered.len() >= MAX_VISIBLE {
            break;
        }
    }

    if !filtered.is_empty() {
        if let Some(first) = list.row_at_index(0) {
            list.select_row(Some(&first));
        }
    }
    *selected.borrow_mut() = filtered;
}

pub fn create(app: &gtk::Application) -> gtk::ApplicationWindow {
    let popup = gtk::ApplicationWindow::new(app);
    popup.set_title(Some("Kira App Launcher"));
    popup.set_decorated(false);
    popup.set_resizable(false);
    popup.set_default_size(420, 470);
    popup.init_layer_shell();
    popup.set_namespace(Some("kira-launcher"));
    popup.set_layer(Layer::Overlay);
    popup.set_anchor(Edge::Top, true);
    popup.set_anchor(Edge::Left, true);
    popup.set_margin(Edge::Top, 50);
    popup.set_margin(Edge::Left, 12);
    popup.set_exclusive_zone(0);
    popup.set_keyboard_mode(KeyboardMode::Exclusive);

    let provider = gtk::CssProvider::new();
    provider.load_from_data(
        ".kira-launcher {background:#0a0908; color:#f2f4f3; border:1px solid #5e503f;}\n\
         .kira-launcher entry {background:#49111c; color:#f2f4f3; border:1px solid #a9927d; border-radius:8px;}\n\
         .kira-launcher list {background:#0a0908;}\n\
         .kira-launcher row {color:#f2f4f3;}\n\
         .kira-launcher row:selected {background:#a9927d; color:#0a0908;}\n\
         .kira-launcher row:hover {background:#49111c; color:#f2f4f3;}\n\
         .kira-launcher label {color:inherit;}\n",
    );
    if let Some(display) = gtk::gdk::Display::default() {
        #[allow(deprecated)]
        gtk::StyleContext::add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }

    let container = gtk::Box::new(gtk::Orientation::Vertical, 8);
    container.add_css_class("kira-launcher");
    container.set_margin_start(0);
    container.set_margin_end(0);
    container.set_margin_top(0);
    container.set_margin_bottom(0);

    let header = gtk::Label::new(Some("KIRA  /  APPLICATIONS"));
    header.set_xalign(0.0);
    header.set_margin_start(14);
    header.set_margin_top(14);
    header.set_margin_bottom(2);
    container.append(&header);

    let search = gtk::SearchEntry::new();
    search.set_placeholder_text(Some("Search installed applications…"));
    search.set_margin_start(10);
    search.set_margin_end(10);
    container.append(&search);

    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::Single);
    list.set_activate_on_single_click(true);
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroll.set_vexpand(true);
    scroll.set_child(Some(&list));
    container.append(&scroll);

    let footer = gtk::Label::new(Some("Enter: launch  •  ↑↓: select  •  Esc: close"));
    footer.set_xalign(0.0);
    footer.set_margin_start(14);
    footer.set_margin_bottom(12);
    container.append(&footer);
    popup.set_child(Some(&container));

    // One read of the app catalog when the popup is created. GTK/GIO
    // already interprets XDG desktop IDs, visibility and Exec field codes.
    let apps = Rc::new(catalog());
    let filtered = Rc::new(RefCell::new(Vec::new()));

    {
        let apps = apps.clone();
        let filtered = filtered.clone();
        let list = list.clone();
        search.connect_search_changed(move |entry| {
            fill(&list, &apps, entry.text().as_str(), &filtered);
        });
    }

    {
        let filtered = filtered.clone();
        let popup = popup.clone();
        list.connect_row_activated(move |_, row| {
            let app = filtered.borrow().get(row.index() as usize).cloned();
            if let Some(app) = app {
                // GIO launch semantics, never sh -c, and ONLY a catalog entry.
                match app.launch(&[], None::<&gtk::gio::AppLaunchContext>) {
                    Ok(()) => popup.hide(),
                    Err(error) => eprintln!("kira-launcher: app launch failed: {error}"),
                }
            }
        });
    }

    {
        let list = list.clone();
        search.connect_activate(move |_| {
            if let Some(row) = list.selected_row().or_else(|| list.row_at_index(0)) {
                row.emit_activate();
            }
        });
    }

    {
        let popup = popup.clone();
        let list = list.clone();
        let keys = gtk::EventControllerKey::new();
        keys.connect_key_pressed(move |_, key, _, _| {
            if key == gtk::gdk::Key::Escape {
                popup.hide();
                return gtk::glib::Propagation::Stop;
            }
            if key == gtk::gdk::Key::Down {
                if let Some(current) = list.selected_row() {
                    if let Some(next) = list.row_at_index(current.index() + 1) {
                        list.select_row(Some(&next));
                    }
                }
                return gtk::glib::Propagation::Stop;
            }
            if key == gtk::gdk::Key::Up {
                if let Some(current) = list.selected_row() {
                    if let Some(prev) = list.row_at_index(current.index() - 1) {
                        list.select_row(Some(&prev));
                    }
                }
                return gtk::glib::Propagation::Stop;
            }
            gtk::glib::Propagation::Proceed
        });
        search.add_controller(keys);
    }

    // GTK focus is re-established on every open; this popup is never shown
    // at startup and cannot spawn any process without a user activation.
    fill(&list, &apps, "", &filtered);
    popup
}

pub fn toggle(popup: &gtk::ApplicationWindow) {
    if popup.is_visible() {
        popup.hide();
    } else {
        popup.present();
        if let Some(root) = popup.child()
            && let Some(search) = root.first_child().and_then(|x| x.next_sibling())
        {
            search.grab_focus();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matching_handles_unicode_and_empty_queries() {
        assert!(matches("Firefox Web Browser", "FIREfox"));
        assert!(matches("Терминал", "термин"));
        assert!(matches("App", ""));
        assert!(!matches("App", "missing"));
    }
}
