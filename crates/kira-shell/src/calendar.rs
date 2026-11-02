//! Bounded, reusable GTK/WebKit popup hosts for concurrent trusted widgets.
//! Each active Calendar/System/Launcher has its own window and IPC manager.
use crate::webview_pool::{Reservation, Surface, WebviewScheduler};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use webkit6::prelude::*;

const TITLE: &str = "Kira Overlay";
// Native fade stays intentionally shorter than the GSAP content landing.
const OVERLAY_FADE_MS: u64 = 160;

fn native_fade_progress(elapsed: Duration) -> f64 {
    let t = (elapsed.as_secs_f64() / Duration::from_millis(OVERLAY_FADE_MS).as_secs_f64())
        .clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}
const CALENDAR_MARKER: &str = "<html lang=\"en\">";
const CALENDAR_SURFACE: &str = "<html lang=\"en\" data-kira-surface=\"calendar\">";

thread_local! {
    static POOL: RefCell<Option<Rc<OverlayPool>>> = const { RefCell::new(None) };
}

/// Top Bar is permanently separate. Only one of these hosts is prewarmed;
/// extra popup WebViews are created on actual concurrent requests.
struct OverlayPool {
    application: gtk::Application,
    document: String,
    scheduler: RefCell<WebviewScheduler>,
    windows: RefCell<HashMap<u32, Rc<CalendarOverlay>>>,
    poll_source: RefCell<Option<gtk::glib::SourceId>>,
    last_tick: Cell<Instant>,
}

impl OverlayPool {
    fn new(application: &gtk::Application, document: String) -> Rc<Self> {
        let pool = Rc::new(Self {
            application: application.clone(),
            document,
            scheduler: RefCell::new(WebviewScheduler::default()),
            windows: RefCell::new(HashMap::new()),
            poll_source: RefCell::new(None),
            last_tick: Cell::new(Instant::now()),
        });
        if let Some(id) = pool.scheduler.borrow_mut().prewarm_one(Instant::now()) {
            let view = build(
                &pool.application,
                pool.document.clone(),
                id,
                Rc::downgrade(&pool),
            );
            pool.windows.borrow_mut().insert(id, view);
        }
        let weak = Rc::downgrade(&pool);
        gtk::glib::timeout_add_local(Duration::from_secs(5), move || {
            let Some(pool) = weak.upgrade() else {
                return gtk::glib::ControlFlow::Break;
            };
            pool.prune();
            gtk::glib::ControlFlow::Continue
        });
        pool
    }

    fn acquire(self: &Rc<Self>, kind: Surface) -> Option<(Rc<CalendarOverlay>, bool)> {
        let choice = self.scheduler.borrow_mut().reserve(kind, Instant::now());
        let (id, existing) = match choice {
            Reservation::Existing(id) => (id, true),
            Reservation::Reused(id) => (id, false),
            Reservation::Created(id) => {
                let view = build(
                    &self.application,
                    self.document.clone(),
                    id,
                    Rc::downgrade(self),
                );
                self.windows.borrow_mut().insert(id, view);
                (id, false)
            }
            Reservation::Full => {
                eprintln!("kira-shell: popup pool full; active widgets never evicted");
                return None;
            }
        };
        let view = self.windows.borrow().get(&id).cloned()?;
        self.ensure_polling();
        self.update_launcher_keyboard_mode();
        Some((view, existing))
    }

