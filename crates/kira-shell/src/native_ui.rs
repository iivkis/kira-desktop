//! Shared native overlay plumbing. Layout, interaction and state remain local
//! to each overlay: only identical infrastructure belongs here.
use gtk::prelude::*;

/// Keep palette declarations and component rules in one provider so GTK named
/// colors resolve reliably, including in Nix's source-filtered package.
pub fn install_styles(styles: &str) {
    let provider = gtk::CssProvider::new();
    let complete = format!("{}\n{}", include_str!("native-palette.css"), styles);
    provider.load_from_data(&complete);
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

pub fn find_overlay(application: &gtk::Application, title: &str) -> Option<gtk::Window> {
    application
        .windows()
        .into_iter()
        .find(|window| window.title().as_deref() == Some(title))
}

pub fn dismiss_overlay(application: &gtk::Application, title: &str) -> bool {
    if let Some(window) = find_overlay(application, title) {
        window.close();
        return true;
    }
    false
}

/// The old 50px popup offset left too much space below the Top Bar.
pub const POPUP_TOP_MARGIN: i32 = 25;

/// Let GTK use the natural content height, capped at 70% of the monitor.
/// Fixed preferred heights used to force needless scrolling on tall outputs.
/// Recheck the actual surface monitor after mapping.
fn apply_popup_height(window: &gtk::ApplicationWindow, scroll: &gtk::ScrolledWindow, width: i32) {
    let display = gtk::prelude::WidgetExt::display(window);
    let monitor = window
        .surface()
        .and_then(|surface| display.monitor_at_surface(&surface))
        .or_else(|| {
            display
                .monitors()
                .item(0)
                .and_then(|item| item.downcast::<gtk::gdk::Monitor>().ok())
        });
    let output_height = monitor.map_or(800, |m| m.geometry().height()).max(1);
    let height_limit = (i64::from(output_height) * 7 / 10) as i32;
    scroll.set_min_content_height(1);
    scroll.set_max_content_height(height_limit);
    scroll.set_propagate_natural_height(true);
    // -1 allows the scroll's natural (bounded) height, rather than
    // forcing the former 490px/600px default sizes.
    window.set_default_size(width, -1);
}

pub fn configure_popup(window: &gtk::ApplicationWindow, scroll: &gtk::ScrolledWindow, width: i32) {
    apply_popup_height(window, scroll, width);
    let scroll = scroll.clone();
    window.connect_map(move |window| {
        apply_popup_height(window, &scroll, width);
    });
}
