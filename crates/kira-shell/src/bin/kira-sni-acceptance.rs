//! Real, disposable StatusNotifierItem for Niri acceptance ONLY.
//! This executable is never autostarted or called by production Shell.
//! It refuses to own a name unless the preview's private-bus attestation
//! and the coordinator's nonce are both present.
use std::cell::{Cell, RefCell};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use gtk::gio;
use gtk::glib::{self, variant::ToVariant};

const ITEM_PATH: &str = "/StatusNotifierItem";
const ITEM_NAME: &str = "org.kira.AcceptanceSNI";
const ITEM_IFACE: &str = "org.kde.StatusNotifierItem";
const WATCHER: &str = "org.kde.StatusNotifierWatcher";
const WATCHER_PATH: &str = "/StatusNotifierWatcher";
const XML: &str = r#"<node>
<interface name="org.kde.StatusNotifierItem">
  <method name="Activate"><arg type="i" direction="in"/><arg type="i" direction="in"/></method>
  <method name="ContextMenu"><arg type="i" direction="in"/><arg type="i" direction="in"/></method>
  <property name="Id" type="s" access="read"/>
  <property name="Title" type="s" access="read"/>
  <property name="Status" type="s" access="read"/>
  <property name="IconName" type="s" access="read"/>
  <property name="IconPixmap" type="a(iiay)" access="read"/>
  <property name="ItemIsMenu" type="b" access="read"/>
  <signal name="NewStatus"><arg type="s"/></signal>
  <signal name="NewTitle"/>
  <signal name="NewIcon"/>
</interface>
</node>"#;

fn isolated_session() -> bool {
    let parent = std::env::var("KIRA_ACCEPTANCE_PARENT_BUS").unwrap_or_default();
    let current = std::env::var("DBUS_SESSION_BUS_ADDRESS").unwrap_or_default();
    let token = std::env::var("KIRA_ACCEPTANCE_TOKEN").unwrap_or_default();
    std::env::var("KIRA_PREVIEW").ok().as_deref() == Some("1")
        && std::env::var("KIRA_ISOLATED_SESSION_BUS").ok().as_deref() == Some("1")
        && std::env::var("KIRA_SNI_PREVIEW").ok().as_deref() == Some("1")
        && token.len() == 32
        && token.bytes().all(|c| c.is_ascii_hexdigit())
        && current.starts_with("unix:")
        && current != parent
}

fn icon_pixmap(attention: bool) -> Vec<(i32, i32, Vec<u8>)> {
    // Real SNI a(iiay) network-order ARGB32, 24x24 pixels. No GTK art.
    let color = if attention {
        [255, 252, 163, 17]
    } else {
        [255, 20, 33, 61]
    };
    let bytes = (0..24 * 24).flat_map(|_| color).collect();
    vec![(24, 24, bytes)]
}

fn log_event(path: &PathBuf, event: &str) {
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{event}");
    }
}

