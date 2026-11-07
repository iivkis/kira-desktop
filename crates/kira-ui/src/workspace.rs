//! Workspace strip behavior ported from main's workspace-window.ts.
//! No compositor calls: this library accepts typed, observed state only.
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use gtk::prelude::*;

use crate::flow_indicator::{FlowAxis, FlowIndicator, FlowStyle};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceItem {
    pub id: String,
    pub index: u8,
    pub name: Option<String>,
    pub output: Option<String>,
    pub active: bool,
    pub focused: bool,
    pub urgent: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub struct WorkspaceWindow<'a> {
    pub items: Vec<&'a WorkspaceItem>,
    pub before: usize,
    pub after: usize,
    pub before_urgent: bool,
    pub after_urgent: bool,
}

/// Matches main's visibleWorkspaceWindow: contiguous range, real hidden counts,
/// focused item visible even if it lies outside the first configured page.
pub fn visible_workspace_window(items: &[WorkspaceItem], limit: usize) -> WorkspaceWindow<'_> {
    let count = limit.clamp(1, 32);
    let focus = items.iter().position(|item| item.focused);
    let start = focus
        .map(|index| index.saturating_sub(count - 1))
        .unwrap_or(0)
        .min(items.len().saturating_sub(count));
    let end = (start + count).min(items.len());
    WorkspaceWindow {
        items: items[start..end].iter().collect(),
        before: start,
        after: items.len() - end,
        before_urgent: items[..start].iter().any(|item| item.urgent),
        after_urgent: items[end..].iter().any(|item| item.urgent),
    }
}

pub fn visible_workspaces(items: &[WorkspaceItem], max_items: usize) -> Vec<&WorkspaceItem> {
    visible_workspace_window(items, max_items).items
}

pub struct WorkspaceStrip {
    root: gtk::Box,
    buttons: gtk::Box,
    selection_flow: FlowIndicator,
    status: gtk::Label,
    before: gtk::Label,
    after: gtk::Label,
    cache: RefCell<HashMap<String, gtk::Button>>,
    on_focus: Rc<dyn Fn(String)>,
}

impl WorkspaceStrip {
    pub fn new(on_focus: impl Fn(String) + 'static) -> Self {
        let root = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        let status = gtk::Label::new(Some("NIRI: ПОДКЛЮЧЕНИЕ…"));
        status.add_css_class("offline");
        root.append(&status);

        let before = gtk::Label::new(None);
        before.add_css_class("workspace-overflow");
        root.append(&before);

        let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        buttons.add_css_class("workspace-strip");
        let selection_flow = FlowIndicator::new(
            &buttons.clone().upcast(),
            FlowAxis::Horizontal,
            FlowStyle::Fill,
        );
        root.append(&selection_flow);

        let after = gtk::Label::new(None);
        after.add_css_class("workspace-overflow");
        root.append(&after);

        Self {
            root,
            buttons,
            selection_flow,
            status,
            before,
            after,
            cache: RefCell::new(HashMap::new()),
            on_focus: Rc::new(on_focus),
        }
    }

