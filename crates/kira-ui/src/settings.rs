//! Reusable native Settings navigation and About view.
//! Consumes only typed snapshots; the host owns data gathering and display actions.
//! The same GTK subtree and selected section survive popup detach/reattach.
use std::cell::Cell;
use std::rc::Rc;

use gtk::prelude::*;

use crate::flow_indicator::{FlowAxis, FlowIndicator, FlowStyle};

const NARROW_WIDTH: i32 = 720;
const LOADING: &str = "Получение данных…";
const UNAVAILABLE: &str = "Недоступно";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SettingsSection {
    #[default]
    About,
    Appearance,
    Displays,
    Network,
    Devices,
}

impl SettingsSection {
    pub const fn id(self) -> &'static str {
        match self {
            Self::About => "about",
            Self::Appearance => "appearance",
            Self::Displays => "displays",
            Self::Network => "network",
            Self::Devices => "devices",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::About => "О системе",
            Self::Appearance => "Внешний вид",
            Self::Displays => "Дисплеи",
            Self::Network => "Сеть",
            Self::Devices => "Устройства",
        }
    }

    pub const fn available(self) -> bool {
        matches!(self, Self::About | Self::Displays)
    }

    pub fn from_id(id: &str) -> Option<Self> {
        ALL_SECTIONS.into_iter().find(|section| section.id() == id)
    }
}

pub const ALL_SECTIONS: [SettingsSection; 5] = [
    SettingsSection::About,
    SettingsSection::Appearance,
    SettingsSection::Displays,
    SettingsSection::Network,
    SettingsSection::Devices,
];

/// Unavailable or unknown sections must never become a navigable blank page.
pub fn settings_destination(current: SettingsSection, requested: &str) -> SettingsSection {
    match SettingsSection::from_id(requested) {
        Some(section) if section.available() => section,
        _ => current,
    }
}

/// Frozen main shows the 220px sidebar only in a *wide detached* window.
/// The compact selector remains available in the popup and narrow windows.
pub const fn settings_sidebar_visible(detached: bool, width: i32) -> bool {
    detached && width > NARROW_WIDTH
}

/// Plain, read-only information. No file paths, D-Bus or Niri authority.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SettingsAbout {
    pub os_name: Option<String>,
    pub kernel: Option<String>,
    pub architecture: Option<String>,
    pub hostname: Option<String>,
    pub cpu_model: Option<String>,
    pub cpu_threads: Option<usize>,
    pub memory_total_kib: Option<u64>,
    pub session: Option<String>,
    pub shell_version: Option<String>,
}

pub fn settings_value(value: Option<&str>) -> String {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.chars().take(160).collect())
        .unwrap_or_else(|| UNAVAILABLE.to_owned())
}

/// Same binary units and one-decimal upper bound as frozen settings-model.ts.
pub fn format_installed_memory(kib: Option<u64>) -> String {
    let Some(kib) = kib.filter(|kib| *kib > 0) else {
        return UNAVAILABLE.to_owned();
    };
    if kib >= 1024 * 1024 {
        let rounded = ((kib as f64 / (1024.0 * 1024.0)) * 10.0).round() / 10.0;
        if rounded.fract() == 0.0 {
            format!("{rounded:.0} ГиБ")
        } else {
            format!("{} ГиБ", format!("{rounded:.1}").replace('.', ","))
        }
    } else {
        format!("{} МиБ", kib.saturating_add(512) / 1024)
    }
}

const ABOUT_FIELDS: [(&str, &str); 8] = [
    ("kernel", "Версия ядра"),
    ("architecture", "Архитектура"),
    ("hostname", "Имя устройства"),
    ("cpu_model", "Процессор"),
    ("cpu_threads", "Доступные логические потоки"),
    ("memory", "Оперативная память"),
    ("session", "Тип сессии"),
    ("shell_version", "Версия Kira Shell"),
];

