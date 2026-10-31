//! Editorial calendar layer-shell overlay with a native *custom* month grid.
//! Uses GTK Grid/Buttons, not GtkCalendar, so controls, day states and layout
//! belong to Kira while dates remain real local Gregorian dates.
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Once;

use gtk::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use crate::calendar_model::{CivilDate, days_in_month, month_grid, shift_month};

const TITLE: &str = "Kira Calendar";

fn label(value: &str, class: &str) -> gtk::Label {
    let result = gtk::Label::new(Some(value));
    result.add_css_class(class);
    result.set_xalign(0.0);
    result
}

fn format_date(date: &gtk::glib::DateTime, pattern: &str) -> String {
    date.format(pattern)
        .map(|text| text.to_string())
        .unwrap_or_default()
}

fn local_datetime(date: CivilDate) -> Option<gtk::glib::DateTime> {
    gtk::glib::DateTime::from_local(
        date.year,
        i32::from(date.month),
        i32::from(date.day),
        12,
        0,
        0.0,
    )
    .ok()
}

fn today() -> CivilDate {
    gtk::glib::DateTime::now_local()
        .ok()
        .and_then(|now| CivilDate::new(now.year(), now.month() as u8, now.day_of_month() as u8))
        .unwrap_or(CivilDate {
            year: 1970,
            month: 1,
            day: 1,
        })
}

fn locale_date(date: CivilDate, pattern: &str) -> String {
    local_datetime(date)
        .map(|dt| format_date(&dt, pattern))
        .unwrap_or_else(|| date.iso())
}

fn refresh_clock(clock: &gtk::Label, date: &gtk::Label, zone: &gtk::Label) {
    if let Ok(now) = gtk::glib::DateTime::now_local() {
        clock.set_text(&format_date(&now, "%H:%M:%S"));
        date.set_text(&format_date(&now, "%A, %d %B %Y"));
        zone.set_text(&format!("LOCAL TIME  /  {}", format_date(&now, "%Z")));
    }
}

#[derive(Clone, Copy)]
struct PickerState {
    month: u8,
    year: i32,
    selected: CivilDate,
    today: CivilDate,
}

struct Picker {
    state: RefCell<PickerState>,
    grid: gtk::Grid,
    month_text: gtk::Label,
    year_text: gtk::Label,
    selected_text: gtk::Label,
    meta_text: gtk::Label,
    copy: gtk::Button,
    prev_month: gtk::Button,
    next_month: gtk::Button,
    prev_year: gtk::Button,
    next_year: gtk::Button,
}

impl Picker {
    fn navigate(self: &Rc<Self>, months: i32) {
        let mut state = self.state.borrow_mut();
        if let Some((year, month)) = shift_month(state.year, state.month, months) {
            // Keep selection visible and clamp February/short-month dates.
            // Month/year navigation should never strand the highlighted day
            // on an invisible month while the details show another date.
            let day = state.selected.day.min(days_in_month(year, month));
            state.selected = CivilDate::new(year, month, day).expect("valid day");
            state.year = year;
            state.month = month;
            drop(state);
            self.render(false);
        }
    }

    fn select(self: &Rc<Self>, date: CivilDate, focus: bool) {
        let mut state = self.state.borrow_mut();
        state.selected = date;
        state.year = date.year;
        state.month = date.month;
        drop(state);
        self.render(focus);
    }