    /// Exactly one GTK source serves all concurrently leased hosts. A warm
    /// idle WebView has ZERO 30/250ms pollers, unlike the legacy per-window
    /// timers. The source tears itself down once the last lease is released.
    fn ensure_polling(self: &Rc<Self>) {
        if self.poll_source.borrow().is_some() {
            return;
        }
        self.last_tick.set(Instant::now());
        let weak = Rc::downgrade(self);
        let source = gtk::glib::timeout_add_local(Duration::from_millis(30), move || {
            let Some(pool) = weak.upgrade() else {
                return gtk::glib::ControlFlow::Break;
            };
            if pool.scheduler.borrow().active_count() == 0 {
                pool.poll_source.borrow_mut().take();
                return gtk::glib::ControlFlow::Break;
            }
            // Snapshot owned handles before calling poll_launcher(): a
            // successful GIO launch may close its own slot mid-iteration.
            let active: Vec<_> = pool
                .windows
                .borrow()
                .values()
                .filter(|view| view.requested.get())
                .cloned()
                .collect();
            for view in &active {
                view.poll_launcher();
                view.poll_pulse();
            }
            if pool.last_tick.get().elapsed() >= Duration::from_millis(250) {
                pool.last_tick.set(Instant::now());
                for view in &active {
                    view.tick();
                }
            }
            gtk::glib::ControlFlow::Continue
        });
        self.poll_source.replace(Some(source));
    }

    fn active(&self, kind: Surface) -> Option<Rc<CalendarOverlay>> {
        let id = self.scheduler.borrow().active_slot(kind)?;
        self.windows.borrow().get(&id).cloned()
    }

    fn release(&self, id: u32) {
        self.scheduler.borrow_mut().release(id, Instant::now());
        self.update_launcher_keyboard_mode();
    }

    fn update_launcher_keyboard_mode(&self) {
        // Exclusive is useful for an isolated type-to-search Launcher but
        // blocks interaction with other simultaneously visible windows.
        // Keep its solo behavior; use OnDemand with multiple popup leases.
        if let Some(launcher) = self.active(Surface::Launcher) {
            launcher.window.set_keyboard_mode(if self.concurrent() {
                KeyboardMode::OnDemand
            } else {
                KeyboardMode::Exclusive
            });
        }
    }

    fn concurrent(&self) -> bool {
        self.scheduler.borrow().active_count() > 1
    }

    fn prune(&self) {
        let expired = self.scheduler.borrow_mut().expired_idle(Instant::now());
        for id in expired {
            let retired = { self.windows.borrow_mut().remove(&id) };
            if let Some(view) = retired {
                view.disposed.set(true);
                // Never close an active window. This reclaims surplus idle
                // WebKit processes, keeping only the newest hot spare.
                view.window.close();
            }
        }
    }
}

struct CalendarOverlay {
    id: u32,
    pool: std::rc::Weak<OverlayPool>,
    disposed: Cell<bool>,
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
    revealed: Cell<bool>,
    native_reduced_motion: Cell<bool>,
    paint_pending: Cell<bool>,
    generation: Cell<u32>,
    was_active: Cell<bool>,
    open_started: Cell<Option<Instant>>,
}

