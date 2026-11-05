//! A narrowly scoped StatusNotifierItem watcher and host for Kira's 42px bar.
//! All D-Bus calls are async; GTK/WebKit stay on the main context. No
//! executable paths, generic D-Bus methods, or user-controlled JS commands.

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;

use gtk::gio;
use gtk::glib::{self, variant::ToVariant};
use gtk::prelude::*;
use webkit6::prelude::*;

const WATCHER: &str = "org.kde.StatusNotifierWatcher";
const WATCHER_PATH: &str = "/StatusNotifierWatcher";
const WATCHER_IFACE: &str = "org.kde.StatusNotifierWatcher";
const ITEM_IFACE: &str = "org.kde.StatusNotifierItem";
const PROPERTIES_IFACE: &str = "org.freedesktop.DBus.Properties";
const MAX_ITEMS: usize = 64;
const MAX_SAVED_PINS: usize = 128;
const MAX_PINS_BYTES: usize = 64 * 1024;
const WATCHER_XML: &str = r#"<node>
 <interface name="org.kde.StatusNotifierWatcher">
  <method name="RegisterStatusNotifierItem"><arg type="s" direction="in"/></method>
  <method name="RegisterStatusNotifierHost"><arg type="s" direction="in"/></method>
  <property name="RegisteredStatusNotifierItems" type="as" access="read"/>
  <property name="IsStatusNotifierHostRegistered" type="b" access="read"/>
  <property name="ProtocolVersion" type="i" access="read"/>
  <signal name="StatusNotifierItemRegistered"><arg type="s"/></signal>
  <signal name="StatusNotifierItemUnregistered"><arg type="s"/></signal>
  <signal name="StatusNotifierHostRegistered"/>
 </interface>
</node>"#;

thread_local! {
    static SERVICE: RefCell<Option<Rc<Service>>> = const { RefCell::new(None) };
}

#[derive(Clone)]
struct Item {
    handle: u32,
    registration: String,
    bus: String,
    path: String,
    key: String,
    title: String,
    icon: Option<String>,
    status: String,
    // Do not expose a synthetic placeholder before reading real SNI data.
    ready: bool,
    item_is_menu: bool,
    refresh_version: u64,
}

struct Service {
    bar: webkit6::WebView,
    connection: RefCell<Option<gio::DBusConnection>>,
    owner: RefCell<Option<gio::OwnerId>>,
    available: Cell<bool>,
    owns_watcher: Cell<bool>,
    watcher_exported: Cell<bool>,
    host_registered: Cell<bool>,
    next_handle: Cell<u32>,
    external_revision: Cell<u64>,
    items: RefCell<Vec<Item>>,
    pins: RefCell<HashSet<String>>,
}

pub fn start(bar: &webkit6::WebView) {
    let service = Rc::new(Service {
        bar: bar.clone(),
        connection: RefCell::new(None),
        owner: RefCell::new(None),
        available: Cell::new(false),
        owns_watcher: Cell::new(false),
        watcher_exported: Cell::new(false),
        host_registered: Cell::new(true), // This process is always a SNI host.
        next_handle: Cell::new(1),
        external_revision: Cell::new(0),
        items: RefCell::new(Vec::new()),
        pins: RefCell::new(load_pins()),
    });
    SERVICE.with(|active| *active.borrow_mut() = Some(service.clone()));

    // A nested preview shares the user's session D-Bus. Never take its
    // watcher name away; it may attach as a host to an existing watcher.
    if std::env::var_os("KIRA_PREVIEW").is_some() {
        gio::bus_get(gio::BusType::Session, None::<&gio::Cancellable>, {
            let weak = Rc::downgrade(&service);
            move |result| {
                if let (Some(service), Ok(connection)) = (weak.upgrade(), result) {
                    service.install_signal_observers(&connection);
                    service.connection.replace(Some(connection));
                    service.adopt_external();
                }
            }
        });
        return;
    }

    let acquired = Rc::downgrade(&service);
    let owned = Rc::downgrade(&service);
    let lost = Rc::downgrade(&service);
    let id = gio::bus_own_name(
        gio::BusType::Session,
        WATCHER,
        gio::BusNameOwnerFlags::DO_NOT_QUEUE,
        move |connection, _| {
            if let Some(service) = acquired.upgrade() {
                service.install_signal_observers(&connection);
                service.connection.replace(Some(connection.clone()));
                service.register_watcher(&connection);
            }
        },
        move |_, _| {
            if let Some(service) = owned.upgrade() {
                if !service.watcher_exported.get() {
                    eprintln!("kira-shell: SNI registration failed; releasing D-Bus watcher name");
                    if let Some(owner) = service.owner.borrow_mut().take() {
                        gio::bus_unown_name(owner);
                    }
                    return;
                }
                service.owns_watcher.set(true);
                service.available.set(true);
                if let Some(connection) = service.connection.borrow().as_ref() {
                    let _ = connection.emit_signal(
                        None,
                        WATCHER_PATH,
                        WATCHER_IFACE,
                        "StatusNotifierHostRegistered",
                        None,
                    );
                }
                service.publish();
                eprintln!("kira-shell: status notifier watcher and host available");
            }
        },
        move |_, _| {
            if let Some(service) = lost.upgrade() {
                service.owns_watcher.set(false);
                service
                    .external_revision
                    .set(service.external_revision.get().wrapping_add(1));
                service.available.set(false);
                service.items.borrow_mut().clear();
                service.publish();
                service.adopt_external();
            }
        },
    );
    service.owner.replace(Some(id));
}

