//! Native launcher: desktop entries via Gio, never shell-interpolated commands.
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Once;

use gtk::gio;
use gtk::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

struct App {
    info: gio::AppInfo,
    search: String,
}

fn installed_apps() -> Vec<App> {
    let mut apps: Vec<_> = gio::AppInfo::all()
        .into_iter()
        .filter(|info| info.should_show())
        .map(|info| {
            let search = format!(
                "{} {}",
                info.display_name(),
                info.description().map(|value| value.to_string()).unwrap_or_default()
            )
            .to_lowercase();
            App { info, search }
        })
        .collect();
    apps.sort_by_key(|app| app.info.display_name().to_string().to_lowercase());
    apps
}

fn label(value: &str, class: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(value));
    label.add_css_class(class);
    label.set_xalign(0.0);
    label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    label
}

fn visible_rows(list: &gtk::ListBox, count: usize) -> Vec<gtk::ListBoxRow> {
    (0..count)
        .filter_map(|index| list.row_at_index(index as i32))
        .filter(|row| row.child_visible())
        .collect()
}

fn launch(apps: &[App], row: &gtk::ListBoxRow, window: &gtk::ApplicationWindow, error: &gtk::Label) {
    let Ok(index) = usize::try_from(row.index()) else {
        return;
    };
    if let Some(app) = apps.get(index) {
        match app.info.launch(&[], None::<&gio::AppLaunchContext>) {
            Ok(()) => window.close(),
            Err(failure) => {
                error.set_text(&format!("Cannot open {}: {failure}", app.info.display_name()));
                error.set_visible(true);
            }
        }
    }
}

fn install_style() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let provider = gtk::CssProvider::new();
        provider.load_from_data(include_str!("launcher.css"));
        if let Some(display) = gtk::gdk::Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }
    });
}