pub fn settings_field(snapshot: Option<&SettingsAbout>, field: &str) -> String {
    let Some(snapshot) = snapshot else {
        return LOADING.to_owned();
    };
    match field {
        "kernel" => settings_value(snapshot.kernel.as_deref()),
        "architecture" => settings_value(snapshot.architecture.as_deref()),
        "hostname" => settings_value(snapshot.hostname.as_deref()),
        "cpu_model" => settings_value(snapshot.cpu_model.as_deref()),
        "cpu_threads" => snapshot
            .cpu_threads
            .filter(|n| *n > 0)
            .map_or_else(|| UNAVAILABLE.to_owned(), |n| n.to_string()),
        "memory" => format_installed_memory(snapshot.memory_total_kib),
        "session" => settings_value(snapshot.session.as_deref()),
        "shell_version" => settings_value(snapshot.shell_version.as_deref()),
        _ => UNAVAILABLE.to_owned(),
    }
}

struct Navigation {
    selected: Cell<SettingsSection>,
    detached: Cell<bool>,
    viewport_width: Cell<i32>,
    sidebar: gtk::ScrolledWindow,
    dropdown: gtk::DropDown,
    stack: gtk::Stack,
    buttons: [gtk::Button; 2],
    indicator: FlowIndicator,
}

impl Navigation {
    fn sync_layout(&self) {
        let visible = settings_sidebar_visible(self.detached.get(), self.viewport_width.get());
        if !visible
            && self.sidebar.is_visible()
            && self.buttons.iter().any(|button| button.has_focus())
        {
            // A focused sidebar must not disappear underneath keyboard users.
            self.dropdown.grab_focus();
        }
        self.sidebar.set_visible(visible);
        self.dropdown.set_visible(!visible);
    }

    fn select(&self, requested: &str) {
        let selected = settings_destination(self.selected.get(), requested);
        self.selected.set(selected);
        self.stack.set_visible_child_name(selected.id());
        let index = if selected == SettingsSection::About {
            0
        } else {
            1
        };
        if self.dropdown.selected() != index {
            self.dropdown.set_selected(index);
        }
        for (position, button) in self.buttons.iter().enumerate() {
            let active = position == index as usize;
            if active {
                button.add_css_class("selected");
            } else {
                button.remove_css_class("selected");
            }
            button.update_property(&[gtk::accessible::Property::Description(if active {
                "Текущий раздел"
            } else {
                "Перейти в раздел"
            })]);
        }
        self.indicator
            .retarget(&self.buttons[index as usize].clone().upcast());
    }
}

/// Embeddable Settings widget. The DisplaySettings subtree is supplied by the
/// host and retained through every section change and window reparent.
pub struct SettingsPanel {
    root: gtk::Box,
    navigation: Rc<Navigation>,
    os_name: gtk::Label,
    fields: Vec<(&'static str, gtk::Label)>,
}

impl SettingsPanel {
    pub fn new(displays: &gtk::Widget) -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 16);
        root.add_css_class("kira-settings");

        let heading = gtk::Label::new(Some("Настройки"));
        heading.set_xalign(0.0);
        heading.add_css_class("section-title");
        heading.add_css_class("settings-heading");
        root.append(&heading);

        let sidebar_content = gtk::Box::new(gtk::Orientation::Vertical, 4);
        sidebar_content.add_css_class("settings-sidebar-content");
        let about_button = gtk::Button::with_label(SettingsSection::About.label());
        let displays_button = gtk::Button::with_label(SettingsSection::Displays.label());
        for button in [&about_button, &displays_button] {
            button.set_hexpand(true);
            button.set_halign(gtk::Align::Fill);
            button.add_css_class("settings-nav-button");
            sidebar_content.append(button);
        }
        let unavailable_header = gtk::Label::new(Some("В ПЛАНАХ"));
        unavailable_header.add_css_class("settings-planned-heading");
        unavailable_header.set_xalign(0.0);
        sidebar_content.append(&unavailable_header);
        for section in ALL_SECTIONS
            .into_iter()
            .filter(|section| !section.available())
        {
            let label = gtk::Label::new(Some(section.label()));
            label.set_xalign(0.0);
            label.add_css_class("settings-planned");
            sidebar_content.append(&label);
        }
        let indicator = FlowIndicator::new(
            &sidebar_content.clone().upcast(),
            FlowAxis::Both,
            FlowStyle::Rail,
        );
        let sidebar = gtk::ScrolledWindow::new();
        sidebar.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        sidebar.set_size_request(220, -1);
        sidebar.add_css_class("settings-sidebar");
        sidebar.set_child(Some(&indicator));
        sidebar.set_visible(false);

