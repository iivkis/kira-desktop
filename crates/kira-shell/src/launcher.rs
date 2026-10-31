//! First launcher slice: a native, keyboard-accessible GTK layer-shell popup.
//! GIO owns XDG desktop-entry semantics and app launching. No user-provided
//! Exec strings are interpreted or sent through the WebKit bridge.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

const MAX_VISIBLE: usize = 64;

type Catalog = Vec<(String, gtk::gio::AppInfo)>;

/// Keep the search widget directly; do not depend on fragile child traversal
/// when the popup is shown a second time.
pub struct Launcher {
    window: gtk::ApplicationWindow,
    search: gtk::SearchEntry,
    feedback: gtk::Label,
}

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
    count: &gtk::Label,
) {
    // Remove only rows, never the empty-state placeholder.
    while let Some(row) = list.row_at_index(0) {
        list.remove(&row);
    }

    let mut filtered = Vec::new();
    for (name, app) in apps {
        if !matches(name, query) {
            continue;
        }
        let row = gtk::ListBoxRow::new();
        row.add_css_class("kira-app-row");
        row.set_tooltip_text(Some(name));

        let content = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        content.set_margin_start(11);
        content.set_margin_end(11);
        content.set_margin_top(7);
        content.set_margin_bottom(7);

        let icon_shell = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        icon_shell.add_css_class("kira-app-icon-shell");
        icon_shell.set_halign(gtk::Align::Center);
        icon_shell.set_valign(gtk::Align::Center);
        let icon = app
            .icon()
            .map(|gicon| gtk::Image::from_gicon(&gicon))
            .unwrap_or_else(|| gtk::Image::from_icon_name("application-x-executable-symbolic"));
        icon.add_css_class("kira-app-icon");
        icon.set_pixel_size(26);
        icon.set_halign(gtk::Align::Center);
        icon.set_valign(gtk::Align::Center);
        icon_shell.append(&icon);
        content.append(&icon_shell);

        let text = gtk::Box::new(gtk::Orientation::Vertical, 3);
        text.set_hexpand(true);
        text.set_valign(gtk::Align::Center);
        let primary = gtk::Label::new(Some(name));
        primary.set_xalign(0.0);
        primary.set_ellipsize(gtk::pango::EllipsizeMode::End);
        primary.add_css_class("kira-app-name");
        text.append(&primary);

        let detail = app
            .description()
            .map(|description| description.to_string())
            .filter(|description| !description.trim().is_empty())
            .unwrap_or_else(|| {
                app.id()
                    .map(|id| id.trim_end_matches(".desktop").to_string())
                    .unwrap_or_else(|| "Installed application".into())
            });
        let detail: String = detail.chars().take(90).collect();
        let secondary = gtk::Label::new(Some(&detail));
        secondary.set_xalign(0.0);
        secondary.set_ellipsize(gtk::pango::EllipsizeMode::End);
        secondary.set_max_width_chars(45);
        secondary.add_css_class("kira-app-detail");
        text.append(&secondary);
        content.append(&text);

        let arrow = gtk::Image::from_icon_name("go-next-symbolic");
        arrow.add_css_class("kira-app-arrow");
        arrow.set_pixel_size(14);
        arrow.set_valign(gtk::Align::Center);
        content.append(&arrow);

        row.set_child(Some(&content));
        list.append(&row);
        filtered.push(app.clone());
        if filtered.len() >= MAX_VISIBLE {
            break;
        }
    }

    count.set_text(&format!("{} shown", filtered.len()));
    if let Some(first) = list.row_at_index(0) {
        list.select_row(Some(&first));
    }
    *selected.borrow_mut() = filtered;
}

/// Minimal adjustment that keeps a selected row visible without jumping
/// if it is already in view. The viewport can be smaller than a row.
fn visible_offset(
    current: f64,
    page: f64,
    lower: f64,
    upper: f64,
    row_top: f64,
    row_bottom: f64,
) -> f64 {
    if page <= 0.0 {
        return current;
    }
    let desired = if row_top < current {
        row_top
    } else if row_bottom > current + page {
        if row_bottom - row_top > page {
            row_top
        } else {
            row_bottom - page
        }
    } else {
        current
    };
    desired.clamp(lower, (upper - page).max(lower))
}

/// GTK ListBox selection and ScrolledWindow adjustment are independent.
/// Adjust the viewport when keyboard selection moves below/above its edges,
/// without transferring text-input focus away from SearchEntry.
fn ensure_selected_visible(
    list: &gtk::ListBox,
    row: &gtk::ListBoxRow,
    scroll: &gtk::ScrolledWindow,
) {
    let Some(bounds) = row.compute_bounds(list) else {
        return; // Not mapped or laid out yet (initial population).
    };
    let adjustment = scroll.vadjustment();
    let current = adjustment.value();
    let target = visible_offset(
        current,
        adjustment.page_size(),
        adjustment.lower(),
        adjustment.upper(),
        f64::from(bounds.y()),
        f64::from(bounds.y() + bounds.height()),
    );
    if (target - current).abs() > f64::EPSILON {
        adjustment.set_value(target);
    }
}

