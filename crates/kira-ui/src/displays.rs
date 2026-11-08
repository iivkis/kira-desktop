//! Reusable GTK4 display editor. All privileged operations are caller-owned.
//! User intents are typed; there is no Niri process, JSON or filesystem access here.
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use gtk::prelude::*;

use crate::display_map::{DisplayMap, MapIntent};

const TRANSFORMS: [&str; 8] = [
    "normal",
    "90",
    "180",
    "270",
    "flipped",
    "flipped-90",
    "flipped-180",
    "flipped-270",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DisplayMode {
    pub id: String,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DisplayOutput {
    pub connector: String,
    pub description: String,
    pub enabled: bool,
    pub modes: Vec<DisplayMode>,
    pub mode: String,
    pub scale: f64,
    pub transform: String,
    pub x: i32,
    pub y: i32,
    pub vrr: bool,
    pub vrr_supported: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DisplayChoice {
    pub connector: String,
    pub enabled: bool,
    pub mode: String,
    pub scale: f64,
    pub transform: String,
    pub x: i32,
    pub y: i32,
    pub vrr: bool,
}

impl From<&DisplayOutput> for DisplayChoice {
    fn from(output: &DisplayOutput) -> Self {
        Self {
            connector: output.connector.clone(),
            enabled: output.enabled,
            mode: output.mode.clone(),
            scale: output.scale,
            transform: output.transform.clone(),
            x: output.x,
            y: output.y,
            vrr: output.vrr,
        }
    }
}

/// The UI checks basic completeness. Trusted services validate every field
/// against the *current* Niri snapshot before acting.
pub fn can_apply(choices: &[DisplayChoice]) -> bool {
    !choices.is_empty()
        && choices.iter().any(|item| item.enabled)
        && choices.len() <= 8
        && {
            let mut connectors = std::collections::HashSet::new();
            choices
                .iter()
                .all(|item| connectors.insert(item.connector.as_str()))
        }
        && choices.iter().all(|item| {
            !item.connector.is_empty()
                && !item.mode.is_empty()
                && item.scale.is_finite()
                && (0.5..=4.0).contains(&item.scale)
                && TRANSFORMS.contains(&item.transform.as_str())
                && (-32000..=32000).contains(&item.x)
                && (-32000..=32000).contains(&item.y)
        })
}

pub fn seconds_left(deadline_unix_ms: u64, now_unix_ms: u64) -> u64 {
    deadline_unix_ms.saturating_sub(now_unix_ms).div_ceil(1000)
}

#[derive(Clone, Debug, PartialEq)]
pub enum DisplayIntent {
    Refresh,
    Apply(Vec<DisplayChoice>),
    Confirm(u64),
    Revert(u64),
    Save,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DisplayPhase {
    Loading,
    Ready,
    Applying,
    Pending { id: u64, deadline_unix_ms: u64 },
    Confirming,
    Confirmed,
    Reverting,
    Reverted,
    Saving,
    Saved,
    Error(String),
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|elapsed| u64::try_from(elapsed.as_millis()).ok())
        .unwrap_or(0)
}

struct Controls {
    outputs: gtk::Box,
    selectors: gtk::Box,
    map: DisplayMap,
    observed: RefCell<Vec<DisplayOutput>>,
    selected: Cell<Option<usize>>,
    sections: RefCell<Vec<gtk::Box>>,
    selector_buttons: RefCell<Vec<gtk::Button>>,
    coordinates: RefCell<Vec<(gtk::SpinButton, gtk::SpinButton)>>,
    status: gtk::Label,
    persistence: gtk::Label,
    refresh: gtk::Button,
    apply: gtk::Button,
    confirm: gtk::Button,
    revert: gtk::Button,
    save: gtk::Button,
    reset: gtk::Button,
    choices: RefCell<Vec<DisplayChoice>>,
    phase: RefCell<DisplayPhase>,
    editing_enabled: Cell<bool>,
    supervisor_lost: Cell<bool>,
    on_intent: Rc<dyn Fn(DisplayIntent)>,
}

impl Controls {
    fn set_phase(&self, phase: DisplayPhase) {
        *self.phase.borrow_mut() = phase;
        self.sync();
    }

    fn editing_allowed(&self) -> bool {
        self.editing_enabled.get()
            && !self.supervisor_lost.get()
            && matches!(
                *self.phase.borrow(),
                DisplayPhase::Ready
                    | DisplayPhase::Confirmed
                    | DisplayPhase::Reverted
                    | DisplayPhase::Saved
                    | DisplayPhase::Error(_)
            )
    }

    fn select(&self, index: usize) {
        if index >= self.choices.borrow().len() {
            return;
        }
        self.selected.set(Some(index));
        for (position, section) in self.sections.borrow().iter().enumerate() {
            section.set_visible(position == index);
        }
        for (position, button) in self.selector_buttons.borrow().iter().enumerate() {
            if position == index {
                button.add_css_class("selected");
            } else {
                button.remove_css_class("selected");
            }
        }
        self.update_map();
    }

    fn update_map(&self) {
        self.map.sync(
            &self.observed.borrow(),
            &self.choices.borrow(),
            self.selected.get(),
            self.editing_allowed(),
        );
    }

    fn move_output(&self, index: usize, x: i32, y: i32) {
        self.edited(index, |item| {
            item.x = x;
            item.y = y;
        });
    }

    fn edited(&self, index: usize, update: impl FnOnce(&mut DisplayChoice)) {
        if !self.editing_allowed() {
            return;
        }
        let changed = {
            let mut choices = self.choices.borrow_mut();
            if let Some(item) = choices.get_mut(index) {
                update(item);
                true
            } else {
                false
            }
        };
        if !changed {
            return;
        }
        let reset_confirmed = matches!(
            &*self.phase.borrow(),
            DisplayPhase::Confirmed | DisplayPhase::Saved
        );
        if reset_confirmed {
            *self.phase.borrow_mut() = DisplayPhase::Ready;
        }
        // Release both RefCell read guards before touching GTK SpinButtons:
        // set_value can synchronously re-enter Controls::edited.
        let item = self.choices.borrow().get(index).cloned();
        let coordinates = self.coordinates.borrow().get(index).cloned();
        if let (Some(item), Some((x, y))) = (item, coordinates) {
            if x.value_as_int() != item.x {
                x.set_value(f64::from(item.x));
            }
            if y.value_as_int() != item.y {
                y.set_value(f64::from(item.y));
            }
        }
        self.sync();
    }

    fn sync(&self) {
        let phase = self.phase.borrow();
        let now = now_millis();
        let message = match &*phase {
            DisplayPhase::Loading => "Чтение настроек дисплеев…".to_owned(),
            DisplayPhase::Ready => "Изменения не применены".to_owned(),
            DisplayPhase::Applying => "Применение временной конфигурации…".to_owned(),
            DisplayPhase::Pending {
                deadline_unix_ms, ..
            } => {
                let remaining = seconds_left(*deadline_unix_ms, now);
                if remaining == 0 {
                    "Ожидание автоматического отката Niri…".to_owned()
                } else {
                    format!("Подтвердите настройки за {remaining} с, иначе Niri откатит их")
                }
            }
            DisplayPhase::Confirming => "Подтверждение конфигурации…".to_owned(),
            DisplayPhase::Confirmed => {
                "Подтверждено. Для записи в KDL нажмите «Сохранить»".to_owned()
            }
            DisplayPhase::Reverting => "Возвращаются предыдущие настройки…".to_owned(),
            DisplayPhase::Reverted => "Восстановлены предыдущие настройки".to_owned(),
            DisplayPhase::Saving => "Проверка владельца и сохранение KDL…".to_owned(),
            DisplayPhase::Saved => "Настройки сохранены в управляемый KDL".to_owned(),
            DisplayPhase::Error(message) => format!("Ошибка: {message}"),
        };
        if self.supervisor_lost.get() {
            self.status.set_text(&format!(
                "{message}\nСупервизор дисплеев остановлен. Перезапустите оболочку для нового сеанса редактирования."
            ));
        } else if self.editing_enabled.get() {
            self.status.set_text(&message);
        } else {
            self.status.set_text(&format!(
                "{message}\nТолько просмотр: управление дисплеями разрешено в изолированном превью."
            ));
        }
        let editing = matches!(
            *phase,
            DisplayPhase::Ready
                | DisplayPhase::Confirmed
                | DisplayPhase::Reverted
                | DisplayPhase::Saved
                | DisplayPhase::Error(_)
        );
        let mutating =
            editing && self.editing_enabled.get() && !self.supervisor_lost.get();
        self.outputs.set_sensitive(mutating);
        self.update_map();
        self.refresh.set_sensitive(editing && !self.supervisor_lost.get());
        self.reset.set_sensitive(mutating);
        self.apply
            .set_sensitive(mutating && can_apply(&self.choices.borrow()));
        self.save.set_sensitive(
            mutating && matches!(*phase, DisplayPhase::Confirmed),
        );
        match &*phase {
            DisplayPhase::Pending {
                deadline_unix_ms, ..
            } => {
                self.confirm.set_sensitive(
                    !self.supervisor_lost.get()
                        && self.editing_enabled.get()
                        && seconds_left(*deadline_unix_ms, now) > 0,
                );
                self.revert.set_sensitive(!self.supervisor_lost.get());
            }
            _ => {
                self.confirm.set_sensitive(false);
                self.revert.set_sensitive(false);
            }
        }
    }

    fn submit(self: &Rc<Self>, action: &str) {
        let can_save = *self.phase.borrow() == DisplayPhase::Confirmed;
        match action {
            "refresh"
                if !self.supervisor_lost.get()
                    && !matches!(*self.phase.borrow(), DisplayPhase::Pending { .. }) => {
                self.set_phase(DisplayPhase::Loading);
                (self.on_intent)(DisplayIntent::Refresh);
            }
            "apply" if self.editing_allowed() && can_apply(&self.choices.borrow()) => {
                let choices = self.choices.borrow().clone();
                self.set_phase(DisplayPhase::Applying);
                (self.on_intent)(DisplayIntent::Apply(choices));
            }
            "confirm" if self.editing_enabled.get() && !self.supervisor_lost.get() => {
                let phase = self.phase.borrow().clone();
                if let DisplayPhase::Pending {
                    id,
                    deadline_unix_ms,
                } = phase
                    && seconds_left(deadline_unix_ms, now_millis()) > 0
                {
                    self.set_phase(DisplayPhase::Confirming);
                    (self.on_intent)(DisplayIntent::Confirm(id));
                }
            }
            "revert" if !self.supervisor_lost.get() => {
                let phase = self.phase.borrow().clone();
                if let DisplayPhase::Pending { id, .. } = phase {
                    self.set_phase(DisplayPhase::Reverting);
                    (self.on_intent)(DisplayIntent::Revert(id));
                }
            }
            "save" if self.editing_enabled.get() && !self.supervisor_lost.get() && can_save => {
                self.set_phase(DisplayPhase::Saving);
                (self.on_intent)(DisplayIntent::Save);
            }
            "reset" if self.editing_allowed() => {
                let observed = self.observed.borrow().clone();
                // Native reset creates the editor controls from the same
                // observed snapshot; it does not mutate the compositor.
                self.set_outputs(observed);
                self.set_phase(DisplayPhase::Ready);
            }
            _ => {}
        }
    }

    fn set_outputs(self: &Rc<Self>, outputs: Vec<DisplayOutput>) {
        let selected_name = self.selected.get().and_then(|index| {
            self.choices
                .borrow()
                .get(index)
                .map(|item| item.connector.clone())
        });
        while let Some(child) = self.selectors.first_child() {
            self.selectors.remove(&child);
        }
        while let Some(child) = self.outputs.first_child() {
            self.outputs.remove(&child);
        }
        self.sections.borrow_mut().clear();
        self.selector_buttons.borrow_mut().clear();
        self.coordinates.borrow_mut().clear();
        *self.observed.borrow_mut() = outputs.clone();
        *self.choices.borrow_mut() = outputs.iter().map(DisplayChoice::from).collect();
        for (index, output) in outputs.iter().enumerate() {
            let choose = gtk::Button::with_label(&format!(
                "{} · {} · {}",
                index + 1,
                output.connector,
                if output.enabled {
                    "Включён"
                } else {
                    "Выключен"
                },
            ));
            choose.add_css_class("display-selector");
            choose.set_hexpand(true);
            choose.set_halign(gtk::Align::Fill);
            choose.update_property(&[gtk::accessible::Property::Label(&format!(
                "Выбрать экран {}: {}",
                index + 1,
                output.connector,
            ))]);
            let weak = Rc::downgrade(self);
            choose.connect_clicked(move |_| {
                if let Some(controls) = weak.upgrade() {
                    controls.select(index);
                }
            });
            self.selectors.append(&choose);
            self.selector_buttons.borrow_mut().push(choose);
            let section = gtk::Box::new(gtk::Orientation::Vertical, 6);
            section.add_css_class("display-output");

            let name = gtk::Label::new(Some(&format!(
                "{} · {}",
                output.connector, output.description
            )));
            name.add_css_class("section-title");
            name.set_xalign(0.0);
            name.set_wrap(true);
            section.append(&name);

            let enabled = gtk::CheckButton::with_label("Монитор включён");
            enabled.set_active(output.enabled);
            section.append(&enabled);
            let weak = Rc::downgrade(self);
            enabled.connect_toggled(move |button| {
                if let Some(controls) = weak.upgrade() {
                    controls.edited(index, |item| item.enabled = button.is_active());
                }
            });

            let names = output
                .modes
                .iter()
                .map(|mode| mode.label.as_str())
                .collect::<Vec<_>>();
            let mode = gtk::DropDown::from_strings(&names);
            let selected = output
                .modes
                .iter()
                .position(|item| item.id == output.mode)
                .unwrap_or(0);
            mode.set_selected(selected as u32);
            mode.set_sensitive(!names.is_empty());
            section.append(&labeled_field("Разрешение и частота", &mode));
            let ids = output
                .modes
                .iter()
                .map(|item| item.id.clone())
                .collect::<Vec<_>>();
            let weak = Rc::downgrade(self);
            mode.connect_selected_notify(move |select| {
                if let Some(mode) = ids.get(select.selected() as usize)
                    && let Some(controls) = weak.upgrade()
                {
                    controls.edited(index, |item| item.mode = mode.clone());
                }
            });

            let scale = gtk::SpinButton::with_range(0.5, 4.0, 0.25);
            scale.set_digits(2);
            scale.set_value(output.scale);
            section.append(&labeled_field("Масштаб", &scale));
            let weak = Rc::downgrade(self);
            scale.connect_value_changed(move |spin| {
                if let Some(controls) = weak.upgrade() {
                    controls.edited(index, |item| item.scale = spin.value());
                }
            });

            let orientation = gtk::DropDown::from_strings(&TRANSFORMS);
            let selected = TRANSFORMS
                .iter()
                .position(|transform| *transform == output.transform)
                .unwrap_or(0);
            orientation.set_selected(selected as u32);
            section.append(&labeled_field("Ориентация", &orientation));
            let weak = Rc::downgrade(self);
            orientation.connect_selected_notify(move |select| {
                if let Some(transform) = TRANSFORMS.get(select.selected() as usize)
                    && let Some(controls) = weak.upgrade()
                {
                    controls.edited(index, |item| item.transform = (*transform).to_owned());
                }
            });

            let position = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            let x = gtk::SpinButton::with_range(-32000.0, 32000.0, 1.0);
            x.set_value(f64::from(output.x));
            let y = gtk::SpinButton::with_range(-32000.0, 32000.0, 1.0);
            y.set_value(f64::from(output.y));
            position.append(&gtk::Label::new(Some("X")));
            position.append(&x);
            position.append(&gtk::Label::new(Some("Y")));
            position.append(&y);
            section.append(&labeled_field("Позиция", &position));
            let weak = Rc::downgrade(self);
            x.connect_value_changed(move |spin| {
                if let Some(controls) = weak.upgrade() {
                    controls.edited(index, |item| item.x = spin.value_as_int());
                }
            });
            let weak = Rc::downgrade(self);
            y.connect_value_changed(move |spin| {
                if let Some(controls) = weak.upgrade() {
                    controls.edited(index, |item| item.y = spin.value_as_int());
                }
            });

            let vrr = gtk::CheckButton::with_label("Адаптивная частота (VRR)");
            vrr.set_active(output.vrr);
            vrr.set_sensitive(output.vrr_supported);
            section.append(&vrr);
            let weak = Rc::downgrade(self);
            vrr.connect_toggled(move |button| {
                if let Some(controls) = weak.upgrade() {
                    controls.edited(index, |item| item.vrr = button.is_active());
                }
            });
            self.coordinates.borrow_mut().push((x, y));
            self.outputs.append(&section);
            self.sections.borrow_mut().push(section);
        }
        if outputs.is_empty() {
            self.selected.set(None);
            self.update_map();
        } else {
            let selected = selected_name
                .and_then(|connector| {
                    outputs
                        .iter()
                        .position(|output| output.connector == connector)
                })
                .unwrap_or(0);
            self.select(selected);
        }
        self.sync();
    }
}

pub struct DisplaySettings {
    root: gtk::Box,
    controls: Rc<Controls>,
}

fn labeled_field(title: &str, field: &impl IsA<gtk::Widget>) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let caption = gtk::Label::new(Some(title));
    caption.set_xalign(0.0);
    caption.set_hexpand(true);
    caption.add_css_class("muted");
    row.append(&caption);
    row.append(field);
    row
}

impl DisplaySettings {
    pub fn new(on_intent: impl Fn(DisplayIntent) + 'static) -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 10);
        root.add_css_class("display-settings");
        let title = gtk::Label::new(Some("Дисплеи"));
        title.add_css_class("section-title");
        title.set_xalign(0.0);
        root.append(&title);

        let explanation = gtk::Label::new(Some(
            "Сначала примените, затем подтвердите в течение 15 секунд. Запись в KDL выполняется только отдельным сохранением.",
        ));
        explanation.set_xalign(0.0);
        explanation.set_wrap(true);
        explanation.add_css_class("muted");
        root.append(&explanation);

        let status = gtk::Label::new(Some("Чтение настроек дисплеев…"));
        status.set_xalign(0.0);
        status.set_wrap(true);
        status.add_css_class("display-status");
        root.append(&status);
        let persistence = gtk::Label::new(Some("Сохранение: статус пока неизвестен"));
        persistence.set_xalign(0.0);
        persistence.set_wrap(true);
        persistence.add_css_class("muted");
        root.append(&persistence);

        let outputs = gtk::Box::new(gtk::Orientation::Vertical, 12);

        let map_host = Rc::new(RefCell::new(std::rc::Weak::<Controls>::new()));
        let map_target = map_host.clone();
        let map = DisplayMap::new(move |intent| {
            let Some(controls) = map_target.borrow().upgrade() else {
                return;
            };
            match intent {
                MapIntent::Select(index) => controls.select(index),
                MapIntent::Move { index, x, y } => controls.move_output(index, x, y),
            }
        });
        let map_heading = gtk::Label::new(Some("РАСПОЛОЖЕНИЕ · ПЕРЕТАЩИТЕ ЭКРАН"));
        map_heading.add_css_class("display-map-heading");
        map_heading.set_xalign(0.0);
        root.append(&map_heading);
        root.append(&map.widget());
        let help = gtk::Label::new(Some(
            "Выбор экрана: мышью или в списке. Перетаскивание и клавиши-стрелки изменяют координаты черновика. Niri может скорректировать пересечения.",
        ));
        help.add_css_class("muted");
        help.set_xalign(0.0);
        help.set_wrap(true);
        root.append(&help);
        let selectors = gtk::Box::new(gtk::Orientation::Vertical, 4);
        selectors.add_css_class("display-selector-list");
        root.append(&selectors);
        root.append(&outputs);

        let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 5);
        let refresh = gtk::Button::with_label("Обновить");
        let apply = gtk::Button::with_label("Применить");
        let confirm = gtk::Button::with_label("Подтвердить");
        let revert = gtk::Button::with_label("Откатить");
        let save = gtk::Button::with_label("Сохранить");
        let reset = gtk::Button::with_label("Сбросить черновик");
        for button in [&refresh, &apply, &confirm, &revert, &save, &reset] {
            buttons.append(button);
        }
        root.append(&buttons);

        let controls = Rc::new(Controls {
            outputs,
            selectors,
            map,
            observed: RefCell::new(Vec::new()),
            selected: Cell::new(None),
            sections: RefCell::new(Vec::new()),
            selector_buttons: RefCell::new(Vec::new()),
            coordinates: RefCell::new(Vec::new()),
            status,
            persistence,
            refresh,
            apply,
            confirm,
            revert,
            save,
            reset,
            choices: RefCell::new(Vec::new()),
            phase: RefCell::new(DisplayPhase::Loading),
            editing_enabled: Cell::new(true),
            supervisor_lost: Cell::new(false),
            on_intent: Rc::new(on_intent),
        });
        *map_host.borrow_mut() = Rc::downgrade(&controls);
        for (button, action) in [
            (&controls.refresh, "refresh"),
            (&controls.apply, "apply"),
            (&controls.confirm, "confirm"),
            (&controls.revert, "revert"),
            (&controls.save, "save"),
            (&controls.reset, "reset"),
        ] {
            let weak = Rc::downgrade(&controls);
            button.connect_clicked(move |_| {
                if let Some(controls) = weak.upgrade() {
                    controls.submit(action);
                }
            });
        }
        let weak = Rc::downgrade(&controls);
        gtk::glib::timeout_add_local(Duration::from_secs(1), move || {
            if let Some(controls) = weak.upgrade() {
                if matches!(*controls.phase.borrow(), DisplayPhase::Pending { .. }) {
                    controls.sync();
                }
                gtk::glib::ControlFlow::Continue
            } else {
                gtk::glib::ControlFlow::Break
            }
        });
        controls.sync();
        Self { root, controls }
    }

    pub fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }

    /// Hosts may restrict mutation while the transaction watchdog is not
    /// supervised outside the UI process. Read-only refresh remains available.
    pub fn set_editing_enabled(&self, enabled: bool) {
        self.controls.editing_enabled.set(enabled);
        self.controls.sync();
    }

    /// A dead supervisor cannot service even read-only Refresh requests.
    /// Unlike ordinary production read-only mode, this failure is terminal
    /// for the current editor session. No request is silently discarded.
    pub fn set_supervisor_lost(&self) {
        self.controls.supervisor_lost.set(true);
        self.controls.editing_enabled.set(false);
        self.controls.sync();
    }

    pub fn set_phase(&self, phase: DisplayPhase) {
        self.controls.set_phase(phase);
    }

    pub fn phase(&self) -> DisplayPhase {
        self.controls.phase.borrow().clone()
    }

    /// Lets the host request rollback when the settings popup becomes hidden.
    pub fn pending_id(&self) -> Option<u64> {
        match &*self.controls.phase.borrow() {
            DisplayPhase::Pending { id, .. } => Some(*id),
            _ => None,
        }
    }

    pub fn set_persistence(&self, status: &str) {
        self.controls
            .persistence
            .set_text(&format!("Владение конфигурацией: {status}"));
    }

    pub fn set_outputs(&self, outputs: Vec<DisplayOutput>) {
        self.controls.set_outputs(outputs);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn output() -> DisplayOutput {
        DisplayOutput {
            connector: "DP-1".into(),
            description: "Monitor".into(),
            enabled: true,
            modes: vec![DisplayMode {
                id: "1920x1080@60.000".into(),
                label: "1920×1080 · 60Hz".into(),
            }],
            mode: "1920x1080@60.000".into(),
            scale: 1.0,
            transform: "normal".into(),
            x: 0,
            y: 0,
            vrr: false,
            vrr_supported: false,
        }
    }

    #[test]
    fn a_display_must_remain_enabled() {
        let one = output();
        assert!(can_apply(&[DisplayChoice::from(&one)]));
        let mut disabled = DisplayChoice::from(&one);
        disabled.enabled = false;
        assert!(!can_apply(&[disabled]));
        assert!(!can_apply(&[]));
    }

    #[test]
    fn invalid_scale_mode_and_position_fail_closed() {
        let mut choice = DisplayChoice::from(&output());
        choice.scale = f64::NAN;
        assert!(!can_apply(&[choice.clone()]));
        choice.scale = 1.0;
        choice.mode.clear();
        assert!(!can_apply(&[choice.clone()]));
        choice.mode = "1920x1080@60.000".into();
        choice.x = 32001;
        assert!(!can_apply(&[choice]));
    }

    #[test]
    fn duplicate_connectors_are_not_applicable() {
        let first = DisplayChoice::from(&output());
        assert!(!can_apply(&[first.clone(), first]));
    }

    #[test]
    fn confirmation_deadline_never_extends() {
        assert_eq!(seconds_left(15_000, 0), 15);
        assert_eq!(seconds_left(15_000, 14_001), 1);
        assert_eq!(seconds_left(15_000, 15_000), 0);
        assert_eq!(seconds_left(15_000, 20_000), 0);
    }
}