impl Service {
    fn register_watcher(self: &Rc<Self>, connection: &gio::DBusConnection) {
        let Ok(xml) = gio::DBusNodeInfo::for_xml(WATCHER_XML) else {
            eprintln!("kira-shell: cannot parse SNI watcher introspection");
            return;
        };
        let Some(info) = xml.lookup_interface(WATCHER_IFACE) else {
            return;
        };
        let weak_method = Rc::downgrade(self);
        let weak_property = Rc::downgrade(self);
        if let Err(error) = connection
            .register_object(WATCHER_PATH, &info)
            .method_call(
                move |connection, sender, _, _, method, params, invocation| {
                    let Some(service) = weak_method.upgrade() else {
                        invocation.return_dbus_error(
                            "org.freedesktop.DBus.Error.Failed",
                            "SNI watcher unavailable",
                        );
                        return;
                    };
                    if !service.owns_watcher.get() {
                        invocation.return_dbus_error(
                            "org.freedesktop.DBus.Error.Failed",
                            "SNI watcher not owned",
                        );
                        return;
                    }
                    match method {
                        "RegisterStatusNotifierItem" => {
                            let registration =
                                params.child_value(0).get::<String>().unwrap_or_default();
                            let Some(sender) = sender else {
                                invocation.return_dbus_error(
                                    "org.freedesktop.DBus.Error.InvalidArgs",
                                    "Missing sender",
                                );
                                return;
                            };
                            service.register_item_request(
                                &connection,
                                registration,
                                sender.to_string(),
                                invocation,
                            );
                        }
                        "RegisterStatusNotifierHost" => {
                            let host = params.child_value(0).get::<String>().unwrap_or_default();
                            if host.is_empty() || host.len() > 240 {
                                invocation.return_dbus_error(
                                    "org.freedesktop.DBus.Error.InvalidArgs",
                                    "Invalid host",
                                );
                            } else {
                                service.host_registered.set(true);
                                invocation.return_value(None);
                                let _ = connection.emit_signal(
                                    None,
                                    WATCHER_PATH,
                                    WATCHER_IFACE,
                                    "StatusNotifierHostRegistered",
                                    None,
                                );
                            }
                        }
                        _ => invocation.return_dbus_error(
                            "org.freedesktop.DBus.Error.UnknownMethod",
                            "Unknown SNI watcher method",
                        ),
                    }
                },
            )
            .property(move |_, _, _, _, property| {
                let Some(service) = weak_property.upgrade() else {
                    return false.to_variant();
                };
                match property {
                    "RegisteredStatusNotifierItems" => service
                        .items
                        .borrow()
                        .iter()
                        .map(|i| i.registration.clone())
                        .collect::<Vec<_>>()
                        .to_variant(),
                    "IsStatusNotifierHostRegistered" => service.host_registered.get().to_variant(),
                    "ProtocolVersion" => 0i32.to_variant(),
                    _ => false.to_variant(),
                }
            })
            .build()
        {
            eprintln!("kira-shell: cannot export SNI watcher: {error}");
        } else {
            self.watcher_exported.set(true);
        }
    }

