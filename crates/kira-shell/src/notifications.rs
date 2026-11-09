//! Native freedesktop.org notification D-Bus endpoint. The main thread
//! owns a single bounded FIFO and reusable GTK4 host. No WebKit or JS bridge.
//! Never seize the preview user's session bus or preempt an existing daemon.
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk::gio;
use gtk::glib::{self, variant::ToVariant};
use gtk::prelude::*;

use kira_services::notification_queue::{
    DEFAULT_DWELL, Notice, NotificationOwnership, NotificationQueue,
};
use kira_ui::{
    NotificationHostSet, NotificationIntent, NotificationView, PopupHost, PopupKind,
    ToastHostSelection, ToastLayout,
};

const OBJECT_PATH: &str = "/org/freedesktop/Notifications";
const INTERFACE: &str = "org.freedesktop.Notifications";
const NAME: &str = "org.freedesktop.Notifications";
const XML: &str = r#"<node>
 <interface name="org.freedesktop.Notifications">
  <method name="GetCapabilities"><arg type="as" direction="out"/></method>
  <method name="GetServerInformation">
    <arg type="s" direction="out"/><arg type="s" direction="out"/>
    <arg type="s" direction="out"/><arg type="s" direction="out"/>
  </method>
  <method name="Notify">
    <arg type="s" direction="in"/><arg type="u" direction="in"/>
    <arg type="s" direction="in"/><arg type="s" direction="in"/>
    <arg type="s" direction="in"/><arg type="as" direction="in"/>
    <arg type="a{sv}" direction="in"/><arg type="i" direction="in"/>
    <arg type="u" direction="out"/>
  </method>
  <method name="CloseNotification"><arg type="u" direction="in"/></method>
  <signal name="NotificationClosed"><arg type="u"/><arg type="u"/></signal>
  <signal name="ActionInvoked"><arg type="u"/><arg type="s"/></signal>
 </interface>
</node>"#;

thread_local! {
    static SERVICE: RefCell<Option<Rc<Service>>> = const { RefCell::new(None) };
}

