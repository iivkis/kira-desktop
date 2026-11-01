//! Native launcher: desktop entries via Gio, never shell-interpolated commands.
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Once;

use gtk::gio;
use gtk::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

const TITLE: &str = "Kira Launcher";

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
                info.description()
                    .map(|value| value.to_string())
                    .unwrap_or_default()
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

// GtkListBoxRow::index() is a view position, not an application's identity.
// Filtering and selection must always resolve a row to its original Gio entry.
fn app_index(row: &gtk::ListBoxRow) -> Option<usize> {
    row.widget_name().strip_prefix("kira-app-")?.parse().ok()
}

fn visible_rows(list: &gtk::ListBox, count: usize) -> Vec<gtk::ListBoxRow> {
    (0..count)
        .filter_map(|index| list.row_at_index(index as i32))
        .filter(|row| row.is_child_visible())
        .collect()
}

fn launch(
    apps: &[App],
    row: &gtk::ListBoxRow,
    window: &gtk::ApplicationWindow,
    error: &gtk::Label,
) {
    let Some(index) = app_index(row) else {
        return;
    };
    if let Some(app) = apps.get(index) {
        match app.info.launch(&[], None::<&gio::AppLaunchContext>) {
            Ok(()) => window.close(),
            Err(failure) => {
                error.set_text(&format!(
                    "Cannot open {}: {failure}",
                    app.info.display_name()
                ));
                error.set_visible(true);
            }
        }
    }
}

fn install_style() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| crate::native_ui::install_styles(include_str!("launcher.css")));
}

/// Close only the Launcher when switching native overlays.
pub fn dismiss(application: &gtk::Application) -> bool {
    crate::native_ui::dismiss_overlay(application, TITLE)
}

/// Show a single keyboard-accessible layer-shell overlay without reserving space.
pub fn show(application: &gtk::Application) {
    if let Some(existing) = crate::native_ui::find_overlay(application, TITLE) {
        existing.present();
        return;
    }
    crate::calendar::dismiss(application);
    crate::system_panel::dismiss(application);
    install_style();
    let window = gtk::ApplicationWindow::new(application);
    window.set_title(Some(TITLE));
    window.set_decorated(false);
    window.set_default_size(600, 520);
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
    let card = gtk::Box::new(gtk::Orientation::Vertical, 0);
    card.add_css_class("kira-launcher-card");
    frame.append(&card);
    window.set_child(Some(&frame));

    // Editorial hierarchy: a quiet black intro and a Prussian Blue workspace.
    // This is one panel with two clear regions, not a stack of framed chips.
    let intro = gtk::Box::new(gtk::Orientation::Vertical, 6);
    intro.add_css_class("kira-launcher-intro");
    intro.append(&label("KIRA  /  APPLICATION INDEX", "kira-eyebrow"));
    intro.append(&label("Applications", "kira-heading"));
    intro.append(&label("Search your installed tools.", "kira-subheading"));
    card.append(&intro);

    let workbench = gtk::Box::new(gtk::Orientation::Vertical, 10);
    workbench.add_css_class("kira-workbench");
    card.append(&workbench);
    workbench.append(&label("FIND  /  OPEN", "kira-workbench-eyebrow"));

    let entry = gtk::SearchEntry::new();
    entry.add_css_class("kira-search-field");
    entry.set_placeholder_text(Some("Search applications…"));
    workbench.append(&entry);

    let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    header.add_css_class("kira-catalog-header");
    header.append(&label("APPLICATION CATALOG", "kira-catalog-label"));
    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    header.append(&spacer);
    let count = label("", "kira-count");
    count.set_xalign(1.0);
    header.append(&count);
    workbench.append(&header);

    let apps = Rc::new(installed_apps());
    let list = gtk::ListBox::new();
    list.add_css_class("kira-app-list");
    list.set_selection_mode(gtk::SelectionMode::Single);
    list.set_activate_on_single_click(true);
    for (index, app) in apps.iter().enumerate() {
        let row = gtk::ListBoxRow::new();
        row.add_css_class("kira-app-row");
        row.set_widget_name(&format!("kira-app-{index}"));
        let content = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        let rail = gtk::Box::new(gtk::Orientation::Vertical, 0);
        rail.add_css_class("kira-app-rail");
        content.append(&rail);
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
        let secondary = app
            .info
            .description()
            .map(|value| value.to_string())
            .unwrap_or_default();
        let secondary = if secondary.trim().is_empty() {
            "Installed application"
        } else {
            secondary.trim()
        };
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
    scroll.set_min_content_height(200);
    scroll.set_vexpand(true);
    scroll.set_child(Some(&list));
    workbench.append(&scroll);

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
        app_index(row)
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
                let current = key_list
                    .selected_row()
                    .and_then(|selected| visible.iter().position(|row| row == &selected));
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