        let dropdown = gtk::DropDown::from_strings(&[
            SettingsSection::About.label(),
            SettingsSection::Displays.label(),
        ]);
        dropdown.set_hexpand(true);
        dropdown.add_css_class("settings-section-picker");
        dropdown.update_property(&[gtk::accessible::Property::Label("Раздел настроек")]);

        let about_page = gtk::Box::new(gtk::Orientation::Vertical, 14);
        about_page.add_css_class("settings-about");
        let about_heading = gtk::Label::new(Some("О системе"));
        about_heading.set_xalign(0.0);
        about_heading.add_css_class("section-title");
        about_page.append(&about_heading);
        let os_card = gtk::Box::new(gtk::Orientation::Vertical, 8);
        os_card.add_css_class("settings-os-card");
        let os_caption = gtk::Label::new(Some("Операционная система"));
        os_caption.set_xalign(0.0);
        os_caption.add_css_class("settings-field-name");
        os_card.append(&os_caption);
        let os_name = gtk::Label::new(Some(LOADING));
        os_name.set_xalign(0.0);
        os_name.set_wrap(true);
        os_name.set_selectable(true);
        os_name.add_css_class("settings-os-name");
        os_card.append(&os_name);
        about_page.append(&os_card);
        let device_heading = gtk::Label::new(Some("Характеристики устройства"));
        device_heading.set_xalign(0.0);
        device_heading.add_css_class("settings-device-heading");
        about_page.append(&device_heading);
        let facts = gtk::FlowBox::new();
        facts.set_selection_mode(gtk::SelectionMode::None);
        facts.set_max_children_per_line(2);
        facts.set_min_children_per_line(1);
        facts.set_column_spacing(16);
        facts.set_row_spacing(8);
        facts.set_homogeneous(true);
        let mut fields = Vec::new();
        for (key, title) in ABOUT_FIELDS {
            let row = gtk::Box::new(gtk::Orientation::Vertical, 5);
            row.set_size_request(190, -1);
            row.add_css_class("settings-fact");
            let caption = gtk::Label::new(Some(title));
            caption.set_xalign(0.0);
            caption.set_wrap(true);
            caption.add_css_class("settings-field-name");
            row.append(&caption);
            let value = gtk::Label::new(Some(LOADING));
            value.set_xalign(0.0);
            value.set_wrap(true);
            value.set_selectable(true);
            value.add_css_class("settings-field-value");
            row.append(&value);
            facts.append(&row);
            fields.push((key, value));
        }
        about_page.append(&facts);

        let stack = gtk::Stack::new();
        stack.set_transition_type(gtk::StackTransitionType::None);
        stack.add_named(&about_page, Some(SettingsSection::About.id()));
        stack.add_named(displays, Some(SettingsSection::Displays.id()));
        stack.set_visible_child_name(SettingsSection::About.id());

        let content = gtk::Box::new(gtk::Orientation::Vertical, 16);
        content.add_css_class("settings-content");
        content.set_hexpand(true);
        content.append(&dropdown);
        content.append(&stack);
        let body = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        body.add_css_class("settings-body");
        body.append(&sidebar);
        body.append(&content);

        // A transparent observer measures *actual allocated width*. Neither
        // an assumed output size nor the detached window's initial size
        // predicts later manual resizing/fractional scaling.
        let overlay = gtk::Overlay::new();
        overlay.set_child(Some(&body));
        let observer = gtk::DrawingArea::new();
        observer.set_can_target(false);
        overlay.add_overlay(&observer);
        root.append(&overlay);