fn bounded(value: String, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

fn notice_from_params(params: &glib::Variant, id: u32) -> Notice {
    let app = bounded(
        params.child_value(0).get::<String>().unwrap_or_default(),
        128,
    );
    let summary = bounded(
        params.child_value(3).get::<String>().unwrap_or_default(),
        256,
    );
    let body = bounded(
        params.child_value(4).get::<String>().unwrap_or_default(),
        2048,
    );
    let raw_actions = params
        .child_value(5)
        .get::<Vec<String>>()
        .unwrap_or_default();
    let (complete_actions, _) = raw_actions.as_chunks::<2>();
    let actions = complete_actions
        .iter()
        .take(8)
        .map(|pair| (bounded(pair[0].clone(), 128), bounded(pair[1].clone(), 128)))
        .collect();
    let hints = glib::VariantDict::new(Some(&params.child_value(6)));
    let critical = hints
        .lookup_value("urgency", None)
        .and_then(|value| value.get::<u8>())
        == Some(2);
    let timeout = params.child_value(7).get::<i32>().unwrap_or(-1);
    let persistent = timeout == 0 || critical;
    let dwell = if timeout <= 0 {
        DEFAULT_DWELL
    } else {
        Duration::from_millis(u64::try_from(timeout.clamp(3000, 30000)).unwrap_or(5000))
    };
    Notice {
        id,
        app,
        summary,
        body,
        actions,
        critical,
        persistent,
        dwell,
    }
}

fn view(notice: &Notice) -> NotificationView {
    NotificationView {
        id: notice.id,
        app: notice.app.clone(),
        summary: notice.summary.clone(),
        body: notice.body.clone(),
        critical: notice.critical,
        actions: notice.actions.clone(),
    }
}

/// Never guess an output geometry or claim notification delivery when GTK
/// cannot expose the actual logical monitor bounds.
fn live_monitors() -> Vec<gtk::gdk::Monitor> {
    let Some(display) = gtk::gdk::Display::default() else {
        return Vec::new();
    };
    let model = display.monitors();
    (0..model.n_items())
        .filter_map(|index| model.item(index)?.downcast::<gtk::gdk::Monitor>().ok())
        .collect()
}

fn pooled_right_transient(popup: &PopupHost) -> bool {
    [
        PopupKind::Calendar,
        PopupKind::System,
        PopupKind::Settings,
        PopupKind::Tray,
    ]
    .into_iter()
    .any(|kind| popup.is_transient_active(kind))
}

fn current_target(
    popup: &PopupHost,
    retained: Option<&gtk::gdk::Monitor>,
) -> Option<(gtk::gdk::Monitor, ToastLayout)> {
    let monitors = live_monitors();
    let outputs = monitors
        .iter()
        .map(|monitor| {
            let rect = monitor.geometry();
            (rect.width(), rect.height())
        })
        .collect::<Vec<_>>();
    let preferred = popup
        .bar_monitor()
        .and_then(|bar| monitors.iter().position(|monitor| monitor == &bar));
    let mut blocked = vec![false; monitors.len()];
    if pooled_right_transient(popup) {
        // Without a mapped bar, we cannot prove which output the transient
        // overlaps. Never route a toast to an arbitrary first monitor.
        let index = preferred?;
        blocked[index] = true;
    }
    let previous = retained.and_then(|last| monitors.iter().position(|monitor| monitor == last));
    let (index, layout) =
        kira_ui::toast_output_plan_stable(&outputs, preferred, previous, &blocked)?;
    Some((monitors[index].clone(), layout))
}

/// A detached xdg-toplevel is managed by Niri, not a pooled overlay.
/// A transient popup only blocks a toast *on its own monitor*. Unknown
/// monitor affinity fails closed until real GDK output data is available.
fn toast_target_unobstructed(
    right_transient_active: bool,
    same_monitor_as_transient: Option<bool>,
) -> bool {
    !right_transient_active || same_monitor_as_transient == Some(false)
}

/// GTK may finish dispatching a button from the previous snapshot
/// after a monitor move. The Shell requires live ownership, an actually
/// painted active card and a still-current physical destination.
fn toast_intent_authorized(owned: bool, painted: bool, current_target: bool) -> bool {
    owned && painted && current_target
}

/// A D-Bus ownership epoch is a real trust boundary. Once the name is
/// gone, do not replay a foreign daemon's previous notifications or accept
/// stale sender leases if this process later reacquires the name.
fn revoke_notification_session(queue: &mut NotificationQueue, senders: &mut NotificationOwnership) {
    *queue = NotificationQueue::default();
    senders.clear();
}

/// Never schedule an already-due FIFO deadline when the host is covered,
/// unowned or unmapped. A real mapping/geometry/popup change resumes it.
fn next_visible_deadline(
    queue: &NotificationQueue,
    now: Instant,
    can_show: bool,
) -> Option<Instant> {
    if can_show {
        queue.next_deadline(now)
    } else {
        None
    }
}

struct Service {
    owned: Cell<bool>,
    owner_id: RefCell<Option<gio::OwnerId>>,
    bus: RefCell<Option<gio::DBusConnection>>,
    queue: RefCell<NotificationQueue>,
    senders: RefCell<NotificationOwnership>,
    host: RefCell<Option<NotificationHostSet>>,
    popup: Rc<PopupHost>,
    timer_revision: Cell<u64>,
}

impl Service {
    fn register_dbus(self: &Rc<Self>, connection: &gio::DBusConnection) {
        let Ok(node) = gio::DBusNodeInfo::for_xml(XML) else {
            eprintln!("kira-shell: invalid built-in Notifications D-Bus introspection");
            return;
        };
        let Some(info) = node.lookup_interface(INTERFACE) else {
            return;
        };
        let weak = Rc::downgrade(self);
        let registration = connection
            .register_object(OBJECT_PATH, &info)
            .method_call(move |_, sender, _, _, method, params, invocation| {
                let Some(service) = weak.upgrade() else {
                    invocation.return_dbus_error(
                        "org.freedesktop.DBus.Error.Failed",
                        "Notification host stopped",
                    );
                    return;
                };
                match method {
                    "GetCapabilities" => {
                        invocation.return_value(Some(
                            &(vec!["body".to_owned(), "actions".to_owned()],).to_variant(),
                        ));
                    }
                    "GetServerInformation" => {
                        invocation.return_value(Some(
                            &("Kira Desktop", "Kira", env!("CARGO_PKG_VERSION"), "1.3")
                                .to_variant(),
                        ));
                    }
                    "Notify" => {
                        if params.size() > 64 * 1024 {
                            invocation.return_dbus_error(
                                "org.freedesktop.Notifications.Error.ExcessData",
                                "Notification payload exceeds 64 KiB",
                            );
                            return;
                        }
                        // Supplied app names and IDs confer no authority.
                        // Only a unique D-Bus sender may replace its own ID.
                        let Some(sender) = sender.filter(|sender| sender.starts_with(':')) else {
                            invocation.return_dbus_error(
                                "org.freedesktop.DBus.Error.AccessDenied",
                                "A unique D-Bus sender is required",
                            );
                            return;
                        };
                        let (id, accepted) = {
                            let mut queue = service.queue.borrow_mut();
                            let id = service.senders.borrow().assign_id(
                                &mut queue,
                                sender,
                                params.child_value(1).get::<u32>().unwrap_or(0),
                            );
                            let notice = notice_from_params(&params, id);
                            (id, queue.push(notice, Instant::now()))
                        };
                        if !accepted {
                            invocation.return_dbus_error(
                                "org.freedesktop.Notifications.Error.QueueFull",
                                "Notification queue is full",
                            );
                            return;
                        }
                        service.senders.borrow_mut().record(id, sender);
                        invocation.return_value(Some(&(id,).to_variant()));
                        service.sync();
                    }
                    "CloseNotification" => {
                        let id = params.child_value(0).get::<u32>().unwrap_or(0);
                        // CloseNotification is an ID-based Freedesktop
                        // operation. Keep cross-process clients compatible;
                        // unlike Notify(replaces_id), it cannot inject
                        // replacement content or new action keys.
                        if service.queue.borrow_mut().close(id) {
                            service.senders.borrow_mut().forget(id);
                            invocation.return_value(None);
                            service.signal_closed(id, 3);
                            service.sync();
                        } else {
                            invocation.return_dbus_error(
                                "org.freedesktop.Notifications.Error.InvalidId",
                                "Unknown notification ID",
                            );
                        }
                    }
                    _ => invocation.return_dbus_error(
                        "org.freedesktop.DBus.Error.UnknownMethod",
                        "Unsupported notification method",
                    ),
                }
            })
            .build();
        if let Err(error) = registration {
            eprintln!("kira-shell: cannot register notification D-Bus endpoint: {error}");
            let weak = Rc::downgrade(self);
            glib::idle_add_local_once(move || {
                if let Some(service) = weak.upgrade() {
                    service.release_name();
                }
            });
        }
    }

    fn release_name(&self) {
        self.owned.set(false);
        self.bus.borrow_mut().take();
        self.timer_revision
            .set(self.timer_revision.get().wrapping_add(1));
        revoke_notification_session(&mut self.queue.borrow_mut(), &mut self.senders.borrow_mut());
        if let Some(host) = self.host.borrow().as_ref() {
            host.hide_all();
        }
        if let Some(id) = self.owner_id.borrow_mut().take() {
            gio::bus_unown_name(id);
        }
    }

    fn signal_closed(&self, id: u32, reason: u32) {
        if !self.owned.get() {
            return;
        }
        if let Some(connection) = self.bus.borrow().as_ref() {
            let _ = connection.emit_signal(
                None,
                OBJECT_PATH,
                INTERFACE,
                "NotificationClosed",
                Some(&(id, reason).to_variant()),
            );
        }
    }

    fn on_intent(self: &Rc<Self>, intent: NotificationIntent) {
        let id = match &intent {
            NotificationIntent::Close(id) | NotificationIntent::Action(id, _) => *id,
        };
        // A GTK signal queued on an old monitor, unpainted card or stale
        // snapshot must not invoke an otherwise live Freedesktop ID.
        let painted = self
            .host
            .borrow()
            .as_ref()
            .is_some_and(|host| host.is_painted(id));
        if !toast_intent_authorized(self.owned.get(), painted, self.target_is_current()) {
            return;
        }
        match intent {
            NotificationIntent::Close(id) => {
                if self.queue.borrow_mut().close(id) {
                    self.senders.borrow_mut().forget(id);
                    self.signal_closed(id, 2);
                    self.sync();
                }
            }
            NotificationIntent::Action(id, key) => {
                // Only keys declared on the currently visible, retained
                // notification may invoke the app's Freedesktop action.
                let allowed = self.queue.borrow().visible().any(|notice| {
                    notice.id == id && notice.actions.iter().any(|(action, _)| action == &key)
                });
                if !allowed {
                    return;
                }
                if let Some(connection) = self.bus.borrow().as_ref() {
                    let _ = connection.emit_signal(
                        None,
                        OBJECT_PATH,
                        INTERFACE,
                        "ActionInvoked",
                        Some(&(id, key.as_str()).to_variant()),
                    );
                }
                if self.queue.borrow_mut().close(id) {
                    self.senders.borrow_mut().forget(id);
                    self.signal_closed(id, 2);
                }
                self.sync();
            }
        }
    }

    fn on_painted(self: &Rc<Self>, ids: &[u32]) {
        if !self.owned.get() || ids.is_empty() || ids.len() > 3 || !self.target_is_current() {
            return;
        }
        if !self
            .queue
            .borrow()
            .visible()
            .map(|notice| notice.id)
            .eq(ids.iter().copied())
        {
            return;
        }
        if !self
            .host
            .borrow()
            .as_ref()
            .is_some_and(|host| host.reveal(ids))
        {
            return;
        }
        if self.queue.borrow_mut().mark_painted(Instant::now(), ids) {
            self.sync();
        }
    }

    fn target_is_current(&self) -> bool {
        let host = self.host.borrow();
        let Some(active) = host.as_ref().and_then(NotificationHostSet::active_monitor) else {
            return false;
        };
        current_target(&self.popup, Some(&active))
            .is_some_and(|(target, _)| target == active && self.can_show_on(&target))
    }

    fn can_show_on(&self, target: &gtk::gdk::Monitor) -> bool {
        // Double-check that a newly raised pooled transient has not
        // invalidated the chosen target while the GTK host was selected.
        let same_monitor = self.popup.bar_monitor().map(|bar| bar == *target);
        toast_target_unobstructed(pooled_right_transient(&self.popup), same_monitor)
    }

    fn sync(self: &Rc<Self>) {
        let now = Instant::now();
        let retained = self
            .host
            .borrow()
            .as_ref()
            .and_then(NotificationHostSet::active_monitor);
        let target = current_target(&self.popup, retained.as_ref());
        let (ready, switched) = {
            let borrowed = self.host.borrow();
            if let Some(host) = borrowed.as_ref() {
                // Remove unplugged GDK surfaces before any new paint ACK.
                let lost = host.reconcile(&live_monitors());
                let selection = if let Some((monitor, layout)) = target.as_ref() {
                    host.select(monitor, *layout)
                } else {
                    host.hide_all();
                    ToastHostSelection::Unavailable
                };
                (
                    selection != ToastHostSelection::Unavailable && host.is_renderable(),
                    lost || selection == ToastHostSelection::Changed,
                )
            } else {
                (false, false)
            }
        };
        let can_show = self.owned.get()
            && ready
            && target
                .as_ref()
                .is_some_and(|(monitor, _)| self.can_show_on(monitor));
        let expired = {
            let mut queue = self.queue.borrow_mut();
            let previous = queue
                .visible()
                .filter(|notice| !notice.persistent)
                .map(|notice| notice.id)
                .collect::<Vec<_>>();
            if switched {
                // A painted ID on another output has NOT been painted on
                // this one. Reset admission and dwell to the actual frame
                // of the newly mapped destination, never its old clock.
                queue.step(now, false);
            }
            queue.step(now, can_show);
            previous
                .into_iter()
                .filter(|id| !queue.contains_active(*id))
                .collect::<Vec<_>>()
        };
        for id in expired {
            self.senders.borrow_mut().forget(id);
            self.signal_closed(id, 1);
        }
        let cards = if can_show {
            self.queue.borrow().visible().map(view).collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        if let Some(host) = self.host.borrow().as_ref() {
            if can_show {
                if let Some((_, layout)) = target.as_ref() {
                    host.set_cards(*layout, &cards);
                }
            } else {
                // Never draw even a non-targetable reverse-ghost above an
                // occluding popup or a monitor that has disappeared.
                host.hide();
            }
        }
        self.schedule_deadline(now, can_show);
    }

    fn schedule_deadline(self: &Rc<Self>, now: Instant, can_show: bool) {
        let revision = self.timer_revision.get().wrapping_add(1);
        self.timer_revision.set(revision);
        let deadline = next_visible_deadline(&self.queue.borrow(), now, can_show);
        if let Some(deadline) = deadline {
            let weak = Rc::downgrade(self);
            glib::timeout_add_local_once(deadline.saturating_duration_since(now), move || {
                if let Some(service) = weak.upgrade()
                    && service.timer_revision.get() == revision
                {
                    service.sync();
                }
            });
        }
    }
}

fn observe_monitor(monitor: &gtk::gdk::Monitor, service: &Rc<Service>) {
    let weak = Rc::downgrade(service);
    monitor.connect_notify_local(Some("geometry"), move |_, _| {
        if let Some(service) = weak.upgrade() {
            service.sync();
        }
    });
    let weak = Rc::downgrade(service);
    monitor.connect_notify_local(Some("scale-factor"), move |_, _| {
        if let Some(service) = weak.upgrade() {
            service.sync();
        }
    });
}

fn observe_outputs(service: &Rc<Service>) {
    let Some(display) = gtk::gdk::Display::default() else {
        return;
    };
    let monitors = display.monitors();
    for index in 0..monitors.n_items() {
        if let Some(monitor) = monitors
            .item(index)
            .and_then(|item| item.downcast::<gtk::gdk::Monitor>().ok())
        {
            observe_monitor(&monitor, service);
        }
    }
    let weak = Rc::downgrade(service);
    monitors.connect_items_changed(move |model, position, _, added| {
        if let Some(service) = weak.upgrade() {
            for index in position..position.saturating_add(added) {
                if let Some(monitor) = model
                    .item(index)
                    .and_then(|item| item.downcast::<gtk::gdk::Monitor>().ok())
                {
                    observe_monitor(&monitor, &service);
                }
            }
            service.sync();
        }
    });
}

/// Called only when the popup occupancy actually changes, never on every
/// 100ms GTK transport tick.
pub fn layout_changed() {
    SERVICE.with(|active| {
        let service = active.borrow().clone();
        if let Some(service) = service {
            service.sync();
        }
    });
}

/// Preview ownership must have BOTH an explicit test opt-in and the
/// coordinator's verified private-bus attestation. Never allow a simple
/// inherited KIRA_NOTIFICATIONS_PREVIEW flag to seize the host's bus.
fn preview_notifications_allowed(
    preview: bool,
    opt_in: Option<&str>,
    isolated: Option<&str>,
) -> bool {
    !preview || (opt_in == Some("1") && isolated == Some("1"))
}

pub fn start(app: &gtk::Application, popup: Rc<PopupHost>) {
    if !preview_notifications_allowed(
        std::env::var_os("KIRA_PREVIEW").is_some(),
        std::env::var("KIRA_NOTIFICATIONS_PREVIEW").ok().as_deref(),
        std::env::var("KIRA_ISOLATED_SESSION_BUS").ok().as_deref(),
    ) {
        eprintln!("kira-shell: Notifications D-Bus disabled without isolated preview bus");
        return;
    }
    if !gtk::gdk::Display::default().is_some_and(|display| display.supports_input_shapes()) {
        eprintln!("kira-shell: no shaped pointer input; leaving notifications unowned");
        return;
    }
    let Some((monitor, layout)) = current_target(&popup, None) else {
        eprintln!("kira-shell: no safe monitor geometry for notifications");
        return;
    };

    let service = Rc::new(Service {
        owned: Cell::new(false),
        owner_id: RefCell::new(None),
        bus: RefCell::new(None),
        queue: RefCell::new(NotificationQueue::default()),
        senders: RefCell::new(NotificationOwnership::default()),
        host: RefCell::new(None),
        popup,
        timer_revision: Cell::new(0),
    });
    let on_intent = Rc::downgrade(&service);
    let on_painted = Rc::downgrade(&service);
    // Failing to construct a native input-shaped surface MUST leave the
    // Freedesktop name unowned, rather than swallowing user notifications.
    let on_ready = Rc::downgrade(&service);
    let Some(host) = NotificationHostSet::new(
        app,
        &monitor,
        layout,
        move |intent| {
            if let Some(service) = on_intent.upgrade() {
                service.on_intent(intent);
            }
        },
        move |ids| {
            if let Some(service) = on_painted.upgrade() {
                service.on_painted(&ids);
            }
        },
        move || {
            if let Some(service) = on_ready.upgrade() {
                service.sync();
            }
        },
    ) else {
        eprintln!("kira-shell: cannot create a shaped GTK notification surface");
        return;
    };
    service.host.replace(Some(host));
    SERVICE.with(|active| *active.borrow_mut() = Some(service.clone()));
    observe_outputs(&service);

    let bus_ready = Rc::downgrade(&service);
    let acquired = Rc::downgrade(&service);
    let lost = Rc::downgrade(&service);
    let id = gio::bus_own_name(
        gio::BusType::Session,
        NAME,
        gio::BusNameOwnerFlags::DO_NOT_QUEUE,
        move |connection, _| {
            if let Some(service) = bus_ready.upgrade() {
                service.register_dbus(&connection);
            }
        },
        move |connection, _| {
            if let Some(service) = acquired.upgrade() {
                service.owned.set(true);
                service.bus.replace(Some(connection));
                service.sync();
                eprintln!("kira-shell: acquired freedesktop Notifications D-Bus name");
            }
        },
        move |_, _| {
            if let Some(service) = lost.upgrade() {
                service.owned.set(false);
                service.bus.borrow_mut().take();
                service
                    .timer_revision
                    .set(service.timer_revision.get().wrapping_add(1));
                revoke_notification_session(
                    &mut service.queue.borrow_mut(),
                    &mut service.senders.borrow_mut(),
                );
                if let Some(host) = service.host.borrow().as_ref() {
                    host.hide_all();
                }
                eprintln!("kira-shell: another notification daemon owns the D-Bus name");
            }
        },
    );
    service.owner_id.replace(Some(id));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn losing_dbus_name_revokes_stale_notifications_and_owner_leases() {
        let mut queue = NotificationQueue::default();
        let mut senders = NotificationOwnership::default();
        let id = senders.assign_id(&mut queue, ":1.32", 0);
        let pending = Notice {
            id,
            app: "App".into(),
            summary: "Waiting".into(),
            body: String::new(),
            actions: vec![("default".into(), "Open".into())],
            critical: false,
            persistent: false,
            dwell: DEFAULT_DWELL,
        };
        assert!(queue.push(pending, Instant::now()));
        senders.record(id, ":1.32");
        assert_eq!(queue.active_count(), 1);
        assert!(senders.is_owner(id, ":1.32"));

        revoke_notification_session(&mut queue, &mut senders);
        assert_eq!(queue.active_count(), 0);
        assert_eq!(senders.active_leases(), 0);
        assert!(!senders.is_owner(id, ":1.32"));
        assert!(!queue.contains_active(id));
        assert_eq!(queue.visible_count(), 0);
        assert!(!queue.has_pending());
        assert_eq!(queue.next_deadline(Instant::now()), None);
    }

    #[test]
    fn notification_preview_never_owns_an_unattested_bus() {
        assert!(preview_notifications_allowed(false, None, None));
        assert!(!preview_notifications_allowed(true, None, None));
        assert!(!preview_notifications_allowed(true, Some("1"), None));
        assert!(!preview_notifications_allowed(true, None, Some("1")));
        assert!(!preview_notifications_allowed(
            true,
            Some("true"),
            Some("1")
        ));
        assert!(preview_notifications_allowed(true, Some("1"), Some("1")));
    }

    #[test]
    fn only_same_output_transient_occludes_notifications() {
        // The current mapped bar and toast are on one output.
        assert!(!toast_target_unobstructed(true, Some(true)));
        // Unsafe/unknown monitor routing remains fail-closed.
        assert!(!toast_target_unobstructed(true, None));
        // A narrow bar output may force a genuinely different safe target.
        assert!(toast_target_unobstructed(true, Some(false)));
        // No transient means detached Niri-managed windows must not starve
        // the queue, even before the bar has mapped.
        assert!(toast_target_unobstructed(false, None));
        assert!(toast_target_unobstructed(false, Some(true)));
    }

    #[test]
    fn notification_intents_require_painted_current_output_and_bus_ownership() {
        assert!(toast_intent_authorized(true, true, true));
        assert!(!toast_intent_authorized(true, false, true));
        assert!(!toast_intent_authorized(true, true, false));
        assert!(!toast_intent_authorized(false, true, true));
        assert!(!toast_intent_authorized(false, false, false));
    }

    #[test]
    fn rerouting_resets_painted_dwell_before_new_destination_ack() {
        let now = Instant::now();
        let mut queue = NotificationQueue::default();
        queue.push(
            Notice {
                id: 42,
                app: "real".into(),
                summary: "move".into(),
                body: String::new(),
                actions: Vec::new(),
                critical: false,
                persistent: false,
                dwell: DEFAULT_DWELL,
            },
            now,
        );
        queue.step(now, true);
        assert!(queue.mark_painted(now, &[42]));
        assert_eq!(queue.painted_count(), 1);
        queue.step(now + Duration::from_secs(1), false);
        assert_eq!(queue.painted_count(), 0);
        queue.step(now + Duration::from_secs(1), true);
        assert_eq!(
            queue.visible().map(|notice| notice.id).collect::<Vec<_>>(),
            vec![42]
        );
        assert_eq!(queue.painted_count(), 0);
        assert!(queue.mark_painted(now + Duration::from_secs(2), &[42]));
        assert_eq!(queue.painted_count(), 1);
    }

    #[test]
    fn hidden_fifo_never_schedules_a_busy_loop() {
        let t = Instant::now();
        let mut queue = NotificationQueue::default();
        queue.push(
            Notice {
                id: 11,
                app: "example".into(),
                summary: "pending".into(),
                body: String::new(),
                actions: vec![],
                critical: false,
                persistent: false,
                dwell: DEFAULT_DWELL,
            },
            t,
        );
        assert_eq!(next_visible_deadline(&queue, t, false), None);
        assert_eq!(next_visible_deadline(&queue, t, true), Some(t));
    }

    #[test]
    fn source_notification_timeout_and_replace_rules_are_kept() {
        let mut queue = NotificationQueue::default();
        let first = queue.allocate_id();
        assert_ne!(first, 0);
        assert_eq!(bounded("абвгд".into(), 3), "абв");
        let notice = Notice {
            id: first,
            app: "app".into(),
            summary: "hi".into(),
            body: "".into(),
            actions: vec![("default".into(), "Open".into())],
            critical: false,
            persistent: false,
            dwell: DEFAULT_DWELL,
        };
        assert!(queue.push(notice.clone(), Instant::now()));
        assert_eq!(view(&notice).id, first);
        assert_eq!(view(&notice).actions[0].0, "default");
    }
}
