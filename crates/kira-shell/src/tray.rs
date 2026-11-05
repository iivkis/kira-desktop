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
}

struct Service {
    bar: webkit6::WebView,
    connection: RefCell<Option<gio::DBusConnection>>,
    owner: RefCell<Option<gio::OwnerId>>,
    available: Cell<bool>,
    owns_watcher: Cell<bool>,
    host_registered: Cell<bool>,
    next_handle: Cell<u32>,
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
        host_registered: Cell::new(true), // This process is always a SNI host.
        next_handle: Cell::new(1),
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
                service.install_signal_observers(connection);
                service.connection.replace(Some(connection.clone()));
                service.register_watcher(connection);
            }
        },
        move |_, _| {
            if let Some(service) = owned.upgrade() {
                service.owns_watcher.set(true);
                service.available.set(true);
                service.publish();
                eprintln!("kira-shell: status notifier watcher and host available");
            }
        },
        move |_, _| {
            if let Some(service) = lost.upgrade() {
                service.owns_watcher.set(false);
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
        let Some(info) = xml.lookup_interface(WATCHER_IFACE) else { return; };
        let weak_method = Rc::downgrade(self);
        let weak_property = Rc::downgrade(self);
        if let Err(error) = connection.register_object(WATCHER_PATH, &info)
            .method_call(move |connection, sender, _, _, method, params, invocation| {
                let Some(service) = weak_method.upgrade() else {
                    invocation.return_dbus_error(
                        "org.freedesktop.DBus.Error.Failed", "SNI watcher unavailable");
                    return;
                };
                if !service.owns_watcher.get() {
                    invocation.return_dbus_error(
                        "org.freedesktop.DBus.Error.Failed", "SNI watcher not owned");
                    return;
                }
                match method {
                    "RegisterStatusNotifierItem" => {
                        let registration = params.child_value(0).get::<String>().unwrap_or_default();
                        let Some(sender) = sender else {
                            invocation.return_dbus_error(
                                "org.freedesktop.DBus.Error.InvalidArgs", "Missing sender");
                            return;
                        };
                        if service.register_item(&registration, sender) {
                            invocation.return_value(None);
                            // Emit precisely one registration for each accepted item.
                            let registered = service.items.borrow().last()
                                .map(|item| item.registration.clone());
                            if let Some(registered) = registered {
                                let _ = connection.emit_signal(
                                    None, WATCHER_PATH, WATCHER_IFACE,
                                    "StatusNotifierItemRegistered",
                                    Some(&(registered,).to_variant()));
                            }
                        } else {
                            invocation.return_dbus_error(
                                "org.freedesktop.DBus.Error.InvalidArgs",
                                "Invalid or duplicate status item registration");
                        }
                    }
                    "RegisterStatusNotifierHost" => {
                        let host = params.child_value(0).get::<String>().unwrap_or_default();
                        if host.is_empty() || host.len() > 240 {
                            invocation.return_dbus_error(
                                "org.freedesktop.DBus.Error.InvalidArgs", "Invalid host");
                        } else {
                            service.host_registered.set(true);
                            invocation.return_value(None);
                            let _ = connection.emit_signal(
                                None, WATCHER_PATH, WATCHER_IFACE,
                                "StatusNotifierHostRegistered", None);
                        }
                    }
                    _ => invocation.return_dbus_error(
                        "org.freedesktop.DBus.Error.UnknownMethod",
                        "Unknown SNI watcher method"),
                }
            })
            .property(move |_, _, _, _, property| {
                let Some(service) = weak_property.upgrade() else {
                    return false.to_variant();
                };
                match property {
                    "RegisteredStatusNotifierItems" => service.items.borrow()
                        .iter().map(|i| i.registration.clone())
                        .collect::<Vec<_>>().to_variant(),
                    "IsStatusNotifierHostRegistered" => service.host_registered.get().to_variant(),
                    "ProtocolVersion" => 0i32.to_variant(),
                    _ => false.to_variant(),
                }
            })
            .build() {
            eprintln!("kira-shell: cannot export SNI watcher: {error}");
        }
    }

    fn install_signal_observers(self: &Rc<Self>, connection: &gio::DBusConnection) {
        // These subscriptions live only for this Shell's session connection.
        // Weak references keep callbacks from owning the service forever.
        let weak = Rc::downgrade(self);
        connection.signal_subscribe(
            None, Some(WATCHER_IFACE), None, Some(WATCHER_PATH), None,
            gio::DBusSignalFlags::NONE,
            move |_, _, _, _, member, params| {
                let Some(service) = weak.upgrade() else { return; };
                if service.owns_watcher.get() { return; }
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
            Some("org.freedesktop.DBus"), Some("org.freedesktop.DBus"),
            Some("NameOwnerChanged"), Some("/org/freedesktop/DBus"), None,
            gio::DBusSignalFlags::NONE,
            move |_, _, _, _, _, params| {
                let Some(service) = weak.upgrade() else { return; };
                let name = params.child_value(0).get::<String>().unwrap_or_default();
                let new_owner = params.child_value(2).get::<String>().unwrap_or_default();
                if new_owner.is_empty() {
                    if name == WATCHER && !service.owns_watcher.get() {
                        service.available.set(false);
                        service.items.borrow_mut().clear();
                        service.publish();
                    } else {
                        service.unregister_bus(&name);
                    }
                } else if name == WATCHER && !service.owns_watcher.get() {
                    service.adopt_external();
                }
            },
        );

        let weak = Rc::downgrade(self);
        connection.signal_subscribe(
            None, Some(ITEM_IFACE), None, None, None,
            gio::DBusSignalFlags::NONE,
            move |_, sender, path, _, member, _| {
                if !matches!(member, "NewIcon" | "NewAttentionIcon" |
                    "NewTitle" | "NewStatus" | "NewIconThemePath") { return; }
                let Some(service) = weak.upgrade() else { return; };
                let items: Vec<(u32, String, String)> = service.items.borrow().iter()
                    .filter(|item| item.path == path
                        && (item.bus == sender || !item.bus.starts_with(':')))
                    .map(|item| (item.handle, item.bus.clone(), item.path.clone()))
                    .collect();
                for (id, bus, object_path) in items {
                    service.refresh_item(id, &bus, &object_path);
                }
            },
        );
    }

    fn adopt_external(self: &Rc<Self>) {
        if self.owns_watcher.get() { return; }
        let Some(connection) = self.connection.borrow().clone() else { return; };
        let Some(unique) = connection.unique_name() else { return; };
        // Existing watchers and SNI hosts coexist; never seize a name.
        connection.call(
            Some(WATCHER), WATCHER_PATH, WATCHER_IFACE,
            "RegisterStatusNotifierHost",
            Some(&(unique.to_string(),).to_variant()), None,
            gio::DBusCallFlags::NONE, 2500,
            None::<&gio::Cancellable>, |_| {},
        );
        let weak = Rc::downgrade(self);
        connection.call(
            Some(WATCHER), WATCHER_PATH, PROPERTIES_IFACE,
            "GetAll", Some(&(WATCHER_IFACE,).to_variant()), None,
            gio::DBusCallFlags::NONE, 2500,
            None::<&gio::Cancellable>, move |reply| {
                let Some(service) = weak.upgrade() else { return; };
                if service.owns_watcher.get() { return; }
                let Ok(reply) = reply else {
                    service.available.set(false);
                    service.publish();
                    return;
                };
                if reply.size() > 64 * 1024 { return; }
                let values = glib::VariantDict::new(Some(&reply.child_value(0)));
                let registrations = values.lookup_value("RegisteredStatusNotifierItems", None)
                    .and_then(|v| v.get::<Vec<String>>()).unwrap_or_default();
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

    fn register_item(self: &Rc<Self>, raw: &str, sender: &str) -> bool {
        if raw.is_empty() || raw.len() > 240 || !valid_bus_name(sender) { return false; }
        // Restrict registrations to the caller's own unique bus name or an
        // object path on that connection. We don't trust a forged well-known
        // bus name from an unrelated D-Bus sender.
        let (bus, path) = if raw.starts_with('/') {
            if !valid_object_path(raw) { return false; }
            (sender.to_string(), raw.to_string())
        } else if raw == sender {
            (sender.to_string(), "/StatusNotifierItem".into())
        } else {
            // Resolve legitimate well-known registrations via their caller
            // later; this narrow initial watcher accepts unique names only.
            return false;
        };
        self.insert_item(raw, &bus, &path)
    }

    fn insert_item(self: &Rc<Self>, registration: &str, bus: &str, path: &str) -> bool {
        if !valid_bus_name(bus) || !valid_object_path(path) ||
            registration.len() > 240 { return false; }
        let mut items = self.items.borrow_mut();
        if items.iter().any(|item| item.registration == registration) ||
            items.len() >= MAX_ITEMS { return false; }
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
        });
        drop(items);
        self.publish();
        self.refresh_item(id, bus, path);
        true
    }

    fn refresh_item(self: &Rc<Self>, id: u32, bus: &str, object_path: &str) {
        let Some(connection) = self.connection.borrow().clone() else { return; };
        let weak = Rc::downgrade(self);
        connection.call(
            Some(bus), object_path, PROPERTIES_IFACE, "GetAll",
            Some(&(ITEM_IFACE,).to_variant()), None,
            gio::DBusCallFlags::NONE, 2500,
            None::<&gio::Cancellable>, move |reply| {
                let Some(service) = weak.upgrade() else { return; };
                let Ok(reply) = reply else { return; };
                if reply.size() > 256 * 1024 { return; }
                let props = glib::VariantDict::new(Some(&reply.child_value(0)));
                let id_property = props.lookup_value("Id", None)
                    .and_then(|v| v.get::<String>());
                let title = props.lookup_value("Title", None)
                    .and_then(|v| v.get::<String>());
                let icon_name = props.lookup_value("IconName", None)
                    .and_then(|v| v.get::<String>());
                let attention_icon = props.lookup_value("AttentionIconName", None)
                    .and_then(|v| v.get::<String>());
                let status = props.lookup_value("Status", None)
                    .and_then(|v| v.get::<String>());
                // GIcon theme names are mapped by GTK, never by arbitrary
                // filesystem paths or URLs supplied to the browser.
                let preferred_icon = if status.as_deref() == Some("NeedsAttention") {
                    attention_icon.filter(|v| !v.is_empty()).or(icon_name)
                } else {
                    icon_name
                };
                let icon = preferred_icon.as_deref()
                    .and_then(resolve_theme_icon);
                let mut items = service.items.borrow_mut();
                let Some(item) = items.iter_mut().find(|item| item.handle == id) else { return; };
                if let Some(value) = id_property.filter(|v| !v.is_empty()) {
                    item.key = clean(&value, 180);
                }
                if let Some(value) = title.filter(|v| !v.is_empty()) {
                    item.title = clean(&value, 120);
                }
                item.status = match status.as_deref() {
                    Some("Passive") => "Passive",
                    Some("NeedsAttention") => "NeedsAttention",
                    _ => "Active",
                }.into();
                item.icon = icon;
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
        let removed: Vec<String> = self.items.borrow().iter()
            .filter(|item| item.bus == bus)
            .map(|item| item.registration.clone()).collect();
        for name in removed {
            self.unregister(&name);
            if self.owns_watcher.get() {
                if let Some(connection) = self.connection.borrow().as_ref() {
                    let _ = connection.emit_signal(
                        None, WATCHER_PATH, WATCHER_IFACE,
                        "StatusNotifierItemUnregistered",
                        Some(&(name,).to_variant()));
                }
            }
        }
    }

    fn snapshot(&self) -> serde_json::Value {
        let pins = self.pins.borrow();
        let items = self.items.borrow().iter()
            .filter(|item| item.status != "Passive")
            .map(|item| serde_json::json!({
                "id": item.handle,
                "key": item.key,
                "title": item.title,
                "icon": item.icon,
                "status": item.status,
                "pinned": pins.contains(&item.key),
            })).collect::<Vec<_>>();
        serde_json::json!({ "available": self.available.get(), "items": items })
    }

    fn dispatch(&self, view: &webkit6::WebView) {
        let payload = self.snapshot();
        // JSON is serialized as data, then JSON-string encoded a second
        // time: external SNI metadata cannot become executable JavaScript.
        let Ok(encoded) = serde_json::to_string(&payload.to_string()) else { return; };
        let script = format!(
            "window.dispatchEvent(new CustomEvent('kira:tray', {{ detail: JSON.parse({encoded}) }}));"
        );
        view.evaluate_javascript(
            &script, None, None, None::<&gio::Cancellable>, |_| {},
        );
    }

    fn publish(&self) {
        self.dispatch(&self.bar);
        if let Some(popup) = crate::calendar::tray_view() {
            self.dispatch(&popup);
        }
    }

    fn action(&self, id: u32, action: &str) {
        let item = self.items.borrow().iter().find(|item| item.handle == id).cloned();
        let Some(item) = item else { return; };
        if action == "pin" {
            let mut pins = self.pins.borrow_mut();
            if !pins.remove(&item.key) {
                pins.insert(item.key.clone());
            }
            let next = pins.clone();
            drop(pins);
            if let Err(error) = save_pins(&next) {
                eprintln!("kira-shell: cannot persist SNI pins: {error}");
            }
            self.publish();
        } else if action == "activate" || action == "secondary" {
            let method = if action == "activate" { "Activate" } else { "ContextMenu" };
            if let Some(connection) = self.connection.borrow().as_ref() {
                connection.call(
                    Some(&item.bus), &item.path, ITEM_IFACE, method,
                    Some(&(0i32, 0i32).to_variant()), None,
                    gio::DBusCallFlags::NONE, 2500,
                    None::<&gio::Cancellable>, |_| {},
                );
            }
        }
    }
}

fn clean(value: &str, limit: usize) -> String {
    value.chars().filter(|c| !c.is_control()).take(limit).collect()
}

fn valid_bus_name(value: &str) -> bool {
    value.len() <= 240 && value.len() >= 3 &&
    value.starts_with(':') && value.chars().all(|c|
        c.is_ascii_alphanumeric() || matches!(c, ':' | '.' | '_' | '-'))
}

fn valid_object_path(value: &str) -> bool {
    value.starts_with('/') && value.len() <= 240 &&
    value != "/" && value.chars().all(|c|
        c.is_ascii_alphanumeric() || matches!(c, '/' | '_'))
}

fn parse_external(value: &str) -> Option<(String, String)> {
    let (bus, path) = if let Some(index) = value.find('/') {
        (&value[..index], &value[index..])
    } else {
        (value, "/StatusNotifierItem")
    };
    // For now we accept unique service-name registrations from external
    // watchers. Well-known names can be mapped when their owner is checked.
    valid_bus_name(bus).then(|| (bus.into(), path.into()))
        .filter(|(_, path)| valid_object_path(path))
}

fn resolve_theme_icon(name: &str) -> Option<String> {
    if name.is_empty() || name.len() > 140 ||
        !name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    { return None; }
    let display = gtk::gdk::Display::default()?;
    let theme = gtk::IconTheme::for_display(&display);
    let path = theme.lookup_icon(
        name, &[], 24, 1, gtk::TextDirection::None,
        gtk::IconLookupFlags::empty(),
    ).file()?.path()?;
    crate::launcher::encode_icon(Some(&path), None)
}

fn pin_path() -> Option<PathBuf> {
    let root = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME")
            .map(|home| PathBuf::from(home).join(".local/state")))?;
    Some(root.join("kira-desktop/tray-pins.json"))
}

fn load_pins() -> HashSet<String> {
    let Some(path) = pin_path() else { return HashSet::new(); };
    let Ok(data) = fs::read_to_string(&path) else { return HashSet::new(); };
    if data.len() > 16 * 1024 { return HashSet::new(); }
    serde_json::from_str::<Vec<String>>(&data)
        .unwrap_or_default().into_iter()
        .filter(|value| !value.is_empty() && value.len() <= 180)
        .take(128).collect()
}

fn save_pins(pins: &HashSet<String>) -> std::io::Result<()> {
    let Some(path) = pin_path() else { return Ok(()); };
    let Some(parent) = path.parent() else { return Ok(()); };
    fs::create_dir_all(parent)?;
    let mut sorted: Vec<_> = pins.iter().cloned().collect();
    sorted.sort();
    let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
    fs::write(&tmp, serde_json::to_vec(&sorted)?)?;
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
        let Some(service) = service.as_ref() else { return; };
        if message == "sync" {
            service.publish();
        } else if let Some(raw) = message.strip_prefix("activate:") {
            if let Ok(id) = raw.parse::<u32>() {
                if id != 0 { service.action(id, "activate"); }
            }
        }
    });
}

pub fn action(id: u32, action: &str) {
    if !matches!(action, "pin" | "activate" | "secondary") { return; }
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
        for invalid in ["", "org.fake.App", ":1.2/../../etc/passwd",
            ":1.2/Tray;touch", ":1.2//icon-name?"] {
            assert!(parse_external(invalid).is_none(), "{invalid}");
        }
    }

    #[test]
    fn strips_control_characters_from_app_metadata() {
        assert_eq!(clean("hello\nworld", 10), "helloworld");
        assert_eq!(clean("abcdef", 3), "abc");
    }
}