    fn install_signal_observers(self: &Rc<Self>, connection: &gio::DBusConnection) {
        // These subscriptions live only for this Shell's session connection.
        // Weak references keep callbacks from owning the service forever.
        let weak = Rc::downgrade(self);
        connection.signal_subscribe(
            Some(WATCHER),
            Some(WATCHER_IFACE),
            None,
            Some(WATCHER_PATH),
            None,
            gio::DBusSignalFlags::NONE,
            move |_, _, _, _, member, params| {
                let Some(service) = weak.upgrade() else {
                    return;
                };
                if service.owns_watcher.get() {
                    return;
                }
                match member {
                    "StatusNotifierItemRegistered" => {
                        if let Some(raw) = params.child_value(0).get::<String>() {
                            service.register_external(&raw);
                        }
                    }
                    "StatusNotifierItemUnregistered" => {
                        if let Some(raw) = params.child_value(0).get::<String>() {
                            service.unregister(&raw);
                        }
                    }
                    _ => {}
                }
            },
        );

        let weak = Rc::downgrade(self);
        connection.signal_subscribe(
            Some("org.freedesktop.DBus"),
            Some("org.freedesktop.DBus"),
            Some("NameOwnerChanged"),
            Some("/org/freedesktop/DBus"),
            None,
            gio::DBusSignalFlags::NONE,
            move |_, _, _, _, _, params| {
                let Some(service) = weak.upgrade() else {
                    return;
                };
                let name = params.child_value(0).get::<String>().unwrap_or_default();
                let old_owner = params.child_value(1).get::<String>().unwrap_or_default();
                let new_owner = params.child_value(2).get::<String>().unwrap_or_default();
                if name == WATCHER && !service.owns_watcher.get() {
                    service
                        .external_revision
                        .set(service.external_revision.get().wrapping_add(1));
                    service.available.set(false);
                    service.items.borrow_mut().clear();
                    service.publish();
                    if !new_owner.is_empty() {
                        service.adopt_external();
                    }
                } else if !old_owner.is_empty() && old_owner != new_owner {
                    // Well-known names can switch owners without ever becoming
                    // empty. Their former SNI object must not target a new app.
                    service.unregister_bus(&name);
                }
            },
        );

        let weak = Rc::downgrade(self);
        connection.signal_subscribe(
            None,
            Some(ITEM_IFACE),
            None,
            None,
            None,
            gio::DBusSignalFlags::NONE,
            move |_, sender, path, _, member, _| {
                if !matches!(
                    member,
                    "NewIcon" | "NewAttentionIcon" | "NewTitle" | "NewStatus" | "NewIconThemePath"
                ) {
                    return;
                }
                let Some(service) = weak.upgrade() else {
                    return;
                };
                service.refresh_matching(sender, path);
            },
        );

        // A subset of clients emit only standard D-Bus PropertiesChanged,
        // not the legacy NewIcon/NewStatus signals. Both paths are bounded by
        // registered object identity and use the same read-only refresh.
        let weak = Rc::downgrade(self);
        connection.signal_subscribe(
            None,
            Some(PROPERTIES_IFACE),
            Some("PropertiesChanged"),
            None,
            None,
            gio::DBusSignalFlags::NONE,
            move |_, sender, path, _, _, params| {
                let Some(service) = weak.upgrade() else {
                    return;
                };
                if params.child_value(0).get::<String>().as_deref() == Some(ITEM_IFACE) {
                    service.refresh_matching(sender, path);
                }
            },
        );
    }

    fn refresh_matching(self: &Rc<Self>, sender: &str, path: &str) {
        let items: Vec<(u32, String, String)> = self
            .items
            .borrow()
            .iter()
            .filter(|item| item.path == path && (item.bus == sender || !item.bus.starts_with(':')))
            .map(|item| (item.handle, item.bus.clone(), item.path.clone()))
            .collect();
        for (id, bus, object_path) in items {
            self.refresh_item(id, &bus, &object_path);
        }
    }