    pub fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }

    pub fn update(&self, connected: bool, items: &[WorkspaceItem], limit: usize, show_names: bool) {
        self.status.set_visible(!connected || items.is_empty());
        self.status.set_text(if !connected {
            "NIRI НЕДОСТУПЕН"
        } else if items.is_empty() {
            "НЕТ РАБОЧИХ СТОЛОВ"
        } else {
            ""
        });
        let active = connected && !items.is_empty();
        self.buttons.set_visible(active);
        self.before.set_visible(active);
        self.after.set_visible(active);
        if !active {
            self.selection_flow.clear();
            return;
        }

        let window = visible_workspace_window(items, limit);
        self.before.set_text(&if window.before > 0 {
            format!("−{}", window.before)
        } else {
            String::new()
        });
        self.after.set_text(&if window.after > 0 {
            format!("+{}", window.after)
        } else {
            String::new()
        });
        for (label, count, urgent, side) in [
            (
                &self.before,
                window.before,
                window.before_urgent,
                "предыдущих",
            ),
            (
                &self.after,
                window.after,
                window.after_urgent,
                "последующих",
            ),
        ] {
            label.set_tooltip_text(
                (count > 0)
                    .then(|| {
                        format!(
                            "{count} {side} рабочих пространств скрыто{}",
                            if urgent { " · срочные" } else { "" }
                        )
                    })
                    .as_deref(),
            );
            if urgent {
                label.add_css_class("urgent");
            } else {
                label.remove_css_class("urgent");
            }
        }
        let retained: HashSet<&str> = window.items.iter().map(|item| item.id.as_str()).collect();
        let stale: Vec<_> = self
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
        let mut focused: Option<gtk::Button> = None;
        for item in window.items {
            let existing = self.cache.borrow().get(&item.id).cloned();
            let button = if let Some(existing) = existing {
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
            let name = item
                .name
                .as_deref()
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .unwrap_or_else(|| item.index.to_string());
            let index_label = item.index.to_string();
            button.set_label(if show_names { &name } else { &index_label });
            let on_output = item
                .output
                .as_deref()
                .map_or(String::new(), |output| format!(" на экране {output}"));
            let state = if item.focused {
                " (в фокусе)"
            } else if item.active {
                " (активно)"
            } else {
                ""
            };
            let urgent = if item.urgent {
                " (требует внимания)"
            } else {
                ""
            };
            button.set_tooltip_text(Some(&format!(
                "Пространство {name}{on_output}{state}{urgent}"
            )));
            button.update_property(&[gtk::accessible::Property::Label(&format!(
                "Переключиться на пространство {name}{on_output}{urgent}"
            ))]);
            for (class, enabled) in [
                ("selected", item.focused),
                ("active", item.active),
                ("urgent", item.urgent),
            ] {
                if enabled {
                    button.add_css_class(class);
                } else {
                    button.remove_css_class(class);
                }
            }
            if item.focused {
                focused = Some(button.clone());
            }
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
        if let Some(button) = focused {
            self.selection_flow.retarget(button.upcast_ref());
        } else {
            self.selection_flow.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(index: u8, focused: bool, urgent: bool) -> WorkspaceItem {
        WorkspaceItem {
            id: index.to_string(),
            index,
            name: (index == 3).then(|| "edit".into()),
            output: Some("DP-1".into()),
            active: focused,
            focused,
            urgent,
        }
    }

    #[test]
    fn main_contiguous_window_and_real_hidden_counts() {
        let entries: Vec<_> = (1..=6).map(|i| item(i, i == 5, false)).collect();
        let view = visible_workspace_window(&entries, 3);
        assert_eq!(
            view.items.iter().map(|w| w.id.as_str()).collect::<Vec<_>>(),
            ["3", "4", "5"]
        );
        assert_eq!((view.before, view.after), (2, 1));
        assert!(!view.before_urgent && !view.after_urgent);
    }

    #[test]
    fn main_hidden_urgent_is_reported_both_sides() {
        let entries: Vec<_> = (1..=6).map(|i| item(i, i == 4, i == 1 || i == 6)).collect();
        let view = visible_workspace_window(&entries, 2);
        assert_eq!(
            view.items.iter().map(|w| w.index).collect::<Vec<_>>(),
            [3, 4]
        );
        assert_eq!((view.before, view.after), (2, 2));
        assert!(view.before_urgent && view.after_urgent);
    }

    #[test]
    fn main_no_focus_and_limit_bounds() {
        let entries: Vec<_> = (1..=4).map(|i| item(i, false, false)).collect();
        assert_eq!(visible_workspace_window(&entries, 2).items.len(), 2);
        assert_eq!(visible_workspace_window(&entries, 0).items.len(), 1);
        assert_eq!(visible_workspace_window(&entries, 999).items.len(), 4);
        assert!(visible_workspace_window(&[], 3).items.is_empty());
    }
}
