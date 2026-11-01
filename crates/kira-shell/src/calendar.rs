//! One warm WebKit layer-shell host shared by the Svelte Calendar and System.
//! GTK owns one window, trusted IPC and clipboard; Svelte renders both widgets.
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use webkit6::prelude::*;

const TITLE: &str = "Kira Overlay";
const CALENDAR_MARKER: &str = "<html lang=\"en\">";
const CALENDAR_SURFACE: &str = "<html lang=\"en\" data-kira-surface=\"calendar\">";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Surface {
    Calendar,
    System,
    Launcher,
}

impl Surface {
    fn name(self) -> &'static str {
        match self {
            Self::Calendar => "calendar",
            Self::System => "system",
            Self::Launcher => "launcher",
        }
    }
}

thread_local! {
    // One GTK/WebKit host, shared by every built-in overlay type.
    static CALENDAR: RefCell<Option<Rc<CalendarOverlay>>> = const { RefCell::new(None) };
}

struct CalendarOverlay {
    window: gtk::ApplicationWindow,
    view: webkit6::WebView,
    document: String,
    host_ready: Cell<bool>,
    widget_ready: Cell<bool>,
    content_height: Cell<i32>,
    active: Cell<Option<Surface>>,
    system: RefCell<Option<crate::system_panel::Model>>,
    launcher: RefCell<Option<crate::launcher::Catalog>>,
    last_metrics: Cell<Option<Instant>>,
    requested: Cell<bool>,
    reposition_pending: Cell<bool>,
    revealed: Cell<bool>,
    paint_pending: Cell<bool>,
    generation: Cell<u32>,
    was_active: Cell<bool>,
    open_started: Cell<Option<Instant>>,
}

impl CalendarOverlay {
    /// The single WebKit layer-shell window has different geometry depending
    /// on the active widget. With no edge anchors, wlr-layer-shell centers
    /// the launcher's fixed-size surface on its output.
    fn position_for(&self, kind: Surface) {
        let centered = kind == Surface::Launcher;

        // Reset margins before removing anchors. Margins only belong to
        // anchored edges, so stale top/right values cannot leak into the
        // centered launcher (or trigger a compositor protocol error).
        self.window.set_margin(Edge::Top, 0);
        self.window.set_margin(Edge::Right, 0);
        self.window.set_margin(Edge::Bottom, 0);
        self.window.set_margin(Edge::Left, 0);
        self.window.set_anchor(Edge::Top, !centered);
        self.window.set_anchor(Edge::Right, !centered);
        self.window.set_anchor(Edge::Bottom, false);
        self.window.set_anchor(Edge::Left, false);

        if centered {
            // Ignore the bar's reserved top zone to center in the entire
            // output, rather than the usable area below the bar.
            self.window.set_exclusive_zone(-1);
        } else {
            // Calendar and System keep their existing top-right placement.
            self.window.set_exclusive_zone(0);
            self.window
                .set_margin(Edge::Top, crate::native_ui::POPUP_TOP_MARGIN);
            self.window.set_margin(Edge::Right, 12);
        }
    }

    fn hide(&self) {
        self.requested.set(false);
        self.reposition_pending.set(false);
        if let Some(model) = self.system.borrow().as_ref() {
            model.set_processes_active(false);
        }
        self.active.set(None);
        self.launcher.borrow_mut().take();
        self.widget_ready.set(false);
        self.content_height.set(0);
        self.revealed.set(false);
        self.paint_pending.set(false);
        self.open_started.take();
        // Deliver a generation-scoped hide. WebKit evaluates scripts
        // asynchronously: a stale hide must never unmount a newer show.
        if self.host_ready.get() {
            let generation = self.generation.get();
            let script = format!(
                "window.dispatchEvent(new CustomEvent('kira:overlay:hide', {{ detail: {{ generation: {generation} }} }}));"
            );
            self.view.evaluate_javascript(
                &script,
                None,
                None,
                None::<&gtk::gio::Cancellable>,
                |result| {
                    if let Err(error) = result {
                        eprintln!("kira-shell: Overlay hide delivery failed: {error}");
                    }
                },
            );
        }
        self.window.set_visible(false);
    }

