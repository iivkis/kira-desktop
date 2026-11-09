//! Launcher content and search semantics ported from main's LauncherWidget
//! and launcher-model.ts. No application discovery or process execution here.
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::{Rc, Weak};

use gtk::prelude::*;

use crate::flow_indicator::{FlowAxis, FlowIndicator, FlowStyle};
use crate::reflow::{ReflowRow, ReflowScope};
use crate::tenet_scroll::TenetScroll;

const MAX_DISPLAYED: usize = 80;
const MAX_ANIMATED_FILTER_ROWS: usize = 20;

/// Main intentionally skips FLIP on broad result sets to keep typing responsive.
pub fn animate_launcher_filter(visible_before: usize) -> bool {
    (1..=MAX_ANIMATED_FILTER_ROWS).contains(&visible_before)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApplicationEntry {
    /// Opaque trusted GIO application ID, never a command.
    pub id: String,
    pub name: String,
    pub description: String,
    /// Serialized GIcon. Resolved only on the GTK main thread.
    pub icon: Option<String>,
}

/// Exact, prefix, word-prefix, substring, name-terms, description-only.
/// Preserves source GIO ordering for equally ranked matches (main contract).
pub fn filter_applications(
    apps: &[ApplicationEntry],
    query: &str,
    limit: usize,
) -> Vec<ApplicationEntry> {
    let terms: Vec<_> = query.split_whitespace().map(str::to_lowercase).collect();
    if terms.is_empty() {
        return apps.iter().take(limit).cloned().collect();
    }
    let joined = terms.join(" ");
    let mut ranked: Vec<_> = apps
        .iter()
        .enumerate()
        .filter_map(|(index, app)| {
            let name = app.name.to_lowercase();
            let haystack = format!("{} {}", name, app.description.to_lowercase());
            if !terms.iter().all(|term| haystack.contains(term)) {
                return None;
            }
            let rank = if name == joined {
                0
            } else if name.starts_with(&joined) {
                1
            } else if name
                .split_whitespace()
                .any(|part| part.starts_with(&joined))
            {
                2
            } else if name.contains(&joined) {
                3
            } else if terms.iter().all(|term| name.contains(term)) {
                4
            } else {
                5
            };
            Some((rank, index, app))
        })
        .collect();
    ranked.sort_by_key(|(rank, index, _)| (*rank, *index));
    ranked
        .into_iter()
        .take(limit)
        .map(|(_, _, app)| app.clone())
        .collect()
}

pub fn next_launcher_selection(current: usize, count: usize, direction: i8) -> usize {
    if count == 0 {
        return 0;
    }
    if direction < 0 {
        current.saturating_sub(1)
    } else {
        current.saturating_add(1).min(count - 1)
    }
}

/// The result list is keyboard-operable even after focus leaves SearchEntry.
/// Navigation never wraps into a nonexistent or undisplayed catalogue row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LauncherNavigation {
    Previous,
    Next,
    First,
    Last,
    PagePrevious,
    PageNext,
}

pub fn launcher_selection_after(current: usize, count: usize, action: LauncherNavigation) -> usize {
    if count == 0 {
        return 0;
    }
    let current = current.min(count - 1);
    match action {
        LauncherNavigation::Previous => next_launcher_selection(current, count, -1),
        LauncherNavigation::Next => next_launcher_selection(current, count, 1),
        LauncherNavigation::First => 0,
        LauncherNavigation::Last => count - 1,
        LauncherNavigation::PagePrevious => current.saturating_sub(8),
        LauncherNavigation::PageNext => current.saturating_add(8).min(count - 1),
    }
}

/// Independent, tested gates for user requests while the Shell's async GIO
/// worker is in flight. GTK never assumes that a launch has succeeded.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LauncherRequests {
    pub refreshing: bool,
    pub launching: bool,
}

impl LauncherRequests {
    pub fn begin_refresh(&mut self) -> bool {
        if self.refreshing {
            return false;
        }
        self.refreshing = true;
        true
    }

    pub fn finish_refresh(&mut self) {
        self.refreshing = false;
    }

    pub fn begin_launch(&mut self) -> bool {
        if self.launching {
            return false;
        }
        self.launching = true;
        true
    }

    pub fn finish_launch(&mut self) {
        self.launching = false;
    }
}

