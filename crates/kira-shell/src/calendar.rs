//! One active widget across transient and promoted Niri windows.
//! Promotion retains the live WebKit/Svelte state until another widget opens.
use crate::popup_layout::{self, PopupDock};
use crate::webview_pool::{Reservation, Surface, WebviewScheduler};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use webkit6::prelude::*;

const TITLE: &str = "Kira Overlay";
// Another 20% increase over the existing 133ms fade, synchronized with
// the 1.44x reversible WebKit scene.
const OVERLAY_FADE_MS: u64 = 111;

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

/// Top Bar stays separate. Only one transient widget is ever visible;
/// additional leased WebViews belong to independently tiled Niri windows.
type OpenRequest = Box<dyn FnOnce(&Rc<OverlayPool>)>;
struct OverlayPool {
    application: gtk::Application,
    context: webkit6::WebContext,
    document: String,
    scheduler: RefCell<WebviewScheduler>,
    windows: RefCell<HashMap<u32, Rc<CalendarOverlay>>>,
    pending: RefCell<Option<OpenRequest>>,
    poll_source: RefCell<Option<gtk::glib::SourceId>>,
    last_tick: Cell<Instant>,
}

impl OverlayPool {
    fn new(
        application: &gtk::Application,
        document: String,
        context: &webkit6::WebContext,
    ) -> Rc<Self> {
        // Install once per popup pool, before any warm GtkApplicationWindow
        // is constructed. The selector never touches the permanent Top Bar.
        crate::native_ui::install_transparent_popup_style();
        // KIRA_WEBVIEW_WARM=0 disables startup prewarm AND the retained
        // hot spare after closing a widget (45s grace still applies).
        // Keep the default unchanged until before/after PSS is measured.
        let cold = std::env::var("KIRA_WEBVIEW_WARM").is_ok_and(|value| value == "0");
        let scheduler = if cold {
            WebviewScheduler::cold()
        } else {
            WebviewScheduler::default()
        };
        let pool = Rc::new(Self {
            application: application.clone(),
            context: context.clone(),
            document,
            scheduler: RefCell::new(scheduler),
            windows: RefCell::new(HashMap::new()),
            pending: RefCell::new(None),
            poll_source: RefCell::new(None),
            last_tick: Cell::new(Instant::now()),
        });
        if let Some(id) = pool.scheduler.borrow_mut().prewarm_one(Instant::now()) {
            let view = build(
                &pool.application,
                &pool.context,
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
                    &self.context,
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
        // An exhausted renderer must not be re-leased on the next click.
        // Keep its slot quarantined until the Shell is restarted.
        if view.disposed.get() {
            self.scheduler.borrow_mut().release(id, Instant::now());
            eprintln!(
                "kira-shell: popup renderer unavailable after repeated crashes; restart Shell"
            );
            return None;
        }
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
                view.poll_displays();
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

    fn popup(&self) -> Option<Rc<CalendarOverlay>> {
        self.windows
            .borrow()
            .values()
            .find(|view| view.requested.get() && !view.detached.get())
            .cloned()
    }

    /// Detached Niri windows keep their own leases and remain alive across
    /// widget changes. Only the single transient popup is ever replaced.
    fn request(self: &Rc<Self>, kind: Surface, toggle: bool, open: OpenRequest) {
        if let Some(current) = self.active(kind).filter(|view| view.requested.get()) {
            // Reopening an existing kind never creates a duplicate instance.
            self.pending.borrow_mut().take();
            if current.detached.get() {
                // A promoted window keeps its WebView, state and generation.
                // A Top Bar click raises it without touching another popup.
                current.cancel_close();
                current.focus_detached();
            } else if toggle && !current.closing.get() {
                current.request_close();
            } else {
                current.cancel_close();
                if !toggle {
                    current.window.present();
                }
            }
            return;
        }
        if let Some(current) = self.popup() {
            // Last request wins, but only an actual transient popup is
            // dismissed. Promoted windows are never part of this queue.
            self.pending.replace(Some(open));
            current.request_close();
        } else {
            self.pending.borrow_mut().take();
            open(self);
        }
    }

    fn run_pending(self: &Rc<Self>) {
        // Wait for the transient popup's reversible close acknowledgement.
        // Detached Niri windows do not block or get closed by the queue.
        if let Some(current) = self.popup() {
            if self.pending.borrow().is_some() {
                current.request_close();
            }
            return;
        }
        // Release the RefCell before the callback can itself open a window.
        let next = self.pending.borrow_mut().take();
        if let Some(open) = next {
            open(self);
        }
    }

    fn choose_dock(&self, kind: Surface) -> PopupDock {
        popup_layout::choose_dock(kind, false)
    }

    fn width_for(&self, view: &CalendarOverlay) -> i32 {
        let output_width = crate::native_ui::popup_output_width(&view.window);
        if view.active.get() == Some(Surface::Settings) {
            popup_layout::settings_popup_width(output_width)
        } else {
            popup_layout::popup_width(output_width, 1, false)
        }
    }

    // Only the single transient window gets layer-shell geometry.
    fn sync_geometry(&self) {
        if let Some(view) = self.popup() {
            crate::native_ui::resize_webview_popup(
                &view.window,
                self.width_for(&view),
                view.content_height.get().max(1),
            );
        }
        crate::notifications::layout_changed();
    }

    fn release(self: &Rc<Self>, id: u32) {
        self.scheduler.borrow_mut().release(id, Instant::now());
        self.sync_geometry();
        self.update_launcher_keyboard_mode();
        self.run_pending();
    }

    fn update_launcher_keyboard_mode(&self) {
        // Exclusive is useful for an isolated type-to-search Launcher but
        // blocks interaction with other simultaneously visible windows.
        // Keep its solo behavior; use OnDemand with multiple popup leases.
        if let Some(launcher) = self
            .active(Surface::Launcher)
            .filter(|view| !view.detached.get())
        {
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
    recovery: RefCell<crate::renderer_recovery::RendererRecovery>,
    window: gtk::ApplicationWindow,
    regular_window: RefCell<Option<gtk::ApplicationWindow>>,
    detached: Cell<bool>,
    view: webkit6::WebView,
    document: String,
    host_ready: Cell<bool>,
    widget_ready: Cell<bool>,
    content_height: Cell<i32>,
    active: Cell<Option<Surface>>,
    system: RefCell<Option<crate::system_panel::Model>>,
    launcher: RefCell<Option<crate::launcher::Catalog>>,
    settings_about: RefCell<Option<serde_json::Value>>,
    settings_displays: RefCell<Option<crate::display_manager::Model>>,
    last_metrics: Cell<Option<Instant>>,
    requested: Cell<bool>,
    closing: Cell<bool>,
    revealed: Cell<bool>,
    native_reduced_motion: Cell<bool>,
    paint_pending: Cell<bool>,
    generation: Cell<u32>,
    dock: Cell<PopupDock>,
    open_started: Cell<Option<Instant>>,
}

impl CalendarOverlay {
    /// The current widget chooses its dock; detached windows are placed by Niri.
    fn position_for(&self, kind: Surface) {
        let dock = self
            .pool
            .upgrade()
            .map(|pool| pool.choose_dock(kind))
            .unwrap_or(popup_layout::choose_dock(kind, false));
        self.apply_dock(dock);
    }

    fn apply_dock(&self, dock: PopupDock) {
        self.dock.set(dock);
        // Reset every edge before retargeting the layer-shell surface.
        self.window.set_margin(Edge::Top, 0);
        self.window.set_margin(Edge::Right, 0);
        self.window.set_margin(Edge::Bottom, 0);
        self.window.set_margin(Edge::Left, 0);
        // With neither horizontal edge anchored, layer-shell centers the
        // launcher. The top edge remains pinned, never vertically centered.
        self.window.set_anchor(Edge::Top, true);
        self.window
            .set_anchor(Edge::Right, dock == PopupDock::Right);
        self.window.set_anchor(Edge::Bottom, false);
        self.window.set_anchor(Edge::Left, false);
        self.window.set_exclusive_zone(0);
        match dock {
            PopupDock::Center => {
                let output_height = crate::native_ui::popup_output_height(&self.window);
                self.window
                    .set_margin(Edge::Top, popup_layout::launcher_top_margin(output_height));
            }
            PopupDock::Right => {
                self.window
                    .set_margin(Edge::Top, crate::native_ui::POPUP_TOP_MARGIN);
                self.window
                    .set_margin(Edge::Right, crate::native_ui::POPUP_RIGHT_MARGIN);
            }
        }
    }

    /// Keep the SAME WebView and Svelte state while moving it from the
    /// layer-shell popup to a decorated xdg-toplevel managed by Niri.
    fn detach(self: &Rc<Self>) {
        if !self.requested.get() || self.closing.get() || self.detached.get() {
            eprintln!(
                "kira-shell: pop-out rejected: requested={} closing={} detached={}",
                self.requested.get(),
                self.closing.get(),
                self.detached.get()
            );
            return;
        }
        if !self.revealed.get() {
            // A fast click while the first native fade is in flight should
            // still promote the ready Svelte widget, not silently do nothing.
            // A view not yet mounted cannot receive a user click.
            if self.widget_ready.get() {
                self.reveal();
            } else {
                eprintln!("kira-shell: pop-out rejected: Svelte widget not mounted");
                return;
            }
        }
        let Some(pool) = self.pool.upgrade() else {
            return;
        };
        let kind = self.active.get().expect("requested view has a surface");
        self.detached.set(true);
        crate::notifications::layout_changed();
        let normal = gtk::ApplicationWindow::new(&pool.application);
        let title = match kind {
            Surface::Calendar => "Kira Calendar",
            Surface::System => "Kira System Monitor",
            Surface::Launcher => "Kira Launcher",
            Surface::Settings => "Kira Settings",
            #[cfg(test)]
            Surface::Synthetic(_) => "Kira Widget",
        };
        normal.set_title(Some(title));
        normal.set_decorated(true);
        if kind == Surface::Settings {
            normal.set_default_size(940, 700);
        } else if kind == Surface::Calendar {
            // A real Niri window hosts the date grid and the offline sky atlas.
            // The 420px transient popup stays unchanged.
            normal.set_default_size(1040, 720);
        } else {
            normal.set_default_size(500, (self.content_height.get() + 32).clamp(420, 760));
        }
        // Different Wayland roles require two GTK toplevels. Reparenting
        // retains the live WebKit process, DOM and generation.
        self.window.set_child(None::<&gtk::Widget>);
        self.window.set_visible(false);
        self.window.set_keyboard_mode(KeyboardMode::None);
        normal.set_child(Some(&self.view));
        let weak = Rc::downgrade(self);
        normal.connect_close_request(move |_| {
            if let Some(view) = weak.upgrade() {
                if view.requested.get() {
                    view.request_close();
                    return gtk::glib::Propagation::Stop;
                }
            }
            gtk::glib::Propagation::Proceed
        });
        self.regular_window.replace(Some(normal.clone()));
        normal.present();
        // Deliver state after GTK queues the new toplevel mapping. An event
        // evaluated against the temporarily unparented WebView can be lost.
        let generation = self.generation.get();
        let weak = Rc::downgrade(self);
        gtk::glib::idle_add_local_once(move || {
            let Some(state) = weak.upgrade() else {
                return;
            };
            if !state.requested.get()
                || !state.detached.get()
                || state.generation.get() != generation
            {
                return;
            }
            let script = format!(
                "document.documentElement.dataset.kiraDetached = 'true'; \
                 window.dispatchEvent(new CustomEvent('kira:overlay:detached', \
                 {{ detail: {{ generation: {generation} }} }}));"
            );
            state.view.evaluate_javascript(
                &script,
                None,
                None,
                None::<&gtk::gio::Cancellable>,
                |result| {
                    if let Err(error) = result {
                        eprintln!("kira-shell: widget detach notification failed: {error}");
                    }
                },
            );
        });
        pool.update_launcher_keyboard_mode();
        pool.run_pending();
    }

    fn focus_detached(&self) -> bool {
        if !self.detached.get() {
            return false;
        }
        if let Some(window) = self.regular_window.borrow().as_ref() {
            window.present();
        }
        true
    }

    /// Emergency/reduced-motion path and completion of the reversible
    /// exit. Never release this pool lease while the Svelte scene is playing.
    fn hide(&self) {
        if !self.requested.replace(false) {
            return;
        }
        self.closing.set(false);
        if let Some(model) = self.system.borrow().as_ref() {
            model.set_processes_active(false);
            model.set_pulse_active(false);
        }
        // Idle WebKit is warm, but it must retain no live sampler or
        // expensive widget-specific model from a previous lease.
        self.system.borrow_mut().take();
        self.active.set(None);
        self.launcher.borrow_mut().take();
        self.settings_about.borrow_mut().take();
        // Dropping the sender while confirmation is pending forces the
        // display worker to roll back even if the WebView was closed.
        self.settings_displays.borrow_mut().take();
        self.widget_ready.set(false);
        self.content_height.set(0);
        self.revealed.set(false);
        self.paint_pending.set(false);
        self.open_started.take();
        // A detached view returns to its inert popup host only when its
        // regular Niri window is closed. No second Svelte mount is needed.
        let regular = self.regular_window.borrow_mut().take();
        if let Some(normal) = regular {
            normal.set_child(None::<&gtk::Widget>);
            self.window.set_child(Some(&self.view));
            normal.close();
        }
        self.detached.set(false);
        // Deliver a generation-scoped hide. WebKit evaluates scripts
        // asynchronously: a stale hide must never unmount a newer show.
        if self.host_ready.get() {
            let generation = self.generation.get();
            let script = format!(
                "delete document.documentElement.dataset.kiraDetached;                  window.dispatchEvent(new CustomEvent('kira:overlay:hide', {{ detail: {{ generation: {generation} }} }}));"
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

    /// Request a TENET exit without tearing down its renderer or layout.
    /// WebKit reverses its recorded entrance and confirms 'closed:<gen>'.
    /// A bounded timeout protects against throttled/crashed WebKit frames.
    fn request_close(self: &Rc<Self>) {
        if !self.requested.get() || self.closing.replace(true) {
            return;
        }
        if let Some(model) = self.system.borrow().as_ref() {
            model.set_processes_active(false);
            model.set_pulse_active(false);
        }
        if self.native_reduced_motion.get()
            || !self.host_ready.get()
            || !self.widget_ready.get()
            || !self.revealed.get()
        {
            self.hide();
            return;
        }
        // The reverse takes place on a fully visible native compositor
        // buffer. Hiding GTK now would erase the recorded intermediate steps.
        if let Some(normal) = self.regular_window.borrow().as_ref() {
            normal.set_opacity(1.0);
        } else {
            self.window.set_opacity(1.0);
        }
        let generation = self.generation.get();
        let script = format!(
            "window.dispatchEvent(new CustomEvent('kira:overlay:close', {{ detail: {{ generation: {generation} }} }}));"
        );
        self.view.evaluate_javascript(
            &script,
            None,
            None,
            None::<&gtk::gio::Cancellable>,
            |result| {
                if let Err(error) = result {
                    eprintln!("kira-shell: TENET reverse request failed: {error}");
                }
            },
        );
        let weak = Rc::downgrade(self);
        gtk::glib::timeout_add_local_once(Duration::from_millis(920), move || {
            if let Some(view) = weak.upgrade()
                && view.requested.get()
                && view.closing.get()
                && view.generation.get() == generation
            {
                eprintln!("kira-shell: TENET close timed out; reclaiming popup safely");
                view.hide();
            }
        });
    }

    /// Reopening mid-reverse retargets the SAME recorded timeline rather
    /// than replacing a live view or starting another mount generation.
    fn cancel_close(&self) {
        if !self.closing.replace(false) {
            return;
        }
        let generation = self.generation.get();
        let script = format!(
            "window.dispatchEvent(new CustomEvent('kira:overlay:reopen', {{ detail: {{ generation: {generation} }} }}));"
        );
        self.view.evaluate_javascript(
            &script,
            None,
            None,
            None::<&gtk::gio::Cancellable>,
            |result| {
                if let Err(error) = result {
                    eprintln!("kira-shell: TENET reopen delivery failed: {error}");
                }
            },
        );
        if !self.focus_detached() {
            self.window.present();
        }
    }

    fn show(self: &Rc<Self>, kind: Surface) {
        // Only a previous transient popup needs to finish its reverse.
        // Promoted Niri windows keep their separate active leases.
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
        self.closing.set(false);
        self.revealed.set(false);
        self.native_reduced_motion.set(false);
        self.widget_ready.set(false);
        self.content_height.set(0);
        self.paint_pending.set(false);
        self.open_started.set(Some(Instant::now()));
        self.generation
            .set(self.generation.get().wrapping_add(1).max(1));
        // An unpainted layer-shell surface must never flash grey. GTK4
        // supports fully transparent toplevels on Wayland.
        self.window.set_opacity(0.0);
        // The next widget will report its intrinsic height before reveal.
        // Reset the previous widget's geometry while still transparent.
        let preferred_width = if kind == Surface::Settings {
            popup_layout::SETTINGS_POPUP_WIDTH
        } else {
            popup_layout::PREFERRED_POPUP_WIDTH
        };
        crate::native_ui::resize_webview_popup(&self.window, preferred_width, 1);
        let generation = self.generation.get();
        if let Some(pool) = self.pool.upgrade() {
            pool.sync_geometry();
        }
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
        gtk::glib::timeout_add_local_once(Duration::from_millis(125), move || {
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
                if state.closing.get() {
                    // The outgoing TENET scene needs an opaque native buffer;
                    // stop the opening fade rather than fighting JS reverse.
                    state.window.set_opacity(1.0);
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
                } else if self.active.get() == Some(Surface::Settings) {
                    self.send_settings_about();
                    if let Some(displays) = self.settings_displays.borrow().as_ref() {
                        displays.refresh();
                    }
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
                if reduced && self.closing.get() {
                    self.hide();
                } else if reduced && self.revealed.get() {
                    self.window.set_opacity(1.0);
                }
            }
            Some(UiCommand::ContentHeight(generation, height))
                if self.requested.get() && self.generation.get() == generation =>
            {
                // Layout-driven dimensions come only from the bounded,
                // trusted overlay channel. Ignore stale generations and
                // no-op resizes to avoid feedback from ResizeObserver.
                if self.detached.get() {
                    return;
                }
                let clamped =
                    height.min(crate::native_ui::webview_popup_height_limit(&self.window));
                if clamped != self.content_height.get() {
                    self.content_height.set(clamped);
                    let width = self
                        .pool
                        .upgrade()
                        .map(|pool| pool.width_for(self))
                        .unwrap_or(popup_layout::PREFERRED_POPUP_WIDTH);
                    crate::native_ui::resize_webview_popup(&self.window, width, clamped);
                    crate::notifications::layout_changed();
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
            Some(UiCommand::Detach(generation))
                if self.requested.get()
                    && self.generation.get() == generation
                    && !self.closing.get() =>
            {
                self.detach();
            }
            Some(UiCommand::DisplaySave(generation))
                if self.requested.get()
                    && !self.closing.get()
                    && self.generation.get() == generation
                    && self.active.get() == Some(Surface::Settings) =>
            {
                if let Some(model) = self.settings_displays.borrow().as_ref() {
                    model.save();
                }
            }
            Some(UiCommand::DisplayRefresh(generation))
                if self.requested.get()
                    && !self.closing.get()
                    && self.generation.get() == generation
                    && self.active.get() == Some(Surface::Settings) =>
            {
                if let Some(model) = self.settings_displays.borrow().as_ref() {
                    model.refresh();
                }
            }
            Some(UiCommand::DisplayApply(generation, payload))
                if self.requested.get()
                    && !self.closing.get()
                    && self.generation.get() == generation
                    && self.active.get() == Some(Surface::Settings) =>
            {
                if let Some(model) = self.settings_displays.borrow().as_ref() {
                    if let Ok(value) = serde_json::from_str(payload) {
                        model.apply(value);
                    }
                }
            }
            Some(UiCommand::DisplayConfirm(generation, id))
                if self.requested.get()
                    && !self.closing.get()
                    && self.generation.get() == generation
                    && self.active.get() == Some(Surface::Settings) =>
            {
                if let Some(model) = self.settings_displays.borrow().as_ref() {
                    model.confirm(id);
                }
            }
            Some(UiCommand::DisplayRevert(generation, id))
                if self.requested.get()
                    && self.generation.get() == generation
                    && self.active.get() == Some(Surface::Settings) =>
            {
                if let Some(model) = self.settings_displays.borrow().as_ref() {
                    model.revert(id);
                }
            }
            Some(UiCommand::Close(generation))
                if self.requested.get() && self.generation.get() == generation =>
            {
                self.request_close();
            }
            Some(UiCommand::Closed(generation))
                if self.requested.get()
                    && self.closing.get()
                    && self.generation.get() == generation =>
            {
                self.hide();
            }
            Some(UiCommand::CopyDate(generation, date))
                if self.requested.get()
                    && self.generation.get() == generation
                    && self.revealed.get()
                    && self.active.get() == Some(Surface::Calendar) =>
            {
                let copied = if let Some(display) = gtk::gdk::Display::default() {
                    display.clipboard().set_text(date);
                    true
                } else {
                    false
                };
                // A validated, generation-bound acknowledgement is sent only
                // after the native clipboard setter runs; no optimistic COPIED.
                let result = serde_json::json!({
                    "generation": generation,
                    "date": date,
                    "ok": copied,
                });
                let script = format!(
                    "window.dispatchEvent(new CustomEvent('kira:calendar:copy-result', {{ detail: {result} }}));"
                );
                self.view.evaluate_javascript(
                    &script,
                    None,
                    None,
                    None::<&gtk::gio::Cancellable>,
                    |_| {},
                );
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
    fn poll_launcher(self: &Rc<Self>) {
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
                crate::launcher::LauncherMessage::Launched(Ok(())) => self.request_close(),
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

    /// Deliver an immutable, read-only snapshot after the settings Svelte
    /// component has subscribed. JSON is double-encoded, never JS source.
    fn send_settings_about(&self) {
        if !self.requested.get() || self.active.get() != Some(Surface::Settings) {
            return;
        }
        let data = self.settings_about.borrow();
        let Some(about) = data.as_ref() else {
            return;
        };
        let payload = serde_json::json!({
            "generation": self.generation.get(),
            "about": about,
        });
        if let Ok(serialized) = serde_json::to_string(&payload)
            && let Ok(encoded) = serde_json::to_string(&serialized)
        {
            let script = format!(
                "window.dispatchEvent(new CustomEvent('kira:settings:about', {{ detail: JSON.parse({encoded}) }}));"
            );
            self.view.evaluate_javascript(
                &script,
                None,
                None,
                None::<&gtk::gio::Cancellable>,
                |result| {
                    if let Err(error) = result {
                        eprintln!("kira-shell: Settings info delivery failed: {error}");
                    }
                },
            );
        }
    }

    fn poll_displays(&self) {
        if !self.requested.get() || self.active.get() != Some(Surface::Settings) {
            return;
        }
        let events = self
            .settings_displays
            .borrow()
            .as_ref()
            .map(crate::display_manager::Model::poll)
            .unwrap_or_default();
        for event in events {
            let payload = serde_json::json!({
                "generation": self.generation.get(),
                "state": event.state,
                "outputs": event.outputs,
                "pending_id": event.pending_id,
                "deadline_unix_ms": event.deadline_unix_ms,
                "error": event.error,
                "persistence": event.persistence,
            });
            if let Ok(serialized) = serde_json::to_string(&payload)
                && let Ok(encoded) = serde_json::to_string(&serialized)
            {
                let script = format!(
                    "window.dispatchEvent(new CustomEvent('kira:settings:displays', {{ detail: JSON.parse({encoded}) }}));"
                );
                self.view.evaluate_javascript(
                    &script,
                    None,
                    None,
                    None::<&gtk::gio::Cancellable>,
                    |result| {
                        if let Err(error) = result {
                            eprintln!("kira-shell: display status dispatch failed: {error}");
                        }
                    },
                );
            }
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
    Detach(u32),
    Closed(u32),
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
    DisplaySave(u32),
    DisplayRefresh(u32),
    DisplayApply(u32, &'a str),
    DisplayConfirm(u32, u64),
    DisplayRevert(u32, u64),
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
    if let Some(value) = message.strip_prefix("display-save:") {
        return parse_ui_generation(value).map(UiCommand::DisplaySave);
    }
    if let Some(value) = message.strip_prefix("display-refresh:") {
        return parse_ui_generation(value).map(UiCommand::DisplayRefresh);
    }
    if let Some(value) = message.strip_prefix("display-apply:") {
        let (generation, payload) = value.split_once(':')?;
        if payload.len() > 8192 || payload.len() < 2 {
            return None;
        }
        return Some(UiCommand::DisplayApply(
            parse_ui_generation(generation)?,
            payload,
        ));
    }
    for (prefix, confirm) in [("display-confirm:", true), ("display-revert:", false)] {
        if let Some(value) = message.strip_prefix(prefix) {
            let (generation, token) = value.split_once(':')?;
            if token.is_empty()
                || token.len() > 20
                || !token.bytes().all(|byte| byte.is_ascii_digit())
            {
                return None;
            }
            let generation = parse_ui_generation(generation)?;
            let token = token.parse::<u64>().ok().filter(|id| *id > 0)?;
            return Some(if confirm {
                UiCommand::DisplayConfirm(generation, token)
            } else {
                UiCommand::DisplayRevert(generation, token)
            });
        }
    }
    if let Some(value) = message.strip_prefix("close:") {
        return parse_ui_generation(value).map(UiCommand::Close);
    }
    if let Some(value) = message.strip_prefix("detach:") {
        return parse_ui_generation(value).map(UiCommand::Detach);
    }
    if let Some(value) = message.strip_prefix("closed:") {
        return parse_ui_generation(value).map(UiCommand::Closed);
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
    context: &webkit6::WebContext,
    document: String,
    id: u32,
    pool: std::rc::Weak<OverlayPool>,
) -> Rc<CalendarOverlay> {
    let window = gtk::ApplicationWindow::new(application);
    // GTK themes normally paint a gray window background behind transparent
    // WebKit pixels. The dedicated class removes that entire rectangle,
    // leaving only Svelte's intentionally opaque widget main visible.
    window.add_css_class("kira-popup-transparent");
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
    // Share only the WebKit context; script-message registrations are per host.
    let manager = webkit6::UserContentManager::new();
    let view = webkit6::WebView::builder()
        .web_context(context)
        .user_content_manager(&manager)
        .build();
    // A black WebKit backing layer would flash as a full rectangle before
    // the GSAP surface mask can reveal the first pixels. Only popup WebViews
    // are transparent; the actual Svelte main remains solid Kira black.
    view.set_background_color(&gtk::gdk::RGBA::new(0.0, 0.0, 0.0, 0.0));
    crate::native_ui::restrict_webview_navigation(&view);

    let overlay = Rc::new(CalendarOverlay {
        id,
        pool,
        disposed: Cell::new(false),
        recovery: RefCell::new(crate::renderer_recovery::RendererRecovery::default()),
        window: window.clone(),
        regular_window: RefCell::new(None),
        detached: Cell::new(false),
        view: view.clone(),
        document,
        host_ready: Cell::new(false),
        widget_ready: Cell::new(false),
        content_height: Cell::new(0),
        active: Cell::new(None),
        system: RefCell::new(None),
        launcher: RefCell::new(None),
        settings_about: RefCell::new(None),
        settings_displays: RefCell::new(None),
        last_metrics: Cell::new(None),
        requested: Cell::new(false),
        closing: Cell::new(false),
        revealed: Cell::new(false),
        native_reduced_motion: Cell::new(false),
        paint_pending: Cell::new(false),
        generation: Cell::new(0),
        dock: Cell::new(PopupDock::Right),
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
        use crate::renderer_recovery::RetryDecision;
        let Some(state) = weak.upgrade() else {
            return;
        };
        if state.disposed.get() {
            return;
        }
        let decision = state.recovery.borrow_mut().on_terminated(Instant::now());
        eprintln!("kira-shell: Popup WebKit renderer terminated ({reason:?}): {decision:?}");
        state.host_ready.set(false);
        // Quarantine before hide(), because hiding releases the lease and can
        // immediately dispatch a queued popup request on this same slot.
        if decision == RetryDecision::Exhausted {
            state.disposed.set(true);
        }
        state.hide();
        if let RetryDecision::After(delay) = decision {
            // Reload outside the WebKit termination callback. Using the shared
            // exponential backoff prevents a busy crash/restart loop.
            let weak = Rc::downgrade(&state);
            gtk::glib::timeout_add_local_once(delay, move || {
                if let Some(state) = weak.upgrade()
                    && !state.disposed.get()
                {
                    state.recovery.borrow_mut().retry_started();
                    state.view.load_html(&state.document, None);
                }
            });
        }
    });

    // Focus is not a close request. Clicking a Top Bar button first
    // deactivates the previous popup, and only then runs its new WebKit
    // command. Hiding on is-active-notify would destroy that first widget
    // before the scheduler has a chance to lease another WebView. Explicit
    // close/Escape and the per-window generation guard remain available.

    // Load WebKit once; only the lightweight JS host survives between
    // shows. The Calendar component, timer and DOM are unmounted on hide.
    window.set_child(Some(&view));
    view.load_html(&overlay.document, None);
    // Timed native work runs only from the pool's single active lease
    // scheduler. This window owns no permanent polling sources.
    overlay
}

fn ensure(
    application: &gtk::Application,
    html: &str,
    context: &webkit6::WebContext,
) -> Option<Rc<OverlayPool>> {
    POOL.with(|cache| {
        if let Some(pool) = cache.borrow().as_ref() {
            return Some(pool.clone());
        }
        let document = calendar_document(html)?;
        let pool = OverlayPool::new(application, document, context);
        *cache.borrow_mut() = Some(pool.clone());
        Some(pool)
    })
}

/// The notifications stay bottom-right, so their only safe adjustment is
/// reducing the upper bound of the bottom-anchored WebView. Never move widgets.
/// A detached Niri-managed window has no known compositor rect in this module;
/// conservatively suspend automatic toast presentation until it closes.
pub fn notification_bottom_clearance(output_width: i32, output_height: i32) -> i32 {
    POOL.with(|cache| {
        let pool = cache.borrow();
        let Some(pool) = pool.as_ref() else { return output_height };
        if pool.windows.borrow().values().any(|view| {
            view.requested.get() && view.detached.get()
        }) {
            return 0;
        }
        let Some(view) = pool.popup() else { return output_height };
        let overlay_width = pool.width_for(&view);
        let overlaps_right_lane = match view.dock.get() {
            PopupDock::Right => true,
            PopupDock::Center => {
                let center_right = (output_width + overlay_width) / 2;
                let toast_left = output_width
                    - crate::notification_layout::PREFERRED_WIDTH
                        .min(output_width.saturating_sub(24))
                    - crate::notification_layout::OUTER_MARGIN;
                center_right + 12 > toast_left
            }
        };
        if !overlaps_right_lane { return output_height }
        let top = match view.dock.get() {
            PopupDock::Right => crate::native_ui::POPUP_TOP_MARGIN,
            PopupDock::Center => popup_layout::launcher_top_margin(output_height),
        };
        // The Svelte-reported natural height can precede GTK's map/configure;
        // use the larger of the last actual allocation and reported content.
        let widget_height = view.content_height.get()
            .max(view.window.height())
            .min((output_height * 7 / 10).max(1));
        (output_height - top - widget_height - 16).max(0)
    })
}

/// One inert, bounded warm popup WebView, no speculative widget DOM or ECharts.
pub fn prewarm(application: &gtk::Application, html: &str, context: &webkit6::WebContext) {
    if ensure(application, html, context).is_none() {
        eprintln!("kira-shell: Overlay HTML root not found");
    }
}

pub fn dismiss(_application: &gtk::Application) -> bool {
    POOL.with(|cache| {
        let Some(pool) = cache.borrow().as_ref().cloned() else {
            return false;
        };
        if let Some(view) = pool.active(Surface::Calendar) {
            view.request_close();
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
            view.request_close();
            return true;
        }
        false
    })
}

pub fn toggle(application: &gtk::Application, html: &str, context: &webkit6::WebContext) {
    let Some(pool) = ensure(application, html, context) else {
        return;
    };
    pool.request(
        Surface::Calendar,
        true,
        Box::new(|pool| {
            let Some((view, existing)) = pool.acquire(Surface::Calendar) else {
                return;
            };
            if existing {
                if !view.focus_detached() {
                    if view.closing.get() {
                        view.cancel_close();
                    } else {
                        view.request_close();
                    }
                }
            } else {
                view.show(Surface::Calendar);
            }
        }),
    );
}

pub fn open_launcher(application: &gtk::Application, html: &str, context: &webkit6::WebContext) {
    let Some(pool) = ensure(application, html, context) else {
        return;
    };
    pool.request(
        Surface::Launcher,
        false,
        Box::new(|pool| {
            let Some((view, existing)) = pool.acquire(Surface::Launcher) else {
                return;
            };
            if existing {
                if !view.focus_detached() {
                    view.cancel_close();
                    view.window.present();
                }
            } else {
                view.launcher.borrow_mut().take();
                view.show(Surface::Launcher);
            }
        }),
    );
}

/// Open the same Settings surface in popup mode; pop-out retains the exact
/// WebView and its in-memory navigation state.
pub fn toggle_settings(application: &gtk::Application, html: &str, context: &webkit6::WebContext) {
    let Some(pool) = ensure(application, html, context) else {
        return;
    };
    pool.request(
        Surface::Settings,
        true,
        Box::new(|pool| {
            let Some((view, existing)) = pool.acquire(Surface::Settings) else {
                return;
            };
            if existing {
                if !view.focus_detached() {
                    if view.closing.get() {
                        view.cancel_close();
                    } else {
                        view.request_close();
                    }
                }
                return;
            }
            *view.settings_about.borrow_mut() = Some(crate::system_info::about_snapshot());
            *view.settings_displays.borrow_mut() = Some(crate::display_manager::Model::new());
            view.show(Surface::Settings);
        }),
    );
}

pub fn toggle_system(
    application: &gtk::Application,
    html: &str,
    context: &webkit6::WebContext,
    metrics: Rc<RefCell<crate::system_metrics::Snapshot>>,
    core: Rc<Cell<bool>>,
    niri: Rc<Cell<bool>>,
) {
    let Some(pool) = ensure(application, html, context) else {
        return;
    };
    pool.request(
        Surface::System,
        true,
        Box::new(move |pool| {
            let Some((view, existing)) = pool.acquire(Surface::System) else {
                return;
            };
            if existing {
                if !view.focus_detached() {
                    if view.closing.get() {
                        view.cancel_close();
                    } else {
                        view.request_close();
                    }
                }
                return;
            }
            *view.system.borrow_mut() = Some(crate::system_panel::Model::new(metrics, core, niri));
            if let Some(model) = view.system.borrow().as_ref() {
                model.set_pulse_active(true);
            }
            view.last_metrics.set(None);
            view.show(Surface::System);
        }),
    );
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
            "closed:",
            "detach:",
            "detach:0",
            "detach:-1",
            "detach:4294967296",
            "detach:12:13",
            "closed:0",
            "closed:-1",
            "closed:4294967296",
            "closed:12:13",
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
        assert_eq!(calendar_command("detach:12"), Some(UiCommand::Detach(12)));
        assert_eq!(calendar_command("closed:12"), Some(UiCommand::Closed(12)));
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
        let half = native_fade_progress(Duration::from_millis(55));
        assert!(half > 0.5 && half < 1.0);
        assert_eq!(native_fade_progress(Duration::from_millis(111)), 1.0);
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
