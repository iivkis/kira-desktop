//! Native StatusNotifierWatcher + StatusNotifierHost using GIO session D-Bus.
//! UI can only issue typed numeric-handle actions. No arbitrary D-Bus method
//! names, JS bridge, commands or synthetic running-app icons.
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::rc::Rc;

use gtk::gio;
use gtk::glib::{self, variant::ToVariant};

use kira_ui::{TrayIntent, TrayItem, TrayMenu, TrayMenuEntry, TrayPixmap, TrayState, TrayStatus};
use crate::tray_menu;

const WATCHER: &str = "org.kde.StatusNotifierWatcher";
const WATCHER_PATH: &str = "/StatusNotifierWatcher";
const WATCHER_IFACE: &str = "org.kde.StatusNotifierWatcher";
const ITEM_IFACE: &str = "org.kde.StatusNotifierItem";
const PROPERTIES: &str = "org.freedesktop.DBus.Properties";
const MENU_INTERFACE: &str = "com.canonical.dbusmenu";
const DBUS: &str = "org.freedesktop.DBus";
const DBUS_PATH: &str = "/org/freedesktop/DBus";
const MAX_ITEMS: usize = 64;
const MAX_PINS: usize = 128;
const MAX_PIN_BYTES: usize = 64 * 1024;
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

#[derive(Clone, Debug)]
struct Item {
    handle: u32,
    registration: String,
    bus: String,
    path: String,
    owner_unique: Option<String>,
    stable_key: String,
    title: String,
    icon_name: Option<String>,
    pixmap: Option<TrayPixmap>,
    status: TrayStatus,
    is_menu: bool,
    menu_path: Option<String>,
    ready: bool,
    refresh_serial: u64,
}

#[derive(Clone, Debug)]
struct MenuSession {
    handle: u32,
    owner: String,
    path: String,
    title: String,
    entries: Vec<TrayMenuEntry>,
}

impl MenuSession {
    fn same_target(&self, other: &Self) -> bool {
        self.handle == other.handle && self.owner == other.owner && self.path == other.path
    }

    fn snapshot(&self) -> TrayMenu {
        TrayMenu {
            handle: self.handle,
            title: self.title.clone(),
            items: self.entries.clone(),
        }
    }
}

struct Service {
    connection: RefCell<Option<gio::DBusConnection>>,
    watcher_owner: RefCell<Option<gio::OwnerId>>,
    host_owner: RefCell<Option<gio::OwnerId>>,
    host_name: String,
    host_owned: Cell<bool>,
    owns_watcher: Cell<bool>,
    watcher_exported: Cell<bool>,
    available: Cell<bool>,
    external_revision: Cell<u64>,
    next_handle: Cell<u32>,
    items: RefCell<Vec<Item>>,
    pins: RefCell<HashSet<String>>,
    menu_serial: Cell<u64>,
    current_menu: RefCell<Option<MenuSession>>,
    on_menu: Box<dyn Fn(Option<TrayMenu>)>,
    on_update: Box<dyn Fn(TrayState)>,
}

fn valid_bus_name(value: &str) -> bool {
    if value.len() < 3 || value.len() > 240 {
        return false;
    }
    let unique = value.starts_with(':');
    let rest = if unique { &value[1..] } else { value };
    let parts = rest.split('.').collect::<Vec<_>>();
    if parts.len() < 2 {
        return false;
    }
    parts.iter().all(|part| {
        let Some(first) = part.chars().next() else {
            return false;
        };
        (if unique {
            first.is_ascii_alphanumeric() || first == '_'
        } else {
            first.is_ascii_alphabetic() || first == '_'
        }) && part
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-'))
    })
}

fn valid_object_path(value: &str) -> bool {
    value.len() >= 2
        && value.len() <= 240
        && value.starts_with('/')
        && value[1..].split('/').all(|part| {
            !part.is_empty()
                && part
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        })
}

fn parse_registration(value: &str) -> Option<(String, String)> {
    let (bus, path) = if let Some(index) = value.find('/') {
        (&value[..index], &value[index..])
    } else {
        (value, "/StatusNotifierItem")
    };
    (valid_bus_name(bus) && valid_object_path(path)).then(|| (bus.into(), path.into()))
}

fn valid_icon_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 120
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'))
        && !value.starts_with('.')
}

