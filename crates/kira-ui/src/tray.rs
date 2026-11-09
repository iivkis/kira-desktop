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

#[derive(Clone)]
struct MenuFocus {
    id: i32,
    button: gtk::Button,
    // GTK BoxLayout anchors allocate child popovers when the menu moves.
    anchor: gtk::Widget,
    submenu: bool,
    depth: usize,
}

/// The key controller also runs on each GDK popover, because native Wayland
/// submenus have their own keyboard surface and may not bubble to the pool.
fn install_menu_keys(
    widget: &gtk::Widget,
    menu_open: Rc<RefCell<bool>>,
    actions: Rc<RefCell<Vec<MenuFocus>>>,
    on_intent: Rc<dyn Fn(TrayIntent)>,
) {
    let key_controller = gtk::EventControllerKey::new();
    key_controller.connect_key_pressed(move |_, key, _, _| {
        if !*menu_open.borrow() {
            return gtk::glib::Propagation::Proceed;
        }
        if key == gtk::gdk::Key::Escape || key == gtk::gdk::Key::Left {
            on_intent(TrayIntent::MenuBack);
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
            let current = actions.iter().find(|entry| entry.button.has_focus());
            let depth = current.map_or_else(
                || actions.iter().map(|entry| entry.depth).max().unwrap_or(0),
                |entry| entry.depth,
            );
            let enabled = actions
                .iter()
                .filter(|entry| entry.depth == depth && entry.button.is_sensitive())
                .map(|entry| entry.id)
                .collect::<Vec<_>>();
            let selected = menu_focus_target(&enabled, current.map(|entry| entry.id), direction);
            selected.and_then(|id| {
                actions
                    .iter()
                    .find(|entry| entry.id == id && entry.depth == depth)
                    .map(|entry| entry.button.clone())
            })
        };
        if let Some(button) = target {
            button.grab_focus();
            gtk::glib::Propagation::Stop
        } else {
            gtk::glib::Propagation::Proceed
        }
    });
    widget.add_controller(key_controller);
}