    fn show(self: &Rc<Self>, kind: Surface) {
        // Moving an already mapped layer-shell surface causes Niri to display
        // its previous WebKit buffer for a frame at the NEW coordinates.
        // Unmap first and allow the compositor to process that unmap in a
        // different main-loop iteration before remapping at the new anchors.
        // Switching Calendar <-> System keeps its instant in-place path.
        let relocating = self.window.is_visible()
            && self
                .active
                .get()
                .is_some_and(|old| (old == Surface::Launcher) != (kind == Surface::Launcher));
        let defer_present = relocating || self.reposition_pending.get();
        if relocating {
            // Unmapping can trigger is_active_notify: it must not count as
            // an outside click that closes the *new* popup.
            self.was_active.set(false);
            self.window.set_visible(false);
        }
        self.reposition_pending.set(defer_present);

        if self.active.get() == Some(Surface::System) && kind != Surface::System {
            if let Some(model) = self.system.borrow().as_ref() {
                model.set_processes_active(false);
            }
        }
        self.position_for(kind);
        self.active.set(Some(kind));
        // The launcher needs reliable keyboard focus for type-to-search;
        // other popups preserve their former on-demand keyboard behavior.
        self.window.set_keyboard_mode(if kind == Surface::Launcher {
            KeyboardMode::Exclusive
        } else {
            KeyboardMode::OnDemand
        });
        self.requested.set(true);
        self.revealed.set(false);
        self.widget_ready.set(false);
        self.content_height.set(0);
        self.paint_pending.set(false);
        self.was_active.set(false);
        self.open_started.set(Some(Instant::now()));
        self.generation
            .set(self.generation.get().wrapping_add(1).max(1));
        // An unpainted layer-shell surface must never flash grey. GTK4
        // supports fully transparent toplevels on Wayland.
        self.window.set_opacity(0.0);
        // The next widget will report its intrinsic height before reveal.
        // Reset the previous widget's geometry while still transparent.
        crate::native_ui::resize_webview_popup(&self.window, 420, 1);
        let generation = self.generation.get();
        if defer_present {
            // Start replacing the Svelte DOM while the old buffer is NOT
            // mapped. The window is revealed only after the new generation
            // has mounted and painted (or the verified-DOM fallback runs).
            self.request_mount();
            let weak = Rc::downgrade(self);
            gtk::glib::timeout_add_local_once(Duration::from_millis(32), move || {
                if let Some(state) = weak.upgrade()
                    && state.requested.get()
                    && state.generation.get() == generation
                {
                    state.reposition_pending.set(false);
                    state.window.present();
                }
            });
        } else {
            self.window.present();
            self.request_mount();
        }
        eprintln!(
            "kira-shell: Overlay requested: {} generation {} (host ready: {}, reposition: {})",
            kind.name(),
            generation,
            self.host_ready.get(),
            defer_present
        );

        // A dead renderer should not leave an invisible focus-stealing popup.
        let generation = self.generation.get();
        let weak = Rc::downgrade(self);
        gtk::glib::timeout_add_local_once(Duration::from_secs(3), move || {
            if let Some(state) = weak.upgrade() {
                if state.requested.get()
                    && state.generation.get() == generation
                    && !state.revealed.get()
                {
                    eprintln!(
                        "kira-shell: Overlay did not mount within 3s (host ready: {}, widget ready: {}); hiding popup",
                        state.host_ready.get(),
                        state.widget_ready.get()
                    );
                    state.hide();
                }
            }
        });
    }

    fn request_mount(&self) {
        if !self.requested.get() || !self.host_ready.get() {
            return;
        }
        let generation = self.generation.get();
        let Some(kind) = self.active.get() else {
            return;
        };
        // Only hardcoded, trusted Svelte widget types may be selected.
        let kind = kind.name();
        let max_height = crate::native_ui::webview_popup_height_limit(&self.window);
        let script = format!(
            "window.dispatchEvent(new CustomEvent('kira:overlay:show', {{ detail: {{ kind: '{kind}', generation: {generation}, maxHeight: {max_height} }} }}));"
        );
        self.view.evaluate_javascript(
            &script,
            None,
            None,
            None::<&gtk::gio::Cancellable>,
            |result| {
                if let Err(error) = result {
                    eprintln!("kira-shell: Calendar mount delivery failed: {error}");
                }
            },
        );
    }