    fn adopt_external(self: &Rc<Self>) {
        if self.owns_watcher.get() {
            return;
        }
        let Some(connection) = self.connection.borrow().clone() else {
            return;
        };
        let Some(unique) = connection.unique_name() else {
            return;
        };
        // Existing watchers and SNI hosts coexist; never seize a name.
        connection.call(
            Some(WATCHER),
            WATCHER_PATH,
            WATCHER_IFACE,
            "RegisterStatusNotifierHost",
            Some(&(unique.to_string(),).to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            2500,
            None::<&gio::Cancellable>,
            |_| {},
        );
        let revision = self.external_revision.get().wrapping_add(1);
        self.external_revision.set(revision);
        let weak = Rc::downgrade(self);
        connection.call(
            Some(WATCHER),
            WATCHER_PATH,
            PROPERTIES_IFACE,
            "GetAll",
            Some(&(WATCHER_IFACE,).to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            2500,
            None::<&gio::Cancellable>,
            move |reply| {
                let Some(service) = weak.upgrade() else {
                    return;
                };
                if service.owns_watcher.get() || service.external_revision.get() != revision {
                    return;
                }
                let Ok(reply) = reply else {
                    service.available.set(false);
                    service.publish();
                    return;
                };
                if reply.size() > 64 * 1024 {
                    return;
                }
                let values = glib::VariantDict::new(Some(&reply.child_value(0)));
                let registrations = values
                    .lookup_value("RegisteredStatusNotifierItems", None)
                    .and_then(|v| v.get::<Vec<String>>())
                    .unwrap_or_default();
                service.available.set(true);
                for registration in registrations.into_iter().take(MAX_ITEMS) {
                    service.register_external(&registration);
                }
                service.publish();
            },
        );
    }

    fn register_external(self: &Rc<Self>, registration: &str) {
        // A watcher returns a registration (well-known service or unique
        // bus name followed by an object path). No sender is available here.
        if let Some((bus, path)) = parse_external(registration) {
            self.insert_item(registration, &bus, &path);
        }
    }

    fn register_item_request(
        self: &Rc<Self>,
        connection: &gio::DBusConnection,
        registration: String,
        sender: String,
        invocation: gio::DBusMethodInvocation,
    ) {
        if registration.is_empty() || registration.len() > 240 || !valid_bus_name(&sender) {
            invocation.return_dbus_error(
                "org.freedesktop.DBus.Error.InvalidArgs",
                "Invalid SNI registration",
            );
            return;
        }
        if registration.starts_with('/') {
            if valid_object_path(&registration) {
                self.finish_registration(
                    connection,
                    &registration,
                    &sender,
                    &registration,
                    invocation,
                );
            } else {
                invocation.return_dbus_error(
                    "org.freedesktop.DBus.Error.InvalidArgs",
                    "Invalid object path",
                );
            }
            return;
        }
        if !valid_bus_name(&registration) {
            invocation.return_dbus_error(
                "org.freedesktop.DBus.Error.InvalidArgs",
                "Invalid service name",
            );
            return;
        }
        if registration == sender {
            self.finish_registration(
                connection,
                &registration,
                &sender,
                "/StatusNotifierItem",
                invocation,
            );
            return;
        }
        // Real SNI apps generally register a well-known org.kde.* bus name.
        // Accept it ONLY after D-Bus confirms that this exact caller owns it.
        let weak = Rc::downgrade(self);
        let reply_connection = connection.clone();
        let owner_name = registration.clone();
        connection.call(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "GetNameOwner",
            Some(&(registration,).to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            2500,
            None::<&gio::Cancellable>,
            move |reply| {
                let owner = reply
                    .ok()
                    .and_then(|value| value.child_value(0).get::<String>());
                if owner.as_deref() != Some(sender.as_str()) {
                    invocation.return_dbus_error(
                        "org.freedesktop.DBus.Error.AccessDenied",
                        "Caller does not own the StatusNotifierItem service",
                    );
                    return;
                }
                if let Some(service) = weak.upgrade() {
                    service.finish_registration(
                        &reply_connection,
                        &owner_name,
                        &owner_name,
                        "/StatusNotifierItem",
                        invocation,
                    );
                } else {
                    invocation.return_dbus_error(
                        "org.freedesktop.DBus.Error.Failed",
                        "SNI watcher unavailable",
                    );
                }
            },
        );
    }

    fn finish_registration(
        self: &Rc<Self>,
        connection: &gio::DBusConnection,
        registration: &str,
        bus: &str,
        path: &str,
        invocation: gio::DBusMethodInvocation,
    ) {
        let advertised = if registration.starts_with('/') {
            format!("{bus}{registration}")
        } else {
            registration.to_string()
        };
        if self.owns_watcher.get() && self.insert_item(&advertised, bus, path) {
            invocation.return_value(None);
            let _ = connection.emit_signal(
                None,
                WATCHER_PATH,
                WATCHER_IFACE,
                "StatusNotifierItemRegistered",
                Some(&(advertised,).to_variant()),
            );
        } else {
            invocation.return_dbus_error(
                "org.freedesktop.DBus.Error.InvalidArgs",
                "Duplicate or full SNI watcher",
            );
        }
    }

    fn insert_item(self: &Rc<Self>, registration: &str, bus: &str, path: &str) -> bool {
        if !valid_bus_name(bus) || !valid_object_path(path) || registration.len() > 240 {
            return false;
        }
        let mut items = self.items.borrow_mut();
        if items.iter().any(|item| item.registration == registration) || items.len() >= MAX_ITEMS {
            return false;
        }
        let id = self.next_handle.get();
        self.next_handle.set(id.wrapping_add(1).max(1));
        let title = bus.trim_start_matches(':').chars().take(120).collect();
        items.push(Item {
            handle: id,
            registration: registration.into(),
            bus: bus.into(),
            path: path.into(),
            key: registration.into(),
            title,
            icon: None,
            status: "Active".into(),
            ready: false,
            item_is_menu: false,
            refresh_version: 0,
        });
        drop(items);
        self.refresh_item(id, bus, path);
        true
    }

    fn refresh_item(self: &Rc<Self>, id: u32, bus: &str, object_path: &str) {
        let Some(connection) = self.connection.borrow().clone() else {
            return;
        };
        // Out-of-order async GetAll responses must not roll back newer
        // titles, attention states or icons.
        let version = {
            let mut items = self.items.borrow_mut();
            let Some(item) = items.iter_mut().find(|item| item.handle == id) else {
                return;
            };
            item.refresh_version = item.refresh_version.wrapping_add(1);
            item.refresh_version
        };
        let weak = Rc::downgrade(self);
        connection.call(
            Some(bus),
            object_path,
            PROPERTIES_IFACE,
            "GetAll",
            Some(&(ITEM_IFACE,).to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            2500,
            None::<&gio::Cancellable>,
            move |reply| {
                let Some(service) = weak.upgrade() else {
                    return;
                };
                let Ok(reply) = reply else {
                    return;
                };
                if !service
                    .items
                    .borrow()
                    .iter()
                    .any(|item| item.handle == id && item.refresh_version == version)
                {
                    return;
                }
                if reply.size() > 512 * 1024 {
                    return;
                }
                let props = glib::VariantDict::new(Some(&reply.child_value(0)));
                let id_property = props
                    .lookup_value("Id", None)
                    .and_then(|v| v.get::<String>());
                let title = props
                    .lookup_value("Title", None)
                    .and_then(|v| v.get::<String>());
                let icon_name = props
                    .lookup_value("IconName", None)
                    .and_then(|v| v.get::<String>());
                let attention_icon = props
                    .lookup_value("AttentionIconName", None)
                    .and_then(|v| v.get::<String>());
                let status = props
                    .lookup_value("Status", None)
                    .and_then(|v| v.get::<String>());
                let item_is_menu = props
                    .lookup_value("ItemIsMenu", None)
                    .and_then(|v| v.get::<bool>())
                    .unwrap_or(false);
                let pixmap = if status.as_deref() == Some("NeedsAttention") {
                    props
                        .lookup_value("AttentionIconPixmap", None)
                        .and_then(|value| pixmap_icon(&value))
                        .or_else(|| {
                            props
                                .lookup_value("IconPixmap", None)
                                .and_then(|value| pixmap_icon(&value))
                        })
                } else {
                    props
                        .lookup_value("IconPixmap", None)
                        .and_then(|value| pixmap_icon(&value))
                };
                // GIcon theme names are mapped by GTK, never by arbitrary
                // filesystem paths or URLs supplied to the browser.
                let preferred_icon = if status.as_deref() == Some("NeedsAttention") {
                    attention_icon.filter(|v| !v.is_empty()).or(icon_name)
                } else {
                    icon_name
                };
                let icon = preferred_icon
                    .as_deref()
                    .and_then(resolve_theme_icon)
                    .or(pixmap);
                let mut items = service.items.borrow_mut();
                let Some(item) = items
                    .iter_mut()
                    .find(|item| item.handle == id && item.refresh_version == version)
                else {
                    return;
                };
                if let Some(value) = id_property.as_deref().filter(|v| !v.is_empty()) {
                    item.key = pin_identity(&item.registration, value);
                }
                if let Some(value) = title
                    .as_deref()
                    .filter(|v| !v.is_empty())
                    .or(id_property.as_deref().filter(|v| !v.is_empty()))
                {
                    item.title = clean(value, 120);
                }
                item.status = match status.as_deref() {
                    Some("Passive") => "Passive",
                    Some("NeedsAttention") => "NeedsAttention",
                    _ => "Active",
                }
                .into();
                item.icon = icon;
                item.item_is_menu = item_is_menu;
                item.ready = true;
                drop(items);
                service.publish();
            },
        );
    }

    fn unregister(&self, registration: &str) {
        let removed = {
            let mut items = self.items.borrow_mut();
            let original = items.len();
            items.retain(|item| item.registration != registration);
            items.len() != original
        };
        if removed {
            self.publish();
        }
    }

    fn unregister_bus(&self, bus: &str) {
        let removed: Vec<String> = self
            .items
            .borrow()
            .iter()
            .filter(|item| item.bus == bus)
            .map(|item| item.registration.clone())
            .collect();
        for name in removed {
            self.unregister(&name);
            if self.owns_watcher.get() {
                if let Some(connection) = self.connection.borrow().as_ref() {
                    let _ = connection.emit_signal(
                        None,
                        WATCHER_PATH,
                        WATCHER_IFACE,
                        "StatusNotifierItemUnregistered",
                        Some(&(name,).to_variant()),
                    );
                }
            }
        }
    }

    fn snapshot(&self) -> serde_json::Value {
        let pins = self.pins.borrow();
        let items = self
            .items
            .borrow()
            .iter()
            .filter(|item| item.ready && item.status != "Passive")
            .map(|item| {
                serde_json::json!({
                    "id": item.handle,
                    "key": item.key,
                    "title": item.title,
                    "icon": item.icon,
                    "status": item.status,
                    "pinned": pins.contains(&item.key),
                })
            })
            .collect::<Vec<_>>();
        serde_json::json!({ "available": self.available.get(), "items": items })
    }

    fn dispatch(&self, view: &webkit6::WebView) {
        let payload = self.snapshot();
        // JSON is serialized as data, then JSON-string encoded a second
        // time: external SNI metadata cannot become executable JavaScript.
        let Ok(encoded) = serde_json::to_string(&payload.to_string()) else {
            return;
        };
        let script = format!(
            "window.dispatchEvent(new CustomEvent('kira:tray', {{ detail: JSON.parse({encoded}) }}));"
        );
        view.evaluate_javascript(&script, None, None, None::<&gio::Cancellable>, |_| {});
    }

    fn publish(&self) {
        self.dispatch(&self.bar);
        if let Some(popup) = crate::calendar::tray_view() {
            self.dispatch(&popup);
        }
    }

    fn action(&self, id: u32, action: &str) {
        let item = self
            .items
            .borrow()
            .iter()
            .find(|item| item.handle == id)
            .cloned();
        let Some(item) = item else {
            return;
        };
        if action == "pin" {
            let mut next = self.pins.borrow().clone();
            if !next.remove(&item.key) {
                next.insert(item.key.clone());
            }
            if let Err(error) = save_pins(&next) {
                // Persistence failure must not show a pin as durable.
                eprintln!("kira-shell: cannot persist SNI pins: {error}");
                return;
            }
            *self.pins.borrow_mut() = next;
            self.publish();
        } else if action == "activate" || action == "secondary" {
            // An ItemIsMenu registration uses its menu for primary clicks.
            // Plain SNI items retain the standard Activate action.
            let method = if action == "activate" && !item.item_is_menu {
                "Activate"
            } else {
                "ContextMenu"
            };
            if let Some(connection) = self.connection.borrow().as_ref() {
                connection.call(
                    Some(&item.bus),
                    &item.path,
                    ITEM_IFACE,
                    method,
                    Some(&(0i32, 0i32).to_variant()),
                    None,
                    gio::DBusCallFlags::NONE,
                    2500,
                    None::<&gio::Cancellable>,
                    |_| {},
                );
            }
        }
    }
}

/// App-provided Id is stable across new unique D-Bus connections. A
/// registration fallback is used only when the app does not expose Id.
fn pin_identity(registration: &str, id: &str) -> String {
    // The local state format bounds keys by UTF-8 bytes. A count of Unicode
    // scalar values alone would silently discard non-ASCII IDs on restart.
    let mut cleaned = String::new();
    for ch in id.chars().filter(|ch| !ch.is_control()).take(177) {
        if cleaned.len() + ch.len_utf8() > 177 {
            break;
        }
        cleaned.push(ch);
    }
    if cleaned.is_empty() {
        registration.to_string()
    } else {
        format!("id:{cleaned}")
    }
}

fn clean(value: &str, limit: usize) -> String {
    value
        .chars()
        .filter(|c| !c.is_control())
        .take(limit)
        .collect()
}

fn valid_bus_name(value: &str) -> bool {
    if value.len() < 3 || value.len() > 240 {
        return false;
    }
    let unique = value.starts_with(':');
    let rest = if unique { &value[1..] } else { value };
    let parts: Vec<_> = rest.split('.').collect();
    if parts.len() < 2 {
        return false;
    }
    parts.iter().all(|part| {
        let Some(first) = part.chars().next() else {
            return false;
        };
        (unique && (first.is_ascii_alphanumeric() || first == '_')
            || !unique && (first.is_ascii_alphabetic() || first == '_'))
            && part
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
    })
}

fn valid_object_path(value: &str) -> bool {
    if value.len() < 2 || value.len() > 240 || !value.starts_with('/') {
        return false;
    }
    // Empty segments and trailing separators are invalid D-Bus paths.
    // Reject them before calling GIO, whose argument preconditions are strict.
    value[1..].split('/').all(|part| {
        !part.is_empty()
            && part
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_')
    })
}

fn parse_external(value: &str) -> Option<(String, String)> {
    let (bus, path) = if let Some(index) = value.find('/') {
        (&value[..index], &value[index..])
    } else {
        (value, "/StatusNotifierItem")
    };
    // Watcher registrations are authoritative; both unique and well-known
    // names are supported. The caller cannot override this via WebKit.
    if valid_bus_name(bus) && valid_object_path(path) {
        Some((bus.to_string(), path.to_string()))
    } else {
        None
    }
}

/// SNI IconPixmap is ARGB32 in network byte order, not host-endian BGRA.
/// Convert to straight-alpha RGBA before asking GDK to serialize a PNG.
fn argb_to_rgba(width: i32, height: i32, pixels: &[u8]) -> Option<Vec<u8>> {
    if !(1..=128).contains(&width) || !(1..=128).contains(&height) {
        return None;
    }
    let expected = usize::try_from(width.checked_mul(height)?.checked_mul(4)?).ok()?;
    if pixels.len() != expected {
        return None;
    }
    let mut rgba = Vec::with_capacity(expected);
    for chunk in pixels.chunks_exact(4) {
        rgba.extend_from_slice(&[chunk[1], chunk[2], chunk[3], chunk[0]]);
    }
    Some(rgba)
}

/// Choose a nearby 24px resolution rather than decoding an unbounded frame.
/// The 512KiB GetAll cap is enforced before inspecting these variants.
fn pixmap_icon(value: &glib::Variant) -> Option<String> {
    if value.type_().as_str() != "a(iiay)" || value.n_children() > 32 {
        return None;
    }
    let mut best: Option<(usize, i32, i32, i32)> = None;
    for index in 0..value.n_children() {
        let entry = value.child_value(index);
        let (Some(width), Some(height)) = (
            entry.child_value(0).get::<i32>(),
            entry.child_value(1).get::<i32>(),
        ) else {
            continue;
        };
        if !(1..=128).contains(&width) || !(1..=128).contains(&height) {
            continue;
        }
        let distance = (width - 24).abs() + (height - 24).abs();
        if best.as_ref().is_none_or(|(_, score, largest, _)| {
            distance < *score || (distance == *score && width > *largest)
        }) {
            best = Some((index, distance, width, height));
        }
    }
    let (index, _, width, height) = best?;
    let pixels = value.child_value(index).child_value(2).get::<Vec<u8>>()?;
    let rgba = argb_to_rgba(width, height, &pixels)?;
    let texture = gtk::gdk::MemoryTexture::new(
        width,
        height,
        gtk::gdk::MemoryFormat::R8g8b8a8,
        &glib::Bytes::from_owned(rgba),
        usize::try_from(width).ok()?.checked_mul(4)?,
    );
    let png = texture.save_to_png_bytes();
    if png.len() > 128 * 1024 {
        return None;
    }
    Some(format!(
        "data:image/png;base64,{}",
        glib::base64_encode(&png)
    ))
}

fn resolve_theme_icon(name: &str) -> Option<String> {
    if name.is_empty()
        || name.len() > 140
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    {
        return None;
    }
    let display = gtk::gdk::Display::default()?;
    let theme = gtk::IconTheme::for_display(&display);
    // GTK can silently resolve a missing name to image-missing; prefer a
    // real app-provided pixmap to that generic placeholder.
    if !theme.has_icon(name) {
        return None;
    }
    let path = theme
        .lookup_icon(
            name,
            &[],
            24,
            1,
            gtk::TextDirection::None,
            gtk::IconLookupFlags::empty(),
        )
        .file()?
        .path()?;
    crate::launcher::encode_icon(Some(&path), None)
}

fn pin_path() -> Option<PathBuf> {
    let root = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state"))
        })?;
    Some(root.join("kira-desktop/tray-pins.json"))
}

fn load_pins() -> HashSet<String> {
    let Some(path) = pin_path() else {
        return HashSet::new();
    };
    let Ok(data) = fs::read_to_string(&path) else {
        return HashSet::new();
    };
    if data.len() > MAX_PINS_BYTES {
        return HashSet::new();
    }
    serde_json::from_str::<Vec<String>>(&data)
        .unwrap_or_default()
        .into_iter()
        .filter(|value| !value.is_empty() && value.len() <= 180)
        .take(MAX_SAVED_PINS)
        .collect()
}

fn save_pins(pins: &HashSet<String>) -> std::io::Result<()> {
    if pins.len() > MAX_SAVED_PINS || pins.iter().any(|pin| pin.is_empty() || pin.len() > 180) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "SNI pin limit exceeded",
        ));
    }
    let Some(path) = pin_path() else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "XDG_STATE_HOME and HOME are unset",
        ));
    };
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    fs::create_dir_all(parent)?;
    let mut sorted: Vec<_> = pins.iter().cloned().collect();
    sorted.sort();
    let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
    let serialized = serde_json::to_vec(&sorted).map_err(std::io::Error::other)?;
    if serialized.len() > MAX_PINS_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "SNI pin file limit exceeded",
        ));
    }
    fs::write(&tmp, serialized)?;
    fs::rename(tmp, path)
}

