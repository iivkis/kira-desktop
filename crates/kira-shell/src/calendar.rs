//! Reusable, prewarmed WebKit/layer-shell host for the Svelte Calendar.
//! GTK owns the window and clipboard; calendar rendering belongs to Svelte.
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gtk::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use webkit6::prelude::*;

const TITLE: &str = "Kira Calendar";
const CALENDAR_MARKER: &str = "<html lang=\"en\">";
const CALENDAR_SURFACE: &str = "<html lang=\"en\" data-kira-surface=\"calendar\">";

thread_local! {
    // Only the GTK thread may touch GTK/WebKit. Hold the view for the entire
    // shell session instead of constructing a new WebKit process per click.
    static CALENDAR: RefCell<Option<Rc<CalendarOverlay>>> = const { RefCell::new(None) };
}

struct CalendarOverlay {
    window: gtk::ApplicationWindow,
    view: webkit6::WebView,
    document: String,
    ready: Cell<bool>,
    requested: Cell<bool>,
    revealed: Cell<bool>,
    paint_pending: Cell<bool>,
    generation: Cell<u64>,
    was_active: Cell<bool>,
}

impl CalendarOverlay {
    fn hide(&self) {
        self.requested.set(false);
        self.revealed.set(false);
        self.paint_pending.set(false);
        self.window.set_visible(false);
        // Do not call close(): that destroys the warm WebKit renderer.
    }

    fn show(self: &Rc<Self>) {
        self.requested.set(true);
        self.revealed.set(false);
        self.paint_pending.set(false);
        self.was_active.set(false);
        self.generation.set(self.generation.get().wrapping_add(1));
        // An unpainted layer-shell surface must never flash grey. GTK4
        // supports fully transparent toplevels on Wayland.
        self.window.set_opacity(0.0);
        self.window.present();
        self.request_paint();

        // A dead renderer should not leave an invisible focus-stealing popup.
        let generation = self.generation.get();
        let weak = Rc::downgrade(self);
        gtk::glib::timeout_add_local_once(Duration::from_secs(3), move || {
            if let Some(state) = weak.upgrade() {
                if state.requested.get()
                    && state.generation.get() == generation
                    && !state.revealed.get()
                {
                    eprintln!("kira-shell: Calendar did not paint within 3s; hiding popup");
                    state.hide();
                }
            }
        });
    }

    fn request_paint(&self) {
        if !self.requested.get() || !self.ready.get() || self.paint_pending.replace(true) {
            return;
        }
        let generation = self.generation.get();
        // Two animation frames give WebKit a chance to commit its Svelte
        // content before GTK makes the popup opaque. This script only sends a
        // fixed-purpose acknowledgement to the private calendar channel.
        let script = format!(
            "requestAnimationFrame(() => requestAnimationFrame(() => \
             window.webkit.messageHandlers.kiraCalendarUi.postMessage('painted:{generation}')));"
        );
        self.view.evaluate_javascript(
            &script,
            None,
            None,
            None::<&gtk::gio::Cancellable>,
            |result| {
                if let Err(error) = result {
                    eprintln!("kira-shell: Calendar frame request failed: {error}");
                }
            },
        );
    }

