//! Non-exclusive Wayland layer-shell window for the Svelte Calendar widget.
//! GTK owns only the window, trusted WebKit bridge and native clipboard.
use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use gtk::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use webkit6::prelude::*;

const TITLE: &str = "Kira Calendar";
const CALENDAR_MARKER: &str = "<html lang=\"en\">";
const CALENDAR_SURFACE: &str = "<html lang=\"en\" data-kira-surface=\"calendar\">";

#[derive(Debug, PartialEq, Eq)]
enum UiCommand<'a> {
    Close,
    CopyDate(&'a str),
}

/// Revalidate untrusted date-copy requests without duplicating the Svelte
/// calendar's grid, navigation or ISO week model in Rust.
fn valid_date(year: i32, month: u8, day: u8) -> bool {
    if !(1..=9999).contains(&year) || !(1..=12).contains(&month) {
        return false;
    }
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let last = match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    (1..=last).contains(&day)
}

/// Only the exact ISO format is allowed into the OS clipboard.
fn calendar_command(message: &str) -> Option<UiCommand<'_>> {
    if message == "close" {
        return Some(UiCommand::Close);
    }
    let date = message.strip_prefix("copy:")?;
    let bytes = date.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !bytes.iter().enumerate().all(|(i, b)| i == 4 || i == 7 || b.is_ascii_digit())
    {
        return None;
    }
    let year = date[..4].parse().ok()?;
    let month = date[5..7].parse().ok()?;
    let day = date[8..10].parse().ok()?;
    if !valid_date(year, month, day) {
        return None;
    }
    Some(UiCommand::CopyDate(date))
}

/// A single trusted frontend bundle serves both surfaces; the calendar marker
/// selects CalendarWidget at mount time, without granting it Core RPC access.
fn calendar_document(html: &str) -> Option<String> {
    html.contains(CALENDAR_MARKER)
        .then(|| html.replacen(CALENDAR_MARKER, CALENDAR_SURFACE, 1))
}

pub fn dismiss(application: &gtk::Application) -> bool {
    crate::native_ui::dismiss_overlay(application, TITLE)
}

pub fn toggle(application: &gtk::Application, html: &str) {
    if dismiss(application) {
        return;
    }
    let Some(document) = calendar_document(html) else {
        eprintln!("kira-shell: calendar HTML root not found");
        return;
    };

    crate::launcher::dismiss(application);
    crate::system_panel::dismiss(application);

    let window = gtk::ApplicationWindow::new(application);
    window.set_title(Some(TITLE));
    window.set_decorated(false);
    window.init_layer_shell();
    window.set_namespace(Some("kira-calendar"));
    window.set_layer(Layer::Overlay);
    window.set_anchor(Edge::Top, true);
    window.set_anchor(Edge::Right, true);
    window.set_margin(Edge::Top, crate::native_ui::POPUP_TOP_MARGIN);
    window.set_margin(Edge::Right, 12);
    window.set_exclusive_zone(0);
    window.set_keyboard_mode(KeyboardMode::OnDemand);
    crate::native_ui::configure_webview_popup(&window, 378, 560);

    // Separate content manager: no kira (Core RPC) or other overlay controls
    // are registered in the calendar renderer.
    let context = webkit6::WebContext::new();
    let manager = webkit6::UserContentManager::new();
    let view = webkit6::WebView::builder()
        .web_context(&context)
        .user_content_manager(&manager)
        .build();
    view.set_background_color(&gtk::gdk::RGBA::new(0.0, 0.0, 0.0, 1.0));
    crate::native_ui::restrict_webview_navigation(&view);

    let weak_window = window.downgrade();
    manager.connect_script_message_received(Some("kiraCalendarUi"), move |_, value| {
        let payload = value.to_string();
        match calendar_command(&payload) {
            Some(UiCommand::Close) => {
                if let Some(window) = weak_window.upgrade() {
                    window.close();
                }
            }
            Some(UiCommand::CopyDate(date)) => {
                if let Some(display) = gtk::gdk::Display::default() {
                    display.clipboard().set_text(date);
                }
            }
            None => eprintln!("kira-shell: rejected invalid calendar UI command"),
        }
    });
    assert!(manager.register_script_message_handler("kiraCalendarUi", None));

    manager.connect_script_message_received(Some("kiraDiagnostics"), |_, value| {
        eprintln!("kira-shell [calendar]: {}", value.to_string());
    });
    assert!(manager.register_script_message_handler("kiraDiagnostics", None));

    view.connect_load_failed(|_, event, uri, error| {
        eprintln!("kira-shell: calendar WebKit load failed ({event:?}) for {uri}: {error}");
        false
    });
    view.connect_web_process_terminated(|_, reason| {
        eprintln!("kira-shell: calendar WebKit renderer terminated: {reason:?}");
    });
    view.connect_load_changed(|webview, event| {
        if event == webkit6::LoadEvent::Finished {
            let webview = webview.clone();
            gtk::glib::timeout_add_local_once(Duration::from_millis(1500), move || {
                webview.evaluate_javascript(
                    "if (!document.querySelector('main[data-kira-ui-root][aria-label=\"Kira Calendar\"]')) { throw new Error('Svelte Calendar did not mount'); }",
                    None,
                    None,
                    None::<&gtk::gio::Cancellable>,
                    |result| {
                        if let Err(error) = result {
                            eprintln!("kira-shell: Svelte calendar DOM check failed: {error}");
                        } else {
                            eprintln!("kira-shell: Svelte calendar DOM verified");
                        }
                    },
                );
            });
        }
    });

    // On-demand keyboard focus: close the popup when it loses active focus,
    // but never close the persistent 42px top bar.
    let was_active = Rc::new(Cell::new(false));
    window.connect_is_active_notify(move |window| {
        if window.is_active() {
            was_active.set(true);
        } else if was_active.get() {
            window.close();
        }
    });

    view.load_html(&document, None);
    window.set_child(Some(&view));
    window.present();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_untrusted_calendar_commands() {
        for invalid in [
            "", "COPY", "toggle", "copy:", "copy:2026-02-30", "copy:1900-02-29",
            "copy:0000-01-01", "copy:2026-13-01", "copy:2026-11-1",
            "copy:2026-11-01\n", "copy:2026-11-01;rm", "copy:🔥",
            r#"{"method":"execute_shell"}"#,
        ] {
            assert_eq!(calendar_command(invalid), None, "{invalid:?}");
        }
        assert_eq!(calendar_command("close"), Some(UiCommand::Close));
        assert_eq!(
            calendar_command("copy:2024-02-29"),
            Some(UiCommand::CopyDate("2024-02-29"))
        );
    }

    #[test]
    fn calendar_uses_trusted_surface_marker_only() {
        assert_eq!(
            calendar_document("<html lang=\"en\"><body></body></html>").as_deref(),
            Some("<html lang=\"en\" data-kira-surface=\"calendar\"><body></body></html>")
        );
        assert!(calendar_document("<html><body></body></html>").is_none());
    }
}
