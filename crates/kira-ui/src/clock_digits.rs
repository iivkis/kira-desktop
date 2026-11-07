//! Reusable live-clock digits: the factual value changes immediately.
//! Visual old/new tracks use the saved 400ms forward/inverted history from main.
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

use gtk::prelude::*;

use crate::motion::{
    CLOCK_DIGIT_DURATION_MS, CLOCK_JUGGLE_FLIGHT_MS, CLOCK_JUGGLE_STAGGER_MS,
    ClockFlight, MotionToken, clock_digit_frame, clock_juggle_frame, next_clock_flights,
};

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
        // Moving old/new glyphs must not change the slot's requisition.
        slot.set_measure_overlay(&layer, false);
        slot.set_clip_overlay(&layer, true);
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
        self.layer.set_child_transform(&self.live, None);
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
        let layer = self.layer.downgrade();
        let live = self.live.downgrade();
        let ghost = self.ghost.downgrade();
        let cleanup_layer = self.layer.downgrade();
        let cleanup_live = self.live.downgrade();
        let cleanup_ghost = self.ghost.downgrade();
        let surface: gtk::Widget = self.slot.clone().upcast();
        self.motion.run(
            &surface,
            CLOCK_DIGIT_DURATION_MS,
            move |progress| {
                let sample = clock_digit_frame(progress);
                if let (Some(layer), Some(live), Some(ghost)) =
                    (layer.upgrade(), live.upgrade(), ghost.upgrade())
                {
                    layer.move_(&live, 0.0, sample.incoming_y * height);
                    layer.move_(&ghost, 0.0, sample.outgoing_y * height);
                    live.set_opacity(sample.incoming_opacity);
                    ghost.set_opacity(sample.outgoing_opacity);
                }
            },
            move || {
                if let (Some(layer), Some(live), Some(ghost)) = (
                    cleanup_layer.upgrade(),
                    cleanup_live.upgrade(),
                    cleanup_ghost.upgrade(),
                ) {
                    layer.move_(&live, 0.0, 0.0);
                    layer.move_(&ghost, 0.0, 0.0);
                    live.set_opacity(1.0);
                    ghost.set_opacity(0.0);
                }
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
    juggle_motion: MotionToken,
    previous_flights: RefCell<Vec<ClockFlight>>,
    juggle_nonce: Cell<u64>,
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
        let juggle_motion = MotionToken::default();
        let settings_handler = gtk::Settings::default().map(|settings| {
            let jog = juggle_motion.clone();
            let weak = Rc::downgrade(&digits);
            let handler = settings.connect_notify_local(
                Some("gtk-enable-animations"),
                move |settings, _| {
                    if !settings.property::<bool>("gtk-enable-animations")
                        && let Some(digits) = weak.upgrade()
                    {
                        jog.cancel();
                        for cell in digits.iter().flatten() {
                            cell.reset();
                            cell.slot.set_overflow(gtk::Overflow::Hidden);
                            cell.slot.set_clip_overlay(&cell.layer, true);
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
            juggle_motion,
            previous_flights: RefCell::new(Vec::new()),
            juggle_nonce: Cell::new(0),
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
        self.stop_juggle();
        let animate = self.initialized.replace(true);
        for (symbol, cell) in time.chars().zip(self.digits.iter()) {
            if let Some(cell) = cell {
                cell.update(symbol, animate);
            }
        }
        self.text.replace(time.to_owned());
    }

    fn stop_juggle(&self) {
        self.juggle_motion.cancel();
        for cell in self.digits.iter().flatten() {
            cell.layer.set_child_transform(&cell.live, None);
            cell.live.set_opacity(1.0);
            cell.slot.set_overflow(gtk::Overflow::Hidden);
            cell.slot.set_clip_overlay(&cell.layer, true);
        }
    }

    /// Source-equivalent click juggling. Every click saves six new flights;
    /// the single outgoing history is replayed backwards, including stagger.
    /// Motion changes only GTK transforms. The clock's factual text never waits.
    pub fn play_juggle(&self) {
        self.clear_motion();
        if !self.root.is_mapped() {
            return;
        }
        let nonce = self.juggle_nonce.get().wrapping_add(1);
        self.juggle_nonce.set(nonce);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos() as u64);
        let mut seed = nanos ^ nonce.wrapping_mul(0x9e37_79b9_7f4a_7c15);
        if seed == 0 {
            seed = 0xa076_1d64_78bd_642f;
        }
        let flights = {
            let previous = self.previous_flights.borrow();
            next_clock_flights(6, &previous, || {
                seed ^= seed >> 12;
                seed ^= seed << 25;
                seed ^= seed >> 27;
                let sample = seed.wrapping_mul(0x2545_f491_4f6c_dd1d);
                (sample >> 11) as f64 / (1u64 << 53) as f64
            })
        };
        self.previous_flights.replace(flights.clone());

        // Weak widget handles prevent a frame-clock callback from keeping
        // the entire GTK clock tree alive after its owner has been dropped.
        // The digit's normal clip is only disabled for this bounded click
        // scene. Re-enable on cancel/complete, including reduced motion.
        for cell in self.digits.iter().flatten() {
            cell.slot.set_overflow(gtk::Overflow::Visible);
            cell.slot.set_clip_overlay(&cell.layer, false);
        }
        let handles: Vec<_> = self
            .digits
            .iter()
            .flatten()
            .map(|cell| {
                (
                    cell.slot.downgrade(),
                    cell.layer.downgrade(),
                    cell.live.downgrade(),
                )
            })
            .collect();
        let on_finish = handles.clone();
        let half_ms = CLOCK_JUGGLE_FLIGHT_MS
            + CLOCK_JUGGLE_STAGGER_MS * (flights.len().saturating_sub(1)) as f64;
        let total_ms = half_ms * 2.0;
        let target: gtk::Widget = self.root.clone().upcast();
        self.juggle_motion.run(
            &target,
            total_ms.ceil() as u64,
            move |cursor| {
                let elapsed = cursor * total_ms;
                let outward_ms = if elapsed <= half_ms {
                    elapsed
                } else {
                    total_ms - elapsed
                };
                for (index, (_, layer, live)) in handles.iter().enumerate() {
                    let (Some(layer), Some(live)) = (layer.upgrade(), live.upgrade()) else {
                        continue;
                    };
                    let local = ((outward_ms - index as f64 * CLOCK_JUGGLE_STAGGER_MS)
                        / CLOCK_JUGGLE_FLIGHT_MS)
                        .clamp(0.0, 1.0);
                    let frame = clock_juggle_frame(flights[index], local * 0.5);
                    let center_x = live.width().max(1) as f32 * 0.5;
                    let center_y = live.height().max(1) as f32 * 0.5;
                    let transform = gtk::gsk::Transform::new()
                        .translate(&gtk::graphene::Point::new(
                            center_x + frame.x as f32,
                            center_y + frame.y as f32,
                        ))
                        .rotate(frame.rotation as f32)
                        .scale(frame.scale as f32, frame.scale as f32)
                        .translate(&gtk::graphene::Point::new(-center_x, -center_y));
                    layer.set_child_transform(&live, Some(&transform));
                    live.set_opacity(frame.opacity);
                }
            },
            move || {
                for (slot, layer, live) in &on_finish {
                    if let (Some(slot), Some(layer), Some(live)) =
                        (slot.upgrade(), layer.upgrade(), live.upgrade())
                    {
                        layer.set_child_transform(&live, None);
                        live.set_opacity(1.0);
                        slot.set_overflow(gtk::Overflow::Hidden);
                        slot.set_clip_overlay(&layer, true);
                    }
                }
            },
        );
    }

    pub fn clear_motion(&self) {
        self.stop_juggle();
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
