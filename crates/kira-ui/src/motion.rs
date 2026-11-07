//! Reusable GTK4 temporal primitives. Motion is subordinate to real UI state.
//! The four calendar flight coordinates and two-time model are ported
//! unchanged from main's calendar-tenet-model.ts. Rendering uses GDK/Cairo,
//! no WebKit, GSAP, DOM, or fake telemetry.
use std::cell::Cell;
use std::rc::Rc;
use std::time::Instant;

use gtk::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Flight {
    pub x: f64,
    pub y: f64,
    pub rotation: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeDirection {
    Forward,
    Inverted,
}

pub fn temporal_progress(progress: f64, direction: TimeDirection) -> f64 {
    let normalized = if progress.is_finite() {
        progress.clamp(0.0, 1.0)
    } else {
        0.0
    };
    match direction {
        TimeDirection::Forward => normalized,
        TimeDirection::Inverted => 1.0 - normalized,
    }
}

pub fn dual_time(progress: f64) -> (f64, f64) {
    (
        temporal_progress(progress, TimeDirection::Forward),
        temporal_progress(progress, TimeDirection::Inverted),
    )
}

pub fn calendar_flights(direction: i32) -> [Flight; 4] {
    let sign = if direction < 0 { -1.0 } else { 1.0 };
    [
        Flight { x: -39.0 * sign, y: -18.0, rotation: -65.0 },
        Flight { x: 31.0 * sign, y: 29.0, rotation: 42.0 },
        Flight { x: -14.0 * sign, y: 40.0, rotation: 80.0 },
        Flight { x: 37.0 * sign, y: -32.0, rotation: -35.0 },
    ]
}

pub fn cubic_in_out(position: f64) -> f64 {
    let t = position.clamp(0.0, 1.0);
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

/// GSAP back.out(overshoot) used for selected surface settling.
/// Geometry is allowed to overshoot; factual readout values never are.
pub fn back_out(progress: f64, overshoot: f64) -> f64 {
    let u = progress.clamp(0.0, 1.0) - 1.0;
    1.0 + (overshoot + 1.0) * u.powi(3) + overshoot * u.powi(2)
}

fn animations_enabled() -> bool {
    gtk::Settings::default().is_none_or(|settings| {
        settings.property::<bool>("gtk-enable-animations")
    })
}

/// One generation per surface: starting a new motion invalidates the old tick.
/// Reduced-motion settles immediately with no orphan animation callback.
#[derive(Clone, Default)]
pub struct MotionToken {
    generation: Rc<Cell<u64>>,
}

impl MotionToken {
    pub fn run(
        &self,
        widget: &gtk::Widget,
        duration_ms: u64,
        step: impl Fn(f64) + 'static,
        complete: impl FnOnce() + 'static,
    ) {
        let generation = self.generation.get().wrapping_add(1);
        self.generation.set(generation);
        if duration_ms == 0 || !animations_enabled() {
            step(1.0);
            complete();
            return;
        }
        step(0.0);
        let started = Instant::now();
        let token = self.clone();
        let mut complete = Some(complete);
        widget.add_tick_callback(move |_, _| {
            if token.generation.get() != generation {
                return gtk::glib::ControlFlow::Break;
            }
            let cursor = (started.elapsed().as_secs_f64() * 1000.0
                / duration_ms as f64).clamp(0.0, 1.0);
            step(cursor);
            if cursor >= 1.0 {
                if let Some(complete) = complete.take() {
                    complete();
                }
                gtk::glib::ControlFlow::Break
            } else {
                gtk::glib::ControlFlow::Continue
            }
        });
    }

    pub fn fade(
        &self,
        widget: &gtk::Widget,
        from: f64,
        to: f64,
        duration_ms: u64,
        complete: impl FnOnce() + 'static,
    ) {
        let surface = widget.clone();
        self.run(
            widget,
            duration_ms,
            move |cursor| {
                surface.set_opacity(from + (to - from) * cubic_in_out(cursor));
            },
            complete,
        );
    }
}

/// A small true INVERTED trace scene drawn on a transparent GTK DrawingArea.
/// Its paths are remembered for the whole event and run back to a real contact
/// point; this is not CSS animation-direction: reverse or a random particle.
pub struct CalendarFragmentScene {
    area: gtk::DrawingArea,
    progress: Rc<Cell<f64>>,
    active: Rc<Cell<bool>>,
    direction: Rc<Cell<i32>>,
    motion: MotionToken,
}

impl CalendarFragmentScene {
    pub fn new() -> Self {
        let area = gtk::DrawingArea::new();
        area.set_content_width(200);
        area.set_content_height(56);
        area.set_can_target(false);
        let progress = Rc::new(Cell::new(0.0));
        let active = Rc::new(Cell::new(false));
        let direction = Rc::new(Cell::new(1));
        {
            let progress = progress.clone();
            let active = active.clone();
            let direction = direction.clone();
            area.set_draw_func(move |_, cr, width, height| {
                if !active.get() {
                    return;
                }
                let inverted = temporal_progress(progress.get(), TimeDirection::Inverted);
                let contacts = calendar_flights(direction.get());
                let opacity = 0.13 * inverted * (1.0 - progress.get() * 0.5);
                cr.set_source_rgba(0.988, 0.639, 0.067, opacity);
                for flight in contacts {
                    let _ = cr.save();
                    cr.translate(
                        f64::from(width) * 0.5 + inverted * flight.x,
                        f64::from(height) * 0.5 + inverted * flight.y,
                    );
                    cr.rotate((inverted * flight.rotation).to_radians());
                    cr.rectangle(-5.0, -1.0, 10.0, 2.0);
                    let _ = cr.fill();
                    let _ = cr.restore();
                }
            });
        }
        Self { area, progress, active, direction, motion: MotionToken::default() }
    }

    pub fn widget(&self) -> gtk::Widget {
        self.area.clone().upcast()
    }

    pub fn play(&self, direction: i32) {
        self.direction.set(direction);
        self.active.set(true);
        let progress = self.progress.clone();
        let active = self.active.clone();
        let area = self.area.clone();
        let finished_area = self.area.clone();
        self.motion.run(
            &self.area.clone().upcast(),
            520,
            move |cursor| {
                progress.set(cursor);
                area.queue_draw();
            },
            move || {
                active.set(false);
                finished_area.queue_draw();
            },
        );
    }
}

impl Default for CalendarFragmentScene {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_dual_time_and_clamping_as_main() {
        assert_eq!(dual_time(0.0), (0.0, 1.0));
        assert_eq!(dual_time(0.25), (0.25, 0.75));
        assert_eq!(dual_time(0.5), (0.5, 0.5));
        assert_eq!(dual_time(1.0), (1.0, 0.0));
        assert_eq!(temporal_progress(-5.0, TimeDirection::Inverted), 1.0);
        assert_eq!(temporal_progress(f64::NAN, TimeDirection::Forward), 0.0);
    }

    #[test]
    fn same_saved_flight_is_reversible_at_all_progress_samples() {
        let outbound = calendar_flights(1);
        assert_eq!(outbound.len(), 4);
        for flight in outbound {
            for p in [0.0, 0.1, 0.5, 0.9, 1.0] {
                let reverse = temporal_progress(p, TimeDirection::Inverted);
                let forward = temporal_progress(1.0 - p, TimeDirection::Forward);
                assert!((flight.x * reverse - flight.x * forward).abs() < 1e-9);
                assert!((flight.y * reverse - flight.y * forward).abs() < 1e-9);
                assert!((flight.rotation * reverse - flight.rotation * forward).abs() < 1e-9);
            }
        }
    }

    #[test]
    fn direction_changes_only_horizontal_fragment_history() {
        let right = calendar_flights(1);
        let left = calendar_flights(-1);
        for i in 0..4 {
            assert_eq!(right[i].x, -left[i].x);
            assert_eq!(right[i].y, left[i].y);
            assert_eq!(right[i].rotation, left[i].rotation);
        }
    }

    #[test]
    fn easing_finishes_at_truthful_endpoints() {
        assert_eq!(cubic_in_out(0.0), 0.0);
        assert_eq!(cubic_in_out(1.0), 1.0);
        assert_eq!(back_out(0.0, 1.18), 0.0);
        assert_eq!(back_out(1.0, 1.18), 1.0);
    }
}
