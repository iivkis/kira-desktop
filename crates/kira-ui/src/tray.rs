//! Reusable live StatusNotifier tray presentation. The UI consumes typed
//! item snapshots and emits bounded numeric handle intents only: it never
//! constructs D-Bus paths, commands, URLs or application menu calls.
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use crate::reflow::{ReflowRow, ReflowScope};

use gtk::prelude::*;

type ToggleHandler = Rc<RefCell<Option<Box<dyn Fn()>>>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuNavigation {
    Previous,
    Next,
    First,
    Last,
}

/// Keyboard traversal is deliberately limited to live, enabled menu controls.
/// The actual D-Bus invocation still goes through validated Shell intents.
pub fn menu_focus_target(
    enabled: &[i32],
    current: Option<i32>,
    direction: MenuNavigation,
) -> Option<i32> {
    if enabled.is_empty() {
        return None;
    }
    let index = current.and_then(|id| enabled.iter().position(|entry| *entry == id));
    let last = enabled.len() - 1;
    let target = match direction {
        MenuNavigation::First => 0,
        MenuNavigation::Last => last,
        MenuNavigation::Next => index.map_or(0, |i| (i + 1) % enabled.len()),
        MenuNavigation::Previous => index.map_or(last, |i| (i + last) % enabled.len()),
    };
    enabled.get(target).copied()
}

struct MenuFocus {
    id: i32,
    button: gtk::Button,
    submenu: bool,
}

pub const MAX_BAR_ICONS: usize = 6;

/// Match the frozen main TrayBar responsive policy, while keeping
/// unmeasured/unmapped GTK surfaces at the four-icon initial budget.
pub fn tray_budget_for_width(width: i32) -> usize {
    if width < 700 {
        2
    } else if width < 1100 {
        4
    } else {
        6
    }
}
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
        for pixel in argb.as_chunks::<4>().0 {
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
    MenuExpand(u32, i32),
    MenuBack,
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
    /// Path of validated unique parent IDs within the same D-Bus menu.
    pub navigation: Vec<i32>,
}

