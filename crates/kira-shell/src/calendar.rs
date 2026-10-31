//! An interactive clock/calendar layer surface opened from the Top Bar.
//! All date data is local to GLib; no arbitrary commands or network requests.
use std::cell::Cell;
use std::rc::Rc;

use gtk::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

const TITLE: &str = "Kira Calendar";

fn label(value: &str, css: &str) -> gtk::Label {
    let result = gtk::Label::new(Some(value));
    result.add_css_class(css);
    result.set_xalign(0.0);
    result
}

fn format_date(date: &gtk::glib::DateTime, pattern: &str) -> String {
    date.format(pattern)
        .map(|text| text.to_string())
        .unwrap_or_default()
}

fn refresh_clock(clock: &gtk::Label, date: &gtk::Label, zone: &gtk::Label) {
    if let Ok(now) = gtk::glib::DateTime::now_local() {
        clock.set_text(&format_date(&now, "%H:%M:%S"));
        date.set_text(&format_date(&now, "%A, %d %B %Y"));
        zone.set_text(&format!("LOCAL TIME  /  {}", format_date(&now, "%Z")));
    }
}

fn refresh_selection(calendar: &gtk::Calendar, selected: &gtk::Label, details: &gtk::Label) {
    let date = calendar.date();
    selected.set_text(&format_date(&date, "%A, %d %B %Y"));
    details.set_text(&format!(
        "ISO WEEK {:02}    /    DAY {:03} OF {}",
        date.week_of_year(),
        date.day_of_year(),
        date.year()
    ));
}

fn find_window(application: &gtk::Application) -> Option<gtk::Window> {
    application
        .windows()
        .into_iter()
        .find(|window| window.title().as_deref() == Some(TITLE))
}

/// Close only this overlay. Never touch the persistent 42px layer surface.
pub fn dismiss(application: &gtk::Application) -> bool {
    if let Some(window) = find_window(application) {
        window.close();
        return true;
    }
    false
}

/// Clicking the Top Bar clock toggles the native surface.
pub fn toggle(application: &gtk::Application) {
    if dismiss(application) {
        return;
    }

    // The two transient overlays must never compete for keyboard focus.
    crate::launcher::dismiss(application);

    let provider = gtk::CssProvider::new();
    provider.load_from_data(include_str!("calendar.css"));
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }

    let window = gtk::ApplicationWindow::new(application);
    window.set_title(Some(TITLE));
    window.set_decorated(false);
    window.set_default_size(378, 465);
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
    let close = gtk::Button::with_label("×");
    close.add_css_class("kira-calendar-close");
    close.set_tooltip_text(Some("Close calendar (Esc)"));
    let close_window = window.clone();
    close.connect_clicked(move |_| close_window.close());
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

    let workbench = gtk::Box::new(gtk::Orientation::Vertical, 10);
    workbench.add_css_class("kira-calendar-workbench");
    card.append(&workbench);
    let month_heading = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    month_heading.append(&label("MONTH  /  SELECT A DAY", "kira-calendar-section"));
    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    month_heading.append(&spacer);
    let today_button = gtk::Button::with_label("TODAY");
    today_button.add_css_class("kira-calendar-today-button");
    month_heading.append(&today_button);
    workbench.append(&month_heading);

    let calendar = gtk::Calendar::new();
    calendar.add_css_class("kira-calendar-grid");
    calendar.set_show_day_names(true);
    calendar.set_show_week_numbers(true);
    calendar.set_show_heading(true);
    calendar.set_hexpand(true);
    workbench.append(&calendar);

    let reset_calendar = calendar.clone();
    today_button.connect_clicked(move |_| {
        if let Ok(today) = gtk::glib::DateTime::now_local() {
            reset_calendar.select_day(&today);
        }
    });

    let footer = gtk::Box::new(gtk::Orientation::Vertical, 10);
    footer.add_css_class("kira-calendar-footer");
    card.append(&footer);
    footer.append(&label("SELECTED DATE", "kira-calendar-eyebrow"));
    let selected = label("", "kira-calendar-selected");
    selected.set_ellipsize(gtk::pango::EllipsizeMode::End);
    footer.append(&selected);
    let details = label("", "kira-calendar-meta");
    footer.append(&details);
    refresh_selection(&calendar, &selected, &details);

    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    actions.add_css_class("kira-calendar-actions");
    let hint = label("ESC  /  CLOSE", "kira-calendar-hint");
    hint.set_hexpand(true);
    actions.append(&hint);
    let copy = gtk::Button::with_label("COPY YYYY-MM-DD");
    copy.add_css_class("kira-calendar-copy");
    let copy_calendar = calendar.clone();
    let copy_button = copy.clone();
    copy.connect_clicked(move |_| {
        if let Ok(date) = copy_calendar.date().format("%Y-%m-%d") {
            if let Some(display) = gtk::gdk::Display::default() {
                display.clipboard().set_text(date.as_str());
                copy_button.set_label("COPIED");
            }
        }
    });
    actions.append(&copy);
    footer.append(&actions);

    // Date property changes when selecting days or changing month/year.
    let selected_label = selected.clone();
    let details_label = details.clone();
    let reset_copy = copy.clone();
    calendar.connect_notify_local(Some("date"), move |cal, _| {
        refresh_selection(cal, &selected_label, &details_label);
        reset_copy.set_label("COPY YYYY-MM-DD");
    });

    // Ticking only while the popup exists; no strong window reference is held
    // by the GLib timer, so closing it also ends this source.
    let weak_window = window.downgrade();
    let weak_clock = clock.downgrade();
    let weak_date = today_date.downgrade();
    let weak_zone = timezone.downgrade();
    gtk::glib::timeout_add_seconds_local(1, move || {
        match (
            weak_window.upgrade(),
            weak_clock.upgrade(),
            weak_date.upgrade(),
            weak_zone.upgrade(),
        ) {
            (Some(_), Some(clock), Some(date), Some(zone)) => {
                refresh_clock(&clock, &date, &zone);
                gtk::glib::ControlFlow::Continue
            }
            _ => gtk::glib::ControlFlow::Break,
        }
    });

    let keys = gtk::EventControllerKey::new();
    keys.connect_key_pressed({
        let window = window.clone();
        move |_, key, _, _| {
            if key == gtk::gdk::Key::Escape {
                window.close();
                gtk::glib::Propagation::Stop
            } else {
                gtk::glib::Propagation::Proceed
            }
        }
    });
    window.add_controller(keys);

    // Dismiss on focus loss after activation, but never close before the
    // compositor has given the new overlay its first focus notification.
    let was_active = Rc::new(Cell::new(false));
    window.connect_is_active_notify(move |window| {
        if window.is_active() {
            was_active.set(true);
        } else if was_active.get() {
            window.close();
        }
    });

    window.present();
    calendar.grab_focus();
}

#[cfg(test)]
mod tests {
    #[test]
    fn native_calendar_style_has_editorial_regions() {
        let css = include_str!("calendar.css");
        for selector in [
            ".kira-calendar-hero",
            ".kira-calendar-workbench",
            ".kira-calendar-grid",
            ".kira-calendar-footer",
            ".kira-calendar-copy",
            ".kira-calendar-today-button",
        ] {
            assert!(css.contains(selector), "missing {selector}");
        }
    }
}