/// A snapshot contains at most 32 candidate real icon frames. Decode only
/// the nearest 24px frame after validating all dimensions and payload bytes.
fn decode_pixmap(value: &glib::Variant) -> Option<TrayPixmap> {
    if value.type_().as_str() != "a(iiay)" || value.n_children() > 32 {
        return None;
    }
    let mut best: Option<(usize, i32, i32)> = None;
    for index in 0..value.n_children() {
        let frame = value.child_value(index);
        let (Some(width), Some(height)) = (
            frame.child_value(0).get::<i32>(),
            frame.child_value(1).get::<i32>(),
        ) else {
            continue;
        };
        if !(1..=128).contains(&width) || !(1..=128).contains(&height) {
            continue;
        }
        let distance = (width - 24).abs() + (height - 24).abs();
        if best
            .as_ref()
            .is_none_or(|(_, previous, selected_width)| {
                distance < *previous || (distance == *previous && width > *selected_width)
            })
        {
            best = Some((index, distance, width));
        }
    }
    let (index, _, _) = best?;
    let frame = value.child_value(index);
    let width = frame.child_value(0).get::<i32>()?;
    let height = frame.child_value(1).get::<i32>()?;
    let bytes = frame.child_value(2).get::<Vec<u8>>()?;
    TrayPixmap::from_argb(width, height, &bytes)
}

fn clean(value: &str, limit: usize) -> String {
    value
        .chars()
        .filter(|ch| !ch.is_control())
        .take(limit)
        .collect()
}

fn pin_identity(registration: &str, id: &str) -> String {
    let mut cleaned = String::new();
    for ch in id.chars().filter(|ch| !ch.is_control()).take(177) {
        if cleaned.len() + ch.len_utf8() > 177 {
            break;
        }
        cleaned.push(ch);
    }
    if cleaned.is_empty() {
        registration.to_owned()
    } else {
        format!("id:{cleaned}")
    }
}

fn pin_path() -> Option<PathBuf> {
    let state = std::env::var_os("XDG_STATE_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local").join("state"))
        })?;
    Some(state.join("kira-desktop").join("tray-pins.json"))
}

fn load_pins() -> HashSet<String> {
    let Some(path) = pin_path() else {
        return HashSet::new();
    };
    let Ok(file) = fs::File::open(path) else {
        return HashSet::new();
    };
    let mut content = Vec::new();
    if file
        .take(MAX_PIN_BYTES as u64 + 1)
        .read_to_end(&mut content)
        .is_err()
        || content.len() > MAX_PIN_BYTES
    {
        return HashSet::new();
    }
    let Ok(values) = serde_json::from_slice::<Vec<String>>(&content) else {
        return HashSet::new();
    };
    values
        .into_iter()
        .take(MAX_PINS)
        .filter(|key| !key.is_empty() && key.len() <= 240 && !key.chars().any(char::is_control))
        .collect()
}

fn save_pins(pins: &HashSet<String>) -> std::io::Result<()> {
    let path = pin_path().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "No XDG state directory")
    })?;
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "No pin directory"))?;
    fs::create_dir_all(parent)?;
    let mut values = pins.iter().cloned().collect::<Vec<_>>();
    values.sort_unstable();
    let encoded = serde_json::to_vec(&values)?;
    if encoded.len() > MAX_PIN_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Pinned app preferences exceed 64 KiB",
        ));
    }
    let staging = parent.join(format!("tray-pins.json.tmp-{}", std::process::id()));
    fs::write(&staging, encoded)?;
    if let Err(error) = fs::rename(&staging, path) {
        let _ = fs::remove_file(&staging);
        return Err(error);
    }
    Ok(())
}

impl Service {
    fn publish(&self) {
        let pins = self.pins.borrow();
        let items = self
            .items
            .borrow()
            .iter()
            .filter(|item| item.ready && item.status != TrayStatus::Passive)
            .map(|item| TrayItem {
                handle: item.handle,
                title: item.title.clone(),
                icon_name: item.icon_name.clone(),
                pixmap: item.pixmap.clone(),
                status: item.status,
                pinned: pins.contains(&item.stable_key),
            })
            .collect();
        (self.on_update)(TrayState {
            available: self.available.get(),
            items,
        });
    }

