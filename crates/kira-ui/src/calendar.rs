//! Reusable native GTK4 Calendar: the main branch's date and interaction model.
//! Clipboard ownership and Niri windows stay outside kira-ui.
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gtk::prelude::*;

use crate::calendar_model::{CivilDate, MONTHS, WEEKDAYS, current_local_date, month_grid};
use crate::clock_digits::ClockDigits;
use crate::motion::CalendarFragmentScene;
use crate::popup_section::PopupSection;

type CopyDate = Rc<dyn Fn(String) -> Result<(), String>>;

struct State {
    days: Vec<gtk::Button>,
    dates: RefCell<Vec<Option<CivilDate>>>,
    weeks: Vec<gtk::Label>,
    selected: Cell<CivilDate>,
    today: Cell<CivilDate>,
    title: gtk::Label,
    date_today: gtk::Label,
    clock: ClockDigits,
    clock_button: gtk::Button,
    selected_title: gtk::Label,
    selected_iso: gtk::Label,
    feedback: gtk::Label,
    copy_button: gtk::Button,
    navigation: Vec<(i32, gtk::Button)>,
    grid_scene: Rc<CalendarFragmentScene>,
    grid_surface: PopupSection,
    copy: CopyDate,
}

impl State {
    fn choose(&self, date: CivilDate) {
        let previous = self.selected.replace(date);
        self.feedback.set_text("");
        self.copy_button.set_label("КОПИРОВАТЬ");
        self.render();
        if previous != date {
            let direction = if (date.year, date.month, date.day)
                < (previous.year, previous.month, previous.day)
            {
                -1
            } else {
                1
            };
            let changed_month = date.month != previous.month || date.year != previous.year;
            let index = self
                .dates
                .borrow()
                .iter()
                .position(|day| *day == Some(date));
            if let Some(index) = index
                && let Some(button) = self.days.get(index)
            {
                self.grid_scene.play_at(direction, &button.clone().upcast());
            }
            if changed_month {
                // Contact geometry is captured before the compositor surface
                // begins its spring from the original 8px/2px position.
                self.grid_surface.play_calendar_grid(direction);
            }
        }
    }

    fn move_days(&self, days: i32) {
        if let Some(date) = self.selected.get().shift_days(days) {
            self.choose(date);
            self.focus_selected();
        }
    }

    fn move_months(&self, months: i32) {
        if let Some(date) = self.selected.get().shift_month(months) {
            self.choose(date);
            self.focus_selected();
        }
    }

    fn focus_selected(&self) {
        let selected = self.selected.get();
        if let Some(index) = self
            .dates
            .borrow()
            .iter()
            .position(|date| *date == Some(selected))
            && let Some(button) = self.days.get(index)
        {
            button.grab_focus();
        }
    }

    fn render(&self) {
        let selected = self.selected.get();
        let today = self.today.get();
        self.title.set_text(&format!(
            "{} {}",
            MONTHS[usize::from(selected.month - 1)],
            selected.year
        ));
        self.selected_title.set_text(&selected.short_label());
        self.selected_iso.set_text(&selected.iso());

        for (months, button) in &self.navigation {
            button.set_sensitive(selected.shift_month(*months).is_some());
        }

        let Some(grid) = month_grid(selected.year, selected.month) else {
            return;
        };
        let mut dates = self.dates.borrow_mut();
        for (week_index, week) in grid.iter().enumerate() {
            let week_number = week[0].map(|day| day.iso_week().1);
            self.weeks[week_index]
                .set_text(&week_number.map_or_else(String::new, |number| format!("{number:02}")));
            for (day_index, day) in week.iter().enumerate() {
                let index = week_index * 7 + day_index;
                dates[index] = *day;
                let button = &self.days[index];
                if let Some(date) = day {
                    button.set_visible(true);
                    button.set_label(&date.day.to_string());
                    let state = date.state(selected, today, selected.year, selected.month);
                    button.set_tooltip_text(Some(&date.full_label()));
                    button.update_property(&[gtk::accessible::Property::Label(&date.full_label())]);
                    for (class, active) in [
                        ("selected", state.selected),
                        ("today", state.today),
                        ("outside", state.outside),
                    ] {
                        if active {
                            button.add_css_class(class);
                        } else {
                            button.remove_css_class(class);
                        }
                    }
                } else {
                    button.set_visible(false);
                }
            }
        }
    }

    fn refresh_clock(&self) {
        if let Ok(now) = gtk::glib::DateTime::now_local() {
            if let Ok(clock) = now.format("%H:%M:%S") {
                self.clock.set_time(&clock);
                let accessible = format!("Анимировать цифры часов, текущее время {clock}");
                self.clock_button.update_property(&[
                    gtk::accessible::Property::Label(&accessible),
                ]);
            }
            if let Some(today) = current_local_date() {
                if self.today.replace(today) != today {
                    self.render();
                }
                self.date_today.set_text(&today.full_label());
            }
        }
    }