pub fn matching_apps_label(number: usize) -> String {
    let rest = number % 100;
    let digit = rest % 10;
    let noun = if (11..=14).contains(&rest) {
        "результатов"
    } else {
        match digit {
            1 => "результат",
            2..=4 => "результата",
            _ => "результатов",
        }
    };
    format!("{number} {noun}")
}

/// Reuse the same button and callbacks for a stable application identity.
/// Rebuild only if the trusted GIO catalogue actually changed its metadata.
fn update_app_row(button: &gtk::Button, app: &ApplicationEntry) {
    let item = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let icon = app
        .icon
        .as_deref()
        .and_then(|value| gtk::gio::Icon::for_string(value).ok())
        .map(|icon| gtk::Image::from_gicon(&icon))
        .unwrap_or_else(|| gtk::Image::from_icon_name("application-x-executable-symbolic"));
    icon.set_pixel_size(32);
    icon.add_css_class("app-icon");
    item.append(&icon);

    let text = gtk::Box::new(gtk::Orientation::Vertical, 3);
    text.set_hexpand(true);
    let title = gtk::Label::new(Some(&app.name));
    title.set_xalign(0.0);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.add_css_class("app-name");
    text.append(&title);
    if !app.description.trim().is_empty() {
        let description = gtk::Label::new(Some(app.description.trim()));
        description.set_xalign(0.0);
        description.set_ellipsize(gtk::pango::EllipsizeMode::End);
        description.set_max_width_chars(44);
        description.add_css_class("app-description");
        text.append(&description);
    }
    item.append(&text);
    button.set_child(Some(&item));
    button.set_hexpand(true);
    button.set_tooltip_text(Some(if app.description.trim().is_empty() {
        &app.name
    } else {
        &app.description
    }));
}

struct LauncherState {
    weak_self: Weak<LauncherState>,
    list: gtk::Box,
    selection_flow: FlowIndicator,
    reflow: ReflowScope,
    scroll_motion: TenetScroll,
    cache: RefCell<HashMap<String, (gtk::Button, ReflowRow, ApplicationEntry)>>,
    visible_ids: RefCell<Vec<String>>,
    search: gtk::SearchEntry,
    clear: gtk::Button,
    refresh: gtk::Button,
    refresh_action: RefCell<Option<Rc<dyn Fn()>>>,
    requests: RefCell<LauncherRequests>,
    count: gtk::Label,
    status: gtk::Label,
    applications: RefCell<Vec<ApplicationEntry>>,
    rows: RefCell<Vec<gtk::Button>>,
    selected: Cell<usize>,
    loaded: Cell<bool>,
    on_launch: Rc<dyn Fn(String)>,
}

impl LauncherState {
    fn feedback(&self, message: &str, error: bool) {
        self.status.set_text(message);
        if error {
            self.status.remove_css_class("muted");
            self.status.add_css_class("config-error");
        } else {
            self.status.remove_css_class("config-error");
            self.status.add_css_class("muted");
        }
    }

    fn update_actions(&self) {
        let pending = *self.requests.borrow();
        self.refresh.set_sensitive(
            self.loaded.get() && !pending.refreshing && self.refresh_action.borrow().is_some(),
        );
        self.clear.set_visible(!self.search.text().is_empty());
        for button in self.rows.borrow().iter() {
            button.set_sensitive(!pending.launching);
        }
    }

    fn request_refresh(&self) {
        // A first discovery is already running in the Shell. Ctrl+R must
        // not start a second scan before the initial catalogue arrives.
        if !self.loaded.get() {
            return;
        }
        let callback = self.refresh_action.borrow().clone();
        let Some(callback) = callback else {
            return;
        };
        if !self.requests.borrow_mut().begin_refresh() {
            return;
        }
        self.feedback("Обновление списка приложений…", false);
        self.update_actions();
        callback();
    }

    fn request_launch(&self, id: &str) {
        if let Some(index) = self.visible_ids.borrow().iter().position(|item| item == id) {
            self.select(index, false);
        } else {
            return;
        }
        if !self.requests.borrow_mut().begin_launch() {
            return;
        }
        self.feedback("Запуск приложения…", false);
        self.update_actions();
        (self.on_launch)(id.to_owned());
    }

