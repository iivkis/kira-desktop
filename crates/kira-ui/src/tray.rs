//! Reusable live StatusNotifier tray presentation. The UI consumes typed
//! item snapshots and emits bounded numeric handle intents only: it never
//! constructs D-Bus paths, commands, URLs or application menu calls.
use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;

type ToggleHandler = Rc<RefCell<Option<Box<dyn Fn()>>>>;

pub const MAX_BAR_ICONS: usize = 4;
pub const MAX_TRAY_ITEMS: usize = 64;

/// SNI sends unpremultiplied ARGB32 pixels in network byte order. Keep
/// the exact decoded real client bytes in a bounded, GTK-independent model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrayPixmap {
    pub width: i32,
    pub height: i32,
    pub rgba: Vec<u8>,
}

impl TrayPixmap {
    pub fn from_argb(width: i32, height: i32, argb: &[u8]) -> Option<Self> {
        if !(1..=128).contains(&width) || !(1..=128).contains(&height) {
            return None;
        }
        let expected = usize::try_from(width.checked_mul(height)?.checked_mul(4)?).ok()?;
        if argb.len() != expected {
            return None;
        }
        let mut rgba = Vec::with_capacity(expected);
        for pixel in argb.chunks_exact(4) {
            rgba.extend_from_slice(&[pixel[1], pixel[2], pixel[3], pixel[0]]);
        }
        Some(Self {
            width,
            height,
            rgba,
        })
    }
}


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
    pub pixmap: Option<TrayPixmap>,
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
    MenuClick(u32, i32),
    DismissMenu,
}

/// Bounded menu descriptions only. The Shell retains the real D-Bus
/// destination and validates all clicks against its live menu session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrayMenuEntry {
    pub id: i32,
    pub label: String,
    pub depth: u8,
    pub enabled: bool,
    pub separator: bool,
    pub checked: bool,
    pub radio: bool,
    pub has_children: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrayMenu {
    pub handle: u32,
    pub title: String,
    pub items: Vec<TrayMenuEntry>,
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
    // A syntactically valid IconName may not exist in the current theme.
    // Choose a real client pixmap before GTK's fake "image-missing" glyph.
    if let Some(name) = item.icon_name.as_deref()
        && let Some(display) = gtk::gdk::Display::default()
        && gtk::IconTheme::for_display(&display).has_icon(name)
    {
        let image = gtk::Image::from_icon_name(name);
        image.set_pixel_size(size);
        return image.upcast();
    }
    if let Some(pixmap) = &item.pixmap {
        let texture = gtk::gdk::MemoryTexture::new(
            pixmap.width,
            pixmap.height,
            gtk::gdk::MemoryFormat::R8g8b8a8,
            &gtk::glib::Bytes::from_owned(pixmap.rgba.clone()),
            usize::try_from(pixmap.width).unwrap_or(0) * 4,
        );
        let picture = gtk::Picture::for_paintable(&texture);
        picture.set_content_fit(gtk::ContentFit::Contain);
        picture.set_size_request(size, size);
        return picture.upcast();
    }
    {
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
    header_title: gtk::Label,
    list_scroll: gtk::ScrolledWindow,
    list: gtk::Box,
    menu_page: gtk::Box,
    menu_list: gtk::Box,
    current_menu: RefCell<Option<TrayMenu>>,
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

        let menu_page = gtk::Box::new(gtk::Orientation::Vertical, 8);
        menu_page.add_css_class("tray-menu-page");
        menu_page.set_visible(false);
        let back = gtk::Button::with_label("‹ Назад");
        back.add_css_class("tray-menu-back");
        back.update_property(&[gtk::accessible::Property::Label("Назад к значкам трея")]);
        menu_page.append(&back);
        let menu_scroll = gtk::ScrolledWindow::new();
        menu_scroll.set_max_content_height(420);
        menu_scroll.set_propagate_natural_height(true);
        menu_scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        let menu_list = gtk::Box::new(gtk::Orientation::Vertical, 2);
        menu_list.add_css_class("tray-menu-list");
        menu_scroll.set_child(Some(&menu_list));
        menu_page.append(&menu_scroll);
        popup.append(&menu_page);

        let on_intent: Rc<dyn Fn(TrayIntent)> = Rc::new(on_intent);
        let dismiss = on_intent.clone();
        back.connect_clicked(move |_| dismiss(TrayIntent::DismissMenu));

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
            header_title: title,
            list_scroll: scroll,
            list,
            menu_page,
            menu_list,
            current_menu: RefCell::new(None),
            state: RefCell::new(TrayState::default()),
            on_intent,
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

    /// A native menu is a separate state in the same pooled GTK popup, never
    /// a second Wayland surface. Entries are always plain text, never markup.
    pub fn set_menu(&self, menu: Option<TrayMenu>) {
        if *self.current_menu.borrow() == menu {
            return;
        }
        *self.current_menu.borrow_mut() = menu.clone();
        while let Some(child) = self.menu_list.first_child() {
            self.menu_list.remove(&child);
        }
        self.menu_page.set_visible(menu.is_some());
        self.list_scroll.set_visible(menu.is_none());
        let Some(menu) = menu else {
            self.header_title.set_text("Скрытые значки");
            return;
        };
        self.header_title.set_text("Меню приложения");
        for entry in menu.items.iter().take(80) {
            if entry.separator {
                let line = gtk::Separator::new(gtk::Orientation::Horizontal);
                self.menu_list.append(&line);
                continue;
            }
            if entry.has_children {
                let group = gtk::Label::new(Some(&entry.label));
                group.set_xalign(0.0);
                group.set_margin_start(i32::from(entry.depth) * 12 + 8);
                group.add_css_class("tray-menu-group");
                self.menu_list.append(&group);
                continue;
            }
            let button = gtk::Button::with_label(&format!(
                "{}{}",
                if entry.checked {
                    if entry.radio { "● " } else { "✓ " }
                } else {
                    ""
                },
                entry.label
            ));
            button.set_halign(gtk::Align::Fill);
            button.set_hexpand(true);
            button.set_sensitive(entry.enabled);
            button.set_margin_start(i32::from(entry.depth) * 12 + 8);
            button.add_css_class("tray-menu-entry");
            let handle = menu.handle;
            let id = entry.id;
            let on_intent = self.on_intent.clone();
            button.connect_clicked(move |_| on_intent(TrayIntent::MenuClick(handle, id)));
            self.menu_list.append(&button);
        }
        if menu.items.is_empty() {
            let empty = gtk::Label::new(Some("У приложения пока нет доступных действий"));
            empty.add_css_class("muted");
            self.menu_list.append(&empty);
        }
    }

    pub fn menu_open(&self) -> bool {
        self.current_menu.borrow().is_some()
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
            pixmap: None,
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
    fn argb32_pixels_are_unpremultiplied_and_bounded() {
        let pix = TrayPixmap::from_argb(2, 1, &[255, 1, 2, 3, 128, 8, 16, 32])
            .expect("real SNI pixel data");
        assert_eq!(pix.rgba, [1, 2, 3, 255, 8, 16, 32, 128]);
        assert!(TrayPixmap::from_argb(0, 1, &[0; 4]).is_none());
        assert!(TrayPixmap::from_argb(129, 1, &[0; 516]).is_none());
        assert!(TrayPixmap::from_argb(1, 1, &[0; 3]).is_none());
        assert!(TrayPixmap::from_argb(1, 1, &[0; 5]).is_none());
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
