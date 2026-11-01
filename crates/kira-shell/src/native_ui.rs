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