    fn select(&self, index: usize, scroll: bool) {
        let rows = self.rows.borrow();
        if rows.is_empty() {
            self.selected.set(0);
            self.selection_flow.clear();
            return;
        }
        let next = index.min(rows.len() - 1);
        for (i, button) in rows.iter().enumerate() {
            if i == next {
                button.add_css_class("selected");
            } else {
                button.remove_css_class("selected");
            }
        }
        self.selected.set(next);
        if let Some(button) = rows.get(next)
            && let Some(row) = button.parent()
        {
            self.selection_flow.retarget(&row);
            if scroll {
                // Use the stable layout row, not its animated child transform.
                self.scroll_motion.ensure_visible(&row, true);
            }
        }
    }

    /// Source FLIP only on a user filter action. Catalog updates are factual,
    /// not sensor motion; they settle their existing GTK buttons immediately.
    fn render(&self, animate: bool) {
        let prior = self.visible_ids.borrow().clone();
        // Capture the old *painted* positions before changing scroll or list.
        let previous = if animate && animate_launcher_filter(prior.len()) {
            let cache = self.cache.borrow();
            let ordered: Vec<_> = prior
                .iter()
                .filter_map(|key| cache.get(key).map(|(_, row, _)| (key.clone(), row.clone())))
                .collect();
            Some(self.reflow.capture(&ordered))
        } else {
            self.reflow.cancel();
            for (_, row, _) in self.cache.borrow().values() {
                row.settle();
            }
            None
        };

        // The new filter selects its first row; do not retain the prior
        // query's scroll position or its in-flight navigation destination.
        self.scroll_motion.scroll_to_start();
        let query = self.search.text().to_string();
        let found = filter_applications(&self.applications.borrow(), &query, usize::MAX);
        self.count
            .set_visible(self.loaded.get() && !query.trim().is_empty());
        self.count.set_text(&matching_apps_label(found.len()));

        // Keep stable keyed rows, including the real input controllers.
        let wanted: HashSet<&str> = found
            .iter()
            .take(MAX_DISPLAYED)
            .map(|app| app.id.as_str())
            .collect();
        let stale: Vec<_> = self
            .cache
            .borrow()
            .keys()
            .filter(|key| !wanted.contains(key.as_str()))
            .cloned()
            .collect();
        for key in stale {
            if let Some((_, row, _)) = self.cache.borrow_mut().remove(&key) {
                self.list.remove(&row);
            }
        }
        let mut child = self.list.first_child();
        while let Some(widget) = child {
            child = widget.next_sibling();
            if !widget.is::<ReflowRow>() {
                self.list.remove(&widget);
            }
        }

        self.rows.borrow_mut().clear();
        let mut predecessor: Option<gtk::Widget> = None;
        let mut visible = Vec::new();
        for app in found.iter().take(MAX_DISPLAYED) {
            let cached = self.cache.borrow().get(&app.id).cloned();
            let (button, row) = if let Some((button, row, model)) = cached {
                if model != *app {
                    update_app_row(&button, app);
                    self.cache
                        .borrow_mut()
                        .insert(app.id.clone(), (button.clone(), row.clone(), app.clone()));
                }
                (button, row)
            } else {
                let button = gtk::Button::new();
                button.add_css_class("app-row");
                button.set_halign(gtk::Align::Fill);
                update_app_row(&button, app);
                let weak = self.weak_self.clone();
                let id = app.id.clone();
                button.connect_clicked(move |_| {
                    if let Some(state) = weak.upgrade() {
                        state.request_launch(&id);
                    }
                });
                let row = ReflowRow::new(&button.clone().upcast());
                self.list.append(&row);
                self.cache
                    .borrow_mut()
                    .insert(app.id.clone(), (button.clone(), row.clone(), app.clone()));
                (button, row)
            };
            let in_position = match &predecessor {
                Some(before) => before.next_sibling().as_ref() == Some(row.upcast_ref()),
                None => self.list.first_child().as_ref() == Some(row.upcast_ref()),
            };
            if !in_position {
                self.list.reorder_child_after(&row, predecessor.as_ref());
            }
            predecessor = Some(row.clone().upcast());
            visible.push((app.id.clone(), row));
            self.rows.borrow_mut().push(button);
        }
        if found.is_empty() {
            let empty = gtk::Box::new(gtk::Orientation::Vertical, 8);
            empty.set_margin_top(22);
            empty.set_margin_bottom(20);
            empty.set_halign(gtk::Align::Center);
            let message = gtk::Label::new(Some(if !self.loaded.get() {
                "ЗАГРУЗКА ПРИЛОЖЕНИЙ…"
            } else if !query.trim().is_empty() {
                "Приложения не найдены"
            } else {
                "Нет доступных приложений"
            }));
            message.add_css_class("muted");
            empty.append(&message);
            if self.loaded.get() && !query.trim().is_empty() {
                let reset = gtk::Button::with_label("ОЧИСТИТЬ ПОИСК");
                reset.add_css_class("launcher-empty-action");
                let weak = self.weak_self.clone();
                reset.connect_clicked(move |_| {
                    if let Some(state) = weak.upgrade() {
                        state.search.set_text("");
                        state.search.grab_focus();
                    }
                });
                empty.append(&reset);
            }
            self.list.append(&empty);
        } else if found.len() > MAX_DISPLAYED {
            let more = gtk::Label::new(Some(&format!(
                "Показаны первые {} приложений. Уточните поиск для остальных.",
                MAX_DISPLAYED
            )));
            more.set_xalign(0.0);
            more.add_css_class("muted");
            self.list.append(&more);
        }
        self.visible_ids
            .replace(visible.iter().map(|(id, _)| id.clone()).collect());
        self.select(0, false);
        self.update_actions();
        if let Some(snapshot) = previous {
            self.reflow.play(snapshot, &visible);
        }
    }