        let navigation = Rc::new(Navigation {
            selected: Cell::new(SettingsSection::About),
            detached: Cell::new(false),
            viewport_width: Cell::new(0),
            sidebar,
            dropdown,
            stack,
            buttons: [about_button, displays_button],
            indicator,
        });
        for (index, section) in [SettingsSection::About, SettingsSection::Displays]
            .into_iter()
            .enumerate()
        {
            let weak = Rc::downgrade(&navigation);
            navigation.buttons[index].connect_clicked(move |_| {
                if let Some(nav) = weak.upgrade() {
                    nav.select(section.id());
                }
            });
        }
        let weak = Rc::downgrade(&navigation);
        navigation.dropdown.connect_selected_notify(move |picker| {
            if let Some(nav) = weak.upgrade() {
                let section = if picker.selected() == 0 {
                    "about"
                } else {
                    "displays"
                };
                nav.select(section);
            }
        });
        let weak = Rc::downgrade(&navigation);
        observer.connect_resize(move |_, width, _| {
            if let Some(nav) = weak.upgrade() {
                nav.viewport_width.set(width);
                nav.sync_layout();
            }
        });
        navigation.sync_layout();
        navigation.select(SettingsSection::About.id());
        Self {
            root,
            navigation,
            os_name,
            fields,
        }
    }

    pub fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }

    pub fn section(&self) -> SettingsSection {
        self.navigation.selected.get()
    }

    pub fn select_section(&self, requested: &str) {
        self.navigation.select(requested);
    }

    /// Called by the host's retained-window lifecycle callback.
    pub fn set_detached(&self, detached: bool) {
        self.navigation.detached.set(detached);
        self.navigation.sync_layout();
    }

    pub fn set_about(&self, snapshot: &SettingsAbout) {
        self.os_name
            .set_text(&settings_value(snapshot.os_name.as_deref()));
        for (key, label) in &self.fields {
            label.set_text(&settings_field(Some(snapshot), key));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unavailable_destinations_never_replace_a_real_page() {
        assert_eq!(ALL_SECTIONS.len(), 5);
        assert_eq!(SettingsSection::About.id(), "about");
        assert_eq!(
            settings_destination(SettingsSection::About, "appearance"),
            SettingsSection::About
        );
        assert_eq!(
            settings_destination(SettingsSection::Displays, "devices"),
            SettingsSection::Displays
        );
        assert_eq!(
            settings_destination(SettingsSection::About, "network"),
            SettingsSection::About
        );
        assert_eq!(
            settings_destination(SettingsSection::About, "made-up"),
            SettingsSection::About
        );
        assert_eq!(
            settings_destination(SettingsSection::About, "displays"),
            SettingsSection::Displays
        );
        assert_eq!(
            settings_destination(SettingsSection::Displays, "about"),
            SettingsSection::About
        );
    }

    #[test]
    fn sidebar_requires_detachment_and_real_width_above_720() {
        assert!(!settings_sidebar_visible(false, 1920));
        assert!(!settings_sidebar_visible(true, 0));
        assert!(!settings_sidebar_visible(true, 720));
        assert!(settings_sidebar_visible(true, 721));
        assert!(settings_sidebar_visible(true, 1920));
    }

    #[test]
    fn missing_device_fields_are_not_synthetic() {
        assert_eq!(settings_field(None, "kernel"), LOADING);
        let facts = SettingsAbout::default();
        assert_eq!(settings_field(Some(&facts), "kernel"), UNAVAILABLE);
        assert_eq!(settings_field(Some(&facts), "cpu_threads"), UNAVAILABLE);
        assert_eq!(settings_field(Some(&facts), "memory"), UNAVAILABLE);
        assert_eq!(settings_value(Some("  ")), UNAVAILABLE);
        assert_eq!(settings_value(Some("niri")), "niri");
    }

    #[test]
    fn installed_memory_matches_binary_units_and_unknown_policy() {
        assert_eq!(format_installed_memory(None), UNAVAILABLE);
        assert_eq!(format_installed_memory(Some(0)), UNAVAILABLE);
        assert_eq!(format_installed_memory(Some(512 * 1024)), "512 МиБ");
        assert_eq!(format_installed_memory(Some(1024 * 1024)), "1 ГиБ");
        assert_eq!(
            format_installed_memory(Some(3 * 1024 * 1024 + 512 * 1024)),
            "3,5 ГиБ"
        );
    }

    #[test]
    fn device_snapshots_do_not_change_current_section() {
        let selected = settings_destination(SettingsSection::About, "displays");
        let info = SettingsAbout {
            hostname: Some("host".to_owned()),
            ..Default::default()
        };
        assert_eq!(settings_field(Some(&info), "hostname"), "host");
        assert_eq!(selected, SettingsSection::Displays);
    }
}
