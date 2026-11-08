//! Native freedesktop.org notification D-Bus endpoint. The main thread
//! owns a single bounded FIFO and reusable GTK4 host. No WebKit or JS bridge.
//! Never seize the preview user's session bus or preempt an existing daemon.
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk::gio;
use gtk::glib::{self, variant::ToVariant};
use gtk::prelude::*;

use kira_services::notification_queue::{DEFAULT_DWELL, Notice, NotificationQueue};
use kira_ui::{
    NotificationHost, NotificationIntent, NotificationView, PopupHost, PopupKind, ToastLayout,
    toast_layout,
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

fn notice_from_params(queue: &mut NotificationQueue, params: &glib::Variant) -> Notice {
    let app = bounded(
        params.child_value(0).get::<String>().unwrap_or_default(),
        128,
    );
    let replace = params.child_value(1).get::<u32>().unwrap_or(0);
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
    let id = if replace != 0 {
        replace
    } else {
        queue.allocate_id()
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
fn current_layout() -> Option<ToastLayout> {
    let display = gtk::gdk::Display::default()?;
    let first = display.monitors().item(0)?;
    let monitor = first.downcast::<gtk::gdk::Monitor>().ok()?;
    let rect = monitor.geometry();
    toast_layout(rect.width(), rect.height(), rect.height())
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
    host: RefCell<Option<NotificationHost>>,
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
            .method_call(move |_, _, _, _, method, params, invocation| {
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
                        let (id, accepted) = {
                            let mut queue = service.queue.borrow_mut();
                            let notice = notice_from_params(&mut queue, &params);
                            let id = notice.id;
                            (id, queue.push(notice, Instant::now()))
                        };
                        if !accepted {
                            invocation.return_dbus_error(
                                "org.freedesktop.Notifications.Error.QueueFull",
                                "Notification queue is full",
                            );
                            return;
                        }
                        invocation.return_value(Some(&(id,).to_variant()));
                        service.sync();
                    }
                    "CloseNotification" => {
                        let id = params.child_value(0).get::<u32>().unwrap_or(0);
                        if service.queue.borrow_mut().close(id) {
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
        if let Some(host) = self.host.borrow().as_ref() {
            host.hide();
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
        if !self.owned.get() {
            return;
        }
        match intent {
            NotificationIntent::Close(id) => {
                if self.queue.borrow_mut().close(id) {
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
                    self.signal_closed(id, 2);
                }
                self.sync();
            }
        }
    }

    fn on_painted(self: &Rc<Self>, ids: &[u32]) {
        if !self.owned.get() || ids.is_empty() || ids.len() > 3 {
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

    fn can_show(&self) -> bool {
        // The right-side pooled popup has unknown compositor extents during
        // its animation. Queue rather than overlap it with unreadable toasts.
        ![
            PopupKind::Calendar,
            PopupKind::System,
            PopupKind::Settings,
            PopupKind::Tray,
        ]
        .into_iter()
        .any(|kind| self.popup.is_active(kind))
    }

    fn sync(self: &Rc<Self>) {
        let now = Instant::now();
        let layout = current_layout();
        let can_show = self.owned.get()
            && self
                .host
                .borrow()
                .as_ref()
                .is_some_and(NotificationHost::is_renderable)
            && layout.is_some()
            && self.can_show();
        let expired = {
            let mut queue = self.queue.borrow_mut();
            let previous = queue
                .visible()
                .filter(|notice| !notice.persistent)
                .map(|notice| notice.id)
                .collect::<Vec<_>>();
            queue.step(now, can_show);
            previous
                .into_iter()
                .filter(|id| !queue.contains_active(*id))
                .collect::<Vec<_>>()
        };
        for id in expired {
            self.signal_closed(id, 1); // Expired, not manually dismissed.
        }

        let cards = if can_show {
            self.queue.borrow().visible().map(view).collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        if let Some(host) = self.host.borrow().as_ref() {
            if let Some(layout) = layout {
                host.set_cards(layout, &cards);
            } else {
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
    let Some(layout) = current_layout() else {
        eprintln!("kira-shell: no safe monitor geometry for notifications");
        return;
    };

    let service = Rc::new(Service {
        owned: Cell::new(false),
        owner_id: RefCell::new(None),
        bus: RefCell::new(None),
        queue: RefCell::new(NotificationQueue::default()),
        host: RefCell::new(None),
        popup,
        timer_revision: Cell::new(0),
    });
    let on_intent = Rc::downgrade(&service);
    let on_painted = Rc::downgrade(&service);
    // Failing to construct a native input-shaped surface MUST leave the
    // Freedesktop name unowned, rather than swallowing user notifications.
    let Some(host) = NotificationHost::new(
        app,
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
    ) else {
        eprintln!("kira-shell: cannot create a shaped GTK notification surface");
        return;
    };
    let weak = Rc::downgrade(&service);
    host.window.connect_map(move |_| {
        if let Some(service) = weak.upgrade() {
            service.sync();
        }
    });
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
                service.queue.borrow_mut().step(Instant::now(), false);
                if let Some(host) = service.host.borrow().as_ref() {
                    host.hide();
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