impl CalendarOverlay {
    /// Each leased WebView has its own layer-shell window. Calendar and
    /// System Monitor ALWAYS share the original top-right anchor, regardless
    /// of how many peer windows are currently in the pool. With no edge
    /// anchors, wlr-layer-shell centers Launcher on its output.
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
            self.window
                .set_margin(Edge::Right, crate::native_ui::POPUP_RIGHT_MARGIN);
        }
    }

    fn hide(&self) {
        if !self.requested.replace(false) {
            return;
        }
        if let Some(model) = self.system.borrow().as_ref() {
            model.set_processes_active(false);
            model.set_pulse_active(false);
        }
        // Idle WebKit is warm, but it must retain no live sampler or
        // expensive widget-specific model from a previous lease.
        self.system.borrow_mut().take();
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
        if let Some(pool) = self.pool.upgrade() {
            pool.release(self.id);
        }
    }

    fn show(self: &Rc<Self>, kind: Surface) {
        // Dedicated leased WebView: no preceding Svelte widget is unmounted
        // and no other Wayland popup is ever unmapped to start this scene.
        debug_assert!(!self.requested.get());
        self.position_for(kind);
        self.active.set(Some(kind));
        // The launcher needs reliable keyboard focus for type-to-search;
        // other popups preserve their former on-demand keyboard behavior.
        self.window.set_keyboard_mode(
            if kind == Surface::Launcher
                && self.pool.upgrade().is_some_and(|pool| !pool.concurrent())
            {
                KeyboardMode::Exclusive
            } else {
                KeyboardMode::OnDemand
            },
        );
        self.requested.set(true);
        self.revealed.set(false);
        self.native_reduced_motion.set(false);
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
        self.window.present();
        self.request_mount();
        eprintln!(
            "kira-shell: Pooled popup {} slot {} generation {} (host ready: {})",
            kind.name(),
            self.id,
            generation,
            self.host_ready.get()
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
        if self.disposed.get() || !self.requested.get() || !self.host_ready.get() {
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

    fn reveal(self: &Rc<Self>) {
        if self.revealed.replace(true) {
            return;
        }
        self.paint_pending.set(false);
        if self.native_reduced_motion.get() {
            self.window.set_opacity(1.0);
        } else {
            // Animate only the already-sized, already-painted native surface.
            // Never animate geometry: Niri must not see a 1px or drifting popup.
            let generation = self.generation.get();
            let started = Instant::now();
            let weak = Rc::downgrade(self);
            gtk::glib::timeout_add_local(Duration::from_millis(16), move || {
                let Some(state) = weak.upgrade() else {
                    return gtk::glib::ControlFlow::Break;
                };
                // A previous opening cannot fade a newer generation.
                if !state.requested.get()
                    || !state.revealed.get()
                    || state.generation.get() != generation
                {
                    return gtk::glib::ControlFlow::Break;
                }
                if state.native_reduced_motion.get() {
                    state.window.set_opacity(1.0);
                    return gtk::glib::ControlFlow::Break;
                }
                let elapsed = started.elapsed();
                state.window.set_opacity(native_fade_progress(elapsed));
                if elapsed >= Duration::from_millis(OVERLAY_FADE_MS) {
                    state.window.set_opacity(1.0);
                    gtk::glib::ControlFlow::Break
                } else {
                    gtk::glib::ControlFlow::Continue
                }
            });
        }
        // Trigger the matching GSAP scene at the START of native compositing.
        // Both tracks now share one visible opening instead of two jumps.
        let generation = self.generation.get();
        let script = format!(
            "window.dispatchEvent(new CustomEvent('kira:overlay:visible', {{ detail: {{ generation: {generation} }} }}));"
        );
        self.view.evaluate_javascript(
            &script,
            None,
            None,
            None::<&gtk::gio::Cancellable>,
            |result| {
                if let Err(error) = result {
                    eprintln!("kira-shell: Overlay visible event delivery failed: {error}");
                }
            },
        );
        if let Some(start) = self.open_started.take() {
            eprintln!(
                "kira-shell: Overlay visible after {} ms",
                start.elapsed().as_millis()
            );
        }
    }

    fn on_message(self: &Rc<Self>, message: &str) {
        if self.disposed.get() {
            return;
        }
        match calendar_command(message) {
            Some(UiCommand::HostReady) => {
                eprintln!("kira-shell: Shared Svelte overlay host ready");
                self.host_ready.set(true);
                self.request_mount();
            }
            Some(UiCommand::Mounted(generation, reduced))
                if self.requested.get() && self.generation.get() == generation =>
            {
                self.native_reduced_motion.set(reduced);
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
            Some(UiCommand::MotionPreference(generation, reduced))
                if self.requested.get() && self.generation.get() == generation =>
            {
                self.native_reduced_motion.set(reduced);
                if reduced && self.revealed.get() {
                    self.window.set_opacity(1.0);
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
            Some(UiCommand::Close(generation))
                if self.requested.get() && self.generation.get() == generation =>
            {
                self.hide()
            }
            Some(UiCommand::CopyDate(generation, date))
                if self.requested.get()
                    && self.generation.get() == generation
                    && self.revealed.get()
                    && self.active.get() == Some(Surface::Calendar) =>
            {
                if let Some(display) = gtk::gdk::Display::default() {
                    display.clipboard().set_text(date);
                }
            }
            Some(UiCommand::ProcessesTab(generation, active))
                if self.requested.get()
                    && self.generation.get() == generation
                    && self.active.get() == Some(Surface::System) =>
            {
                if let Some(model) = self.system.borrow().as_ref() {
                    model.set_processes_active(active);
                    model.set_pulse_active(!active);
                }
            }
            Some(UiCommand::CopyReport(generation))
                if self.requested.get()
                    && self.generation.get() == generation
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
                if self.launcher.borrow().is_none() {
                    // Do not enumerate installed apps on GTK's event loop.
                    match crate::launcher::Catalog::discover() {
                        Ok(catalog) => *self.launcher.borrow_mut() = Some(catalog),
                        Err(error) => self.dispatch_launcher(
                            "kira:launcher:error",
                            serde_json::json!({ "message": error }),
                        ),
                    }
                } else {
                    let ready = self
                        .launcher
                        .borrow()
                        .as_ref()
                        .and_then(|catalog| catalog.ready_json());
                    if let Some(payload) = ready {
                        self.dispatch_launcher("kira:launcher:apps", payload);
                    }
                }
            }
            Some(UiCommand::LauncherIcons(generation, ids))
                if self.requested.get()
                    && self.active.get() == Some(Surface::Launcher)
                    && self.generation.get() == generation =>
            {
                // GTK only looks up themed file paths. A worker reads the
                // icons and encodes bounded image bytes asynchronously.
                let result = self
                    .launcher
                    .borrow()
                    .as_ref()
                    .ok_or_else(|| "Application catalog unavailable".to_string())
                    .and_then(|catalog| catalog.request_icons(&ids));
                if let Err(error) = result {
                    self.dispatch_launcher(
                        "kira:launcher:error",
                        serde_json::json!({ "message": error }),
                    );
                }
            }
            Some(UiCommand::LaunchApp(generation, id))
                if self.requested.get()
                    && self.revealed.get()
                    && self.active.get() == Some(Surface::Launcher)
                    && self.generation.get() == generation =>
            {
                // Launching via GIO can also block (desktop portals/D-Bus).
                // Retain the trusted index and await the worker result.
                let result = self
                    .launcher
                    .borrow()
                    .as_ref()
                    .ok_or_else(|| "Application catalog unavailable".to_string())
                    .and_then(|catalog| catalog.launch(id));
                if let Err(error) = result {
                    self.dispatch_launcher(
                        "kira:launcher:error",
                        serde_json::json!({ "message": error }),
                    );
                }
            }
            _ => eprintln!("kira-shell: ignored invalid/stale Overlay UI command"),
        }
    }

    /// Drain a small number of completed worker operations per GTK frame.
    /// Closing/reopening Launcher drops its worker receiver, so stale results
    /// cannot update a newer overlay generation.
    fn poll_launcher(&self) {
        if !self.requested.get() || self.active.get() != Some(Surface::Launcher) {
            return;
        }
        for _ in 0..4 {
            let message = self
                .launcher
                .borrow_mut()
                .as_mut()
                .and_then(|catalog| catalog.poll());
            let Some(message) = message else {
                break;
            };
            match message {
                crate::launcher::LauncherMessage::Apps(payload) => {
                    self.dispatch_launcher("kira:launcher:apps", payload);
                }
                crate::launcher::LauncherMessage::Icons(payload) => {
                    self.dispatch_launcher("kira:launcher:icons", payload);
                }
                crate::launcher::LauncherMessage::Launched(Ok(())) => self.hide(),
                crate::launcher::LauncherMessage::Launched(Err(error))
                | crate::launcher::LauncherMessage::Error(error) => {
                    self.dispatch_launcher(
                        "kira:launcher:error",
                        serde_json::json!({ "message": error }),
                    );
                }
            }
            if self.active.get() != Some(Surface::Launcher) {
                break;
            }
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

    fn poll_pulse(&self) {
        if !self.requested.get() || self.active.get() != Some(Surface::System) {
            return;
        }
        let pulse = self
            .system
            .borrow()
            .as_ref()
            .and_then(|model| model.poll_pulse());
        if let Some(payload) = pulse {
            self.dispatch_data("kira:system:pulse", payload);
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
    Close(u32),
    HostReady,
    Mounted(u32, bool),
    MotionPreference(u32, bool),
    Painted(u32),
    ContentHeight(u32, i32),
    CopyDate(u32, &'a str),
    ProcessesTab(u32, bool),
    CopyReport(u32),
    LauncherCatalog(u32),
    LauncherIcons(u32, Vec<usize>),
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

fn parse_motion_message(value: &str) -> Option<(u32, bool)> {
    let (generation, reduced) = value.split_once(':')?;
    if generation.is_empty()
        || generation.len() > 10
        || !generation.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let generation = generation.parse::<u32>().ok().filter(|v| *v > 0)?;
    let reduced = match reduced {
        "0" => false,
        "1" => true,
        _ => return None,
    };
    Some((generation, reduced))
}

fn parse_ui_generation(value: &str) -> Option<u32> {
    if value.is_empty() || value.len() > 10 || !value.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    value.parse::<u32>().ok().filter(|v| *v > 0)
}

fn calendar_command(message: &str) -> Option<UiCommand<'_>> {
    if message == "host-ready" {
        return Some(UiCommand::HostReady);
    }
    if let Some(value) = message.strip_prefix("close:") {
        return parse_ui_generation(value).map(UiCommand::Close);
    }
    if let Some(value) = message.strip_prefix("copy-report:") {
        return parse_ui_generation(value).map(UiCommand::CopyReport);
    }
    if let Some(value) = message.strip_prefix("tab:") {
        let (generation, tab) = value.split_once(':')?;
        let active = match tab {
            "overview" => false,
            "processes" => true,
            _ => return None,
        };
        return Some(UiCommand::ProcessesTab(
            parse_ui_generation(generation)?,
            active,
        ));
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
    if let Some(value) = message.strip_prefix("icons-request:") {
        let (generation, raw_ids) = value.split_once(':')?;
        if generation.is_empty()
            || generation.len() > 10
            || !generation.bytes().all(|b| b.is_ascii_digit())
            || raw_ids.len() > 79
        {
            return None;
        }
        let generation = generation.parse::<u32>().ok().filter(|v| *v > 0)?;
        let parts: Vec<_> = raw_ids.split(',').collect();
        if parts.is_empty() || parts.len() > 16 {
            return None;
        }
        let mut ids = Vec::with_capacity(parts.len());
        for raw in parts {
            if raw.is_empty() || raw.len() > 4 || !raw.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            let id = raw.parse::<usize>().ok().filter(|id| *id < 1024)?;
            if ids.contains(&id) {
                return None;
            }
            ids.push(id);
        }
        return Some(UiCommand::LauncherIcons(generation, ids));
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
    // A bounded boolean accompanies each mount; native motion must honor
    // the exact WebKit prefers-reduced-motion setting for this generation.
    if let Some(value) = message.strip_prefix("mounted:") {
        let (generation, reduced) = parse_motion_message(value)?;
        return Some(UiCommand::Mounted(generation, reduced));
    }
    if let Some(value) = message.strip_prefix("motion:") {
        let (generation, reduced) = parse_motion_message(value)?;
        return Some(UiCommand::MotionPreference(generation, reduced));
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
    let value = message.strip_prefix("copy:")?;
    let (generation, date) = value.split_once(':')?;
    let generation = parse_ui_generation(generation)?;
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
    Some(UiCommand::CopyDate(generation, date))
}

fn calendar_document(html: &str) -> Option<String> {
    html.contains(CALENDAR_MARKER)
        .then(|| html.replacen(CALENDAR_MARKER, CALENDAR_SURFACE, 1))
}

fn build(
    application: &gtk::Application,
    document: String,
    id: u32,
    pool: std::rc::Weak<OverlayPool>,
) -> Rc<CalendarOverlay> {
    let window = gtk::ApplicationWindow::new(application);
    window.set_title(Some(TITLE));
    window.set_decorated(false);
    window.init_layer_shell();
    window.set_namespace(Some("kira-overlay"));
    window.set_layer(Layer::Overlay);
    window.set_anchor(Edge::Top, true);
    window.set_anchor(Edge::Right, true);
    window.set_margin(Edge::Top, crate::native_ui::POPUP_TOP_MARGIN);
    window.set_margin(Edge::Right, crate::native_ui::POPUP_RIGHT_MARGIN);
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
        id,
        pool,
        disposed: Cell::new(false),
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
        revealed: Cell::new(false),
        native_reduced_motion: Cell::new(false),
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
        if let Some(state) = weak.upgrade()
            && !state.disposed.get()
        {
            state.host_ready.set(false);
            state.hide();
        }
        false
    });
    let weak = Rc::downgrade(&overlay);
    view.connect_web_process_terminated(move |_, reason| {
        eprintln!("kira-shell: Calendar WebKit renderer terminated: {reason:?}");
        if let Some(state) = weak.upgrade()
            && !state.disposed.get()
        {
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
                // Focus shifts BETWEEN our concurrent popup windows must
                // never cancel another visible widget's GSAP timeline.
                if !state.pool.upgrade().is_some_and(|pool| pool.concurrent()) {
                    state.hide();
                }
            }
        }
    });

    // Load WebKit once; only the lightweight JS host survives between
    // shows. The Calendar component, timer and DOM are unmounted on hide.
    window.set_child(Some(&view));
    view.load_html(&overlay.document, None);
    // Timed native work runs only from the pool's single active lease
    // scheduler. This window owns no permanent polling sources.
    overlay
}

fn ensure(application: &gtk::Application, html: &str) -> Option<Rc<OverlayPool>> {
    POOL.with(|cache| {
        if let Some(pool) = cache.borrow().as_ref() {
            return Some(pool.clone());
        }
        let document = calendar_document(html)?;
        let pool = OverlayPool::new(application, document);
        *cache.borrow_mut() = Some(pool.clone());
        Some(pool)
    })
}

/// One inert, bounded warm popup WebView, no speculative widget DOM or ECharts.
pub fn prewarm(application: &gtk::Application, html: &str) {
    if ensure(application, html).is_none() {
        eprintln!("kira-shell: Overlay HTML root not found");
    }
}

pub fn dismiss(_application: &gtk::Application) -> bool {
    POOL.with(|cache| {
        let Some(pool) = cache.borrow().as_ref().cloned() else {
            return false;
        };
        if let Some(view) = pool.active(Surface::Calendar) {
            view.hide();
            return true;
        }
        false
    })
}

pub fn dismiss_system(_application: &gtk::Application) -> bool {
    POOL.with(|cache| {
        let Some(pool) = cache.borrow().as_ref().cloned() else {
            return false;
        };
        if let Some(view) = pool.active(Surface::System) {
            view.hide();
            return true;
        }
        false
    })
}

pub fn toggle(application: &gtk::Application, html: &str) {
    let Some(pool) = ensure(application, html) else {
        return;
    };
    let Some((view, existing)) = pool.acquire(Surface::Calendar) else {
        return;
    };
    if existing {
        view.hide();
    } else {
        view.show(Surface::Calendar);
    }
}

pub fn open_launcher(application: &gtk::Application, html: &str) {
    let Some(pool) = ensure(application, html) else {
        return;
    };
    let Some((view, existing)) = pool.acquire(Surface::Launcher) else {
        return;
    };
    if existing {
        view.window.present();
    } else {
        view.launcher.borrow_mut().take();
        view.show(Surface::Launcher);
    }
}

pub fn toggle_system(
    application: &gtk::Application,
    html: &str,
    metrics: Rc<RefCell<crate::system_metrics::Snapshot>>,
    core: Rc<Cell<bool>>,
    niri: Rc<Cell<bool>>,
) {
    let Some(pool) = ensure(application, html) else {
        return;
    };
    let Some((view, existing)) = pool.acquire(Surface::System) else {
        return;
    };
    if existing {
        view.hide();
        return;
    }
    *view.system.borrow_mut() = Some(crate::system_panel::Model::new(metrics, core, niri));
    if let Some(model) = view.system.borrow().as_ref() {
        model.set_pulse_active(true);
    }
    view.last_metrics.set(None);
    view.show(Surface::System);
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
            "close",
            "close:0",
            "close:-2",
            "close:4294967296",
            "tab:overview",
            "tab:0:processes",
            "copy-report",
            "copy-report:zero",
            "painted:",
            "painted:2;exit",
            "painted:-1",
            "painted:18446744073709551616",
            "tab:everything",
            "apps-request:",
            "apps-request:abc",
            "icons-request:",
            "icons-request:0:1",
            "icons-request:1:",
            "icons-request:1:1024",
            "icons-request:1:1,,2",
            "icons-request:1:1,1",
            "icons-request:1:1;sh",
            "icons-request:1:1:2",
            "icons-request:4294967296:1",
            "launch:1:1024",
            "launch:1:-1",
            "launch:1:0;sh",
            "launch:18446744073709551616:0",
            "copy:2026-01-01;exit",
            "mounted:",
            "mounted:abc",
            "mounted:4294967296",
            "mounted:2",
            "mounted:2:2",
            "mounted:2:1:0",
            "mounted:0:1",
            "motion:0:1",
            "motion:2:false",
            "motion:2:1;exit",
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
        assert_eq!(calendar_command("close:12"), Some(UiCommand::Close(12)));
        assert_eq!(calendar_command("host-ready"), Some(UiCommand::HostReady));
        assert_eq!(
            calendar_command("tab:12:processes"),
            Some(UiCommand::ProcessesTab(12, true))
        );
        assert_eq!(
            calendar_command("tab:12:overview"),
            Some(UiCommand::ProcessesTab(12, false))
        );
        assert_eq!(
            calendar_command("copy-report:12"),
            Some(UiCommand::CopyReport(12))
        );
        assert_eq!(
            calendar_command("apps-request:12"),
            Some(UiCommand::LauncherCatalog(12))
        );
        assert_eq!(
            calendar_command("launch:12:42"),
            Some(UiCommand::LaunchApp(12, 42))
        );
        assert_eq!(
            calendar_command("mounted:42:0"),
            Some(UiCommand::Mounted(42, false))
        );
        assert_eq!(
            calendar_command("mounted:42:1"),
            Some(UiCommand::Mounted(42, true))
        );
        assert_eq!(
            calendar_command("motion:42:1"),
            Some(UiCommand::MotionPreference(42, true))
        );
        assert_eq!(
            calendar_command("motion:42:0"),
            Some(UiCommand::MotionPreference(42, false))
        );
        assert_eq!(
            calendar_command("size:42:512"),
            Some(UiCommand::ContentHeight(42, 512))
        );
        assert_eq!(calendar_command("painted:42"), Some(UiCommand::Painted(42)));
        assert_eq!(
            calendar_command("copy:12:2024-02-29"),
            Some(UiCommand::CopyDate(12, "2024-02-29"))
        );
    }

    #[test]
    fn native_fade_is_monotone_and_settles_exactly() {
        assert_eq!(native_fade_progress(Duration::ZERO), 0.0);
        let half = native_fade_progress(Duration::from_millis(80));
        assert!(half > 0.5 && half < 1.0);
        assert_eq!(native_fade_progress(Duration::from_millis(160)), 1.0);
        assert_eq!(native_fade_progress(Duration::from_millis(500)), 1.0);
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
