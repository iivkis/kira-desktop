//! Workspace strip rendering has no knowledge of Niri, IPC or Core.
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use gtk::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceItem {
    pub id: String,
    pub index: u8,
    pub name: Option<String>,
    pub focused: bool,
    pub urgent: bool,
}

/// Always include the focused workspace, even when it is past the visible limit.
pub fn visible_workspaces(items: &[WorkspaceItem], max_items: usize) -> Vec<&WorkspaceItem> {
    let mut visible: Vec<_> = items.iter().take(max_items).collect();
    if !visible.is_empty()
        && !visible.iter().any(|item| item.focused)
        && let Some(focused) = items.iter().skip(max_items).find(|item| item.focused)
    {
        visible.pop();
        visible.push(focused);
    }
    visible
}

/// Retains button objects and keyboard focus across ordinary state updates.
pub struct WorkspaceStrip {
    root: gtk::Box,
    buttons: gtk::Box,
    offline: gtk::Label,
    cache: RefCell<HashMap<String, gtk::Button>>,
    on_focus: Rc<dyn Fn(String)>,
}

impl WorkspaceStrip {
    pub fn new(on_focus: impl Fn(String) + 'static) -> Self {
        let root = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        let offline = gtk::Label::new(Some("NIRI OFFLINE"));
        offline.add_css_class("offline");
        root.append(&offline);
        let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        root.append(&buttons);
        Self {
            root,
            buttons,
            offline,
            cache: RefCell::new(HashMap::new()),
            on_focus: Rc::new(on_focus),
        }
    }

    pub fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }

    pub fn update(&self, connected: bool, items: &[WorkspaceItem], limit: usize, show_names: bool) {
        self.offline.set_visible(!connected);
        self.buttons.set_visible(connected);
        if !connected {
            return;
        }

        let selected = visible_workspaces(items, limit);
        let retained: HashSet<&str> = selected.iter().map(|item| item.id.as_str()).collect();
        let stale: Vec<String> = self
            .cache
            .borrow()
            .keys()
            .filter(|id| !retained.contains(id.as_str()))
            .cloned()
            .collect();
        for id in stale {
            if let Some(button) = self.cache.borrow_mut().remove(&id) {
                self.buttons.remove(&button);
            }
        }

        let mut predecessor: Option<gtk::Widget> = None;
        for item in selected {
            let old = self.cache.borrow().get(&item.id).cloned();
            let button = if let Some(existing) = old {
                existing
            } else {
                let button = gtk::Button::new();
                button.add_css_class("workspace");
                let id = item.id.clone();
                let on_focus = self.on_focus.clone();
                button.connect_clicked(move |_| on_focus(id.clone()));
                self.buttons.append(&button);
                self.cache
                    .borrow_mut()
                    .insert(item.id.clone(), button.clone());
                button
            };
            let label = if show_names {
                item.name.clone().unwrap_or_else(|| item.index.to_string())
            } else {
                item.index.to_string()
            };
            button.set_label(&label);
            button.set_tooltip_text(Some(&format!("Workspace {}", item.index)));
            if item.focused {
                button.add_css_class("selected");
            } else {
                button.remove_css_class("selected");
            }
            if item.urgent {
                button.add_css_class("urgent");
            } else {
                button.remove_css_class("urgent");
            }
            // Reordering only when necessary retains focus and the original widget instance.
            let in_position = match &predecessor {
                Some(before) => before.next_sibling().as_ref() == Some(button.upcast_ref()),
                None => self.buttons.first_child().as_ref() == Some(button.upcast_ref()),
            };
            if !in_position {
                self.buttons
                    .reorder_child_after(&button, predecessor.as_ref());
            }
            predecessor = Some(button.upcast());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str, focused: bool) -> WorkspaceItem {
        WorkspaceItem {
            id: id.to_owned(),
            index: id.parse().unwrap(),
            name: None,
            focused,
            urgent: false,
        }
    }

    #[test]
    fn focused_workspace_is_visible_after_limit() {
        let workspaces = vec![item("1", false), item("2", false), item("3", true)];
        let visible = visible_workspaces(&workspaces, 2);
        assert_eq!(
            visible.iter().map(|w| w.id.as_str()).collect::<Vec<_>>(),
            ["1", "3"]
        );
    }

    #[test]
    fn normal_limit_and_empty_limit_are_stable() {
        let workspaces = vec![item("1", true), item("2", false), item("3", false)];
        assert_eq!(visible_workspaces(&workspaces, 2).len(), 2);
        assert!(visible_workspaces(&workspaces, 0).is_empty());
    }
}