    fn request_paint(&self) {
        if !self.requested.get()
            || !self.widget_ready.get()
            || self.content_height.get() == 0
            || self.paint_pending.replace(true)
        {
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
    // Compositors may throttle requestAnimationFrame for a transparent
    // popup. A verified, synchronously flushed Svelte mount must never
    // remain invisible solely because the paint acknowledgement was skipped.
    fn schedule_reveal_fallback(self: &Rc<Self>, generation: u32) {
        let weak = Rc::downgrade(self);
        gtk::glib::timeout_add_local_once(Duration::from_millis(150), move || {
            let Some(state) = weak.upgrade() else {
                return;
            };
            if state.requested.get()
                && state.generation.get() == generation
                && state.widget_ready.get()
                && !state.revealed.get()
            {
                eprintln!(
                    "kira-shell: Overlay frame acknowledgement delayed; revealing verified Svelte DOM"
                );
                state.reveal();
            }
        });
    }

    fn reveal(&self) {
        self.revealed.set(true);
        self.paint_pending.set(false);
        self.window.set_opacity(1.0);
        if let Some(start) = self.open_started.take() {
            eprintln!(
                "kira-shell: Overlay visible after {} ms",
                start.elapsed().as_millis()
            );
        }
    }

    fn on_message(self: &Rc<Self>, message: &str) {
        match calendar_command(message) {
            Some(UiCommand::HostReady) => {
                eprintln!("kira-shell: Shared Svelte overlay host ready");
                self.host_ready.set(true);
                self.request_mount();
            }
            Some(UiCommand::Mounted(generation))
                if self.requested.get() && self.generation.get() == generation =>
            {
                eprintln!("kira-shell: Svelte overlay DOM verified");
                if self.active.get() == Some(Surface::System) {
                    self.send_metrics();
                }
                self.widget_ready.set(true);
                if self.content_height.get() > 0 {
                    self.request_paint();
                    self.schedule_reveal_fallback(generation);
                }
            }
            Some(UiCommand::ContentHeight(generation, height))
                if self.requested.get() && self.generation.get() == generation =>
            {
                // Layout-driven dimensions come only from the bounded,
                // trusted overlay channel. Ignore stale generations and
                // no-op resizes to avoid feedback from ResizeObserver.
                let clamped =
                    height.min(crate::native_ui::webview_popup_height_limit(&self.window));
                if clamped != self.content_height.get() {
                    self.content_height.set(clamped);
                    crate::native_ui::resize_webview_popup(&self.window, 420, clamped);
                }
                if self.widget_ready.get() && !self.paint_pending.get() && !self.revealed.get() {
                    self.request_paint();
                    self.schedule_reveal_fallback(generation);
                }
            }
            Some(UiCommand::Painted(generation))
                if self.requested.get()
                    && self.widget_ready.get()
                    && self.paint_pending.get()
                    && self.generation.get() == generation =>
            {
                self.reveal();
            }
            Some(UiCommand::Close) if self.requested.get() => self.hide(),
            Some(UiCommand::CopyDate(date))
                if self.requested.get()
                    && self.revealed.get()
                    && self.active.get() == Some(Surface::Calendar) =>
            {
                if let Some(display) = gtk::gdk::Display::default() {
                    display.clipboard().set_text(date);
                }
            }
            Some(UiCommand::ProcessesTab(active))
                if self.requested.get() && self.active.get() == Some(Surface::System) =>
            {
                if let Some(model) = self.system.borrow().as_ref() {
                    model.set_processes_active(active);
                }
            }
            Some(UiCommand::CopyReport)
                if self.requested.get()
                    && self.revealed.get()
                    && self.active.get() == Some(Surface::System) =>
            {
                if let Some(model) = self.system.borrow().as_ref()
                    && let Some(display) = gtk::gdk::Display::default()
                {
                    display.clipboard().set_text(&model.report());
                }
            }
            Some(UiCommand::LauncherCatalog(generation))
                if self.requested.get()
                    && self.active.get() == Some(Surface::Launcher)
                    && self.generation.get() == generation =>
            {
                let needs_catalog = self.launcher.borrow().is_none();
                if needs_catalog {
                    *self.launcher.borrow_mut() = Some(crate::launcher::Catalog::discover());
                }
                let data = self
                    .launcher
                    .borrow()
                    .as_ref()
                    .map(|catalog| catalog.as_json());
                if let Some(payload) = data {
                    self.dispatch_launcher("kira:launcher:apps", payload);
                }
            }
            Some(UiCommand::LaunchApp(generation, id))
                if self.requested.get()
                    && self.revealed.get()
                    && self.active.get() == Some(Surface::Launcher)
                    && self.generation.get() == generation =>
            {
                let result = self
                    .launcher
                    .borrow()
                    .as_ref()
                    .map(|catalog| catalog.launch(id));
                match result {
                    Some(Ok(())) => self.hide(),
                    Some(Err(error)) => {
                        self.dispatch_launcher(
                            "kira:launcher:error",
                            serde_json::json!({ "message": error }),
                        );
                    }
                    None => {
                        self.dispatch_launcher(
                            "kira:launcher:error",
                            serde_json::json!({ "message": "Application catalog unavailable" }),
                        );
                    }
                }
            }
            _ => eprintln!("kira-shell: ignored invalid/stale Overlay UI command"),
        }
    }

    fn dispatch_launcher(&self, event: &str, payload: serde_json::Value) {
        if !self.requested.get() || self.active.get() != Some(Surface::Launcher) {
            return;
        }
        // Serialize data twice so installed app names never become JS source.
        if let Ok(serialized) = serde_json::to_string(&payload)
            && let Ok(encoded) = serde_json::to_string(&serialized)
        {
            let script = format!(
                "window.dispatchEvent(new CustomEvent('{event}', {{ detail: JSON.parse({encoded}) }}));"
            );
            self.view.evaluate_javascript(
                &script,
                None,
                None,
                None::<&gtk::gio::Cancellable>,
                |result| {
                    if let Err(error) = result {
                        eprintln!("kira-shell: Launcher catalog delivery failed: {error}");
                    }
                },
            );
        }
    }

    fn dispatch_data(&self, event: &str, payload: serde_json::Value) {
        if !self.requested.get() || self.active.get() != Some(Surface::System) {
            return;
        }
        // Use JSON.parse on a quoted JSON string, never inject untrusted
        // process names as executable JavaScript.
        if let Ok(serialized) = serde_json::to_string(&payload)
            && let Ok(encoded) = serde_json::to_string(&serialized)
        {
            let script = format!(
                "window.dispatchEvent(new CustomEvent('{event}', {{ detail: JSON.parse({encoded}) }}));"
            );
            self.view.evaluate_javascript(
                &script,
                None,
                None,
                None::<&gtk::gio::Cancellable>,
                |result| {
                    if let Err(error) = result {
                        eprintln!("kira-shell: System data delivery failed: {error}");
                    }
                },
            );
        }
    }

    fn send_metrics(&self) {
        let data = self
            .system
            .borrow()
            .as_ref()
            .map(|model| model.metrics_json());
        if let Some(payload) = data {
            self.dispatch_data("kira:system:data", payload);
            self.last_metrics.set(Some(Instant::now()));
        }
    }

    fn tick(&self) {
        if !self.requested.get() || self.active.get() != Some(Surface::System) {
            return;
        }
        if self
            .last_metrics
            .get()
            .is_none_or(|then| then.elapsed() >= Duration::from_secs(2))
        {
            self.send_metrics();
        }
        let process = self
            .system
            .borrow()
            .as_ref()
            .and_then(|model| model.poll_processes());
        if let Some(payload) = process {
            self.dispatch_data("kira:system:processes", payload);
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum UiCommand<'a> {
    Close,
    HostReady,
    Mounted(u32),
    Painted(u32),
    ContentHeight(u32, i32),
    CopyDate(&'a str),
    ProcessesTab(bool),
    CopyReport,
    LauncherCatalog(u32),
    LaunchApp(u32, usize),
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
        "host-ready" => return Some(UiCommand::HostReady),
        "tab:processes" => return Some(UiCommand::ProcessesTab(true)),
        "tab:overview" => return Some(UiCommand::ProcessesTab(false)),
        "copy-report" => return Some(UiCommand::CopyReport),
        _ => {}
    }
    if let Some(value) = message.strip_prefix("apps-request:") {
        if value.is_empty() || value.len() > 10 || !value.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        return value
            .parse::<u32>()
            .ok()
            .filter(|v| *v > 0)
            .map(UiCommand::LauncherCatalog);
    }
    if let Some(value) = message.strip_prefix("launch:") {
        let (generation, id) = value.split_once(':')?;
        if generation.is_empty()
            || generation.len() > 10
            || !generation.bytes().all(|b| b.is_ascii_digit())
            || id.is_empty()
            || id.len() > 4
            || !id.bytes().all(|b| b.is_ascii_digit())
        {
            return None;
        }
        return Some(UiCommand::LaunchApp(
            generation.parse::<u32>().ok().filter(|v| *v > 0)?,
            id.parse::<usize>().ok().filter(|v| *v < 1024)?,
        ));
    }
    if let Some(value) = message.strip_prefix("mounted:") {
        if value.is_empty() || value.len() > 10 || !value.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        return value.parse().ok().map(UiCommand::Mounted);
    }
    if let Some(value) = message.strip_prefix("size:") {
        let (generation, height) = value.split_once(':')?;
        if generation.is_empty()
            || generation.len() > 10
            || !generation.bytes().all(|b| b.is_ascii_digit())
            || height.is_empty()
            || height.len() > 5
            || !height.bytes().all(|b| b.is_ascii_digit())
        {
            return None;
        }
        return Some(UiCommand::ContentHeight(
            generation.parse::<u32>().ok().filter(|v| *v > 0)?,
            height
                .parse::<i32>()
                .ok()
                .filter(|v| (1..=20_000).contains(v))?,
        ));
    }
    if let Some(value) = message.strip_prefix("painted:") {
        if value.is_empty() || value.len() > 10 || !value.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        return value.parse().ok().map(UiCommand::Painted);
    }
    let date = message.strip_prefix("copy:")?;
    let bytes = date.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !bytes
            .iter()
            .enumerate()
            .all(|(i, b)| i == 4 || i == 7 || b.is_ascii_digit())
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
    window.set_namespace(Some("kira-overlay"));
    window.set_layer(Layer::Overlay);
    window.set_anchor(Edge::Top, true);
    window.set_anchor(Edge::Right, true);
    window.set_margin(Edge::Top, crate::native_ui::POPUP_TOP_MARGIN);
    window.set_margin(Edge::Right, 12);
    window.set_exclusive_zone(0);
    window.set_keyboard_mode(KeyboardMode::OnDemand);
    crate::native_ui::configure_webview_popup(&window, 420, 1);

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
        host_ready: Cell::new(false),
        widget_ready: Cell::new(false),
        content_height: Cell::new(0),
        active: Cell::new(None),
        system: RefCell::new(None),
        launcher: RefCell::new(None),
        last_metrics: Cell::new(None),
        requested: Cell::new(false),
        reposition_pending: Cell::new(false),
        revealed: Cell::new(false),
        paint_pending: Cell::new(false),
        generation: Cell::new(0),
        was_active: Cell::new(false),
        open_started: Cell::new(None),
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
            state.host_ready.set(false);
            state.hide();
        }
        false
    });
    let weak = Rc::downgrade(&overlay);
    view.connect_web_process_terminated(move |_, reason| {
        eprintln!("kira-shell: Calendar WebKit renderer terminated: {reason:?}");
        if let Some(state) = weak.upgrade() {
            state.host_ready.set(false);
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

    // Load WebKit once; only the lightweight JS host survives between
    // shows. The Calendar component, timer and DOM are unmounted on hide.
    window.set_child(Some(&view));
    view.load_html(&overlay.document, None);
    let weak = Rc::downgrade(&overlay);
    gtk::glib::timeout_add_local(Duration::from_millis(250), move || {
        if let Some(state) = weak.upgrade() {
            state.tick();
            gtk::glib::ControlFlow::Continue
        } else {
            gtk::glib::ControlFlow::Break
        }
    });
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

pub fn dismiss_system(_application: &gtk::Application) -> bool {
    CALENDAR.with(|cache| {
        if let Some(state) = cache.borrow().as_ref()
            && state.requested.get()
            && state.active.get() == Some(Surface::System)
        {
            state.hide();
            return true;
        }
        false
    })
}

pub fn toggle(application: &gtk::Application, html: &str) {
    let Some(state) = ensure(application, html) else {
        eprintln!("kira-shell: Overlay HTML root not found");
        return;
    };
    if state.requested.get() && state.active.get() == Some(Surface::Calendar) {
        state.hide();
        return;
    }
    state.show(Surface::Calendar);
}

pub fn open_launcher(application: &gtk::Application, html: &str) {
    let Some(state) = ensure(application, html) else {
        eprintln!("kira-shell: Overlay HTML root not found");
        return;
    };
    // Opening the launcher again should focus its existing search field.
    if state.requested.get() && state.active.get() == Some(Surface::Launcher) {
        state.window.present();
        return;
    }
    state.launcher.borrow_mut().take();
    state.show(Surface::Launcher);
}

pub fn toggle_system(
    application: &gtk::Application,
    html: &str,
    metrics: Rc<RefCell<crate::system_metrics::Snapshot>>,
    core: Rc<Cell<bool>>,
    niri: Rc<Cell<bool>>,
) {
    let Some(state) = ensure(application, html) else {
        eprintln!("kira-shell: Overlay HTML root not found");
        return;
    };
    if state.requested.get() && state.active.get() == Some(Surface::System) {
        state.hide();
        return;
    }
    *state.system.borrow_mut() = Some(crate::system_panel::Model::new(metrics, core, niri));
    state.last_metrics.set(None);
    state.show(Surface::System);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_untrusted_calendar_commands() {
        for invalid in [
            "",
            "COPY",
            "toggle",
            "painted:",
            "painted:2;exit",
            "painted:-1",
            "painted:18446744073709551616",
            "tab:everything",
            "apps-request:",
            "apps-request:abc",
            "launch:1:1024",
            "launch:1:-1",
            "launch:1:0;sh",
            "launch:18446744073709551616:0",
            "copy:2026-01-01;exit",
            "mounted:",
            "mounted:abc",
            "mounted:4294967296",
            "size:",
            "size:0:100",
            "size:1:0",
            "size:1:20001",
            "size:1:-1",
            "size:1:4;rm",
            "copy:",
            "copy:2026-02-30",
            "copy:1900-02-29",
            "copy:0000-01-01",
            "copy:2026-13-01",
            "copy:2026-11-1",
            "copy:2026-11-01\n",
            "copy:2026-11-01;rm",
            "copy:🔥",
            r#"{"method":"execute_shell"}"#,
        ] {
            assert_eq!(calendar_command(invalid), None, "{invalid:?}");
        }
        assert_eq!(calendar_command("close"), Some(UiCommand::Close));
        assert_eq!(calendar_command("host-ready"), Some(UiCommand::HostReady));
        assert_eq!(
            calendar_command("tab:processes"),
            Some(UiCommand::ProcessesTab(true))
        );
        assert_eq!(
            calendar_command("tab:overview"),
            Some(UiCommand::ProcessesTab(false))
        );
        assert_eq!(calendar_command("copy-report"), Some(UiCommand::CopyReport));
        assert_eq!(
            calendar_command("apps-request:12"),
            Some(UiCommand::LauncherCatalog(12))
        );
        assert_eq!(
            calendar_command("launch:12:42"),
            Some(UiCommand::LaunchApp(12, 42))
        );
        assert_eq!(calendar_command("mounted:42"), Some(UiCommand::Mounted(42)));
        assert_eq!(
            calendar_command("size:42:512"),
            Some(UiCommand::ContentHeight(42, 512))
        );
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