    fn launch_selected(&self) {
        if let Some(id) = self.visible_ids.borrow().get(self.selected.get()).cloned() {
            self.request_launch(&id);
        }
    }
}

/// SearchEntry and rows share navigation. SearchEntry keeps ordinary
/// Home/End caret movement; Ctrl+Home/End navigates results explicitly.
fn launcher_key_controller(state: &Rc<LauncherState>) -> gtk::EventControllerKey {
    let key = gtk::EventControllerKey::new();
    let weak = Rc::downgrade(state);
    key.connect_key_pressed(move |_, pressed, _, modifiers| {
        let Some(state) = weak.upgrade() else {
            return gtk::glib::Propagation::Proceed;
        };
        if modifiers.contains(gtk::gdk::ModifierType::CONTROL_MASK)
            && (pressed == gtk::gdk::Key::r || pressed == gtk::gdk::Key::R)
        {
            state.request_refresh();
            return gtk::glib::Propagation::Stop;
        }
        if state.search.has_focus()
            && !modifiers.contains(gtk::gdk::ModifierType::CONTROL_MASK)
            && matches!(pressed, gtk::gdk::Key::Home | gtk::gdk::Key::End)
        {
            return gtk::glib::Propagation::Proceed;
        }
        let action = match pressed {
            gtk::gdk::Key::Down => LauncherNavigation::Next,
            gtk::gdk::Key::Up => LauncherNavigation::Previous,
            gtk::gdk::Key::Home => LauncherNavigation::First,
            gtk::gdk::Key::End => LauncherNavigation::Last,
            gtk::gdk::Key::Page_Up => LauncherNavigation::PagePrevious,
            gtk::gdk::Key::Page_Down => LauncherNavigation::PageNext,
            _ => return gtk::glib::Propagation::Proceed,
        };
        let count = state.rows.borrow().len();
        let next = launcher_selection_after(state.selected.get(), count, action);
        state.select(next, true);
        gtk::glib::Propagation::Stop
    });
    key
}

pub struct Launcher {
    root: gtk::Box,
    state: Rc<LauncherState>,
}

