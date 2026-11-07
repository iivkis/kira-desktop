//! Reusable live-clock digits: the factual value changes immediately.
//! Visual old/new tracks use the saved 400ms forward/inverted history from main.
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk::prelude::*;

use crate::motion::{CLOCK_DIGIT_DURATION_MS, MotionToken, clock_digit_frame};

struct DigitCell {
    slot: gtk::Overlay,
    layer: gtk::Fixed,
    live: gtk::Label,
    ghost: gtk::Label,
    value: Cell<char>,
    motion: MotionToken,
}

impl DigitCell {
    fn new() -> Self {
        let slot = gtk::Overlay::new();
        slot.set_overflow(gtk::Overflow::Hidden);
        slot.add_css_class("calendar-digit-slot");
        // An invisible real GTK label, not a fixed-width pixel measurement,
        // determines the slot width and height at the current font/scale.
        let reserve = gtk::Label::builder()
            .label("8")
            .accessible_role(gtk::AccessibleRole::Presentation)
            .build();
        reserve.add_css_class("calendar-digit");
        reserve.set_opacity(0.0);
        slot.set_child(Some(&reserve));
        let layer = gtk::Fixed::new();
        layer.set_halign(gtk::Align::Fill);
        layer.set_valign(gtk::Align::Fill);
        slot.add_overlay(&layer);
        let live = gtk::Label::builder()
            .label("0")
            .accessible_role(gtk::AccessibleRole::Presentation)
            .build();
        live.add_css_class("calendar-digit");
        live.set_halign(gtk::Align::Start);
        live.set_valign(gtk::Align::Start);
        let ghost = gtk::Label::builder()
            .accessible_role(gtk::AccessibleRole::Presentation)
            .build();
        ghost.add_css_class("calendar-digit");
        ghost.add_css_class("muted");
        ghost.set_halign(gtk::Align::Start);
        ghost.set_valign(gtk::Align::Start);
        ghost.set_opacity(0.0);
        ghost.set_can_target(false);
        layer.put(&live, 0.0, 0.0);
        layer.put(&ghost, 0.0, 0.0);
        Self {
            slot,
            layer,
            live,
            ghost,
            value: Cell::new('0'),
            motion: MotionToken::default(),
        }
    }

    fn reset(&self) {
        self.motion.cancel();
        self.layer.move_(&self.live, 0.0, 0.0);
        self.layer.move_(&self.ghost, 0.0, 0.0);
        self.live.set_opacity(1.0);
        self.ghost.set_opacity(0.0);
    }

    fn update(&self, next: char, animate: bool) {
        let previous = self.value.replace(next);
        if previous == next {
            return;
        }
        self.reset();
        self.live.set_text(&next.to_string());
        if !animate || !self.slot.is_mapped() {
            return;
        }
        self.ghost.set_text(&previous.to_string());
        let height = self.slot.height().max(self.live.height()).max(1) as f64;
        let layer = self.layer.clone();
        let live = self.live.clone();
        let ghost = self.ghost.clone();
        let cleanup_layer = self.layer.clone();
        let cleanup_live = self.live.clone();
        let cleanup_ghost = self.ghost.clone();
        let surface: gtk::Widget = self.slot.clone().upcast();
        self.motion.run(
            &surface,
            CLOCK_DIGIT_DURATION_MS,
            move |progress| {
                let sample = clock_digit_frame(progress);
                layer.move_(&live, 0.0, sample.incoming_y * height);
                layer.move_(&ghost, 0.0, sample.outgoing_y * height);
                live.set_opacity(sample.incoming_opacity);
                ghost.set_opacity(sample.outgoing_opacity);
            },
            move || {
                cleanup_layer.move_(&cleanup_live, 0.0, 0.0);
                cleanup_layer.move_(&cleanup_ghost, 0.0, 0.0);
                cleanup_live.set_opacity(1.0);
                cleanup_ghost.set_opacity(0.0);
            },
        );
    }
}

/// GTK-only clock view; callers supply real local clock text independently
/// of animation. The view has no timer, IPC, locale or system dependency.
pub struct ClockDigits {
    root: gtk::Box,
    digits: Rc<Vec<Option<DigitCell>>>,
    initialized: Cell<bool>,
    text: RefCell<String>,
    settings_handler: Option<(gtk::Settings, gtk::glib::SignalHandlerId)>,
}

impl ClockDigits {
    pub fn new() -> Self {
        let root = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        root.add_css_class("calendar-clock");
        let mut digits = Vec::with_capacity(8);
        for symbol in "00:00:00".chars() {
            if symbol == ':' {
                let colon = gtk::Label::builder()
                    .label(":")
                    .accessible_role(gtk::AccessibleRole::Presentation)
                    .build();
                colon.add_css_class("calendar-digit");
                root.append(&colon);
                digits.push(None);
            } else {
                let cell = DigitCell::new();
                root.append(&cell.slot);
                digits.push(Some(cell));
            }
        }
        let digits = Rc::new(digits);
        let settings_handler = gtk::Settings::default().map(|settings| {
            let weak = Rc::downgrade(&digits);
            let handler = settings.connect_notify_local(
                Some("gtk-enable-animations"),
                move |settings, _| {
                    if !settings.property::<bool>("gtk-enable-animations")
                        && let Some(digits) = weak.upgrade()
                    {
                        for cell in digits.iter().flatten() {
                            cell.reset();
                        }
                    }
                },
            );
            (settings, handler)
        });
        Self {
            root,
            digits,
            initialized: Cell::new(false),
            text: RefCell::new(String::new()),
            settings_handler,
        }
    }

    pub fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }

    pub fn set_time(&self, time: &str) {
        // Malformed or unavailable time must never animate speculative digits.
        if time.len() != 8 || time.as_bytes()[2] != b':' || time.as_bytes()[5] != b':'
            || !time.bytes().enumerate().all(|(i, b)| i == 2 || i == 5 || b.is_ascii_digit())
        {
            self.clear_motion();
            return;
        }
        if self.text.borrow().as_str() == time {
            return;
        }
        let animate = self.initialized.replace(true);
        for (symbol, cell) in time.chars().zip(self.digits.iter()) {
            if let Some(cell) = cell {
                cell.update(symbol, animate);
            }
        }
        self.text.replace(time.to_owned());
    }

    pub fn clear_motion(&self) {
        for cell in self.digits.iter().flatten() {
            cell.reset();
        }
    }
}

impl Drop for ClockDigits {
    fn drop(&mut self) {
        if let Some((settings, handler)) = self.settings_handler.take() {
            settings.disconnect(handler);
        }
        self.clear_motion();
    }
}

impl Default for ClockDigits {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn clock_digit_seconds_retain_two_colons() {
        let value = "23:59:08";
        assert_eq!(value.chars().filter(|c| *c == ':').count(), 2);
        assert_eq!(value.len(), 8);
    }
}