    fn render(self: &Rc<Self>, focus_selected: bool) {
        let state = *self.state.borrow();
        let month_start = CivilDate::new(state.year, state.month, 1).expect("valid month");
        self.month_text.set_text(&locale_date(month_start, "%B"));
        self.year_text.set_text(&state.year.to_string());
        self.prev_month
            .set_sensitive(shift_month(state.year, state.month, -1).is_some());
        self.next_month
            .set_sensitive(shift_month(state.year, state.month, 1).is_some());
        self.prev_year
            .set_sensitive(shift_month(state.year, state.month, -12).is_some());
        self.next_year
            .set_sensitive(shift_month(state.year, state.month, 12).is_some());
        self.selected_text
            .set_text(&locale_date(state.selected, "%A, %d %B %Y"));
        let (iso_year, iso_week) = state.selected.iso_week();
        self.meta_text.set_text(&format!(
            "ISO WEEK {:02}  /  {}    DAY {:03} / {}",
            iso_week,
            iso_year,
            state.selected.day_of_year(),
            state.selected.year
        ));
        self.copy.set_label("COPY DATE");
        while let Some(child) = self.grid.first_child() {
            self.grid.remove(&child);
        }

        let week_header = label("WK", "kira-grid-header");
        week_header.set_xalign(0.5);
        self.grid.attach(&week_header, 0, 0, 1, 1);
        // 2024-01-01 is a Monday; GLib names respect the user's locale.
        for weekday in 0..7 {
            let reference = CivilDate::new(2024, 1, weekday + 1).expect("weekday");
            let caption = label(&locale_date(reference, "%a"), "kira-grid-header");
            caption.set_xalign(0.5);
            self.grid.attach(&caption, i32::from(weekday) + 1, 0, 1, 1);
        }

        let mut selected_button = None;
        for (row, week) in month_grid(state.year, state.month).iter().enumerate() {
            let week_number = week[0].map(|date| date.iso_week().1).unwrap_or(0);
            let week_label = label(&format!("{week_number:02}"), "kira-grid-week");
            week_label.set_xalign(0.5);
            self.grid.attach(&week_label, 0, row as i32 + 1, 1, 1);

            for (column, date) in week.iter().enumerate() {
                let button = gtk::Button::new();
                button.add_css_class("kira-day");
                button.set_hexpand(true);
                button.set_size_request(34, 36);
                if let Some(date) = date {
                    button.set_label(&date.day.to_string());
                    button.set_tooltip_text(Some(&locale_date(*date, "%A, %d %B %Y")));
                    if date.month != state.month || date.year != state.year {
                        button.add_css_class("kira-day-adjacent");
                    }
                    if *date == state.today {
                        button.add_css_class("kira-day-today");
                    }
                    if *date == state.selected {
                        button.add_css_class("kira-day-selected");
                        button.set_tooltip_text(Some(&format!(
                            "{} / SELECTED",
                            locale_date(*date, "%A, %d %B %Y")
                        )));
                        selected_button = Some(button.clone());
                    }
                    let weak = Rc::downgrade(self);
                    let date = *date;
                    button.connect_clicked(move |_| {
                        if let Some(picker) = weak.upgrade() {
                            picker.select(date, true);
                        }
                    });
                } else {
                    button.set_sensitive(false);
                }
                self.grid
                    .attach(&button, column as i32 + 1, row as i32 + 1, 1, 1);
            }
        }

        if focus_selected {
            if let Some(button) = selected_button {
                button.grab_focus();
            }
        }
    }
}

fn navigation_button(text: &str, tooltip: &str) -> gtk::Button {
    let button = gtk::Button::with_label(text);
    button.add_css_class("kira-calendar-nav");
    button.set_tooltip_text(Some(tooltip));
    button.set_size_request(29, 32);
    button
}

fn find_window(application: &gtk::Application) -> Option<gtk::Window> {
    application
        .windows()
        .into_iter()
        .find(|window| window.title().as_deref() == Some(TITLE))
}

/// Close only this overlay, never the persistent 42px Top Bar.
pub fn dismiss(application: &gtk::Application) -> bool {
    if let Some(window) = find_window(application) {
        window.close();
        return true;
    }
    false
}

fn install_style() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let provider = gtk::CssProvider::new();
        provider.load_from_data(include_str!("calendar.css"));
        if let Some(display) = gtk::gdk::Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }
    });
}