/// Project a bounded preorder menu tree into exactly one native page.
/// The root has depth 0; each subsequent path component must be a unique,
/// enabled direct child of the preceding parent. A lazy but still empty
/// submenu is valid and yields an empty page until the service refreshes it.
pub fn menu_page(items: &[TrayMenuEntry], navigation: &[i32]) -> Option<Vec<TrayMenuEntry>> {
    let mut start = 0;
    let mut end = items.len();
    let mut depth = 0u8;
    for parent_id in navigation {
        if *parent_id <= 0 {
            return None;
        }
        let mut parent_index = None;
        for (index, entry) in items.iter().enumerate().take(end).skip(start) {
            if entry.depth == depth && entry.id == *parent_id {
                if parent_index.is_some()
                    || !entry.has_children
                    || !entry.enabled
                    || entry.separator
                {
                    return None;
                }
                parent_index = Some(index);
            }
        }
        // A duplicate ID, even in a separate branch, cannot become a
        // privileged destination through an apparently unique local page.
        if items.iter().filter(|entry| entry.id == *parent_id).count() != 1 {
            return None;
        }
        let position = parent_index?;
        start = position + 1;
        end = (start..end)
            .find(|&index| items[index].depth <= depth)
            .unwrap_or(end);
        depth = depth.checked_add(1)?;
        if depth > 3 {
            return None;
        }
    }
    Some(
        items[start..end]
            .iter()
            .filter(|entry| entry.depth == depth)
            .cloned()
            .collect(),
    )
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

/// Retain GTK row identity across live SNI updates. Reordering pinned
/// items must not destroy a row's widgets or restart its event controllers.
struct TrayLiveRow {
    widget: ReflowRow,
    title: gtk::Label,
    open: gtk::Button,
    icon_host: gtk::Box,
    attention: gtk::Label,
    pin: gtk::Button,
    snapshot: TrayItem,
}

impl TrayLiveRow {
    fn new(item: &TrayItem, on_intent: Rc<dyn Fn(TrayIntent)>) -> Self {
        let root = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        root.add_css_class("tray-row");
        let open = gtk::Button::new();
        open.add_css_class("tray-row-activate");
        open.set_hexpand(true);
        let content = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let icon_host = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        content.append(&icon_host);
        let title = gtk::Label::new(None);
        title.set_xalign(0.0);
        title.set_hexpand(true);
        title.set_ellipsize(gtk::pango::EllipsizeMode::End);
        content.append(&title);
        let attention = gtk::Label::new(Some("•"));
        attention.add_css_class("tray-attention");
        content.append(&attention);
        open.set_child(Some(&content));
        let handle = item.handle;
        let callback = on_intent.clone();
        open.connect_clicked(move |_| callback(TrayIntent::Activate(handle)));
        connect_secondary(&open, handle, on_intent.clone());
        root.append(&open);
        let pin = gtk::Button::new();
        pin.add_css_class("tray-pin");
        let callback = on_intent.clone();
        pin.connect_clicked(move |_| callback(TrayIntent::TogglePin(handle)));
        root.append(&pin);
        let widget = ReflowRow::new(&root.upcast());
        widget.set_hexpand(true);
        let mut row = Self {
            widget,
            title,
            open,
            icon_host,
            attention,
            pin,
            snapshot: item.clone(),
        };
        row.refresh(item);
        row
    }

    fn refresh(&mut self, item: &TrayItem) {
        if self.snapshot.icon_name != item.icon_name
            || self.snapshot.pixmap != item.pixmap
            || self.icon_host.first_child().is_none()
        {
            while let Some(child) = self.icon_host.first_child() {
                self.icon_host.remove(&child);
            }
            self.icon_host.append(&icon(item, 24));
        }
        self.title.set_text(&item.title);
        self.attention
            .set_visible(item.status == TrayStatus::NeedsAttention);
        self.open.set_tooltip_text(Some(&item.title));
        self.open
            .update_property(&[gtk::accessible::Property::Label(&format!(
                "Открыть {}",
                item.title
            ))]);
        self.pin.set_label(if item.pinned { "★" } else { "☆" });
        if item.pinned {
            self.pin.add_css_class("pinned");
        } else {
            self.pin.remove_css_class("pinned");
        }
        self.pin
            .update_property(&[gtk::accessible::Property::Label(&format!(
                "{} {}",
                if item.pinned {
                    "Открепить"
                } else {
                    "Закрепить"
                },
                item.title
            ))]);
        self.pin.set_tooltip_text(Some(if item.pinned {
            "Убрать в трей"
        } else {
            "Закрепить в Top Bar"
        }));
        self.snapshot = item.clone();
    }
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
    live_rows: RefCell<HashMap<u32, TrayLiveRow>>,
    reflow: ReflowScope,
    menu_page: gtk::Box,
    menu_list: gtk::Box,
    current_menu: RefCell<Option<TrayMenu>>,
    keyboard_menu: Rc<RefCell<bool>>,
    menu_focus: Rc<RefCell<Vec<MenuFocus>>>,
    state: RefCell<TrayState>,
    on_intent: Rc<dyn Fn(TrayIntent)>,
    on_toggle: ToggleHandler,
    on_close: ToggleHandler,
    bar_budget: Cell<usize>,
    rendered_budget: Cell<usize>,
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
        let keyboard_menu = Rc::new(RefCell::new(false));
        let menu_focus = Rc::new(RefCell::new(Vec::<MenuFocus>::new()));
        let escape = gtk::EventControllerKey::new();
        let menu_open = keyboard_menu.clone();
        let actions = menu_focus.clone();
        let on_escape = on_intent.clone();
        escape.connect_key_pressed(move |_, key, _, _| {
            if !*menu_open.borrow() {
                return gtk::glib::Propagation::Proceed;
            }
            if key == gtk::gdk::Key::Escape || key == gtk::gdk::Key::Left {
                on_escape(TrayIntent::MenuBack);
                return gtk::glib::Propagation::Stop;
            }
            if key == gtk::gdk::Key::Right {
                let submenu = actions
                    .borrow()
                    .iter()
                    .find(|entry| entry.submenu && entry.button.has_focus())
                    .map(|entry| entry.button.clone());
                if let Some(button) = submenu {
                    button.emit_clicked();
                    return gtk::glib::Propagation::Stop;
                }
                return gtk::glib::Propagation::Proceed;
            }
            let direction = match key {
                gtk::gdk::Key::Up => MenuNavigation::Previous,
                gtk::gdk::Key::Down => MenuNavigation::Next,
                gtk::gdk::Key::Home => MenuNavigation::First,
                gtk::gdk::Key::End => MenuNavigation::Last,
                _ => return gtk::glib::Propagation::Proceed,
            };
            let target = {
                let actions = actions.borrow();
                let enabled = actions
                    .iter()
                    .filter(|entry| entry.button.is_sensitive())
                    .map(|entry| entry.id)
                    .collect::<Vec<_>>();
                let current = actions
                    .iter()
                    .find(|entry| entry.button.has_focus())
                    .map(|entry| entry.id);
                let selected = menu_focus_target(&enabled, current, direction);
                let button = selected.and_then(|id| {
                    actions
                        .iter()
                        .find(|entry| entry.id == id)
                        .map(|entry| entry.button.clone())
                });
                button
            };
            if let Some(button) = target {
                button.grab_focus();
                gtk::glib::Propagation::Stop
            } else {
                gtk::glib::Propagation::Proceed
            }
        });
        popup.add_controller(escape);
        let dismiss = on_intent.clone();
        back.connect_clicked(move |_| dismiss(TrayIntent::MenuBack));

        let on_toggle: ToggleHandler = Rc::new(RefCell::new(None));
        let on_close: ToggleHandler = Rc::new(RefCell::new(None));
        {
            let on_toggle = on_toggle.clone();
            overflow.connect_clicked(move |_| {
                if let Some(toggle) = on_toggle.borrow().as_ref() {
                    toggle();
                }
            });
        }
        {
            let on_close = on_close.clone();
            close.connect_clicked(move |_| {
                if let Some(close) = on_close.borrow().as_ref() {
                    close();
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
            reflow: ReflowScope::new(&list.clone().upcast()),
            live_rows: RefCell::new(HashMap::new()),
            list,
            menu_page,
            menu_list,
            current_menu: RefCell::new(None),
            keyboard_menu,
            menu_focus,
            state: RefCell::new(TrayState::default()),
            on_intent,
            on_toggle,
            on_close,
            bar_budget: Cell::new(4),
            rendered_budget: Cell::new(usize::MAX),
        };
        ui.update(&TrayState::default());
        ui
    }

    pub fn set_toggle_handler(&self, on_toggle: impl Fn() + 'static) {
        *self.on_toggle.borrow_mut() = Some(Box::new(on_toggle));
    }

    /// The Tray's own close button dismisses the active presentation.
    /// It must not accidentally raise an independently detached window.
    pub fn set_close_handler(&self, on_close: impl Fn() + 'static) {
        *self.on_close.borrow_mut() = Some(Box::new(on_close));
    }

    /// Reflow the real visible pinned icons only when the mapped bar crosses
    /// the source responsive threshold. Never fabricate overflow identities.
    pub fn set_bar_width(&self, width: i32) {
        if width <= 0 {
            return;
        }
        let next = tray_budget_for_width(width);
        if self.bar_budget.replace(next) != next {
            let snapshot = self.state.borrow().clone();
            self.update(&snapshot);
        }
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
        let previous_focus = self
            .menu_focus
            .borrow()
            .iter()
            .find(|entry| entry.button.has_focus())
            .map(|entry| entry.id);
        *self.current_menu.borrow_mut() = menu.clone();
        *self.keyboard_menu.borrow_mut() = menu.is_some();
        self.menu_focus.borrow_mut().clear();
        while let Some(child) = self.menu_list.first_child() {
            self.menu_list.remove(&child);
        }
        self.menu_page.set_visible(menu.is_some());
        self.list_scroll.set_visible(menu.is_none());
        let Some(menu) = menu else {
            self.header_title.set_text("Скрытые значки");
            return;
        };
        let entries = menu_page(&menu.items, &menu.navigation).unwrap_or_default();
        let heading = menu.navigation.last().and_then(|id| {
            menu.items
                .iter()
                .find(|entry| entry.id == *id)
                .map(|entry| entry.label.as_str())
        });
        self.header_title.set_text(heading.unwrap_or(&menu.title));
        for entry in entries.iter().take(80) {
            if entry.separator {
                let line = gtk::Separator::new(gtk::Orientation::Horizontal);
                self.menu_list.append(&line);
                continue;
            }
            if entry.has_children {
                // AboutToShow(parent_id) lets truly lazy apps populate
                // submenus. The Shell revalidates the unique enabled parent
                // and refreshes the same pooled menu, never a new surface.
                let group = gtk::Button::with_label(&format!("▸ {}", entry.label));
                group.set_halign(gtk::Align::Fill);
                group.set_sensitive(entry.enabled);
                group.set_margin_start(8);
                group.add_css_class("tray-menu-group");
                let handle = menu.handle;
                let id = entry.id;
                let on_intent = self.on_intent.clone();
                group.connect_clicked(move |_| on_intent(TrayIntent::MenuExpand(handle, id)));
                group.update_property(&[gtk::accessible::Property::Label(&format!(
                    "Открыть подменю {}",
                    entry.label,
                ))]);
                self.menu_list.append(&group);
                self.menu_focus.borrow_mut().push(MenuFocus {
                    id,
                    button: group,
                    submenu: true,
                });
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
            button.set_margin_start(8);
            button.add_css_class("tray-menu-entry");
            let handle = menu.handle;
            let id = entry.id;
            let on_intent = self.on_intent.clone();
            button.connect_clicked(move |_| on_intent(TrayIntent::MenuClick(handle, id)));
            button.update_property(&[gtk::accessible::Property::Label(&format!(
                "{}{}",
                entry.label,
                if entry.checked {
                    ", выбрано"
                } else {
                    ""
                },
            ))]);
            self.menu_list.append(&button);
            self.menu_focus.borrow_mut().push(MenuFocus {
                id,
                button,
                submenu: false,
            });
        }
        if entries.is_empty() {
            let empty = gtk::Label::new(Some("В этом разделе нет доступных действий"));
            empty.add_css_class("muted");
            self.menu_list.append(&empty);
        }
        // A live LayoutUpdated can rebuild controls while the user navigates.
        // Return to the same enabled D-Bus ID if it survives, else first
        // available control. Never focus a hidden or different active window.
        let focus = {
            let menu = self.menu_focus.borrow();
            let enabled = menu
                .iter()
                .filter(|entry| entry.button.is_sensitive())
                .map(|entry| entry.id)
                .collect::<Vec<_>>();
            let chosen = if previous_focus.is_some_and(|id| enabled.contains(&id)) {
                previous_focus
            } else {
                menu_focus_target(&enabled, None, MenuNavigation::First)
            };
            chosen.and_then(|id| {
                menu.iter()
                    .find(|entry| entry.id == id)
                    .map(|entry| entry.button.clone())
            })
        };
        if let Some(button) = focus {
            gtk::glib::idle_add_local_once(move || {
                if button.is_mapped()
                    && button.is_sensitive()
                    && button
                        .root()
                        .and_then(|root| root.downcast::<gtk::Window>().ok())
                        .is_some_and(|window| window.is_active())
                {
                    button.grab_focus();
                }
            });
        }
    }

    pub fn menu_open(&self) -> bool {
        self.current_menu.borrow().is_some()
    }

    pub fn update(&self, state: &TrayState) {
        if *self.state.borrow() == *state
            && self.rendered_budget.get() == self.bar_budget.get()
        {
            return;
        }
        *self.state.borrow_mut() = state.clone();
        self.rendered_budget.set(self.bar_budget.get());
        while let Some(child) = self.icons.first_child() {
            self.icons.remove(&child);
        }
        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }

        let view = tray_bar_model(state, self.bar_budget.get());
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

        // Capture the *painted* positions before mutating order. Surviving
        // ReflowRow identities traverse a real 340ms back.out FLIP; hidden
        // popups or reduced motion settle with zero extra GTK timers.
        let previous = self
            .live_rows
            .borrow()
            .iter()
            .map(|(id, live)| (id.to_string(), live.widget.clone()))
            .collect::<Vec<_>>();
        let snapshot = self.reflow.capture(&previous);
        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }
        let mut rows = self.live_rows.borrow_mut();
        let wanted = visible.iter().map(|item| item.handle).collect::<Vec<_>>();
        rows.retain(|id, _| wanted.contains(id));
        let mut current = Vec::new();

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
        for item in &visible {
            let live = rows
                .entry(item.handle)
                .or_insert_with(|| TrayLiveRow::new(item, self.on_intent.clone()));
            live.refresh(item);
            self.list.append(&live.widget);
            current.push((item.handle.to_string(), live.widget.clone()));
        }
        drop(rows);
        self.reflow.play(snapshot, &current);
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

    fn entry(id: i32, depth: u8, has_children: bool) -> TrayMenuEntry {
        TrayMenuEntry {
            id,
            label: format!("Item {id}"),
            depth,
            enabled: true,
            separator: false,
            checked: false,
            radio: false,
            has_children,
        }
    }

    #[test]
    fn keyboard_menu_navigation_is_bounded_and_revisitable() {
        let enabled = [11, 13, 17];
        assert_eq!(
            menu_focus_target(&enabled, None, MenuNavigation::First),
            Some(11)
        );
        assert_eq!(
            menu_focus_target(&enabled, None, MenuNavigation::Previous),
            Some(17)
        );
        assert_eq!(
            menu_focus_target(&enabled, Some(17), MenuNavigation::Next),
            Some(11)
        );
        assert_eq!(
            menu_focus_target(&enabled, Some(11), MenuNavigation::Previous),
            Some(17)
        );
        assert_eq!(
            menu_focus_target(&enabled, Some(11), MenuNavigation::Last),
            Some(17)
        );
        assert_eq!(
            menu_focus_target(&enabled, Some(13), MenuNavigation::Next),
            Some(17)
        );
        assert_eq!(
            menu_focus_target(&enabled, Some(99), MenuNavigation::Next),
            Some(11)
        );
        assert_eq!(menu_focus_target(&[], Some(11), MenuNavigation::Last), None);
    }

    #[test]
    fn native_menu_navigation_displays_only_direct_children() {
        let entries = vec![
            entry(1, 0, true),
            entry(2, 1, false),
            entry(3, 1, true),
            entry(4, 2, false),
            entry(5, 0, false),
            entry(6, 0, true),
        ];
        assert_eq!(
            menu_page(&entries, &[])
                .unwrap()
                .iter()
                .map(|item| item.id)
                .collect::<Vec<_>>(),
            vec![1, 5, 6]
        );
        assert_eq!(
            menu_page(&entries, &[1])
                .unwrap()
                .iter()
                .map(|item| item.id)
                .collect::<Vec<_>>(),
            vec![2, 3]
        );
        assert_eq!(
            menu_page(&entries, &[1, 3])
                .unwrap()
                .iter()
                .map(|item| item.id)
                .collect::<Vec<_>>(),
            vec![4]
        );
        // A lazy parent remains a valid empty menu until AboutToShow.
        assert_eq!(menu_page(&entries, &[6]).unwrap().len(), 0);
        assert!(menu_page(&entries, &[5]).is_none());
        assert!(menu_page(&entries, &[3]).is_none());
        assert!(menu_page(&entries, &[1, 6]).is_none());
        assert!(menu_page(&entries, &[0]).is_none());
    }

    #[test]
    fn duplicate_and_disabled_parent_ids_fail_closed() {
        let mut entries = vec![entry(1, 0, true), entry(2, 1, false), entry(1, 0, true)];
        assert!(menu_page(&entries, &[1]).is_none());
        entries.pop();
        entries[0].enabled = false;
        assert!(menu_page(&entries, &[1]).is_none());
    }

    #[test]
    fn responsive_icon_budget_matches_frozen_main_and_never_invents_items() {
        assert_eq!(MAX_BAR_ICONS, 6);
        for (width, budget) in [(360, 2), (699, 2), (700, 4),
            (1099, 4), (1100, 6), (1920, 6)]
        {
            assert_eq!(tray_budget_for_width(width), budget);
        }
        let state = TrayState {
            available: true,
            items: (1..=8)
                .map(|id| item(id, TrayStatus::Active, true))
                .collect(),
        };
        for (width, visible) in [(650, 2), (960, 4), (1300, 6)] {
            let view = tray_bar_model(&state, tray_budget_for_width(width));
            assert_eq!(view.visible.len(), visible);
            assert_eq!(view.overflow_count, 8 - visible);
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