    fn copy_selected(&self) {
        let iso = self.selected.get().iso();
        match (self.copy)(iso.clone()) {
            Ok(()) => {
                self.copy_button.set_label("СКОПИРОВАНО");
                self.feedback
                    .set_text(&format!("Дата {iso} передана в буфер обмена"));
            }
            Err(error) => {
                self.copy_button.set_label("КОПИРОВАТЬ");
                self.feedback
                    .set_text(&format!("НЕ УДАЛОСЬ СКОПИРОВАТЬ: {error}"));
            }
        }
    }
}

pub struct Calendar {
    root: gtk::Box,
    state: Rc<State>,
}

fn labeled_button(label: &str, tooltip: &str) -> gtk::Button {
    let button = gtk::Button::with_label(label);
    button.set_tooltip_text(Some(tooltip));
    button.update_property(&[gtk::accessible::Property::Label(tooltip)]);
    button
}

impl Calendar {
    pub fn new(copy: impl Fn(String) -> Result<(), String> + 'static) -> Self {
        let today =
            current_local_date().expect("a supported local civil date (year 1..9999) is required");
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.add_css_class("kira-calendar");

        let header = gtk::Box::new(gtk::Orientation::Vertical, 4);
        header.add_css_class("calendar-heading");
        let clock = ClockDigits::new();
        let clock_button = gtk::Button::new();
        clock_button.add_css_class("calendar-clock-play");
        clock_button.set_tooltip_text(Some("Анимировать цифры часов"));
        clock_button
            .update_property(&[gtk::accessible::Property::Label("Анимировать цифры часов")]);
        clock_button.set_child(Some(&clock.widget()));
        header.append(&clock_button);
        let date_today = gtk::Label::new(None);
        date_today.set_xalign(0.0);
        date_today.add_css_class("muted");
        header.append(&date_today);
        root.append(&header);

        let section = gtk::Box::new(gtk::Orientation::Vertical, 10);
        section.add_css_class("calendar-section");
        let navigation_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let title = gtk::Label::new(None);
        title.set_xalign(0.0);
        title.set_hexpand(true);
        title.add_css_class("calendar-month");
        navigation_row.append(&title);
        let navigation = vec![
            (-12, labeled_button("«", "Предыдущий год")),
            (-1, labeled_button("‹", "Предыдущий месяц")),
            (1, labeled_button("›", "Следующий месяц")),
            (12, labeled_button("»", "Следующий год")),
        ];
        for (_, button) in &navigation {
            button.add_css_class("calendar-nav");
            navigation_row.append(button);
        }
        section.append(&navigation_row);

        let grid = gtk::Grid::new();
        grid.set_column_homogeneous(true);
        grid.set_row_homogeneous(true);
        grid.add_css_class("calendar-grid");
        let week_caption = gtk::Label::new(Some("НЕД"));
        week_caption.add_css_class("calendar-week-header");
        week_caption.set_tooltip_text(Some("Номер недели"));
        grid.attach(&week_caption, 0, 0, 1, 1);
        for (i, weekday) in WEEKDAYS.iter().enumerate() {
            let label = gtk::Label::new(Some(weekday));
            label.add_css_class("calendar-weekday");
            grid.attach(&label, i as i32 + 1, 0, 1, 1);
        }

        let mut days = Vec::with_capacity(42);
        let mut weeks = Vec::with_capacity(6);
        for week in 0..6 {
            let week_number = gtk::Label::new(None);
            week_number.add_css_class("calendar-week-number");
            week_number.set_tooltip_text(Some("Номер недели ISO 8601"));
            grid.attach(&week_number, 0, week + 1, 1, 1);
            weeks.push(week_number);
            for day in 0..7 {
                let button = gtk::Button::with_label("");
                button.add_css_class("calendar-day");
                button.set_size_request(32, 34);
                grid.attach(&button, day + 1, week + 1, 1, 1);
                days.push(button);
            }
        }
        let grid_scene = Rc::new(CalendarFragmentScene::new());
        let grid_surface = PopupSection::calendar_grid(&grid.clone().upcast());
        let grid_overlay = gtk::Overlay::new();
        grid_overlay.set_child(Some(&grid_surface));
        let traces = grid_scene.widget();
        grid_overlay.add_overlay(&traces);
        grid_overlay.set_measure_overlay(&traces, false);
        grid_overlay.set_clip_overlay(&traces, false);
        section.append(&grid_overlay);
        root.append(&section);

        let footer = gtk::Box::new(gtk::Orientation::Vertical, 8);
        footer.add_css_class("calendar-footer");
        let selected_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let selected_title = gtk::Label::new(None);
        selected_title.set_xalign(0.0);
        selected_title.set_hexpand(true);
        selected_title.add_css_class("calendar-selected-title");
        selected_row.append(&selected_title);
        let selected_iso = gtk::Label::new(None);
        selected_iso.add_css_class("calendar-iso");
        selected_row.append(&selected_iso);
        footer.append(&selected_row);

        let footer_actions = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let today_button = labeled_button("СЕГОДНЯ", "Выбрать сегодняшнюю дату");
        today_button.add_css_class("calendar-footer-action");
        today_button.set_hexpand(true);
        today_button.set_halign(gtk::Align::Start);
        footer_actions.append(&today_button);
        let copy_button = labeled_button(
            "КОПИРОВАТЬ",
            "Скопировать выбранную дату в формате ISO 8601",
        );
        copy_button.add_css_class("calendar-footer-action");
        footer_actions.append(&copy_button);
        footer.append(&footer_actions);
        let feedback = gtk::Label::new(None);
        feedback.add_css_class("calendar-feedback");
        feedback.set_xalign(0.0);
        feedback.set_wrap(true);
        footer.append(&feedback);
        root.append(&footer);

        let state = Rc::new(State {
            days,
            dates: RefCell::new(vec![None; 42]),
            weeks,
            selected: Cell::new(today),
            today: Cell::new(today),
            title,
            date_today,
            clock,
            clock_button: clock_button.clone(),
            selected_title,
            selected_iso,
            feedback,
            copy_button: copy_button.clone(),
            navigation,
            grid_scene,
            grid_surface,
            copy: Rc::new(copy),
        });

        for (step, button) in &state.navigation {
            let weak = Rc::downgrade(&state);
            let step = *step;
            button.connect_clicked(move |_| {
                if let Some(state) = weak.upgrade() {
                    state.move_months(step);
                }
            });
        }
        for (index, button) in state.days.iter().enumerate() {
            let weak = Rc::downgrade(&state);
            button.connect_clicked(move |_| {
                if let Some(state) = weak.upgrade() {
                    let date = state.dates.borrow().get(index).copied().flatten();
                    if let Some(date) = date {
                        state.choose(date);
                    }
                }
            });
            let key = gtk::EventControllerKey::new();
            let weak = Rc::downgrade(&state);
            key.connect_key_pressed(move |_, keyval, _, modifiers| {
                let Some(state) = weak.upgrade() else {
                    return gtk::glib::Propagation::Proceed;
                };
                let days = match keyval {
                    gtk::gdk::Key::Left => Some(-1),
                    gtk::gdk::Key::Right => Some(1),
                    gtk::gdk::Key::Up => Some(-7),
                    gtk::gdk::Key::Down => Some(7),
                    _ => None,
                };
                if let Some(days) = days {
                    state.move_days(days);
                } else if keyval == gtk::gdk::Key::Home {
                    state.choose(state.today.get());
                    state.focus_selected();
                } else if keyval == gtk::gdk::Key::Page_Up || keyval == gtk::gdk::Key::Page_Down {
                    let direction = if keyval == gtk::gdk::Key::Page_Up {
                        -1
                    } else {
                        1
                    };
                    let increment = if modifiers.contains(gtk::gdk::ModifierType::CONTROL_MASK) {
                        12
                    } else {
                        1
                    };
                    state.move_months(direction * increment);
                } else {
                    return gtk::glib::Propagation::Proceed;
                }
                gtk::glib::Propagation::Stop
            });
            button.add_controller(key);
        }
        today_button.connect_clicked({
            let weak = Rc::downgrade(&state);
            move |_| {
                if let Some(state) = weak.upgrade() {
                    state.choose(state.today.get());
                    state.focus_selected();
                }
            }
        });
        copy_button.connect_clicked({
            let weak = Rc::downgrade(&state);
            move |_| {
                if let Some(state) = weak.upgrade() {
                    state.copy_selected();
                }
            }
        });
        clock_button.connect_clicked({
            let weak = Rc::downgrade(&state);
            move |_| {
                if let Some(state) = weak.upgrade() {
                    state.clock.play_juggle();
                }
            }
        });
        state.render();
        state.refresh_clock();
        gtk::glib::timeout_add_local(Duration::from_secs(1), {
            let weak = Rc::downgrade(&state);
            move || {
                if let Some(state) = weak.upgrade() {
                    state.refresh_clock();
                    gtk::glib::ControlFlow::Continue
                } else {
                    gtk::glib::ControlFlow::Break
                }
            }
        });
        Self { root, state }
    }

    pub fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }

    pub fn selected(&self) -> CivilDate {
        self.state.selected.get()
    }

    pub fn select(&self, date: CivilDate) {
        self.state.choose(date);
    }

    pub fn focus_selected(&self) {
        self.state.focus_selected();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selection_civil_date_is_separate_from_live_clock() {
        let chosen = CivilDate::new(2026, 11, 4).unwrap();
        let now = CivilDate::new(2026, 11, 7).unwrap();
        assert!(!chosen.state(chosen, now, 2026, 11).today);
        assert!(chosen.state(chosen, now, 2026, 11).selected);
    }
}