/// A separate, floating GTK layer-shell surface, created only after the
/// user clicks KIRA (not on normal Top Bar startup).
pub fn create(app: &gtk::Application) -> Launcher {
    let popup = gtk::ApplicationWindow::new(app);
    popup.set_title(Some("Kira Applications"));
    popup.set_decorated(false);
    popup.set_resizable(false);
    popup.set_default_size(478, 558);
    popup.add_css_class("kira-launcher-window");
    popup.init_layer_shell();
    popup.set_namespace(Some("kira-launcher"));
    popup.set_layer(Layer::Overlay);
    popup.set_anchor(Edge::Top, true);
    popup.set_anchor(Edge::Left, true);
    popup.set_margin(Edge::Top, 46);
    popup.set_margin(Edge::Left, 10);
    popup.set_exclusive_zone(0);
    popup.set_keyboard_mode(KeyboardMode::Exclusive);

    let provider = gtk::CssProvider::new();
    provider.connect_parsing_error(|_, _, error| {
        eprintln!("kira-launcher: GTK theme CSS: {error}");
    });
    provider.load_from_data(include_str!("launcher.css"));
    if let Some(display) = gtk::gdk::Display::default() {
        #[allow(deprecated)]
        gtk::StyleContext::add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }

    // A transparent root leaves breathing room around the rounded card
    // and allows its shadow to render without claiming extra exclusive zone.
    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
    root.set_margin_start(10);
    root.set_margin_end(10);
    root.set_margin_top(10);
    root.set_margin_bottom(10);
    let card = gtk::Box::new(gtk::Orientation::Vertical, 0);
    card.add_css_class("kira-launcher-card");

    let header = gtk::Box::new(gtk::Orientation::Vertical, 4);
    header.set_margin_start(22);
    header.set_margin_end(22);
    header.set_margin_top(21);
    header.set_margin_bottom(18);

    let kicker = gtk::Label::new(Some("KIRA  /  DESKTOP"));
    kicker.add_css_class("kira-eyebrow");
    kicker.set_xalign(0.0);
    header.append(&kicker);

    let title = gtk::Label::new(Some("Applications"));
    title.add_css_class("kira-heading");
    title.set_xalign(0.0);
    title.set_margin_top(4);
    header.append(&title);

    let subtitle = gtk::Label::new(Some("Find what you need, without the noise."));
    subtitle.add_css_class("kira-subheading");
    subtitle.set_xalign(0.0);
    header.append(&subtitle);
    card.append(&header);

    let search = gtk::SearchEntry::new();
    search.add_css_class("kira-search-field");
    search.set_placeholder_text(Some("Search apps..."));
    search.set_margin_start(18);
    search.set_margin_end(18);
    search.set_margin_bottom(20);
    search.set_size_request(-1, 47);
    card.append(&search);

    let section = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    section.set_margin_start(23);
    section.set_margin_end(23);
    section.set_margin_bottom(9);
    let section_title = gtk::Label::new(Some("YOUR APPLICATIONS"));
    section_title.add_css_class("kira-section-label");
    section_title.set_xalign(0.0);
    section_title.set_hexpand(true);
    section.append(&section_title);
    let count = gtk::Label::new(None);
    count.add_css_class("kira-count");
    section.append(&count);
    card.append(&section);

    let list = gtk::ListBox::new();
    list.add_css_class("kira-results-list");
    list.set_selection_mode(gtk::SelectionMode::Single);
    list.set_activate_on_single_click(true);
    let empty = gtk::Box::new(gtk::Orientation::Vertical, 6);
    empty.set_margin_top(45);
    empty.set_margin_bottom(45);
    let empty_icon = gtk::Image::from_icon_name("edit-find-symbolic");
    empty_icon.set_pixel_size(24);
    empty.append(&empty_icon);
    let empty_title = gtk::Label::new(Some("Nothing found"));
    empty_title.add_css_class("kira-empty-title");
    empty.append(&empty_title);
    let empty_detail = gtk::Label::new(Some("Try a different application name."));
    empty_detail.add_css_class("kira-empty-detail");
    empty.append(&empty_detail);
    list.set_placeholder(Some(&empty));

    let scroll = gtk::ScrolledWindow::new();
    scroll.add_css_class("kira-results-scroll");
    scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroll.set_vexpand(true);
    scroll.set_margin_start(11);
    scroll.set_margin_end(11);
    scroll.set_child(Some(&list));
    {
        let scroll = scroll.clone();
        // Preserve auto-scroll when ↑/↓ moves past the visible viewport.
        list.connect_row_selected(move |list, selected| {
            if let Some(row) = selected {
                ensure_selected_visible(list, row, &scroll);
            }
        });
    }
    card.append(&scroll);

    let feedback = gtk::Label::new(None);
    feedback.add_css_class("kira-launch-error");
    feedback.set_xalign(0.0);
    feedback.set_wrap(true);
    feedback.set_margin_start(19);
    feedback.set_margin_end(19);
    feedback.set_margin_bottom(8);
    feedback.set_visible(false);
    card.append(&feedback);

    let footer = gtk::Box::new(gtk::Orientation::Horizontal, 9);
    footer.add_css_class("kira-footer");
    footer.set_margin_start(20);
    footer.set_margin_end(20);
    footer.set_margin_top(8);
    footer.set_margin_bottom(16);
    for (key, hint) in [("↵", "Open"), ("↑ ↓", "Navigate"), ("Esc", "Close")] {
        let key_label = gtk::Label::new(Some(key));
        key_label.add_css_class("kira-keycap");
        footer.append(&key_label);
        let hint_label = gtk::Label::new(Some(hint));
        hint_label.add_css_class("kira-footer-hint");
        footer.append(&hint_label);
    }
    card.append(&footer);
    root.append(&card);
    popup.set_child(Some(&root));

    // GIO owns visibility, icon and Exec semantics. Never accept an Exec
    // command or app path from the WebKit bridge.
    let apps = Rc::new(catalog());
    let filtered = Rc::new(RefCell::new(Vec::new()));

    {
        let apps = apps.clone();
        let filtered = filtered.clone();
        let list = list.clone();
        let count = count.clone();
        search.connect_search_changed(move |entry| {
            let query: String = entry.text().chars().take(128).collect();
            fill(&list, &apps, &query, &filtered, &count);
        });
    }

    {
        let filtered = filtered.clone();
        let popup = popup.clone();
        let feedback = feedback.clone();
        list.connect_row_activated(move |_, row| {
            let app = filtered.borrow().get(row.index() as usize).cloned();
            if let Some(app) = app {
                match app.launch(&[], None::<&gtk::gio::AppLaunchContext>) {
                    Ok(()) => popup.set_visible(false),
                    Err(error) => {
                        eprintln!("kira-launcher: app launch failed: {error}");
                        feedback.set_text(&format!("Cannot open application: {error}"));
                        feedback.set_visible(true);
                    }
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
                popup.set_visible(false);
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

    fill(&list, &apps, "", &filtered, &count);
    Launcher {
        window: popup,
        search,
        feedback,
    }
}

pub fn toggle(launcher: &Launcher) {
    if launcher.window.is_visible() {
        launcher.window.set_visible(false);
    } else {
        // Start from a clean, predictable search each time the overlay opens.
        launcher.feedback.set_visible(false);
        launcher.search.set_text("");
        launcher.window.present();
        launcher.search.grab_focus();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_row_scrolls_only_when_outside_viewport() {
        // A 100px viewport starts at y=0: visible rows do not jump.
        assert_eq!(visible_offset(0.0, 100.0, 0.0, 1000.0, 10.0, 40.0), 0.0);
        // Moving down one 40px row at a time scrolls only at the edge.
        assert_eq!(visible_offset(0.0, 100.0, 0.0, 1000.0, 80.0, 120.0), 20.0);
        assert_eq!(visible_offset(20.0, 100.0, 0.0, 1000.0, 120.0, 160.0), 60.0);
        // Reverse direction: scroll back up when selection leaves viewport.
        assert_eq!(visible_offset(120.0, 100.0, 0.0, 1000.0, 60.0, 100.0), 60.0);
        assert_eq!(visible_offset(60.0, 100.0, 0.0, 1000.0, 0.0, 40.0), 0.0);
        // Bottom of list is clamped to upper - page size.
        assert_eq!(
            visible_offset(850.0, 100.0, 0.0, 900.0, 865.0, 910.0),
            800.0
        );
        // Rows taller than the viewport align their top, not their bottom.
        assert_eq!(visible_offset(0.0, 80.0, 0.0, 800.0, 100.0, 240.0), 100.0);
    }

    #[test]
    fn empty_or_unlaid_out_viewport_does_not_scroll() {
        assert_eq!(visible_offset(25.0, 0.0, 0.0, 500.0, 90.0, 120.0), 25.0);
        assert_eq!(visible_offset(0.0, 100.0, 0.0, 80.0, 0.0, 60.0), 0.0);
    }

    #[test]
    fn matching_handles_unicode_and_empty_queries() {
        assert!(matches("Firefox Web Browser", "FIREfox"));
        assert!(matches("Терминал", "термин"));
        assert!(matches("App", ""));
        assert!(!matches("App", "missing"));
    }
}