impl Launcher {
    pub fn new(on_launch: impl Fn(String) + 'static) -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 12);
        root.add_css_class("launcher");
        let title = gtk::Label::new(Some("Приложения"));
        title.set_xalign(0.0);
        title.add_css_class("section-title");
        root.append(&title);

        let query_row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        let search = gtk::SearchEntry::new();
        search.add_css_class("launcher-contact");
        search.set_placeholder_text(Some("Поиск приложений…"));
        search.set_hexpand(true);
        search.update_property(&[gtk::accessible::Property::Label("Поиск приложений")]);
        query_row.append(&search);
        let clear = gtk::Button::with_label("×");
        clear.set_tooltip_text(Some("Очистить поиск"));
        clear.update_property(&[gtk::accessible::Property::Label("Очистить поиск")]);
        clear.add_css_class("launcher-clear");
        clear.set_visible(false);
        query_row.append(&clear);
        let refresh = gtk::Button::with_label("⟳");
        refresh.add_css_class("launcher-refresh");
        refresh.set_tooltip_text(Some("Обновить список установленных приложений (Ctrl+R)"));
        refresh.update_property(&[gtk::accessible::Property::Label(
            "Обновить список приложений",
        )]);
        refresh.set_sensitive(false);
        query_row.append(&refresh);
        root.append(&query_row);

        let count = gtk::Label::new(None);
        count.add_css_class("muted");
        count.set_xalign(0.0);
        count.set_visible(false);
        root.append(&count);

        let scroll = gtk::ScrolledWindow::new();
        scroll.set_min_content_height(240);
        scroll.set_max_content_height(480);
        scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        let list = gtk::Box::new(gtk::Orientation::Vertical, 2);
        list.set_hexpand(true);
        let reflow = ReflowScope::new(&list.clone().upcast());
        let selection_flow =
            FlowIndicator::new(&list.clone().upcast(), FlowAxis::Both, FlowStyle::Rail);
        scroll.set_child(Some(&selection_flow));
        let scroll_motion = TenetScroll::new(&scroll);
        root.append(&scroll);

        let status = gtk::Label::new(None);
        status.set_xalign(0.0);
        status.set_wrap(true);
        status.add_css_class("muted");
        root.append(&status);

        let state = Rc::new_cyclic(|weak| LauncherState {
            weak_self: weak.clone(),
            list,
            selection_flow,
            reflow,
            scroll_motion,
            cache: RefCell::new(HashMap::new()),
            visible_ids: RefCell::new(Vec::new()),
            search: search.clone(),
            clear: clear.clone(),
            refresh: refresh.clone(),
            refresh_action: RefCell::new(None),
            requests: RefCell::new(LauncherRequests::default()),
            count,
            status,
            applications: RefCell::new(Vec::new()),
            rows: RefCell::new(Vec::new()),
            selected: Cell::new(0),
            loaded: Cell::new(false),
            on_launch: Rc::new(on_launch),
        });
        search.connect_search_changed({
            let weak = Rc::downgrade(&state);
            move |_| {
                if let Some(state) = weak.upgrade() {
                    if !state.requests.borrow().launching && !state.requests.borrow().refreshing {
                        state.feedback("", false);
                    }
                    state.render(true);
                }
            }
        });
        search.connect_activate({
            let weak = Rc::downgrade(&state);
            move |_| {
                if let Some(state) = weak.upgrade() {
                    state.launch_selected();
                }
            }
        });
        clear.connect_clicked({
            let weak = Rc::downgrade(&state);
            move |_| {
                if let Some(state) = weak.upgrade() {
                    state.search.set_text("");
                    state.search.grab_focus();
                }
            }
        });
        refresh.connect_clicked({
            let weak = Rc::downgrade(&state);
            move |_| {
                if let Some(state) = weak.upgrade() {
                    state.request_refresh();
                }
            }
        });
        search.add_controller(launcher_key_controller(&state));
        // Capture before child buttons/ScrolledWindow can consume Home,
        // End or Page keys; the search field still owns plain caret Home/End.
        let root_keys = launcher_key_controller(&state);
        root_keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        root.add_controller(root_keys);
        state.render(false);
        Self { root, state }
    }

    pub fn set_applications(&self, apps: Vec<ApplicationEntry>) {
        *self.state.applications.borrow_mut() = apps;
        self.state.loaded.set(true);
        let refreshed = self.state.requests.borrow().refreshing;
        self.state.requests.borrow_mut().finish_refresh();
        if refreshed {
            self.state.feedback("Список приложений обновлён", false);
        }
        self.state.render(false);
    }

    /// The host performs GIO discovery in a worker. Repeated refresh clicks
    /// are suppressed until set_applications delivers the observed catalogue.
    pub fn set_refresh_handler(&self, on_refresh: impl Fn() + 'static) {
        *self.state.refresh_action.borrow_mut() = Some(Rc::new(on_refresh));
        self.state.update_actions();
    }

    /// Only the checked worker result ends an in-flight launch; UI never
    /// optimistically reports that a desktop application started.
    pub fn finish_launch(&self, result: &Result<(), String>) {
        self.state.requests.borrow_mut().finish_launch();
        self.state.update_actions();
        match result {
            Ok(()) => self.state.feedback("Приложение запущено", false),
            Err(error) => self.state.feedback(error, true),
        }
    }

    pub fn focus_search(&self) {
        self.state.search.grab_focus();
    }

