//! Private-session-only Freedesktop Notifications protocol acceptance client.
//! Two real independent GIO bus connections prove same-sender replacement,
//! foreign-sender isolation, and stale-ID rejection. No GTK display needed.
//! Never run this against a user's regular session bus.
use std::collections::HashMap;

use gtk::gio;
use gtk::glib::{self, variant::ToVariant};

const NAME: &str = "org.freedesktop.Notifications";
const PATH: &str = "/org/freedesktop/Notifications";

fn isolated_session() -> bool {
    let parent = std::env::var("KIRA_ACCEPTANCE_PARENT_BUS").unwrap_or_default();
    let current = std::env::var("DBUS_SESSION_BUS_ADDRESS").unwrap_or_default();
    let token = std::env::var("KIRA_ACCEPTANCE_TOKEN").unwrap_or_default();
    std::env::var("KIRA_PREVIEW").ok().as_deref() == Some("1")
        && std::env::var("KIRA_ISOLATED_SESSION_BUS").ok().as_deref() == Some("1")
        && std::env::var("KIRA_NOTIFICATIONS_PREVIEW").ok().as_deref() == Some("1")
        && token.len() == 32
        && token.bytes().all(|byte| byte.is_ascii_hexdigit())
        && current.starts_with("unix:")
        && current != parent
}

fn connect_private_bus(address: &str) -> Result<gio::DBusConnection, glib::Error> {
    gio::DBusConnection::for_address_sync(
        address,
        gio::DBusConnectionFlags::AUTHENTICATION_CLIENT
            | gio::DBusConnectionFlags::MESSAGE_BUS_CONNECTION,
        None::<&gio::DBusAuthObserver>,
        None::<&gio::Cancellable>,
    )
}

fn notify_params(replaces_id: u32, summary: &str) -> glib::Variant {
    (
        "Kira Private Acceptance",
        replaces_id,
        "",
        summary,
        "Real sender identity protocol probe",
        Vec::<String>::new(),
        HashMap::<String, glib::Variant>::new(),
        0i32,
    )
        .to_variant()
}

fn notify(
    connection: &gio::DBusConnection,
    replaces_id: u32,
    summary: &str,
) -> Result<u32, String> {
    let reply = connection
        .call_sync(
            Some(NAME),
            PATH,
            NAME,
            "Notify",
            Some(&notify_params(replaces_id, summary)),
            None,
            gio::DBusCallFlags::NONE,
            5000,
            None::<&gio::Cancellable>,
        )
        .map_err(|error| format!("Private Notify failed: {error}"))?;
    if reply.type_().as_str() != "(u)" {
        return Err("Unexpected notification reply signature".into());
    }
    reply
        .child_value(0)
        .get::<u32>()
        .filter(|id| *id != 0)
        .ok_or_else(|| "Private notification returned an invalid ID".into())
}

fn close(connection: &gio::DBusConnection, id: u32) -> Result<(), String> {
    let reply = connection
        .call_sync(
            Some(NAME),
            PATH,
            NAME,
            "CloseNotification",
            Some(&(id,).to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            5000,
            None::<&gio::Cancellable>,
        )
        .map_err(|error| format!("Private CloseNotification failed: {error}"))?;
    if reply.type_().as_str() != "()" {
        return Err("Unexpected CloseNotification reply signature".into());
    }
    Ok(())
}

fn verify_server(connection: &gio::DBusConnection, policy_fixture: bool) -> Result<(), String> {
    let reply = connection
        .call_sync(
            Some(NAME),
            PATH,
            NAME,
            "GetServerInformation",
            None,
            None,
            gio::DBusCallFlags::NONE,
            5000,
            None::<&gio::Cancellable>,
        )
        .map_err(|error| format!("Private notification server unavailable: {error}"))?;
    let (expected_name, expected_vendor) = if policy_fixture {
        ("Kira Policy Fixture", "Kira Test Policy")
    } else {
        ("Kira Desktop", "Kira")
    };
    if reply.child_value(0).str() != Some(expected_name)
        || reply.child_value(1).str() != Some(expected_vendor)
    {
        return Err("Foreign notification daemon on the private acceptance bus".into());
    }
    Ok(())
}

fn run_probe(address: &str, policy_fixture: bool) -> Result<(), String> {
    // Explicit new bus connections, not gio::bus_get_sync's shared,
    // singleton session connection. The unique sender names MUST differ.
    let owner = connect_private_bus(address).map_err(|error| error.to_string())?;
    let foreign = connect_private_bus(address).map_err(|error| error.to_string())?;
    if owner.unique_name().is_none()
        || foreign.unique_name().is_none()
        || owner.unique_name() == foreign.unique_name()
    {
        return Err("Protocol probe did not obtain two distinct D-Bus peers".into());
    }
    verify_server(&owner, policy_fixture)?;
    let original = notify(&owner, 0, "owner_original")?;
    let updated = notify(&owner, original, "owner_updated")?;
    if original != updated {
        return Err("A real same-sender replaces_id changed the notification ID".into());
    }
    let foreign_id = notify(&foreign, original, "foreign_replacement_attempt")?;
    if foreign_id == original {
        return Err("A foreign D-Bus sender replaced another sender's notification".into());
    }
    close(&owner, original)?;
    let stale_id = notify(&owner, original, "stale_replacement_attempt")?;
    if stale_id == original || stale_id == foreign_id {
        return Err("A closed/stale replaces_id reused an active notification ID".into());
    }
    if close(&owner, original).is_ok() {
        return Err("A stale CloseNotification unexpectedly closed an old ID".into());
    }
    close(&foreign, foreign_id)?;
    close(&owner, stale_id)?;
    println!(
        "{{\"same_sender_replaced\":true,\"foreign_sender_isolated\":true,\"stale_id_rejected\":true,\"private_bus\":true}}"
    );
    Ok(())
}

fn main() {
    if !isolated_session() {
        eprintln!("kira-notification-acceptance: refusing non-isolated session bus");
        std::process::exit(2);
    }
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let policy_fixture = match args.as_slice() {
        [] => false,
        [flag] if flag == "--policy-fixture" => true,
        _ => {
            eprintln!("kira-notification-acceptance: unrecognized private probe arguments");
            std::process::exit(2);
        }
    };
    let address = std::env::var("DBUS_SESSION_BUS_ADDRESS").unwrap_or_default();
    if let Err(error) = run_probe(&address, policy_fixture) {
        eprintln!("kira-notification-acceptance: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_uses_exact_freedesktop_notify_signature_with_no_external_actions() {
        let params = notify_params(77, "Owner");
        assert_eq!(params.type_().as_str(), "(susssasa{sv}i)");
        assert_eq!(params.child_value(1).get::<u32>(), Some(77));
        assert_eq!(params.child_value(3).str(), Some("Owner"));
        assert_eq!(params.child_value(5).n_children(), 0);
        assert_eq!(params.child_value(6).n_children(), 0);
    }
}
