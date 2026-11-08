//! A bounded D-Bus menu decoder. Only leaf IDs in the currently verified
//! app-owned snapshot may dispatch a fixed "clicked" Event from native GTK.
use gtk::glib;
use gtk::glib::variant::ToVariant;
use kira_ui::TrayMenuEntry as MenuEntry;

const MAX_MENU_ENTRIES: usize = 80;
const MAX_MENU_LEVEL: u8 = 3;
const MAX_MENU_REPLY_BYTES: usize = 256 * 1024;

pub fn parse_layout(reply: &glib::Variant) -> Option<(u32, Vec<MenuEntry>)> {
    if reply.type_().as_str() != "(u(ia{sv}av))" || reply.size() > MAX_MENU_REPLY_BYTES {
        return None;
    }
    // Version is authoritative: a late GetLayout may not replace a newer
    // LayoutUpdated revision, even when both callbacks belong to one session.
    let version = reply.child_value(0).get::<u32>()?;
    let tree = reply.child_value(1);
    let mut entries = Vec::new();
    read_node(&tree, 0, &mut entries);
    Some((version, entries))
}

fn read_node(node: &glib::Variant, depth: u8, entries: &mut Vec<MenuEntry>) {
    if depth > MAX_MENU_LEVEL
        || entries.len() >= MAX_MENU_ENTRIES
        || node.type_().as_str() != "(ia{sv}av)"
    {
        return;
    }
    let Some(id) = node.child_value(0).get::<i32>() else {
        return;
    };
    let properties = glib::VariantDict::new(Some(&node.child_value(1)));
    if properties.lookup::<bool>("visible").ok().flatten() == Some(false) {
        return;
    }
    let children = node.child_value(2);
    // Lazily populated submenus advertise children-display="submenu"
    // before any children arrive in GetLayout(0). Show an expandable
    // GTK control and call AboutToShow(parent_id) on actual user intent.
    let has_children = children.n_children() > 0
        || properties
            .lookup::<String>("children-display")
            .ok()
            .flatten()
            .as_deref()
            == Some("submenu");
    if depth > 0 {
        if id <= 0 {
            return;
        }
        let type_name = properties.lookup::<String>("type").ok().flatten();
        let separator = type_name.as_deref() == Some("separator");
        let label = properties
            .lookup::<String>("label")
            .ok()
            .flatten()
            .unwrap_or_default();
        let toggle_type = properties.lookup::<String>("toggle-type").ok().flatten();
        let checked = properties.lookup::<i32>("toggle-state").ok().flatten() == Some(1);
        let enabled = properties
            .lookup::<bool>("enabled")
            .ok()
            .flatten()
            .unwrap_or(true);
        // Child nodes render indented under their parent in the first-phase menu.
        // Only leaf nodes are actionable, even if a malicious client reuses IDs.
        entries.push(MenuEntry {
            id,
            label: clean_label(&label),
            depth: depth - 1,
            enabled,
            separator,
            checked,
            radio: toggle_type.as_deref() == Some("radio"),
            has_children,
        });
    }
    if depth == MAX_MENU_LEVEL {
        return;
    }
    for child in 0..children.n_children().min(MAX_MENU_ENTRIES) {
        if entries.len() >= MAX_MENU_ENTRIES {
            break;
        }
        let Some(next) = children.child_value(child).as_variant() else {
            continue;
        };
        read_node(&next, depth + 1, entries);
    }
}

pub fn expandable(entries: &[MenuEntry], id: i32) -> bool {
    id > 0
        && entries.iter().filter(|entry| entry.id == id).count() == 1
        && entries
            .iter()
            .any(|entry| entry.id == id && entry.enabled && !entry.separator && entry.has_children)
}

/// Revision numbers are unsigned and may wrap. Only the same or strictly
/// newer revision can replace a live menu, following RFC1982 serial order.
pub fn revision_is_current(received: u32, previous: u32) -> bool {
    received == previous || received.wrapping_sub(previous) < 0x8000_0000
}

pub fn clickable(entries: &[MenuEntry], id: i32) -> bool {
    id > 0
        && entries.iter().filter(|entry| entry.id == id).count() == 1
        && entries
            .iter()
            .any(|entry| entry.id == id && entry.enabled && !entry.separator && !entry.has_children)
}

fn clean_label(value: &str) -> String {
    let mut clean = String::new();
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch.is_control() {
            continue;
        }
        if ch == '_' {
            if chars.peek() == Some(&'_') {
                chars.next();
            } else {
                continue;
            }
        }
        if clean.chars().count() >= 120 {
            break;
        }
        clean.push(ch);
    }
    clean
}

pub fn click_params(id: i32) -> glib::Variant {
    // Event(int32 id, string "clicked", variant data, uint32 timestamp)
    // Zero is permitted when no reliable compositor timestamp is exposed.
    (
        id,
        "clicked",
        glib::Variant::from_variant(&0i32.to_variant()),
        0u32,
    )
        .to_variant()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn entry(id: i32, enabled: bool) -> MenuEntry {
        MenuEntry {
            id,
            enabled,
            label: String::from("Example"),
            depth: 0,
            checked: false,
            radio: false,
            separator: false,
            has_children: false,
        }
    }
    #[test]
    fn only_unique_enabled_leaf_menu_entries_are_clickable() {
        let mut entries = vec![entry(1, true), entry(2, false)];
        assert!(clickable(&entries, 1));
        assert!(!clickable(&entries, 2));
        assert!(!clickable(&entries, 0));
        entries[0].has_children = true;
        assert!(!clickable(&entries, 1));
        entries.push(entry(2, true));
        assert!(!clickable(&entries, 2));
    }
    #[test]
    fn lazy_submenus_are_selected_only_by_unique_enabled_parent() {
        let mut entries = vec![entry(10, true), entry(11, false)];
        entries[0].has_children = true;
        entries[1].has_children = true;
        assert!(expandable(&entries, 10));
        assert!(!expandable(&entries, 11));
        assert!(!expandable(&entries, 0));
        assert!(!clickable(&entries, 10));
        entries.push(entry(10, true));
        assert!(!expandable(&entries, 10));
    }

    #[test]
    fn older_layout_versions_never_replace_new_menu_state() {
        assert!(revision_is_current(12, 12));
        assert!(revision_is_current(13, 12));
        assert!(!revision_is_current(11, 12));
        assert!(revision_is_current(0, u32::MAX));
        assert!(!revision_is_current(u32::MAX, 0));
    }

    #[test]
    fn labels_are_bounded_and_mnemonics_do_not_leak() {
        assert_eq!(clean_label("_Open __Tools\n"), "Open _Tools");
        assert_eq!(clean_label(&"x".repeat(300)).chars().count(), 120);
        assert_eq!(click_params(3).type_().as_str(), "(isvu)");
    }
    #[test]
    fn malformed_layout_is_ignored() {
        assert!(parse_layout(&"invalid".to_variant()).is_none());
    }
}