    fn register_watcher(self: &Rc<Self>, connection: &gio::DBusConnection) {
        let Ok(node) = gio::DBusNodeInfo::for_xml(WATCHER_XML) else {
            return;
        };
        let Some(info) = node.lookup_interface(WATCHER_IFACE) else {
            return;
        };
        let methods = Rc::downgrade(self);
        let properties = Rc::downgrade(self);
        let result = connection
            .register_object(WATCHER_PATH, &info)
            .method_call(
                move |connection, sender, _, _, method, params, invocation| {
                    let Some(service) = methods.upgrade() else {
                        invocation.return_dbus_error(
                            "org.freedesktop.DBus.Error.Failed",
                            "SNI watcher stopped",
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
                                    "Missing D-Bus sender",
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
                            if !valid_bus_name(&host) {
                                invocation.return_dbus_error(
                                    "org.freedesktop.DBus.Error.InvalidArgs",
                                    "Invalid SNI host name",
                                );
                                return;
                            }
                            invocation.return_value(None);
                            let _ = connection.emit_signal(
                                None,
                                WATCHER_PATH,
                                WATCHER_IFACE,
                                "StatusNotifierHostRegistered",
                                None,
                            );
                        }
                        _ => invocation.return_dbus_error(
                            "org.freedesktop.DBus.Error.UnknownMethod",
                            "Unknown watcher method",
                        ),
                    }
                },
            )
            .property(move |_, _, _, _, property| {
                let Some(service) = properties.upgrade() else {
                    return false.to_variant();
                };
                match property {
                    "RegisteredStatusNotifierItems" => service
                        .items
                        .borrow()
                        .iter()
                        .map(|item| item.registration.clone())
                        .collect::<Vec<_>>()
                        .to_variant(),
                    "IsStatusNotifierHostRegistered" => true.to_variant(),
                    "ProtocolVersion" => 0i32.to_variant(),
                    _ => false.to_variant(),
                }
            })
            .build();
        if let Err(error) = result {
            eprintln!("kira-shell: failed to export StatusNotifierWatcher: {error}");
        } else {
            self.watcher_exported.set(true);
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
                "Invalid registration",
            );
            return;
        }
        if registration.starts_with('/') {
            if valid_object_path(&registration) {
                self.finish_registration(
                    connection,
                    &format!("{sender}{registration}"),
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
        // Well-known names need an owner check. Otherwise any application
        // could register someone else's service and steal its UI handle.
        let weak = Rc::downgrade(self);
        let reply_connection = connection.clone();
        let advertised = registration.clone();
        connection.call(
            Some(DBUS),
            DBUS_PATH,
            DBUS,
            "GetNameOwner",
            Some(&(registration,).to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            2500,
            None::<&gio::Cancellable>,
            move |reply| {
                let owner = reply.ok().and_then(|v| v.child_value(0).get::<String>());
                if owner.as_deref() != Some(sender.as_str()) {
                    invocation.return_dbus_error(
                        "org.freedesktop.DBus.Error.AccessDenied",
                        "Caller is not the registered service owner",
                    );
                    return;
                }
                if let Some(service) = weak.upgrade() {
                    service.finish_registration(
                        &reply_connection,
                        &advertised,
                        &advertised,
                        "/StatusNotifierItem",
                        invocation,
                    );
                } else {
                    invocation
                        .return_dbus_error("org.freedesktop.DBus.Error.Failed", "Watcher stopped");
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
        if self.owns_watcher.get() && self.insert_item(registration, bus, path) {
            invocation.return_value(None);
            let _ = connection.emit_signal(
                None,
                WATCHER_PATH,
                WATCHER_IFACE,
                "StatusNotifierItemRegistered",
                Some(&(registration,).to_variant()),
            );
        } else {
            invocation.return_dbus_error(
                "org.freedesktop.DBus.Error.InvalidArgs",
                "Duplicate or full item registration",
            );
        }
    }

    fn insert_item(self: &Rc<Self>, registration: &str, bus: &str, path: &str) -> bool {
        if !valid_bus_name(bus) || !valid_object_path(path) || registration.len() > 240 {
            return false;
        }
        let mut items = self.items.borrow_mut();
        if items.len() >= MAX_ITEMS || items.iter().any(|item| item.registration == registration) {
            return false;
        }
        let handle = self.next_handle.get();
        self.next_handle.set(handle.wrapping_add(1).max(1));
        items.push(Item {
            handle,
            registration: registration.into(),
            bus: bus.into(),
            path: path.into(),
            owner_unique: bus.starts_with(':').then(|| bus.to_string()),
            stable_key: registration.into(),
            title: String::new(),
            icon_name: None,
            pixmap: None,
            status: TrayStatus::Active,
            is_menu: false,
            menu_path: None,
            ready: false,
            refresh_serial: 0,
        });
        drop(items);
        self.resolve_owner(handle, bus);
        true
    }

    fn resolve_owner(self: &Rc<Self>, handle: u32, bus: &str) {
        if bus.starts_with(':') {
            self.refresh_item(handle);
            return;
        }
        let Some(connection) = self.connection.borrow().clone() else {
            return;
        };
        let weak = Rc::downgrade(self);
        let requested = bus.to_owned();
        connection.call(
            Some(DBUS),
            DBUS_PATH,
            DBUS,
            "GetNameOwner",
            Some(&(bus,).to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            2500,
            None::<&gio::Cancellable>,
            move |reply| {
                let Some(service) = weak.upgrade() else {
                    return;
                };
                let owner = reply
                    .ok()
                    .and_then(|v| v.child_value(0).get::<String>())
                    .filter(|v| valid_bus_name(v) && v.starts_with(':'));
                if let Some(item) = service
                    .items
                    .borrow_mut()
                    .iter_mut()
                    .find(|item| item.handle == handle && item.bus == requested)
                {
                    item.owner_unique = owner;
                }
                service.refresh_item(handle);
            },
        );
    }

    fn refresh_item(self: &Rc<Self>, handle: u32) {
        let (owner, path, revision) = {
            let mut items = self.items.borrow_mut();
            let Some(item) = items.iter_mut().find(|item| item.handle == handle) else {
                return;
            };
            let Some(owner) = item.owner_unique.clone() else {
                return;
            };
            item.refresh_serial = item.refresh_serial.wrapping_add(1);
            (owner, item.path.clone(), item.refresh_serial)
        };
        let Some(connection) = self.connection.borrow().clone() else {
            return;
        };
        let weak = Rc::downgrade(self);
        let callback_owner = owner.clone();
        let callback_path = path.clone();
        connection.call(
            Some(&owner),
            &path,
            PROPERTIES,
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
                if reply.size() > 512 * 1024 {
                    return;
                }
                let props = glib::VariantDict::new(Some(&reply.child_value(0)));
                let id = props
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
                let is_menu = props
                    .lookup_value("ItemIsMenu", None)
                    .and_then(|v| v.get::<bool>())
                    .unwrap_or(false);
                // Freedesktop SNI Menu is D-Bus type 'o', not String.
                let menu_path = props
                    .lookup_value("Menu", None)
                    .and_then(|value| {
                        (value.type_().as_str() == "o")
                            .then(|| value.str().map(str::to_owned))
                            .flatten()
                    })
                    .filter(|path| {
                        valid_object_path(path) && !path.ends_with("/NO_DBUSMENU")
                    });
                let status = match status.as_deref() {
                    Some("Passive") => TrayStatus::Passive,
                    Some("NeedsAttention") => TrayStatus::NeedsAttention,
                    _ => TrayStatus::Active,
                };
                let pixmap = if status == TrayStatus::NeedsAttention {
                    props
                        .lookup_value("AttentionIconPixmap", None)
                        .and_then(|value| decode_pixmap(&value))
                        .or_else(|| {
                            props
                                .lookup_value("IconPixmap", None)
                                .and_then(|value| decode_pixmap(&value))
                        })
                } else {
                    props
                        .lookup_value("IconPixmap", None)
                        .and_then(|value| decode_pixmap(&value))
                };
                let icon = (if status == TrayStatus::NeedsAttention {
                    attention_icon
                        .filter(|name| valid_icon_name(name))
                        .or(icon_name)
                } else {
                    icon_name
                })
                .filter(|name| valid_icon_name(name));
                let mut items = service.items.borrow_mut();
                let Some(item) = items.iter_mut().find(|item| {
                    item.handle == handle
                        && item.refresh_serial == revision
                        && item.owner_unique.as_deref() == Some(callback_owner.as_str())
                        && item.path == callback_path
                }) else {
                    return;
                };
                item.stable_key = id
                    .as_deref()
                    .map(|id| pin_identity(&item.registration, id))
                    .unwrap_or_else(|| item.registration.clone());
                item.title = clean(
                    title
                        .as_deref()
                        .filter(|v| !v.is_empty())
                        .or(id.as_deref())
                        .unwrap_or(&item.registration),
                    120,
                );
                item.icon_name = icon;
                item.pixmap = pixmap;
                item.status = status;
                item.is_menu = is_menu;
                item.menu_path = menu_path;
                item.ready = true;
                drop(items);
                service.publish();
                service.clear_stale_menu();
            },
        );
    }

    // The pinned GIO API still exposes signal_subscribe here. Its callback
    // signature is fixed; newer Gio's alternate API will be adopted separately.
    #[allow(deprecated)]
    fn install_observers(self: &Rc<Self>, connection: &gio::DBusConnection) {
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
                // StatusNotifierHostRegistered has no parameters. Never
                // index its empty tuple when monitoring the watcher interface.
                if !matches!(
                    member,
                    "StatusNotifierItemRegistered" | "StatusNotifierItemUnregistered"
                ) || params.n_children() == 0
                {
                    return;
                }
                if let Some(registration) = params.child_value(0).get::<String>() {
                    match member {
                        "StatusNotifierItemRegistered" => service.register_external(&registration),
                        "StatusNotifierItemUnregistered" => service.unregister(&registration),
                        _ => {}
                    }
                }
            },
        );
        let weak = Rc::downgrade(self);
        connection.signal_subscribe(
            Some(DBUS),
            Some(DBUS),
            Some("NameOwnerChanged"),
            Some(DBUS_PATH),
            None,
            gio::DBusSignalFlags::NONE,
            move |_, _, _, _, _, params| {
                let Some(service) = weak.upgrade() else {
                    return;
                };
                let name = params.child_value(0).get::<String>().unwrap_or_default();
                let old = params.child_value(1).get::<String>().unwrap_or_default();
                let new = params.child_value(2).get::<String>().unwrap_or_default();
                if name == WATCHER && !service.owns_watcher.get() {
                    service
                        .external_revision
                        .set(service.external_revision.get().wrapping_add(1));
                    service.available.set(false);
                    service.items.borrow_mut().clear();
                    service.publish();
                    service.clear_menu();
                    if !new.is_empty() {
                        service.adopt_external();
                    }
                } else if !old.is_empty() && old != new {
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
            move |_, sender, path, _, method, _| {
                if !matches!(
                    method,
                    "NewTitle" | "NewIcon" | "NewStatus" | "NewAttentionIcon"
                ) {
                    return;
                }
                if let Some(service) = weak.upgrade() {
                    service.refresh_sender(sender, path);
                }
            },
        );
        let weak = Rc::downgrade(self);
        connection.signal_subscribe(
            None,
            Some(PROPERTIES),
            Some("PropertiesChanged"),
            None,
            None,
            gio::DBusSignalFlags::NONE,
            move |_, sender, path, _, _, params| {
                if params.child_value(0).get::<String>().as_deref() == Some(ITEM_IFACE)
                    && let Some(service) = weak.upgrade()
                {
                    service.refresh_sender(sender, path);
                }
            },
        );
    }

    fn refresh_sender(self: &Rc<Self>, sender: &str, path: &str) {
        let handles = self
            .items
            .borrow()
            .iter()
            .filter(|item| item.owner_unique.as_deref() == Some(sender) && item.path == path)
            .map(|item| item.handle)
            .collect::<Vec<_>>();
        for handle in handles {
            self.refresh_item(handle);
        }
    }

    fn unregister(&self, registration: &str) {
        let mut items = self.items.borrow_mut();
        items.retain(|item| item.registration != registration);
        drop(items);
        self.publish();
        self.clear_stale_menu();
    }

    fn unregister_bus(&self, bus: &str) {
        let mut items = self.items.borrow_mut();
        let changed = items.len();
        items.retain(|item| item.bus != bus && item.owner_unique.as_deref() != Some(bus));
        if items.len() != changed {
            drop(items);
            self.publish();
            self.clear_stale_menu();
        }
    }

    fn register_external(self: &Rc<Self>, registration: &str) {
        if let Some((bus, path)) = parse_registration(registration) {
            self.insert_item(registration, &bus, &path);
        }
    }

    fn register_external_host(&self) {
        if !self.host_owned.get() || self.owns_watcher.get() {
            return;
        }
        let Some(connection) = self.connection.borrow().clone() else {
            return;
        };
        connection.call(
            Some(WATCHER),
            WATCHER_PATH,
            WATCHER_IFACE,
            "RegisterStatusNotifierHost",
            Some(&(self.host_name.as_str(),).to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            2500,
            None::<&gio::Cancellable>,
            |_| {},
        );
    }

    fn adopt_external(self: &Rc<Self>) {
        if self.owns_watcher.get() {
            return;
        }
        let Some(connection) = self.connection.borrow().clone() else {
            return;
        };
        self.register_external_host();
        let revision = self.external_revision.get().wrapping_add(1);
        self.external_revision.set(revision);
        let weak = Rc::downgrade(self);
        connection.call(
            Some(WATCHER),
            WATCHER_PATH,
            PROPERTIES,
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
                let props = glib::VariantDict::new(Some(&reply.child_value(0)));
                let roster = props
                    .lookup_value("RegisteredStatusNotifierItems", None)
                    .and_then(|v| v.get::<Vec<String>>())
                    .unwrap_or_default();
                service.available.set(true);
                for registration in roster.into_iter().take(MAX_ITEMS) {
                    service.register_external(&registration);
                }
                service.publish();
            },
        );
    }

    fn publish_menu(&self) {
        let snapshot = self.current_menu.borrow().as_ref().map(MenuSession::snapshot);
        (self.on_menu)(snapshot);
    }

    /// Clears both the current menu and all outstanding async menu replies.
    fn clear_menu(&self) {
        self.menu_serial
            .set(self.menu_serial.get().wrapping_add(1));
        if self.current_menu.borrow_mut().take().is_some() {
            self.publish_menu();
        }
    }

    fn menu_item_is_live(&self, menu: &MenuSession) -> bool {
        self.items.borrow().iter().any(|item| {
            item.ready
                && item.status != TrayStatus::Passive
                && item.handle == menu.handle
                && item.owner_unique.as_deref() == Some(menu.owner.as_str())
                && item.menu_path.as_deref() == Some(menu.path.as_str())
        })
    }

    fn clear_stale_menu(&self) {
        let stale = self
            .current_menu
            .borrow()
            .as_ref()
            .is_some_and(|menu| !self.menu_item_is_live(menu));
        if stale {
            self.clear_menu();
        }
    }

    fn call_item_method(&self, item: &Item, method: &str) {
        let Some(owner) = item.owner_unique.as_deref() else {
            return;
        };
        if !matches!(method, "Activate" | "ContextMenu")
            || !self.items.borrow().iter().any(|current| {
                current.ready
                    && current.handle == item.handle
                    && current.path == item.path
                    && current.owner_unique.as_deref() == Some(owner)
            })
        {
            return;
        }
        if let Some(connection) = self.connection.borrow().as_ref() {
            connection.call(
                Some(owner),
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

    /// Fixed, bounded application menu. The only authority is the verified
    /// registration's current unique sender and its advertised object path.
    fn open_menu(self: &Rc<Self>, item: &Item, path: &str) {
        self.clear_menu();
        let revision = self.menu_serial.get();
        let Some(connection) = self.connection.borrow().clone() else {
            return;
        };
        let Some(owner) = item.owner_unique.clone() else {
            return;
        };
        let session = MenuSession {
            handle: item.handle,
            owner,
            path: path.to_owned(),
            title: item.title.clone(),
            entries: Vec::new(),
        };
        if !self.menu_item_is_live(&session) {
            return;
        }
        let request = session.clone();
        let weak = Rc::downgrade(self);
        let menu_connection = connection.clone();
        connection.call(
            Some(&session.owner),
            &session.path,
            MENU_INTERFACE,
            "AboutToShow",
            Some(&(0i32,).to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            1000,
            None::<&gio::Cancellable>,
            move |_| {
                let Some(service) = weak.upgrade() else {
                    return;
                };
                if service.menu_serial.get() != revision || !service.menu_item_is_live(&request) {
                    return;
                }
                let weak = Rc::downgrade(&service);
                let menu = request.clone();
                menu_connection.call(
                    Some(&request.owner),
                    &request.path,
                    MENU_INTERFACE,
                    "GetLayout",
                    Some(&(0i32, -1i32, Vec::<String>::new()).to_variant()),
                    None,
                    gio::DBusCallFlags::NONE,
                    2500,
                    None::<&gio::Cancellable>,
                    move |reply| {
                        let Some(service) = weak.upgrade() else {
                            return;
                        };
                        if service.menu_serial.get() != revision
                            || !service.menu_item_is_live(&menu)
                        {
                            return;
                        }
                        let Some(entries) =
                            reply.ok().as_ref().and_then(tray_menu::parse_layout)
                        else {
                            // Legacy clients may only implement ContextMenu.
                            if let Some(item) = service.items.borrow().iter().find(|item| {
                                item.handle == menu.handle
                                    && item.owner_unique.as_deref() == Some(menu.owner.as_str())
                            }) {
                                service.call_item_method(item, "ContextMenu");
                            }
                            return;
                        };
                        let mut ready = menu;
                        ready.entries = entries;
                        service.current_menu.replace(Some(ready));
                        service.publish_menu();
                    },
                );
            },
        );
    }

    fn refresh_menu_after_action(self: &Rc<Self>, menu: MenuSession, revision: u64) {
        if self.menu_serial.get() != revision || !self.menu_item_is_live(&menu) {
            return;
        }
        let Some(connection) = self.connection.borrow().clone() else {
            return;
        };
        let weak = Rc::downgrade(self);
        let call_owner = menu.owner.clone();
        let call_path = menu.path.clone();
        connection.call(
            Some(&call_owner),
            &call_path,
            MENU_INTERFACE,
            "GetLayout",
            Some(&(0i32, -1i32, Vec::<String>::new()).to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            2500,
            None::<&gio::Cancellable>,
            move |reply| {
                let Some(service) = weak.upgrade() else {
                    return;
                };
                if service.menu_serial.get() != revision || !service.menu_item_is_live(&menu) {
                    return;
                }
                let Some(entries) = reply.ok().as_ref().and_then(tray_menu::parse_layout) else {
                    return; // Keep the last safe menu on failed refresh.
                };
                let mut current = service.current_menu.borrow_mut();
                let Some(active) = current.as_mut() else {
                    return;
                };
                if !active.same_target(&menu) || active.entries == entries {
                    return;
                }
                active.entries = entries;
                drop(current);
                service.publish_menu();
            },
        );
    }

    fn menu_click(self: &Rc<Self>, handle: u32, id: i32) {
        let selected = self
            .current_menu
            .borrow()
            .as_ref()
            .filter(|menu| menu.handle == handle && tray_menu::clickable(&menu.entries, id))
            .cloned();
        let Some(menu) = selected else {
            return;
        };
        if !self.menu_item_is_live(&menu) {
            self.clear_stale_menu();
            return;
        }
        let Some(connection) = self.connection.borrow().clone() else {
            return;
        };
        let revision = self.menu_serial.get().wrapping_add(1);
        self.menu_serial.set(revision);
        let weak = Rc::downgrade(self);
        let call_owner = menu.owner.clone();
        let call_path = menu.path.clone();
        connection.call(
            Some(&call_owner),
            &call_path,
            MENU_INTERFACE,
            "Event",
            Some(&tray_menu::click_params(id)),
            None,
            gio::DBusCallFlags::NONE,
            2500,
            None::<&gio::Cancellable>,
            move |reply| {
                let Some(service) = weak.upgrade() else {
                    return;
                };
                if reply.is_ok() && service.menu_serial.get() == revision {
                    service.refresh_menu_after_action(menu, revision);
                }
            },
        );
    }

    fn action(self: &Rc<Self>, intent: TrayIntent) {
        if matches!(intent, TrayIntent::DismissMenu) {
            self.clear_menu();
            return;
        }
        if let TrayIntent::MenuClick(handle, id) = intent {
            self.menu_click(handle, id);
            return;
        }
        let handle = match intent {
            TrayIntent::Activate(id) | TrayIntent::Secondary(id) | TrayIntent::TogglePin(id) => id,
            TrayIntent::MenuClick(_, _) | TrayIntent::DismissMenu => return,
        };
        let item = self
            .items
            .borrow()
            .iter()
            .find(|item| item.handle == handle && item.ready && item.status != TrayStatus::Passive)
            .cloned();
        let Some(item) = item else {
            return;
        };
        if matches!(intent, TrayIntent::TogglePin(_)) {
            let mut pins = self.pins.borrow().clone();
            if !pins.remove(&item.stable_key) {
                if pins.len() >= MAX_PINS {
                    return;
                }
                pins.insert(item.stable_key);
            }
            if let Err(error) = save_pins(&pins) {
                eprintln!("kira-shell: cannot persist tray pins: {error}");
                return;
            }
            self.pins.replace(pins);
            self.publish();
            return;
        }
        let context = matches!(intent, TrayIntent::Secondary(_)) || item.is_menu;
        if context {
            if let Some(path) = item.menu_path.as_deref() {
                self.open_menu(&item, path);
                return;
            }
        }
        self.call_item_method(&item, if context { "ContextMenu" } else { "Activate" });
    }
}

/// Typed UI intents are looked up against current, verified SNI registrations.
pub fn action(intent: TrayIntent) {
    SERVICE.with(|active| {
        if let Some(service) = active.borrow().as_ref() {
            service.action(intent);
        }
    });
}

/// Closing pooled popup windows cancels pending menu replies, including
/// dismissal via Escape handled by the PopupHost rather than an explicit row.
pub fn dismiss_menu() {
    SERVICE.with(|active| {
        if let Some(service) = active.borrow().as_ref() {
            service.clear_menu();
        }
    });
}

pub fn start(
    on_update: impl Fn(TrayState) + 'static,
    on_menu: impl Fn(Option<TrayMenu>) + 'static,
) {
    let service = Rc::new(Service {
        connection: RefCell::new(None),
        watcher_owner: RefCell::new(None),
        host_owner: RefCell::new(None),
        host_name: format!("org.kde.StatusNotifierHost-{}", std::process::id()),
        host_owned: Cell::new(false),
        owns_watcher: Cell::new(false),
        watcher_exported: Cell::new(false),
        available: Cell::new(false),
        external_revision: Cell::new(0),
        next_handle: Cell::new(1),
        items: RefCell::new(Vec::new()),
        pins: RefCell::new(load_pins()),
        menu_serial: Cell::new(0),
        current_menu: RefCell::new(None),
        on_menu: Box::new(on_menu),
        on_update: Box::new(on_update),
    });
    service.publish();
    SERVICE.with(|active| *active.borrow_mut() = Some(service.clone()));

    let host_ready = Rc::downgrade(&service);
    let host_lost = Rc::downgrade(&service);
    let host_id = gio::bus_own_name(
        gio::BusType::Session,
        &service.host_name,
        gio::BusNameOwnerFlags::DO_NOT_QUEUE,
        |_, _| {},
        move |_, _| {
            if let Some(service) = host_ready.upgrade() {
                service.host_owned.set(true);
                service.register_external_host();
            }
        },
        move |_, _| {
            if let Some(service) = host_lost.upgrade() {
                service.host_owned.set(false);
            }
        },
    );
    service.host_owner.replace(Some(host_id));

    if std::env::var_os("KIRA_PREVIEW").is_some() {
        // A nested compositor may share the parent session bus; never
        // seize its SNI watcher. Host registration is not ownership.
        let weak = Rc::downgrade(&service);
        gio::bus_get(
            gio::BusType::Session,
            None::<&gio::Cancellable>,
            move |reply| {
                if let (Some(service), Ok(connection)) = (weak.upgrade(), reply) {
                    service.install_observers(&connection);
                    service.connection.replace(Some(connection));
                    service.adopt_external();
                }
            },
        );
        return;
    }

    let bus_ready = Rc::downgrade(&service);
    let acquired = Rc::downgrade(&service);
    let lost = Rc::downgrade(&service);
    let owner = gio::bus_own_name(
        gio::BusType::Session,
        WATCHER,
        gio::BusNameOwnerFlags::DO_NOT_QUEUE,
        move |connection, _| {
            if let Some(service) = bus_ready.upgrade() {
                service.install_observers(&connection);
                service.connection.replace(Some(connection.clone()));
                service.register_watcher(&connection);
            }
        },
        move |_, _| {
            if let Some(service) = acquired.upgrade() {
                if !service.watcher_exported.get() {
                    if let Some(owner) = service.watcher_owner.borrow_mut().take() {
                        gio::bus_unown_name(owner);
                    }
                    return;
                }
                service.owns_watcher.set(true);
                service.available.set(true);
                service.publish();
                if let Some(connection) = service.connection.borrow().as_ref() {
                    let _ = connection.emit_signal(
                        None,
                        WATCHER_PATH,
                        WATCHER_IFACE,
                        "StatusNotifierHostRegistered",
                        None,
                    );
                }
            }
        },
        move |_, _| {
            if let Some(service) = lost.upgrade() {
                service.owns_watcher.set(false);
                service.watcher_exported.set(false);
                service.available.set(false);
                service.items.borrow_mut().clear();
                service.publish();
                service.clear_menu();
                service.adopt_external();
            }
        },
    );
    service.watcher_owner.replace(Some(owner));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registration_addresses_are_checked_before_gio() {
        assert!(valid_bus_name("org.kde.StatusNotifierItem-1"));
        assert!(valid_bus_name(":1.22"));
        assert!(!valid_bus_name("localhost"));
        assert!(!valid_bus_name("org.example;reboot"));
        assert!(valid_object_path("/StatusNotifierItem"));
        assert!(!valid_object_path("/invalid//path"));
        assert!(!valid_object_path("/bad/path/"));
        assert_eq!(
            parse_registration(":1.22/StatusNotifierItem"),
            Some((":1.22".into(), "/StatusNotifierItem".into()))
        );
        assert_eq!(
            parse_registration("org.kde.App"),
            Some(("org.kde.App".into(), "/StatusNotifierItem".into()))
        );
        assert_eq!(parse_registration("org.bad/../unsafe"), None);
    }

    #[test]
    fn malformed_sni_icon_variants_are_never_interpreted_as_pixels() {
        assert!(decode_pixmap(&"not an icon".to_variant()).is_none());
        assert!(decode_pixmap(&Vec::<(i32, i32, Vec<u8>)>::new().to_variant()).is_none());
    }

    #[test]
    fn icon_and_pin_keys_do_not_trust_unbounded_external_text() {
        assert!(valid_icon_name("application-symbolic"));
        assert!(!valid_icon_name("/tmp/untrusted.png"));
        assert!(!valid_icon_name("https://example.org/icon"));
        assert!(!valid_icon_name(".."));
        assert_eq!(pin_identity("org.kde.App", "app-id"), "id:app-id");
        assert_eq!(pin_identity("org.kde.App", ""), "org.kde.App");
        assert!(pin_identity("org.kde.App", &"Ж".repeat(120)).len() <= 240);
        assert_eq!(clean("A\nB", 120), "AB");
    }
}
