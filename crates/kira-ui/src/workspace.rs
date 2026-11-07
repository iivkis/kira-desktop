//! Workspace strip behavior ported from main's workspace-window.ts.
//! No compositor calls: this library accepts typed, observed state only.
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use gtk::prelude::*;

use crate::flow_indicator::{FlowAxis, FlowIndicator, FlowStyle};
use crate::reflow::{ReflowRow, ReflowScope};

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
    reflow: ReflowScope,
    visible_ids: RefCell<Vec<String>>,
    status: gtk::Label,
    before: gtk::Label,
    after: gtk::Label,
    cache: RefCell<HashMap<String, (gtk::Button, ReflowRow)>>,
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
        let reflow = ReflowScope::new(&buttons.clone().upcast());
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
            reflow,
            visible_ids: RefCell::new(Vec::new()),
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
            self.reflow.cancel();
            // A stopped compositor must never leave an in-flight paint offset
            // on reusable workspace buttons when Niri reconnects.
            for (_, row) in self.cache.borrow().values() {
                row.settle();
            }
            self.visible_ids.borrow_mut().clear();
            return;
        }

        let window = visible_workspace_window(items, limit);
        let next_ids: Vec<String> = window.items.iter().map(|item| item.id.clone()).collect();
        let previous_ids = self.visible_ids.borrow().clone();
        let capture = if previous_ids != next_ids && !previous_ids.is_empty() {
            let cache = self.cache.borrow();
            let before: Vec<_> = previous_ids
                .iter()
                .filter_map(|key| cache.get(key).map(|(_, row)| (key.clone(), row.clone())))
                .collect();
            Some(self.reflow.capture(&before))
        } else {
            None
        };
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
            if let Some((_, row)) = self.cache.borrow_mut().remove(&id) {
                self.buttons.remove(&row);
            }
        }

        let mut predecessor: Option<gtk::Widget> = None;
        let mut focused: Option<ReflowRow> = None;
        for item in window.items {
            let existing = self.cache.borrow().get(&item.id).cloned();
            let (button, row) = if let Some(existing) = existing {
                existing
            } else {
                let button = gtk::Button::new();
                button.add_css_class("workspace");
                let id = item.id.clone();
                let on_focus = self.on_focus.clone();
                button.connect_clicked(move |_| on_focus(id.clone()));
                let row = ReflowRow::new(&button.clone().upcast());
                self.buttons.append(&row);
                self.cache
                    .borrow_mut()
                    .insert(item.id.clone(), (button.clone(), row.clone()));
                (button, row)
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
                focused = Some(row.clone());
            }
            let in_position = match &predecessor {
                Some(before) => before.next_sibling().as_ref() == Some(row.upcast_ref()),
                None => self.buttons.first_child().as_ref() == Some(row.upcast_ref()),
            };
            if !in_position {
                self.buttons.reorder_child_after(&row, predecessor.as_ref());
            }
            predecessor = Some(row.upcast());
        }
        if let Some(row) = focused {
            self.selection_flow.retarget(row.upcast_ref());
        } else {
            self.selection_flow.clear();
        }
        self.visible_ids.replace(next_ids.clone());
        if let Some(snapshot) = capture {
            let cache = self.cache.borrow();
            let after: Vec<_> = next_ids
                .iter()
                .filter_map(|key| cache.get(key).map(|(_, row)| (key.clone(), row.clone())))
                .collect();
            self.reflow.play(snapshot, &after);
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
