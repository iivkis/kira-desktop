//! Reusable live StatusNotifier tray presentation. The UI consumes typed
//! item snapshots and emits bounded numeric handle intents only: it never
//! constructs D-Bus paths, commands, URLs or application menu calls.
use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;

type ToggleHandler = Rc<RefCell<Option<Box<dyn Fn()>>>>;

pub const MAX_BAR_ICONS: usize = 4;
pub const MAX_TRAY_ITEMS: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayStatus {
    Active,
    Passive,
    NeedsAttention,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrayItem {
    pub handle: u32,
    pub title: String,
    pub icon_name: Option<String>,
    pub status: TrayStatus,
    pub pinned: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TrayState {
    pub available: bool,
    pub items: Vec<TrayItem>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TrayIntent {
    Activate(u32),
    Secondary(u32),
    TogglePin(u32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayBarModel {
    pub visible: Vec<TrayItem>,
    pub overflow_count: usize,
    pub overflow_attention: bool,
}

/// True source-derived pin/overflow model. Passive items have no presence
/// and an overflow attention mark counts only *actually clipped* items.
pub fn tray_bar_model(state: &TrayState, max_visible: usize) -> TrayBarModel {
    let mut pinned = Vec::new();
    let mut hidden = Vec::new();
    for item in state
        .items
        .iter()
        .filter(|item| item.handle != 0 && item.status != TrayStatus::Passive)
        .take(MAX_TRAY_ITEMS)
    {
        if item.pinned {
            pinned.push(item.clone());
        } else {
            hidden.push(item.clone());
        }
    }
    let limit = max_visible.min(MAX_TRAY_ITEMS);
    let visible = pinned.iter().take(limit).cloned().collect::<Vec<_>>();
    let overflow = hidden
        .iter()
        .chain(pinned.iter().skip(visible.len()))
        .collect::<Vec<_>>();
    TrayBarModel {
        visible,
        overflow_count: overflow.len(),
        overflow_attention: overflow
            .iter()
            .any(|item| item.status == TrayStatus::NeedsAttention),
    }
}

fn icon(item: &TrayItem, size: i32) -> gtk::Widget {
    if let Some(name) = &item.icon_name {
        let image = gtk::Image::from_icon_name(name);
        image.set_pixel_size(size);
        image.upcast()
    } else {
        let letter = item
            .title
            .chars()
            .next()
            .unwrap_or('•')
            .to_uppercase()
            .collect::<String>();
        let label = gtk::Label::new(Some(&letter));
        label.add_css_class("tray-initial");
        label.upcast()
    }
}

fn connect_secondary(button: &gtk::Button, id: u32, on_intent: Rc<dyn Fn(TrayIntent)>) {
    let click = gtk::GestureClick::new();
    click.set_button(3);
    click.connect_pressed(move |_, _, _, _| on_intent(TrayIntent::Secondary(id)));
    button.add_controller(click);
}

/// Stable reusable tray slot and pooled-popup page. All pinning and SNI
/// dispatch stays with the Shell's trusted host, not GTK.
pub struct TrayUi {
    strip: gtk::Box,
    icons: gtk::Box,
    overflow: gtk::Button,
    popup: gtk::Box,
    list: gtk::Box,
    state: RefCell<TrayState>,
    on_intent: Rc<dyn Fn(TrayIntent)>,
    on_toggle: ToggleHandler,
}

impl TrayUi {
    pub fn new(on_intent: impl Fn(TrayIntent) + 'static) -> Self {
        let strip = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        strip.add_css_class("tray-strip");
        let icons = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        strip.append(&icons);
        let overflow = gtk::Button::with_label("⌄");
        overflow.add_css_class("tray-overflow");
        overflow.set_tooltip_text(Some("Скрытые значки трея"));
        overflow.update_property(&[gtk::accessible::Property::Label("Открыть трей")]);
        strip.append(&overflow);

        let popup = gtk::Box::new(gtk::Orientation::Vertical, 10);
        popup.add_css_class("tray-widget");
        let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let title = gtk::Label::new(Some("Скрытые значки"));
        title.add_css_class("section-title");
        title.set_xalign(0.0);
        title.set_hexpand(true);
        header.append(&title);
        let close = gtk::Button::with_label("×");
        close.update_property(&[gtk::accessible::Property::Label("Закрыть трей")]);
        header.append(&close);
        popup.append(&header);
        let scroll = gtk::ScrolledWindow::new();
        scroll.set_max_content_height(360);
        scroll.set_propagate_natural_height(true);
        scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        let list = gtk::Box::new(gtk::Orientation::Vertical, 4);
        list.add_css_class("tray-list");
        scroll.set_child(Some(&list));
        popup.append(&scroll);

        let on_toggle: ToggleHandler = Rc::new(RefCell::new(None));
        for button in [&overflow, &close] {
            let on_toggle = on_toggle.clone();
            button.connect_clicked(move |_| {
                if let Some(toggle) = on_toggle.borrow().as_ref() {
                    toggle();
                }
            });
        }
        let ui = Self {
            strip,
            icons,
            overflow,
            popup,
            list,
            state: RefCell::new(TrayState::default()),
            on_intent: Rc::new(on_intent),
            on_toggle,
        };
        ui.update(&TrayState::default());
        ui
    }

    pub fn set_toggle_handler(&self, on_toggle: impl Fn() + 'static) {
        *self.on_toggle.borrow_mut() = Some(Box::new(on_toggle));
    }

    pub fn bar_widget(&self) -> gtk::Widget {
        self.strip.clone().upcast()
    }

    pub fn popup_widget(&self) -> gtk::Widget {
        self.popup.clone().upcast()
    }

    pub fn update(&self, state: &TrayState) {
        if *self.state.borrow() == *state && self.icons.first_child().is_some() {
            return;
        }
        *self.state.borrow_mut() = state.clone();
        while let Some(child) = self.icons.first_child() {
            self.icons.remove(&child);
        }
        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }

        let view = tray_bar_model(state, MAX_BAR_ICONS);
        for item in &view.visible {
            let button = gtk::Button::new();
            button.add_css_class("tray-item-button");
            if item.status == TrayStatus::NeedsAttention {
                button.add_css_class("needs-attention");
            }
            button.set_child(Some(&icon(item, 20)));
            button.set_tooltip_text(Some(&item.title));
            button.update_property(&[gtk::accessible::Property::Label(&format!(
                "Открыть {}{}",
                item.title,
                if item.status == TrayStatus::NeedsAttention {
                    ", требует внимания"
                } else {
                    ""
                }
            ))]);
            let id = item.handle;
            let on_intent = self.on_intent.clone();
            button.connect_clicked(move |_| on_intent(TrayIntent::Activate(id)));
            connect_secondary(&button, id, self.on_intent.clone());
            self.icons.append(&button);
        }
        self.overflow
            .set_tooltip_text(Some(&format!("Скрытых значков: {}", view.overflow_count)));
        self.overflow
            .update_property(&[gtk::accessible::Property::Label(&format!(
                "Открыть трей, скрытых значков: {}",
                view.overflow_count
            ))]);
        if view.overflow_attention {
            self.overflow.add_css_class("needs-attention");
        } else {
            self.overflow.remove_css_class("needs-attention");
        }

        let mut visible = state
            .items
            .iter()
            .filter(|item| item.handle != 0 && item.status != TrayStatus::Passive)
            .take(MAX_TRAY_ITEMS)
            .cloned()
            .collect::<Vec<_>>();
        visible.sort_by_key(|item| item.pinned);
        if visible.is_empty() {
            let status = if state.available {
                "НЕТ ПРИЛОЖЕНИЙ В ТРЕЕ"
            } else {
                "СИСТЕМНЫЙ ТРЕЙ НЕДОСТУПЕН"
            };
            let empty = gtk::Label::new(Some(status));
            empty.add_css_class("muted");
            empty.set_xalign(0.0);
            self.list.append(&empty);
        }
        for item in visible {
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            row.add_css_class("tray-row");
            let activate = gtk::Button::new();
            activate.add_css_class("tray-row-activate");
            activate.set_hexpand(true);
            let row_content = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            row_content.append(&icon(&item, 24));
            let title = gtk::Label::new(Some(&item.title));
            title.set_xalign(0.0);
            title.set_hexpand(true);
            title.set_ellipsize(gtk::pango::EllipsizeMode::End);
            row_content.append(&title);
            if item.status == TrayStatus::NeedsAttention {
                let attention = gtk::Label::new(Some("•"));
                attention.add_css_class("tray-attention");
                row_content.append(&attention);
            }
            activate.set_child(Some(&row_content));
            let id = item.handle;
            let on_intent = self.on_intent.clone();
            activate.connect_clicked(move |_| on_intent(TrayIntent::Activate(id)));
            connect_secondary(&activate, id, self.on_intent.clone());
            row.append(&activate);
            // Pin controls are intents, not optimistic GtkToggleButtons.
            // The source state changes only after trusted durable persistence.
            let pin = gtk::Button::with_label(if item.pinned { "★" } else { "☆" });
            pin.add_css_class("tray-pin");
            if item.pinned {
                pin.add_css_class("pinned");
            }
            pin.update_property(&[gtk::accessible::Property::Label(&format!(
                "{} {}",
                if item.pinned {
                    "Открепить"
                } else {
                    "Закрепить"
                },
                item.title
            ))]);
            pin.set_tooltip_text(Some(if item.pinned {
                "Убрать в трей"
            } else {
                "Закрепить в Top Bar"
            }));
            let on_intent = self.on_intent.clone();
            pin.connect_clicked(move |_| on_intent(TrayIntent::TogglePin(id)));
            row.append(&pin);
            self.list.append(&row);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: u32, status: TrayStatus, pinned: bool) -> TrayItem {
        TrayItem {
            handle: id,
            title: format!("App {id}"),
            icon_name: None,
            status,
            pinned,
        }
    }

    #[test]
    fn real_live_items_split_into_pinned_and_overflow() {
        let state = TrayState {
            available: true,
            items: vec![
                item(1, TrayStatus::Active, true),
                item(2, TrayStatus::Passive, true),
                item(3, TrayStatus::NeedsAttention, false),
                item(4, TrayStatus::Active, true),
            ],
        };
        let model = tray_bar_model(&state, 1);
        assert_eq!(
            model.visible.iter().map(|x| x.handle).collect::<Vec<_>>(),
            vec![1]
        );
        assert_eq!(model.overflow_count, 2);
        assert!(model.overflow_attention);
        assert_eq!(tray_bar_model(&state, 5).overflow_count, 1);
    }

    #[test]
    fn empty_or_passive_items_never_claim_running_icons() {
        let state = TrayState {
            available: false,
            items: vec![
                item(0, TrayStatus::Active, true),
                item(5, TrayStatus::Passive, true),
            ],
        };
        let model = tray_bar_model(&state, MAX_BAR_ICONS);
        assert!(model.visible.is_empty());
        assert_eq!(model.overflow_count, 0);
        assert!(!model.overflow_attention);
    }
}
