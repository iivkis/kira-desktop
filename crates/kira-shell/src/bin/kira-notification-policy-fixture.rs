//! Headless D-Bus *policy fixture* for the CI private bus, NOT the GTK Shell.
//! It reuses production kira-services admission/ownership/queue logic and
//! exposes the Freedesktop method signatures to two real GIO client peers.
//! Production presentation, name ownership and hit testing are not inferred
//! from this deliberately separate acceptance-only service.
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use gtk::gio;
use gtk::glib::{self, variant::ToVariant};
use kira_services::notification_queue::{
    DEFAULT_DWELL, Notice, NotificationOwnership, NotificationQueue, admit_notification,
    close_notification,
};

const NAME: &str = "org.freedesktop.Notifications";
const PATH: &str = "/org/freedesktop/Notifications";
const XML: &str = r#"<node><interface name="org.freedesktop.Notifications">
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
</interface></node>"#;

#[derive(Default)]
struct PolicyState {
    queue: NotificationQueue,
    owners: NotificationOwnership,
}

fn isolated_session() -> bool {
    let parent = std::env::var("KIRA_ACCEPTANCE_PARENT_BUS").unwrap_or_default();
    let current = std::env::var("DBUS_SESSION_BUS_ADDRESS").unwrap_or_default();
    let token = std::env::var("KIRA_ACCEPTANCE_TOKEN").unwrap_or_default();
    std::env::var("KIRA_PREVIEW").ok().as_deref() == Some("1")
        && std::env::var("KIRA_ISOLATED_SESSION_BUS").ok().as_deref() == Some("1")
        && std::env::var("KIRA_NOTIFICATIONS_PREVIEW").ok().as_deref() == Some("1")
        && std::env::var("KIRA_POLICY_FIXTURE_PREVIEW").ok().as_deref() == Some("1")
        && token.len() == 32
        && token.bytes().all(|byte| byte.is_ascii_hexdigit())
        && current.starts_with("unix:")
        && current != parent
}

fn notification_from_params(params: &glib::Variant) -> Notice {
    Notice {
        id: 0,
        app: params
            .child_value(0)
            .str()
            .unwrap_or_default()
            .chars()
            .take(128)
            .collect(),
        summary: params
            .child_value(3)
            .str()
            .unwrap_or_default()
            .chars()
            .take(256)
            .collect(),
        body: params
            .child_value(4)
            .str()
            .unwrap_or_default()
            .chars()
            .take(2048)
            .collect(),
        actions: Vec::new(), // no GTK actions in this protocol-only fixture
        critical: false,
        persistent: params.child_value(7).get::<i32>() == Some(0),
        dwell: DEFAULT_DWELL,
    }
}

fn main() {
    if !isolated_session() || std::env::args_os().len() != 1 {
        eprintln!("kira-notification-policy-fixture: refusing a non-private test bus");
        std::process::exit(2);
    }
    let state = Rc::new(RefCell::new(PolicyState::default()));
    let loop_ = glib::MainLoop::new(None, false);
    let registration_state = state.clone();
    let lost_loop = loop_.clone();
    let _owner = gio::bus_own_name(
        gio::BusType::Session,
        NAME,
        gio::BusNameOwnerFlags::DO_NOT_QUEUE,
        move |connection, _| {
            let Ok(node) = gio::DBusNodeInfo::for_xml(XML) else {
                return;
            };
            let Some(info) = node.lookup_interface(NAME) else {
                return;
            };
            let shared = registration_state.clone();
            if let Err(error) = connection
                .register_object(PATH, &info)
                .method_call(
                    move |_, sender, _, _, method, params, invocation| match method {
                        "GetServerInformation" => {
                            invocation.return_value(Some(
                                &("Kira Policy Fixture", "Kira Test Policy", "0.1", "1.3")
                                    .to_variant(),
                            ));
                        }
                        "Notify" => {
                            if params.size() > 64 * 1024 {
                                invocation.return_dbus_error(
                                    "org.freedesktop.Notifications.Error.ExcessData",
                                    "Payload exceeds the bounded policy fixture limit",
                                );
                                return;
                            }
                            let Some(sender) = sender.filter(|s| s.starts_with(':')) else {
                                invocation.return_dbus_error(
                                    "org.freedesktop.DBus.Error.AccessDenied",
                                    "No unique D-Bus sender",
                                );
                                return;
                            };
                            let mut state = shared.borrow_mut();
                            let PolicyState { queue, owners } = &mut *state;
                            let admitted = admit_notification(
                                queue,
                                owners,
                                sender,
                                params.child_value(1).get::<u32>().unwrap_or(0),
                                notification_from_params(&params),
                                Instant::now(),
                            );
                            if let Some(id) = admitted {
                                invocation.return_value(Some(&(id,).to_variant()));
                            } else {
                                invocation.return_dbus_error(
                                    "org.freedesktop.Notifications.Error.QueueFull",
                                    "Policy fixture admission refused",
                                );
                            }
                        }
                        "CloseNotification" => {
                            let id = params.child_value(0).get::<u32>().unwrap_or(0);
                            let mut state = shared.borrow_mut();
                            let PolicyState { queue, owners } = &mut *state;
                            if close_notification(queue, owners, id) {
                                invocation.return_value(None);
                            } else {
                                invocation.return_dbus_error(
                                    "org.freedesktop.Notifications.Error.InvalidId",
                                    "Unknown policy fixture notification ID",
                                );
                            }
                        }
                        _ => invocation.return_dbus_error(
                            "org.freedesktop.DBus.Error.UnknownMethod",
                            "Unsupported policy fixture method",
                        ),
                    },
                )
                .build()
            {
                eprintln!("kira-notification-policy-fixture: cannot export GIO object: {error}");
            }
        },
        move |_, _| {},
        move |_, _| {
            eprintln!("kira-notification-policy-fixture: private name lost");
            lost_loop.quit();
        },
    );
    loop_.run();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_dbus_signatures_are_exact_and_cannot_claim_ui_or_actions() {
        let node = gio::DBusNodeInfo::for_xml(XML).unwrap();
        assert!(node.lookup_interface(NAME).is_some());
        let params = (
            "app",
            12u32,
            "",
            "summary",
            "body",
            Vec::<String>::new(),
            std::collections::HashMap::<String, glib::Variant>::new(),
            0i32,
        )
            .to_variant();
        let notice = notification_from_params(&params);
        assert_eq!(notice.summary, "summary");
        assert!(notice.persistent);
        assert!(notice.actions.is_empty());
    }
}