/// Build one menu level into an existing GTK container. The numeric handle
/// is all the UI knows; the Shell performs the owner-scoped D-Bus validation.
fn append_menu_rows(
    list: &gtk::Box,
    menu: &TrayMenu,
    entries: &[TrayMenuEntry],
    depth: usize,
    on_intent: &Rc<dyn Fn(TrayIntent)>,
) -> Vec<MenuFocus> {
    let mut focus = Vec::new();
    for entry in entries.iter().take(80) {
        if entry.separator {
            list.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
            continue;
        }
        let label = if entry.has_children {
            format!("{} ▸", entry.label)
        } else {
            format!(
                "{}{}",
                if entry.checked {
                    if entry.radio { "● " } else { "✓ " }
                } else {
                    ""
                },
                entry.label
            )
        };
        let button = gtk::Button::with_label(&label);
        button.set_halign(gtk::Align::Fill);
        button.set_hexpand(true);
        button.set_sensitive(entry.enabled);
        button.add_css_class(if entry.has_children {
            "tray-menu-group"
        } else {
            "tray-menu-entry"
        });
        button.update_property(&[gtk::accessible::Property::Label(&format!(
            "{}{}",
            entry.label,
            if entry.checked {
                ", выбрано"
            } else {
                ""
            }
        ))]);
        if entry.has_children {
            // BoxLayout supports anchoring live GTK popovers and allocating
            // their positions, unlike an arbitrary GtkButton with no layout.
            let anchor = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            anchor.append(&button);
            list.append(&anchor);
            let handle = menu.handle;
            let id = entry.id;
            let callback = on_intent.clone();
            let epoch = menu.epoch;
            button.connect_clicked(move |_| callback(TrayIntent::MenuExpand(handle, id, epoch)));
            focus.push(MenuFocus {
                id,
                button,
                anchor: anchor.upcast(),
                submenu: true,
                depth,
            });
        } else {
            let handle = menu.handle;
            let id = entry.id;
            let callback = on_intent.clone();
            let epoch = menu.epoch;
            button.connect_clicked(move |_| callback(TrayIntent::MenuClick(handle, id, epoch)));
            focus.push(MenuFocus {
                id,
                anchor: button.clone().upcast(),
                button,
                submenu: false,
                depth,
            });
        }
    }
    if entries.is_empty() {
        let empty = gtk::Label::new(Some("В этом разделе нет доступных действий"));
        empty.add_css_class("muted");
        list.append(&empty);
    }
    focus
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
    /// The third field is a Shell-issued revision of the exact painted menu.
    /// Queued callbacks from removed GTK controls cannot be reused against
    /// another D-Bus owner, menu, or LayoutUpdated tree.
    MenuClick(u32, i32, u64),
    MenuExpand(u32, i32, u64),
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
    /// Changed by the trusted Shell on every published menu snapshot.
    pub epoch: u64,
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

/// All visible cascade levels, including their ancestor pages, come from
/// the same bounded owner-validated preorder snapshot.
pub fn menu_cascade_pages(
    items: &[TrayMenuEntry],
    navigation: &[i32],
) -> Option<Vec<Vec<TrayMenuEntry>>> {
    if navigation.len() > 3 || items.len() > 80 {
        return None;
    }
    (0..=navigation.len())
        .map(|depth| menu_page(items, &navigation[..depth]))
        .collect()
}

/// A parent on any visible ancestor can switch a sibling branch. Invalid,
/// duplicate, disabled or hidden IDs never create a trusted navigation path.
pub fn menu_expansion_path(
    items: &[TrayMenuEntry],
    navigation: &[i32],
    id: i32,
) -> Option<Vec<i32>> {
    let pages = menu_cascade_pages(items, navigation)?;
    let matched = items
        .iter()
        .filter(|entry| entry.id == id)
        .collect::<Vec<_>>();
    if matched.len() != 1 {
        return None;
    }
    let entry = matched[0];
    let depth = usize::from(entry.depth);
    if id <= 0
        || depth >= 3
        || depth > navigation.len()
        || !entry.enabled
        || entry.separator
        || !entry.has_children
        || !pages[depth].iter().any(|visible| visible.id == id)
    {
        return None;
    }
    let mut path = navigation[..depth].to_vec();
    path.push(id);
    menu_cascade_pages(items, &path)?;
    Some(path)
}

/// Leaf actions in the visible root or ancestor pages stay usable when a
/// child cascade is open, without exposing invisible sibling branches.
pub fn menu_visible_leaf(items: &[TrayMenuEntry], navigation: &[i32], id: i32) -> bool {
    let Some(pages) = menu_cascade_pages(items, navigation) else {
        return false;
    };
    if id <= 0 || items.iter().filter(|entry| entry.id == id).count() != 1 {
        return false;
    }
    pages.iter().any(|page| {
        page.iter()
            .any(|entry| entry.id == id && entry.enabled && !entry.separator && !entry.has_children)
    })
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
    // Child GDK popovers are attached only to real, enabled parent buttons.
    // This is a presentation stack, not another SNI client/controller.
    menu_popovers: Rc<RefCell<Vec<gtk::Popover>>>,
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
        install_menu_keys(
            &popup.clone().upcast(),
            keyboard_menu.clone(),
            menu_focus.clone(),
            on_intent.clone(),
        );
        let menu_popovers = Rc::new(RefCell::new(Vec::<gtk::Popover>::new()));
        let weak_popovers = Rc::downgrade(&menu_popovers);
        popup.connect_map(move |_| {
            if let Some(popovers) = weak_popovers.upgrade()
                && let Some(first) = popovers.borrow().first()
            {
                first.popup();
            }
        });
        let dismiss = on_intent.clone();
        back.connect_clicked(move |_| dismiss(TrayIntent::DismissMenu));

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
            menu_popovers,
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

    /// Root menu controls stay in the retained GTK Tray tree. Each active
    /// validated descendant is displayed in a native side-anchored popover,
    /// not by replacing its parent page or creating another D-Bus client.
    pub fn set_menu(&self, menu: Option<TrayMenu>) {
        if *self.current_menu.borrow() == menu {
            return;
        }
        let previous_focus = self
            .menu_focus
            .borrow()
            .iter()
            .find(|entry| entry.button.has_focus())
            .map(|entry| (entry.id, entry.depth));
        // Descendants are unparented deepest-first before any owner/menu
        // refresh destroys their anchor buttons. This immediately drops
        // stale Wayland pointer targets and keyboard action controls.
        for popover in self.menu_popovers.borrow_mut().drain(..).rev() {
            popover.popdown();
            if popover.parent().is_some() {
                popover.unparent();
            }
        }
        self.menu_focus.borrow_mut().clear();
        while let Some(child) = self.menu_list.first_child() {
            self.menu_list.remove(&child);
        }
        *self.current_menu.borrow_mut() = menu.clone();
        *self.keyboard_menu.borrow_mut() = menu.is_some();
        self.menu_page.set_visible(menu.is_some());
        self.list_scroll.set_visible(menu.is_none());
        let Some(menu) = menu else {
            self.header_title.set_text("Скрытые значки");
            return;
        };
        self.header_title.set_text(&menu.title);

        let Some(pages) = menu_cascade_pages(&menu.items, &menu.navigation) else {
            let status = gtk::Label::new(Some("Меню обновилось: действия недоступны"));
            status.add_css_class("muted");
            self.menu_list.append(&status);
            return;
        };
        let mut previous_rows =
            append_menu_rows(&self.menu_list, &menu, &pages[0], 0, &self.on_intent);
        let mut all_rows = previous_rows.clone();
        for (depth, entries) in pages.iter().enumerate().skip(1) {
            let Some(anchor) = previous_rows
                .iter()
                .find(|row| row.id == menu.navigation[depth - 1] && row.submenu)
                .map(|row| row.anchor.clone())
            else {
                break; // Lost validated GTK anchor, never display orphan actions.
            };

            let container = gtk::Box::new(gtk::Orientation::Vertical, 6);
            container.add_css_class("tray-cascade-content");
            let heading = gtk::Button::with_label(&format!(
                "‹ {}",
                menu.items
                    .iter()
                    .find(|entry| entry.id == menu.navigation[depth - 1])
                    .map(|entry| entry.label.as_str())
                    .unwrap_or("Назад")
            ));
            heading.add_css_class("tray-menu-back");
            heading.update_property(&[gtk::accessible::Property::Label(
                "Назад к родительскому меню",
            )]);
            let callback = self.on_intent.clone();
            heading.connect_clicked(move |_| callback(TrayIntent::MenuBack));
            container.append(&heading);
            let scroll = gtk::ScrolledWindow::new();
            scroll.set_size_request(220, -1);
            scroll.set_max_content_height(400);
            scroll.set_propagate_natural_height(true);
            scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
            let list = gtk::Box::new(gtk::Orientation::Vertical, 2);
            list.add_css_class("tray-menu-list");
            let rows = append_menu_rows(&list, &menu, entries, depth, &self.on_intent);
            scroll.set_child(Some(&list));
            container.append(&scroll);

            let popover = gtk::Popover::new();
            popover.add_css_class("tray-cascade-popover");
            popover.set_position(gtk::PositionType::Right);
            popover.set_has_arrow(false);
            // Non-modal: ancestors must remain interactive for real sibling
            // changes; explicit key handlers own Left/Escape transitions.
            popover.set_autohide(false);
            popover.set_cascade_popdown(false);
            popover.set_child(Some(&container));
            popover.set_parent(&anchor);
            install_menu_keys(
                &popover.clone().upcast(),
                self.keyboard_menu.clone(),
                self.menu_focus.clone(),
                self.on_intent.clone(),
            );
            self.menu_popovers.borrow_mut().push(popover);
            all_rows.extend(rows.iter().cloned());
            previous_rows = rows;
        }
        *self.menu_focus.borrow_mut() = all_rows;

        // Every nested popover is opened only after its own parent maps.
        // GTK will flip the preferred Right placement near an output edge.
        // Detach/reattach also remaps the retained root, reopening the path.
        let popovers = self.menu_popovers.borrow().clone();
        for pair in popovers.windows(2) {
            let next = pair[1].downgrade();
            pair[0].connect_map(move |_| {
                if let Some(next) = next.upgrade() {
                    next.popup();
                }
            });
        }
        if self.popup.is_mapped()
            && let Some(first) = popovers.first()
        {
            first.popup();
        }

        // A live LayoutUpdated keeps the same enabled ID in the deepest
        // visible page; a disappearing ID selects its first enabled peer.
        let depth = menu.navigation.len();
        let focus = {
            let actions = self.menu_focus.borrow();
            let enabled = actions
                .iter()
                .filter(|entry| entry.depth == depth && entry.button.is_sensitive())
                .map(|entry| entry.id)
                .collect::<Vec<_>>();
            let previous = previous_focus
                .filter(|(_, page)| *page == depth)
                .map(|(id, _)| id);
            let chosen = if previous.is_some_and(|id| enabled.contains(&id)) {
                previous
            } else {
                menu_focus_target(&enabled, None, MenuNavigation::First)
            };
            chosen.and_then(|id| {
                actions
                    .iter()
                    .find(|entry| entry.id == id && entry.depth == depth)
                    .map(|entry| entry.button.clone())
            })
        };
        if let Some(button) = focus {
            let root = self.popup.clone();
            let activate = move || {
                if root.is_mapped()
                    && button.is_mapped()
                    && button.is_sensitive()
                    && root
                        .root()
                        .and_then(|root| root.downcast::<gtk::Window>().ok())
                        .is_some_and(|window| window.is_active())
                {
                    button.grab_focus();
                }
            };
            if let Some(last) = popovers.last() {
                last.connect_map(move |_| activate());
            } else {
                gtk::glib::idle_add_local_once(activate);
            }
        }
    }

    pub fn menu_open(&self) -> bool {
        self.current_menu.borrow().is_some()
    }

    pub fn update(&self, state: &TrayState) {
        if *self.state.borrow() == *state && self.rendered_budget.get() == self.bar_budget.get() {
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
    fn menu_intents_carry_exact_painted_revision() {
        assert_ne!(
            TrayIntent::MenuClick(7, 21, 100),
            TrayIntent::MenuClick(7, 21, 101)
        );
        assert_ne!(
            TrayIntent::MenuExpand(7, 20, 101),
            TrayIntent::MenuExpand(7, 20, 102)
        );
        // A handle and a numeric ID are insufficient to identify a live
        // remote D-Bus action. The epoch is not a UI-generated owner name.
        let menu = TrayMenu {
            handle: 7,
            epoch: 102,
            title: "App".into(),
            items: vec![entry(21, 0, false)],
            navigation: Vec::new(),
        };
        assert_eq!(menu.epoch, 102);
    }

    #[test]
    fn cascade_pages_retain_ancestors_and_refuse_orphan_paths() {
        let entries = vec![
            entry(10, 0, true),
            entry(11, 1, false),
            entry(12, 1, true),
            entry(13, 2, false),
            entry(20, 0, true),
            entry(21, 1, false),
        ];
        let pages = menu_cascade_pages(&entries, &[10, 12]).unwrap();
        assert_eq!(pages.len(), 3);
        assert_eq!(
            pages[0].iter().map(|x| x.id).collect::<Vec<_>>(),
            vec![10, 20]
        );
        assert_eq!(
            pages[1].iter().map(|x| x.id).collect::<Vec<_>>(),
            vec![11, 12]
        );
        assert_eq!(pages[2].iter().map(|x| x.id).collect::<Vec<_>>(), vec![13]);
        assert!(menu_cascade_pages(&entries, &[20, 12]).is_none());
        assert!(menu_cascade_pages(&entries, &[10, 12, 13, 14]).is_none());
        assert!(menu_cascade_pages(&vec![entry(1, 0, false); 81], &[]).is_none());
    }

    #[test]
    fn visible_ancestor_clicks_switch_branches_without_hidden_dispatch() {
        let mut entries = vec![
            entry(10, 0, true),
            entry(11, 1, false),
            entry(12, 1, true),
            entry(13, 2, false),
            entry(20, 0, true),
            entry(21, 1, false),
            entry(30, 0, false),
        ];
        assert_eq!(menu_expansion_path(&entries, &[10, 12], 20), Some(vec![20]));
        assert_eq!(menu_expansion_path(&entries, &[10, 12], 10), Some(vec![10]));
        assert_eq!(
            menu_expansion_path(&entries, &[10, 12], 12),
            Some(vec![10, 12])
        );
        assert_eq!(menu_expansion_path(&entries, &[10, 12], 21), None);
        assert_eq!(menu_expansion_path(&entries, &[20], 12), None);
        assert!(menu_visible_leaf(&entries, &[10, 12], 11));
        assert!(menu_visible_leaf(&entries, &[10, 12], 13));
        assert!(menu_visible_leaf(&entries, &[10, 12], 30));
        assert!(!menu_visible_leaf(&entries, &[10, 12], 21));
        assert!(!menu_visible_leaf(&entries, &[10, 12], 999));
        entries.push(entry(11, 0, false));
        assert!(!menu_visible_leaf(&entries, &[10, 12], 11));
        assert_eq!(menu_expansion_path(&entries, &[10, 12], 20), Some(vec![20]));
    }

    #[test]
    fn live_layout_change_revokes_disabled_or_disappeared_ancestors() {
        let mut entries = vec![entry(10, 0, true), entry(12, 1, true), entry(13, 2, false)];
        assert!(menu_visible_leaf(&entries, &[10, 12], 13));
        entries[1].enabled = false;
        assert!(!menu_visible_leaf(&entries, &[10, 12], 13));
        assert!(menu_expansion_path(&entries, &[10, 12], 12).is_none());
        entries.remove(0);
        assert!(menu_cascade_pages(&entries, &[10, 12]).is_none());
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
        for (width, budget) in [
            (360, 2),
            (699, 2),
            (700, 4),
            (1099, 4),
            (1100, 6),
            (1920, 6),
        ] {
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