/// Show a single keyboard-accessible layer-shell overlay without reserving space.
pub fn show(application: &gtk::Application) {
    if let Some(existing) = application
        .windows()
        .into_iter()
        .find(|window| window.title().as_deref() == Some("Kira Launcher"))
    {
        existing.present();
        return;
    }
    install_style();
    let window = gtk::ApplicationWindow::new(application);
    window.set_title(Some("Kira Launcher"));
    window.set_decorated(false);
    window.set_default_size(600, 500);
    window.add_css_class("kira-launcher-window");
    window.init_layer_shell();
    window.set_namespace(Some("kira-launcher"));
    window.set_layer(Layer::Overlay);
    window.set_anchor(Edge::Top, true);
    window.set_margin(Edge::Top, 58);
    window.set_exclusive_zone(0);
    window.set_keyboard_mode(KeyboardMode::Exclusive);

    let frame = gtk::Box::new(gtk::Orientation::Vertical, 0);
    frame.set_margin_start(12);
    frame.set_margin_end(12);
    frame.set_margin_top(6);
    frame.set_margin_bottom(12);
    let card = gtk::Box::new(gtk::Orientation::Vertical, 12);
    card.add_css_class("kira-launcher-card");
    frame.append(&card);
    window.set_child(Some(&frame));
    card.append(&label("KIRA / DESKTOP", "kira-eyebrow"));
    card.append(&label("Applications", "kira-heading"));
    card.append(&label("Find and open installed applications.", "kira-subheading"));

    let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    header.add_css_class("kira-catalog-header");
    header.append(&label("CATALOG", "kira-eyebrow"));
    let count = label("", "kira-count");
    count.set_hexpand(true);
    count.set_xalign(1.0);
    header.append(&count);
    card.append(&header);

    let entry = gtk::SearchEntry::new();
    entry.add_css_class("kira-search-field");
    entry.set_placeholder_text(Some("Search applications…"));
    card.append(&entry);

    let apps = Rc::new(installed_apps());
    let list = gtk::ListBox::new();
    list.add_css_class("kira-app-list");
    list.set_selection_mode(gtk::SelectionMode::Single);
    list.set_activate_on_single_click(true);
    for app in apps.iter() {
        let row = gtk::ListBoxRow::new();
        row.add_css_class("kira-app-row");
        let content = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        let icon_shell = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        icon_shell.add_css_class("kira-app-icon-shell");
        if let Some(icon) = app.info.icon() {
            let image = gtk::Image::from_gicon(&icon);
            image.set_pixel_size(24);
            icon_shell.append(&image);
        } else {
            icon_shell.append(&label("K", "kira-app-icon-fallback"));
        }
        content.append(&icon_shell);
        let description = gtk::Box::new(gtk::Orientation::Vertical, 3);
        description.set_hexpand(true);
        description.append(&label(&app.info.display_name(), "kira-app-name"));
        let secondary = app.info.description().map(|value| value.to_string()).unwrap_or_default();
        let secondary = if secondary.trim().is_empty() { "Installed application" } else { secondary.trim() };
        description.append(&label(secondary, "kira-app-detail"));
        content.append(&description);
        row.set_child(Some(&content));
        list.append(&row);
    }
    let empty = label("No matching applications", "kira-empty");
    empty.set_margin_top(16);
    empty.set_margin_bottom(16);
    list.set_placeholder(Some(&empty));
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroll.set_min_content_height(240);
    scroll.set_vexpand(true);
    scroll.set_child(Some(&list));
    card.append(&scroll);

    let error = label("", "kira-launch-error");
    error.set_visible(false);
    card.append(&error);
    let footer = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    footer.add_css_class("kira-launcher-footer");
    for (key, description) in [("↑ ↓", "Navigate"), ("ENTER", "Open"), ("ESC", "Close")] {
        footer.append(&label(key, "kira-keycap"));
        footer.append(&label(description, "kira-key-hint"));
    }
    card.append(&footer);

    let filter = Rc::new(RefCell::new(String::new()));
    let filtered_apps = apps.clone();
    let query = filter.clone();
    list.set_filter_func(move |row| {
        usize::try_from(row.index())
            .ok()
            .and_then(|index| filtered_apps.get(index))
            .is_some_and(|app| app.search.contains(query.borrow().as_str()))
    });
    count.set_text(&format!("{} APPS", apps.len()));
    if let Some(row) = list.row_at_index(0) {
        list.select_row(Some(&row));
    }

    let search_list = list.clone();
    let search_apps = apps.clone();
    entry.connect_search_changed(move |entry| {
        *filter.borrow_mut() = entry.text().to_string().to_lowercase();
        search_list.invalidate_filter();
        let visible = visible_rows(&search_list, search_apps.len());
        count.set_text(&format!("{} / {} APPS", visible.len(), search_apps.len()));
        search_list.select_row(visible.first());
    });
    let activate_apps = apps.clone();
    let activate_window = window.clone();
    let activate_error = error.clone();
    list.connect_row_activated(move |_, row| {
        launch(&activate_apps, row, &activate_window, &activate_error);
    });

    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    let key_list = list.clone();
    let key_apps = apps.clone();
    let key_window = window.clone();
    let key_error = error.clone();
    keys.connect_key_pressed(move |_, key, _, _| match key {
        gtk::gdk::Key::Escape => {
            key_window.close();
            gtk::glib::Propagation::Stop
        }
        gtk::gdk::Key::Down | gtk::gdk::Key::Up => {
            let visible = visible_rows(&key_list, key_apps.len());
            if !visible.is_empty() {
                let current = key_list.selected_row().and_then(|selected| {
                    visible.iter().position(|row| row == &selected)
                });
                let next = match (current, key) {
                    (Some(index), gtk::gdk::Key::Down) => (index + 1).min(visible.len() - 1),
                    (Some(index), _) => index.saturating_sub(1),
                    (None, _) => 0,
                };
                key_list.select_row(visible.get(next));
            }
            gtk::glib::Propagation::Stop
        }
        gtk::gdk::Key::Return | gtk::gdk::Key::KP_Enter => {
            if let Some(row) = key_list.selected_row() {
                launch(&key_apps, &row, &key_window, &key_error);
            }
            gtk::glib::Propagation::Stop
        }
        _ => gtk::glib::Propagation::Proceed,
    });
    window.add_controller(keys);
    window.present();
    entry.grab_focus();
}

#[cfg(test)]
mod tests {
    #[test]
    fn launcher_style_has_all_sections() {
        let css = include_str!("launcher.css");
        for selector in [
            ".kira-launcher-card",
            ".kira-app-row:selected",
            ".kira-keycap",
            ".kira-eyebrow",
            ".kira-heading",
            ".kira-app-icon-shell",
        ] {
            assert!(css.contains(selector), "missing {selector}");
        }
    }
}