    pub fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(id: &str, name: &str, description: &str) -> ApplicationEntry {
        ApplicationEntry {
            id: id.into(),
            name: name.into(),
            description: description.into(),
            icon: None,
        }
    }

    #[test]
    fn broad_launcher_search_does_not_start_decorative_flip() {
        assert!(!animate_launcher_filter(0));
        assert!(animate_launcher_filter(1));
        assert!(animate_launcher_filter(20));
        assert!(!animate_launcher_filter(21));
        assert!(!animate_launcher_filter(MAX_DISPLAYED));
    }

    #[test]
    fn main_searches_descriptions_and_preserves_gio_identity() {
        let apps = vec![
            app("7", "Firefox", "Web browser"),
            app("2", "Files", "Browse folders"),
            app("21", "Terminal", "Command line"),
        ];
        assert_eq!(filter_applications(&apps, "fire", 80)[0].id, "7");
        assert_eq!(filter_applications(&apps, "browse folders", 80)[0].id, "2");
        assert_eq!(filter_applications(&apps, "WEB BROWSER", 80)[0].id, "7");
        assert_eq!(
            filter_applications(&apps, "terminal command", 80)[0].id,
            "21"
        );
        assert!(filter_applications(&apps, "unavailable", 80).is_empty());
        assert_eq!(filter_applications(&apps, "", 80), apps);
    }

    #[test]
    fn main_exact_and_prefix_rank_before_description_only() {
        let apps = vec![
            app("10", "Browser tools", "Utilities"),
            app("11", "WebKit diagnostics", "Browser testing"),
            app("12", "Browser", "The browser"),
            app("13", "My browser", "Utility browser"),
            app("14", "Browser editor", "Window browser"),
            app("15", "Cool Browser", "Browser tool"),
        ];
        assert_eq!(
            filter_applications(&apps, "browser", 80)
                .iter()
                .map(|a| a.id.as_str())
                .collect::<Vec<_>>(),
            ["12", "10", "14", "13", "15", "11"]
        );
        assert_eq!(filter_applications(&apps, "browser editor", 80)[0].id, "14");
    }

    #[test]
    fn selection_is_bounded_and_empty_is_safe() {
        assert_eq!(next_launcher_selection(0, 4, -1), 0);
        assert_eq!(next_launcher_selection(3, 4, 1), 3);
        assert_eq!(next_launcher_selection(1, 4, 1), 2);
        assert_eq!(next_launcher_selection(0, 0, 1), 0);
    }

    #[test]
    fn full_keyboard_navigation_is_bounded_and_respects_displayed_rows() {
        use LauncherNavigation as Move;
        assert_eq!(launcher_selection_after(4, 0, Move::Last), 0);
        assert_eq!(launcher_selection_after(4, 5, Move::Next), 4);
        assert_eq!(launcher_selection_after(1, 5, Move::Previous), 0);
        assert_eq!(launcher_selection_after(3, 80, Move::First), 0);
        assert_eq!(launcher_selection_after(3, 80, Move::Last), 79);
        assert_eq!(launcher_selection_after(30, 80, Move::PagePrevious), 22);
        assert_eq!(launcher_selection_after(30, 80, Move::PageNext), 38);
        assert_eq!(launcher_selection_after(2, 5, Move::PagePrevious), 0);
        assert_eq!(launcher_selection_after(2, 5, Move::PageNext), 4);
        assert_eq!(launcher_selection_after(100, 5, Move::Next), 4);
    }

    #[test]
    fn overlapping_refresh_and_launch_intents_wait_for_real_completion() {
        let mut requests = LauncherRequests::default();
        assert!(requests.begin_refresh());
        assert!(!requests.begin_refresh());
        assert!(requests.begin_launch());
        assert!(!requests.begin_launch());
        requests.finish_refresh();
        assert!(requests.begin_refresh());
        assert!(!requests.begin_launch());
        requests.finish_launch();
        assert!(requests.begin_launch());
        assert!(!requests.begin_launch());
    }

    #[test]
    fn count_uses_the_same_russian_agreement() {
        assert_eq!(matching_apps_label(1), "1 результат");
        assert_eq!(matching_apps_label(2), "2 результата");
        assert_eq!(matching_apps_label(5), "5 результатов");
        assert_eq!(matching_apps_label(11), "11 результатов");
        assert_eq!(matching_apps_label(21), "21 результат");
    }
}