/// Toggled only by the trusted 'kiraCalendar' WebKit handler.
pub fn toggle(application: &gtk::Application) {
    if dismiss(application) {
        return;
    }
    crate::launcher::dismiss(application);
    install_style();

    let window = gtk::ApplicationWindow::new(application);
    window.set_title(Some(TITLE));
    window.set_decorated(false);
    window.set_default_size(378, 490);
    window.add_css_class("kira-calendar-window");
    window.init_layer_shell();
    window.set_namespace(Some("kira-calendar"));
    window.set_layer(Layer::Overlay);
    window.set_anchor(Edge::Top, true);
    window.set_anchor(Edge::Right, true);
    window.set_margin(Edge::Top, 50);
    window.set_margin(Edge::Right, 12);
    window.set_exclusive_zone(0);
    window.set_keyboard_mode(KeyboardMode::OnDemand);

    let card = gtk::Box::new(gtk::Orientation::Vertical, 0);
    card.add_css_class("kira-calendar-card");
    window.set_child(Some(&card));

    let hero = gtk::Box::new(gtk::Orientation::Vertical, 8);
    hero.add_css_class("kira-calendar-hero");
    let hero_top = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let eyebrow = label("KIRA  /  TIME & DATE", "kira-calendar-eyebrow");
    eyebrow.set_hexpand(true);
    hero_top.append(&eyebrow);
    let close = navigation_button("×", "Close calendar (Esc)");
    close.add_css_class("kira-calendar-close");
    let close_window = window.downgrade();
    close.connect_clicked(move |_| {
        if let Some(window) = close_window.upgrade() {
            window.close();
        }
    });
    hero_top.append(&close);
    hero.append(&hero_top);

    let clock = label("", "kira-calendar-time");
    hero.append(&clock);
    let today_date = label("", "kira-calendar-today-date");
    hero.append(&today_date);
    let timezone = label("", "kira-calendar-timezone");
    hero.append(&timezone);
    card.append(&hero);
    refresh_clock(&clock, &today_date, &timezone);

    let workbench = gtk::Box::new(gtk::Orientation::Vertical, 12);
    workbench.add_css_class("kira-calendar-workbench");
    card.append(&workbench);
    let heading = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    heading.append(&label("CALENDAR  /  SELECT A DAY", "kira-calendar-section"));
    let gap = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    gap.set_hexpand(true);
    heading.append(&gap);
    let reset = gtk::Button::with_label("↺  TODAY");
    reset.add_css_class("kira-calendar-today-button");
    reset.set_tooltip_text(Some("Go to current local date"));
    heading.append(&reset);
    workbench.append(&heading);

    // One month and one year control group, both using the same custom
    // navigation language. No GtkCalendar heading/buttons are involved.
    let toolbar = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    toolbar.add_css_class("kira-calendar-toolbar");
    let prev_month = navigation_button("‹", "Previous month");
    let next_month = navigation_button("›", "Next month");
    let prev_year = navigation_button("‹", "Previous year");
    let next_year = navigation_button("›", "Next year");
    let month_text = label("", "kira-calendar-month");
    month_text.set_xalign(0.5);
    month_text.set_hexpand(true);
    let year_text = label("", "kira-calendar-year");
    year_text.set_xalign(0.5);
    toolbar.append(&prev_month);
    toolbar.append(&month_text);
    toolbar.append(&next_month);
    let divider = gtk::Separator::new(gtk::Orientation::Vertical);
    divider.add_css_class("kira-calendar-toolbar-divider");
    toolbar.append(&divider);
    toolbar.append(&prev_year);
    toolbar.append(&year_text);
    toolbar.append(&next_year);
    workbench.append(&toolbar);

    let grid = gtk::Grid::new();
    grid.add_css_class("kira-calendar-grid");
    grid.set_column_homogeneous(true);
    grid.set_row_spacing(3);
    grid.set_column_spacing(3);
    grid.set_hexpand(true);
    workbench.append(&grid);

    let footer = gtk::Box::new(gtk::Orientation::Vertical, 9);
    footer.add_css_class("kira-calendar-footer");
    card.append(&footer);
    footer.append(&label("SELECTED DATE", "kira-calendar-eyebrow"));
    let selected_text = label("", "kira-calendar-selected");
    selected_text.set_ellipsize(gtk::pango::EllipsizeMode::End);
    footer.append(&selected_text);
    let meta_text = label("", "kira-calendar-meta");
    footer.append(&meta_text);

    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    actions.add_css_class("kira-calendar-actions");
    let hint = label("ESC  /  CLOSE", "kira-calendar-hint");
    hint.set_hexpand(true);
    actions.append(&hint);
    let copy = gtk::Button::with_label("COPY DATE");
    copy.add_css_class("kira-calendar-copy");
    copy.set_tooltip_text(Some("Copy selected date as YYYY-MM-DD"));
    actions.append(&copy);
    footer.append(&actions);

    let date = today();
    let picker = Rc::new(Picker {
        state: RefCell::new(PickerState {
            month: date.month,
            year: date.year,
            selected: date,
            today: date,
        }),
        grid,
        month_text,
        year_text,
        selected_text,
        meta_text,
        copy: copy.clone(),
        prev_month: prev_month.clone(),
        next_month: next_month.clone(),
        prev_year: prev_year.clone(),
        next_year: next_year.clone(),
    });
    picker.render(false);

    for (button, months) in [
        (&prev_month, -1),
        (&next_month, 1),
        (&prev_year, -12),
        (&next_year, 12),
    ] {
        let weak = Rc::downgrade(&picker);
        button.connect_clicked(move |_| {
            if let Some(picker) = weak.upgrade() {
                picker.navigate(months);
            }
        });
    }
    let weak = Rc::downgrade(&picker);
    reset.connect_clicked(move |_| {
        if let Some(picker) = weak.upgrade() {
            picker.select(today(), true);
        }
    });
    let weak = Rc::downgrade(&picker);
    copy.connect_clicked(move |_| {
        if let Some(picker) = weak.upgrade() {
            let value = picker.state.borrow().selected.iso();
            if let Some(display) = gtk::gdk::Display::default() {
                display.clipboard().set_text(&value);
                picker.copy.set_label("COPIED ✓");
            }
        }
    });

    // Keep the model alive for as long as its GTK window lives. The day and
    // navigation callbacks are Weak<Picker>, so there is no widget reference
    // cycle when the window is closed.
    let owner = picker.clone();
    window.connect_close_request(move |_| {
        let _ = &owner;
        gtk::glib::Propagation::Proceed
    });

    let weak_window = window.downgrade();
    let weak_clock = clock.downgrade();
    let weak_date = today_date.downgrade();
    let weak_zone = timezone.downgrade();
    let weak_picker = Rc::downgrade(&picker);
    gtk::glib::timeout_add_seconds_local(1, move || {
        match (
            weak_window.upgrade(),
            weak_clock.upgrade(),
            weak_date.upgrade(),
            weak_zone.upgrade(),
            weak_picker.upgrade(),
        ) {
            (Some(_), Some(clock), Some(date), Some(zone), Some(picker)) => {
                refresh_clock(&clock, &date, &zone);
                let current = today();
                if picker.state.borrow().today != current {
                    picker.state.borrow_mut().today = current;
                    picker.render(false);
                }
                gtk::glib::ControlFlow::Continue
            }
            _ => gtk::glib::ControlFlow::Break,
        }
    });

    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    let key_picker = Rc::downgrade(&picker);
    let key_window = window.downgrade();
    keys.connect_key_pressed(move |_, key, _, modifiers| {
        if key == gtk::gdk::Key::Escape {
            if let Some(window) = key_window.upgrade() {
                window.close();
            }
            return gtk::glib::Propagation::Stop;
        }
        let Some(picker) = key_picker.upgrade() else {
            return gtk::glib::Propagation::Proceed;
        };
        let selected = picker.state.borrow().selected;
        let next = match key {
            gtk::gdk::Key::Left => selected.shift_days(-1),
            gtk::gdk::Key::Right => selected.shift_days(1),
            gtk::gdk::Key::Up => selected.shift_days(-7),
            gtk::gdk::Key::Down => selected.shift_days(7),
            gtk::gdk::Key::Home => Some(today()),
            gtk::gdk::Key::Page_Up => {
                picker.navigate(
                    if modifiers.contains(gtk::gdk::ModifierType::CONTROL_MASK) {
                        -12
                    } else {
                        -1
                    },
                );
                return gtk::glib::Propagation::Stop;
            }
            gtk::gdk::Key::Page_Down => {
                picker.navigate(
                    if modifiers.contains(gtk::gdk::ModifierType::CONTROL_MASK) {
                        12
                    } else {
                        1
                    },
                );
                return gtk::glib::Propagation::Stop;
            }
            _ => return gtk::glib::Propagation::Proceed,
        };
        if let Some(date) = next {
            picker.select(date, true);
        }
        gtk::glib::Propagation::Stop
    });
    window.add_controller(keys);

    let was_active = Rc::new(Cell::new(false));
    window.connect_is_active_notify(move |window| {
        if window.is_active() {
            was_active.set(true);
        } else if was_active.get() {
            window.close();
        }
    });
    window.present();
}

#[cfg(test)]
mod tests {
    #[test]
    fn custom_picker_style_contract() {
        let css = include_str!("calendar.css");
        for selector in [
            ".kira-calendar-hero",
            ".kira-calendar-workbench",
            ".kira-calendar-grid",
            "button.kira-day",
            ".kira-day-selected",
            ".kira-day-today",
            ".kira-calendar-nav",
            ".kira-calendar-copy",
        ] {
            assert!(css.contains(selector), "missing {selector}");
        }
    }
}