    fn on_message(&self, message: &str) {
        match calendar_command(message) {
            Some(UiCommand::Ready) => {
                self.ready.set(true);
                self.request_paint();
            }
            Some(UiCommand::Painted(generation))
                if self.requested.get()
                    && self.paint_pending.get()
                    && self.generation.get() == generation =>
            {
                self.revealed.set(true);
                self.paint_pending.set(false);
                self.window.set_opacity(1.0);
            }
            Some(UiCommand::Close) if self.requested.get() => self.hide(),
            Some(UiCommand::CopyDate(date)) if self.requested.get() && self.revealed.get() => {
                if let Some(display) = gtk::gdk::Display::default() {
                    display.clipboard().set_text(date);
                }
            }
            _ => eprintln!("kira-shell: ignored invalid/stale Calendar UI command"),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum UiCommand<'a> {
    Close,
    Ready,
    Painted(u64),
    CopyDate(&'a str),
}

/// Revalidate date-copy requests without duplicating the Svelte date grid.
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

fn calendar_command(message: &str) -> Option<UiCommand<'_>> {
    match message {
        "close" => return Some(UiCommand::Close),
        "ready" => return Some(UiCommand::Ready),
        _ => {}
    }
    if let Some(value) = message.strip_prefix("painted:") {
        // Ignore arbitrary strings or oversized/noncanonical frame numbers.
        if value.is_empty() || value.len() > 20 || !value.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        return value.parse().ok().map(UiCommand::Painted);
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

fn calendar_document(html: &str) -> Option<String> {
    html.contains(CALENDAR_MARKER)
        .then(|| html.replacen(CALENDAR_MARKER, CALENDAR_SURFACE, 1))
}

fn build(application: &gtk::Application, document: String) -> Rc<CalendarOverlay> {
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

    // The calendar has no general Core RPC bridge, only its own messages.
    let context = webkit6::WebContext::new();
    let manager = webkit6::UserContentManager::new();
    let view = webkit6::WebView::builder()
        .web_context(&context)
        .user_content_manager(&manager)
        .build();
    view.set_background_color(&gtk::gdk::RGBA::new(0.0, 0.0, 0.0, 1.0));
    crate::native_ui::restrict_webview_navigation(&view);

    let overlay = Rc::new(CalendarOverlay {
        window: window.clone(),
        view: view.clone(),
        document,
        ready: Cell::new(false),
        requested: Cell::new(false),
        revealed: Cell::new(false),
        paint_pending: Cell::new(false),
        generation: Cell::new(0),
        was_active: Cell::new(false),
    });
    let weak = Rc::downgrade(&overlay);
    manager.connect_script_message_received(Some("kiraCalendarUi"), move |_, value| {
        if let Some(overlay) = weak.upgrade() {
            overlay.on_message(&value.to_string());
        }
    });
    assert!(manager.register_script_message_handler("kiraCalendarUi", None));

    manager.connect_script_message_received(Some("kiraDiagnostics"), |_, value| {
        eprintln!("kira-shell [calendar]: {}", value.to_string());
    });
    assert!(manager.register_script_message_handler("kiraDiagnostics", None));

    let weak = Rc::downgrade(&overlay);
    view.connect_load_failed(move |_, event, uri, error| {
        eprintln!("kira-shell: Calendar WebKit load failed ({event:?}) for {uri}: {error}");
        if let Some(state) = weak.upgrade() {
            state.ready.set(false);
            state.hide();
        }
        false
    });
    let weak = Rc::downgrade(&overlay);
    view.connect_web_process_terminated(move |_, reason| {
        eprintln!("kira-shell: Calendar WebKit renderer terminated: {reason:?}");
        if let Some(state) = weak.upgrade() {
            state.ready.set(false);
            state.hide();
            state.view.load_html(&state.document, None);
        }
    });

    let weak = Rc::downgrade(&overlay);
    window.connect_is_active_notify(move |window| {
        if let Some(state) = weak.upgrade() {
            if window.is_active() {
                state.was_active.set(true);
            } else if state.requested.get() && state.was_active.get() {
                state.hide();
            }
        }
    });

    // Load once at startup with no visible layer-shell surface. GTK will map
    // and paint it only on demand, preserving the Svelte component in memory.
    window.set_child(Some(&view));
    view.load_html(&overlay.document, None);
    overlay
}

fn ensure(application: &gtk::Application, html: &str) -> Option<Rc<CalendarOverlay>> {
    CALENDAR.with(|cache| {
        let existing = cache.borrow().clone();
        if existing.is_some() {
            return existing;
        }
        let document = calendar_document(html)?;
        let overlay = build(application, document);
        *cache.borrow_mut() = Some(overlay.clone());
        Some(overlay)
    })
}

/// Start WebKit loading shortly after the bar is activated, not on first click.
pub fn prewarm(application: &gtk::Application, html: &str) {
    if ensure(application, html).is_none() {
        eprintln!("kira-shell: Calendar HTML root not found");
    }
}

pub fn dismiss(_application: &gtk::Application) -> bool {
    CALENDAR.with(|cache| {
        if let Some(state) = cache.borrow().as_ref() {
            if state.requested.get() {
                state.hide();
                return true;
            }
        }
        false
    })
}

pub fn toggle(application: &gtk::Application, html: &str) {
    if dismiss(application) {
        return;
    }
    let Some(state) = ensure(application, html) else {
        eprintln!("kira-shell: Calendar HTML root not found");
        return;
    };
    crate::launcher::dismiss(application);
    crate::system_panel::dismiss(application);
    state.show();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_untrusted_calendar_commands() {
        for invalid in [
            "", "COPY", "toggle", "painted:", "painted:2;exit", "painted:-1",
            "painted:18446744073709551616", "copy:", "copy:2026-02-30",
            "copy:1900-02-29", "copy:0000-01-01", "copy:2026-13-01",
            "copy:2026-11-1", "copy:2026-11-01\n", "copy:2026-11-01;rm",
            "copy:🔥", r#"{"method":"execute_shell"}"#,
        ] {
            assert_eq!(calendar_command(invalid), None, "{invalid:?}");
        }
        assert_eq!(calendar_command("close"), Some(UiCommand::Close));
        assert_eq!(calendar_command("ready"), Some(UiCommand::Ready));
        assert_eq!(calendar_command("painted:42"), Some(UiCommand::Painted(42)));
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