pub fn snapshot_to(view: &webkit6::WebView) {
    SERVICE.with(|active| {
        if let Some(service) = active.borrow().as_ref() {
            service.dispatch(view);
        }
    });
}

pub fn handle(message: &str) {
    SERVICE.with(|active| {
        let service = active.borrow();
        let Some(service) = service.as_ref() else {
            return;
        };
        if message == "sync" {
            service.publish();
        } else if let Some((action, raw)) = message.split_once(':') {
            if matches!(action, "activate" | "pin" | "secondary") {
                if let Ok(id) = raw.parse::<u32>() {
                    if id != 0 {
                        service.action(id, action);
                    }
                }
            }
        }
    });
}

pub fn action(id: u32, action: &str) {
    if !matches!(action, "pin" | "activate" | "secondary") {
        return;
    }
    SERVICE.with(|active| {
        if let Some(service) = active.borrow().as_ref() {
            service.action(id, action);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_bounded_unique_service_and_path() {
        assert_eq!(
            parse_external(":1.42/StatusNotifierItem"),
            Some((":1.42".into(), "/StatusNotifierItem".into()))
        );
        assert!(parse_external(":1.42").is_some());
        assert!(parse_external("org.kde.StatusNotifierItem-1234-1").is_some());
        for invalid in [
            "",
            "invalid",
            ":1.2/../../etc/passwd",
            ":1.2/Tray;touch",
            ":1.2//icon-name?",
        ] {
            assert!(parse_external(invalid).is_none(), "{invalid}");
        }
    }

    #[test]
    fn icon_pixmap_requires_valid_bounded_argb_network_bytes() {
        assert_eq!(
            argb_to_rgba(1, 2, &[255, 10, 20, 30, 128, 40, 50, 60]),
            Some(vec![10, 20, 30, 255, 40, 50, 60, 128])
        );
        assert!(argb_to_rgba(0, 1, &[]).is_none());
        assert!(argb_to_rgba(129, 1, &[]).is_none());
        assert!(argb_to_rgba(1, 1, &[0, 1, 2]).is_none());
    }

    #[test]
    fn rejects_malformed_dbus_names_before_gio_calls() {
        for invalid in [
            "", ":1", ":::1.2", ":1.:2", ":1..2", "org..app",
            "1.org", "org.2app", "org.-app", "org.app:", "org.app/",
        ] {
            assert!(!valid_bus_name(invalid), "{invalid}");
        }
        for valid in [
            ":1.42", "org.kde.StatusNotifierItem-1234-1",
            "org.example_app.Tray_1",
        ] {
            assert!(valid_bus_name(valid), "{valid}");
        }
        assert!(!valid_object_path(""));
        for invalid in [
            "/", "//StatusNotifierItem", "/Status//Notifier",
            "/StatusNotifierItem/", "/Bad.Path", "/Bad-Path",
        ] {
            assert!(!valid_object_path(invalid), "{invalid}");
            assert!(parse_external(&format!(":1.2{invalid}")).is_none(), "{invalid}");
        }
        assert!(valid_object_path("/StatusNotifierItem"));
        assert!(valid_object_path("/org/example_2/Tray"));
    }

    #[test]
    fn unicode_pin_ids_fit_the_persisted_utf8_key_budget() {
        let id = "😀".repeat(160);
        let key = pin_identity(":1.4/StatusNotifierItem", &id);
        assert!(key.len() <= 180);
        assert_eq!(key, pin_identity(":1.9/StatusNotifierItem", &id));
    }

    #[test]
    fn pin_identity_survives_unique_bus_name_changes() {
        assert_eq!(
            pin_identity(":1.34/StatusNotifierItem", "org.example.Client"),
            pin_identity(":1.89/StatusNotifierItem", "org.example.Client")
        );
        assert_eq!(
            pin_identity(":1.34/StatusNotifierItem", ""),
            ":1.34/StatusNotifierItem"
        );
        assert_eq!(pin_identity("org.example.Client", "app\n"), "id:app");
    }

    #[test]
    fn strips_control_characters_from_app_metadata() {
        assert_eq!(clean("hello\nworld", 10), "helloworld");
        assert_eq!(clean("abcdef", 3), "abc");
    }
}