fn main() {
    if !isolated_session() {
        eprintln!("kira-sni-acceptance: refusing a non-isolated or untrusted session bus");
        std::process::exit(2);
    }
    let mut args = std::env::args_os();
    let _ = args.next();
    let Some(path) = args.next().map(PathBuf::from) else {
        eprintln!("usage: kira-sni-acceptance /absolute/sandbox/event-log.txt");
        std::process::exit(2);
    };
    if !path.is_absolute() || path.exists() {
        eprintln!("kira-sni-acceptance: event log must be a NEW absolute sandbox path");
        std::process::exit(2);
    }
    let event_loop = glib::MainLoop::new(None, false);
    let status = Rc::new(Cell::new(false));
    let title = Rc::new(RefCell::new(String::from("Kira private SNI fixture")));
    let connection_holder: Rc<RefCell<Option<gio::DBusConnection>>> =
        Rc::new(RefCell::new(None));
    let bus_ready = connection_holder.clone();
    let property_status = status.clone();
    let property_title = title.clone();
    let method_log = path.clone();
    let name_log = path.clone();
    let name_connection = connection_holder.clone();
    let lost_loop = event_loop.clone();

    let _owner = gio::bus_own_name(
        gio::BusType::Session,
        ITEM_NAME,
        gio::BusNameOwnerFlags::DO_NOT_QUEUE,
        move |connection, _| {
            let Ok(node) = gio::DBusNodeInfo::for_xml(XML) else {
                return;
            };
            let Some(info) = node.lookup_interface(ITEM_IFACE) else {
                return;
            };
            let method_log = method_log.clone();
            let status = property_status.clone();
            let title = property_title.clone();
            if let Err(error) = connection
                .register_object(ITEM_PATH, &info)
                .method_call(move |_, _, _, _, method, _, invocation| {
                    match method {
                        "Activate" | "ContextMenu" => {
                            log_event(&method_log, method);
                            invocation.return_value(None);
                        }
                        _ => invocation.return_dbus_error(
                            "org.freedesktop.DBus.Error.UnknownMethod",
                            "Unsupported fixture method",
                        ),
                    }
                })
                .property(move |_, _, _, _, name| match name {
                    "Id" => "kira-sni-acceptance".to_variant(),
                    "Title" => title.borrow().to_variant(),
                    "Status" => (if status.get() { "NeedsAttention" } else { "Active" }).to_variant(),
                    "IconName" => "".to_variant(),
                    "IconPixmap" => icon_pixmap(status.get()).to_variant(),
                    "ItemIsMenu" => false.to_variant(),
                    _ => false.to_variant(),
                })
                .build()
            {
                eprintln!("kira-sni-acceptance: cannot export StatusNotifierItem: {error}");
            }
            bus_ready.replace(Some(connection.clone()));
        },
        move |_, _| {
            log_event(&name_log, "NameAcquired");
            let weak = Rc::downgrade(&name_connection);
            let registered = Rc::new(Cell::new(false));
            let registered_check = registered.clone();
            let mut attempts = 0u8;
            glib::timeout_add_local(Duration::from_millis(250), move || {
                if registered_check.get() || attempts >= 40 {
                    return glib::ControlFlow::Break;
                }
                attempts += 1;
                let Some(connection) = weak.upgrade().and_then(|slot| slot.borrow().clone()) else {
                    return glib::ControlFlow::Continue;
                };
                let registered = registered_check.clone();
                connection.call(
                    Some(WATCHER),
                    WATCHER_PATH,
                    WATCHER,
                    "RegisterStatusNotifierItem",
                    Some(&(ITEM_PATH,).to_variant()),
                    None,
                    gio::DBusCallFlags::NONE,
                    500,
                    None::<&gio::Cancellable>,
                    move |reply| {
                        if reply.is_ok() {
                            registered.set(true);
                        }
                    },
                );
                glib::ControlFlow::Continue
            });
        },
        move |_, _| {
            eprintln!("kira-sni-acceptance: lost private D-Bus name");
            lost_loop.quit();
        },
    );

    let changed_status = status.clone();
    let changed_connection = connection_holder.clone();
    let change_log = path.clone();
    glib::timeout_add_local_once(Duration::from_millis(1400), move || {
        changed_status.set(true);
        if let Some(connection) = changed_connection.borrow().as_ref() {
            let _ = connection.emit_signal(
                None, ITEM_PATH, ITEM_IFACE, "NewStatus",
                Some(&("NeedsAttention",).to_variant()),
            );
            let _ = connection.emit_signal(None, ITEM_PATH, ITEM_IFACE, "NewIcon", None);
        }
        log_event(&change_log, "NeedsAttention");
    });
    event_loop.run();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_refuses_the_user_session_bus() {
        let fixture = icon_pixmap(false);
        assert_eq!(fixture.len(), 1);
        assert_eq!(fixture[0].2.len(), 24 * 24 * 4);
        assert_eq!(&fixture[0].2[..4], &[255, 20, 33, 61]);
        assert_eq!(icon_pixmap(true)[0].2[..4], [255, 252, 163, 17]);
    }
}
