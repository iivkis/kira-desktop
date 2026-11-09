//! Reusable GTK4 temporal primitives. Motion is subordinate to real UI state.
//! The four calendar flight coordinates and two-time model are ported
//! unchanged from main's calendar-tenet-model.ts. Rendering uses GDK/Cairo,
//! no WebKit, GSAP, DOM, or fake telemetry.
use std::cell::{Cell, RefCell};
use std::rc::Rc;

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
        Flight {
            x: -39.0 * sign,
            y: -18.0,
            rotation: -65.0,
        },
        Flight {
            x: 31.0 * sign,
            y: 29.0,
            rotation: 42.0,
        },
        Flight {
            x: -14.0 * sign,
            y: 40.0,
            rotation: 80.0,
        },
        Flight {
            x: 37.0 * sign,
            y: -32.0,
            rotation: -35.0,
        },
    ]
}

pub fn cubic_in_out(position: f64) -> f64 {
    let t = if position.is_finite() {
        position.clamp(0.0, 1.0)
    } else {
        0.0
    };
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

/// GSAP back.out(overshoot) used for selected surface settling.
/// Geometry is allowed to overshoot; factual readout values never are.
pub fn back_out(progress: f64, overshoot: f64) -> f64 {
    if !progress.is_finite() || progress <= 0.0 {
        return 0.0;
    }
    if progress >= 1.0 {
        return 1.0;
    }
    let u = progress - 1.0;
    1.0 + (overshoot + 1.0) * u.powi(3) + overshoot * u.powi(2)
}

/** Equivalent of main's 400ms per-digit INVERTED entry. Coordinates are
 * expressed as a fraction of the measured digit height, not hardcoded pixels.
 * The outgoing and incoming tracks share one normalized cursor. */
pub const CLOCK_DIGIT_DURATION_MS: u64 = 400;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DigitFrame {
    pub outgoing_y: f64,
    pub outgoing_opacity: f64,
    pub incoming_y: f64,
    pub incoming_opacity: f64,
}

fn bounded(progress: f64) -> f64 {
    if progress.is_finite() {
        progress.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

pub fn clock_digit_frame(progress: f64) -> DigitFrame {
    let p = bounded(progress);
    // Main: outgoing power3.out, incoming reverse of power3.out.
    let outbound = 1.0 - (1.0 - p).powi(3);
    let inbound = p.powi(3);
    DigitFrame {
        outgoing_y: -0.85 * outbound,
        outgoing_opacity: 1.0 - outbound,
        incoming_y: 0.85 * (1.0 - inbound),
        incoming_opacity: inbound,
    }
}

/// Exact flight contract from frozen main's clock-flight.ts: eight angular
/// sectors, one remembered route per numeral per click. No GTK dependency.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClockFlight {
    pub sector: usize,
    pub x: f64,
    pub y: f64,
    pub rotation: f64,
    pub scale: f64,
    pub opacity: f64,
}

pub const CLOCK_JUGGLE_FLIGHT_MS: f64 = 380.0;
pub const CLOCK_JUGGLE_STAGGER_MS: f64 = 26.0;

pub fn next_clock_flights(
    count: usize,
    previous: &[ClockFlight],
    mut random: impl FnMut() -> f64,
) -> Vec<ClockFlight> {
    let unit = || -> f64 {
        let sample = random();
        if sample.is_finite() {
            sample.clamp(0.0, f64::from_bits(0x3fef_ffff_ffff_ffff))
        } else {
            0.0
        }
    };
    let mut unit = unit;
    let mut used = [false; 8];
    let mut flights = Vec::with_capacity(count.min(32));
    for index in 0..count.min(32) {
        let old = previous.get(index).map(|flight| flight.sector);
        let mut sectors: Vec<usize> = (0..8)
            .filter(|sector| !used[*sector] && old != Some(*sector))
            .collect();
        if sectors.is_empty() {
            sectors = (0..8).filter(|sector| old != Some(*sector)).collect();
        }
        let selected = (unit() * sectors.len() as f64) as usize;
        let sector = sectors[selected.min(sectors.len() - 1)];
        used[sector] = true;
        let angle = (sector as f64 + 0.16 + unit() * 0.68) * std::f64::consts::FRAC_PI_4;
        let distance = 24.0 + unit() * 23.0;
        let sign = if unit() < 0.5 { -1.0 } else { 1.0 };
        let spin = sign * (32.0 + unit() * 73.0);
        flights.push(ClockFlight {
            sector,
            x: angle.cos() * distance,
            y: angle.sin() * distance,
            rotation: spin,
            scale: 0.64 + unit() * 0.20,
            opacity: 0.48 + unit() * 0.29,
        });
    }
    flights
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClockJuggleFrame {
    pub x: f64,
    pub y: f64,
    pub rotation: f64,
    pub scale: f64,
    pub opacity: f64,
}

/// A single 380ms+stagger history runs forward then backwards, with no
/// reroll at the turnaround. progress=0 and progress=1 both mean 'settled'.
pub fn clock_juggle_frame(flight: ClockFlight, progress: f64) -> ClockJuggleFrame {
    let t = bounded(progress);
    let half = if t <= 0.5 { t * 2.0 } else { (1.0 - t) * 2.0 };
    let eased = 1.0 - (1.0 - half).powi(3);
    ClockJuggleFrame {
        x: flight.x * eased,
        y: flight.y * eased,
        rotation: flight.rotation * eased,
        scale: 1.0 + (flight.scale - 1.0) * eased,
        opacity: 1.0 + (flight.opacity - 1.0) * eased,
    }
}

/// Frozen main calendar grid's forward compositor track: 440ms geometry,
/// 200ms opacity and one shared recorded contact point for INVERTED traces.
pub const CALENDAR_GRID_DURATION_MS: u64 = 440;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CalendarGridFrame {
    pub x: f64,
    pub y: f64,
    pub scale: f64,
    pub opacity: f64,
}

pub fn calendar_grid_frame(progress: f64, direction: i32) -> CalendarGridFrame {
    let t = bounded(progress);
    let eased = back_out(t, 1.18);
    let fade = (t * 440.0 / 200.0).min(1.0);
    let faded = 1.0 - (1.0 - fade).powi(2);
    CalendarGridFrame {
        x: 8.0 * if direction < 0 { -1.0 } else { 1.0 } * (1.0 - eased),
        y: 2.0 * (1.0 - eased),
        scale: 0.991 + 0.009 * eased,
        opacity: 0.94 + 0.06 * faded,
    }
}

/// Rewinds main's 520ms power3.out fragment flights, staggered by 25ms,
/// along the same 440ms forward master cursor. No new flight is generated.
pub fn calendar_grid_trace_frame(progress: f64, direction: i32, index: usize) -> PopupTraceFrame {
    let flights = calendar_flights(direction);
    let flight = flights[index % flights.len()];
    let inverted_cursor = 1.0 - bounded(progress);
    let flight_ms = inverted_cursor * (520.0 + 25.0 * 3.0);
    let local = ((flight_ms - index.min(3) as f64 * 25.0) / 520.0).clamp(0.0, 1.0);
    let extent = 1.0 - (1.0 - local).powi(3);
    PopupTraceFrame {
        x: flight.x * extent,
        y: flight.y * extent,
        rotation: flight.rotation * extent,
        opacity: 0.85 * extent,
    }
}

/// One popup master cursor is retained for entrance, dismissal and reopening.
/// The source master runs 540ms at timescale 1.44, giving 375ms on screen.
/// The cursor's direction changes without rebuilding any keyframe or geometry.
pub const POPUP_MASTER_DURATION_MS: f64 = 375.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PopupPhase {
    Hidden,
    Opening,
    Visible,
    Closing,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PopupTimeline {
    progress: f64,
    phase: PopupPhase,
}

impl Default for PopupTimeline {
    fn default() -> Self {
        Self {
            progress: 0.0,
            phase: PopupPhase::Hidden,
        }
    }
}

impl PopupTimeline {
    pub fn progress(&self) -> f64 {
        self.progress
    }

    pub fn phase(&self) -> PopupPhase {
        self.phase
    }

    pub fn open(&mut self) {
        self.phase = if self.progress >= 1.0 {
            PopupPhase::Visible
        } else {
            PopupPhase::Opening
        };
    }

    pub fn close(&mut self) {
        self.phase = if self.progress <= 0.0 {
            PopupPhase::Hidden
        } else {
            PopupPhase::Closing
        };
    }

    pub fn settle(&mut self) {
        self.progress = match self.phase {
            PopupPhase::Hidden | PopupPhase::Closing => 0.0,
            PopupPhase::Opening | PopupPhase::Visible => 1.0,
        };
        self.phase = if self.progress == 0.0 {
            PopupPhase::Hidden
        } else {
            PopupPhase::Visible
        };
    }

    pub fn advance_ms(&mut self, delta_ms: f64) {
        let delta = if delta_ms.is_finite() {
            delta_ms.max(0.0) / POPUP_MASTER_DURATION_MS
        } else {
            0.0
        };
        match self.phase {
            PopupPhase::Opening => {
                self.progress = (self.progress + delta).min(1.0);
                if self.progress >= 1.0 {
                    self.phase = PopupPhase::Visible;
                }
            }
            PopupPhase::Closing => {
                self.progress = (self.progress - delta).max(0.0);
                if self.progress <= 0.0 {
                    self.phase = PopupPhase::Hidden;
                }
            }
            PopupPhase::Hidden | PopupPhase::Visible => {}
        }
    }

    pub fn frame(&self, direction: i32) -> PopupFrame {
        popup_frame(self.progress, direction)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PopupFrame {
    /// Fraction of the fixed Wayland surface revealed by the compositor clip.
    pub clip: f64,
    /// Position of the moving seam as fraction of the panel's height.
    pub seam: f64,
    pub seam_opacity: f64,
    /// Fraction of the original 9px section offset left at this instant.
    pub section_offset: f64,
    pub section_scale: f64,
    pub section_opacity: f64,
}

pub fn popup_frame(progress: f64, direction: i32) -> PopupFrame {
    let t = bounded(progress);
    let ms = 540.0 * t;
    let clip = cubic_in_out(t);
    // The source seam first fades in after 25ms and fades out at 370ms.
    let in_progress = ((ms - 25.0) / 90.0).clamp(0.0, 1.0);
    let out_progress = ((ms - 370.0) / 170.0).clamp(0.0, 1.0);
    let seam_opacity = 0.82 * (1.0 - (1.0 - in_progress).powi(2)) * (1.0 - out_progress.powi(2));
    let section_p = (ms / 440.0).clamp(0.0, 1.0);
    let spring = back_out(section_p, 1.22);
    let fade_p = (ms / 220.0).clamp(0.0, 1.0);
    PopupFrame {
        clip,
        seam: if direction < 0 { 1.0 - clip } else { clip },
        seam_opacity,
        section_offset: 1.0 - spring,
        section_scale: 0.988 + 0.012 * spring,
        section_opacity: 0.94 + 0.06 * (1.0 - (1.0 - fade_p).powi(2)),
    }
}

/// Visual state of a content section on main's shared 540ms master clock.
/// There are at most three animated sections; factual content is already set.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PopupSectionFrame {
    pub translate_y: f64,
    pub scale: f64,
    pub opacity: f64,
}

pub fn popup_section_frame(progress: f64, index: usize) -> PopupSectionFrame {
    let ms = 540.0 * bounded(progress);
    let delay = index.min(2) as f64 * 35.0;
    let spring_p = ((ms - delay) / 440.0).clamp(0.0, 1.0);
    let fade_p = ((ms - delay) / 220.0).clamp(0.0, 1.0);
    let spring = back_out(spring_p, 1.22);
    let opacity = 0.94 + 0.06 * (1.0 - (1.0 - fade_p).powi(2));
    PopupSectionFrame {
        translate_y: 9.0 * (1.0 - spring),
        scale: 0.988 + 0.012 * spring,
        opacity,
    }
}

/// Recorded, source-defined trace history. Each fragment starts at its
/// original outbound position and returns to the SAME contact point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PopupTraceFrame {
    pub x: f64,
    pub y: f64,
    pub rotation: f64,
    pub opacity: f64,
}

pub fn popup_trace_frame(progress: f64, direction: i32, index: usize) -> PopupTraceFrame {
    let flights = calendar_flights(direction);
    let flight = flights[index % flights.len()];
    let delay = (flights.len() - 1 - (index % flights.len())) as f64 * 12.0;
    let local = ((540.0 * bounded(progress) - delay) / 440.0).clamp(0.0, 1.0);
    // power3.in: reverse playback traverses the same values in reverse order.
    let remainder = 1.0 - local.powi(3);
    PopupTraceFrame {
        x: flight.x * remainder,
        y: flight.y * remainder,
        rotation: flight.rotation * remainder,
        opacity: 0.13 * remainder,
    }
}

pub(crate) fn animations_enabled() -> bool {
    gtk::Settings::default()
        .is_none_or(|settings| settings.property::<bool>("gtk-enable-animations"))
}

/// Convert measured GDK frame-clock microseconds to a finite normalized
/// cursor. The initial frame is always p=0, regardless of how long the
/// widget was waiting for its first Wayland map / GTK allocation.
pub fn frame_clock_progress(first_us: i64, now_us: i64, duration_ms: u64) -> f64 {
    if duration_ms == 0 {
        return 1.0;
    }
    let elapsed = now_us.saturating_sub(first_us).max(0) as f64;
    (elapsed / (duration_ms as f64 * 1000.0)).clamp(0.0, 1.0)
}

/// One generation per surface: starting a new motion invalidates the old tick.
/// Reduced-motion settles immediately with no orphan animation callback.
#[derive(Clone, Default)]
pub struct MotionToken {
    generation: Rc<Cell<u64>>,
}

impl MotionToken {
    /// Invalidate in-flight callbacks before replacing a scene. The caller
    /// owns resetting decorative widgets to the factual final state.
    pub fn cancel(&self) {
        self.generation.set(self.generation.get().wrapping_add(1));
    }

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
        // Starting at the API call used to skip the opening clip if GTK had
        // not yet mapped the window. Time belongs to painted GTK frames, not
        // to user-space work/Wayland scheduling before the first tick.
        let first_frame = Cell::new(None::<i64>);
        let last_progress = Cell::new(0.0_f64);
        let token = self.clone();
        let completed = Rc::new(RefCell::new(Some(complete)));
        widget.add_tick_callback(move |_, clock| {
            if token.generation.get() != generation {
                return gtk::glib::ControlFlow::Break;
            }
            if !animations_enabled() {
                step(1.0);
                if let Some(complete) = completed.borrow_mut().take() {
                    complete();
                }
                return gtk::glib::ControlFlow::Break;
            }
            let at = clock.frame_time();
            let origin = first_frame.get().unwrap_or_else(|| {
                first_frame.set(Some(at));
                at
            });
            // Defensive monotonicity: even a clock discontinuity must not
            // rewind a physically recorded scene or reduce accessible state.
            let cursor = frame_clock_progress(origin, at, duration_ms)
                .max(last_progress.get());
            last_progress.set(cursor);
            step(cursor);
            if cursor >= 1.0 {
                let callback = completed.borrow_mut().take();
                if let Some(complete) = callback {
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
    contact: Rc<Cell<Option<(f64, f64)>>>,
    motion: MotionToken,
    unmap_handler: Option<gtk::glib::SignalHandlerId>,
}

impl CalendarFragmentScene {
    pub fn new() -> Self {
        let area = gtk::DrawingArea::new();
        area.set_halign(gtk::Align::Fill);
        area.set_valign(gtk::Align::Fill);
        area.set_can_target(false);
        let progress = Rc::new(Cell::new(0.0));
        let active = Rc::new(Cell::new(false));
        let direction = Rc::new(Cell::new(1));
        let contact = Rc::new(Cell::new(None));
        let motion = MotionToken::default();
        {
            let progress = progress.clone();
            let active = active.clone();
            let direction = direction.clone();
            let contact = contact.clone();
            area.set_draw_func(move |_, cr, _, _| {
                if !active.get() {
                    return;
                }
                let Some((cx, cy)) = contact.get() else {
                    return;
                };
                for index in 0..4 {
                    let trace = calendar_grid_trace_frame(progress.get(), direction.get(), index);
                    if trace.opacity <= 0.0 {
                        continue;
                    }
                    let _ = cr.save();
                    cr.translate(cx + trace.x, cy + trace.y);
                    cr.rotate(trace.rotation.to_radians());
                    if index % 2 == 0 {
                        cr.set_source_rgba(0.988, 0.639, 0.067, trace.opacity);
                    } else {
                        cr.set_source_rgba(0.898, 0.898, 0.898, trace.opacity);
                    }
                    let scale = trace.opacity / 0.85;
                    cr.rectangle(-2.0 * scale, -2.0 * scale, 4.0 * scale, 4.0 * scale);
                    let _ = cr.fill();
                    let _ = cr.restore();
                }
            });
        }
        let unmap_handler = area.connect_unmap({
            let motion = motion.clone();
            let progress = progress.clone();
            let active = active.clone();
            let contact = contact.clone();
            move |_| {
                // The next map must not replay a fragment from a past date.
                motion.cancel();
                active.set(false);
                progress.set(1.0);
                contact.set(None);
            }
        });
        Self {
            area,
            progress,
            active,
            direction,
            contact,
            motion,
            unmap_handler: Some(unmap_handler),
        }
    }

    pub fn widget(&self) -> gtk::Widget {
        self.area.clone().upcast()
    }

    /// The actual selected day is the interaction's sole remembered contact.
    /// No speculative center-of-grid animation if GTK has no measured bounds.
    pub fn play_at(&self, direction: i32, target: &gtk::Widget) {
        self.motion.cancel();
        self.active.set(false);
        if !self.area.is_mapped() {
            return;
        }
        let Some(bounds) = target.compute_bounds(&self.area) else {
            return;
        };
        if bounds.width() <= 0.0 || bounds.height() <= 0.0 {
            return;
        }
        self.contact.set(Some((
            f64::from(bounds.x() + bounds.width() / 2.0),
            f64::from(bounds.y() + bounds.height() / 2.0),
        )));
        self.direction.set(direction);
        self.active.set(true);
        let progress = self.progress.clone();
        let active = self.active.clone();
        let area = self.area.downgrade();
        let finished_area = self.area.downgrade();
        self.motion.run(
            &self.area.clone().upcast(),
            CALENDAR_GRID_DURATION_MS,
            move |cursor| {
                progress.set(cursor);
                if let Some(area) = area.upgrade() {
                    area.queue_draw();
                }
            },
            move || {
                active.set(false);
                if let Some(area) = finished_area.upgrade() {
                    area.queue_draw();
                }
            },
        );
    }
}

impl Default for CalendarFragmentScene {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for CalendarFragmentScene {
    fn drop(&mut self) {
        self.motion.cancel();
        if let Some(handler) = self.unmap_handler.take() {
            self.area.disconnect(handler);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_painted_frame_is_zero_even_after_mapping_delay() {
        let started_on_first_tick = 9_000_000_i64;
        assert_eq!(
            frame_clock_progress(started_on_first_tick, started_on_first_tick, 420),
            0.0
        );
        assert_eq!(
            frame_clock_progress(started_on_first_tick, started_on_first_tick + 210_000, 420),
            0.5
        );
        assert_eq!(
            frame_clock_progress(started_on_first_tick, started_on_first_tick + 420_000, 420),
            1.0
        );
        assert_eq!(
            frame_clock_progress(started_on_first_tick, started_on_first_tick + 10_000_000, 420),
            1.0
        );
    }

    #[test]
    fn frame_clock_never_interpolates_nonfinite_or_reversed_time() {
        assert_eq!(frame_clock_progress(20_000, 19_000, 420), 0.0);
        assert_eq!(frame_clock_progress(i64::MAX, i64::MIN, 420), 0.0);
        assert_eq!(frame_clock_progress(100, 110, 0), 1.0);
        assert_eq!(cubic_in_out(f64::NAN), 0.0);
        assert_eq!(cubic_in_out(f64::INFINITY), 0.0);
    }

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
    fn individual_digits_retrace_main_outbound_history() {
        assert_eq!(clock_digit_frame(0.0).outgoing_y, 0.0);
        assert_eq!(clock_digit_frame(0.0).incoming_y, 0.85);
        assert_eq!(clock_digit_frame(1.0).incoming_y, 0.0);
        assert_eq!(clock_digit_frame(1.0).outgoing_opacity, 0.0);
        for p in [0.0_f64, 0.05, 0.25, 0.5, 0.95, 1.0] {
            let f = clock_digit_frame(p);
            assert!((f.incoming_y - 0.85 * (1.0 - p.powi(3))).abs() < 1e-10);
            assert!((f.outgoing_opacity - (1.0 - p).powi(3)).abs() < 1e-10);
        }
        assert_eq!(clock_digit_frame(f64::NAN), clock_digit_frame(0.0));
    }

    #[test]
    fn click_flights_avoid_repeat_sectors_even_with_identical_random() {
        let first = next_clock_flights(6, &[], || 0.5);
        assert_eq!(first.len(), 6);
        for (index, flight) in first.iter().enumerate() {
            assert!(flight.sector < 8);
            assert!(first[..index].iter().all(|old| old.sector != flight.sector));
            assert!((0.64..=0.84).contains(&flight.scale));
            assert!((0.48..=0.77).contains(&flight.opacity));
            assert!((24.0..=47.0).contains(&flight.x.hypot(flight.y)));
        }
        let second = next_clock_flights(6, &first, || 0.5);
        for (a, b) in first.iter().zip(second.iter()) {
            assert_ne!(a.sector, b.sector);
        }
        assert_eq!(next_clock_flights(0, &first, || 0.5).len(), 0);
        assert_eq!(next_clock_flights(100, &first, || 0.5).len(), 32);
    }

    #[test]
    fn clock_juggle_reverse_reuses_identical_geometry() {
        let flights = next_clock_flights(6, &[], || 0.2);
        for flight in flights {
            let initial = clock_juggle_frame(flight, 0.0);
            let returned = clock_juggle_frame(flight, 1.0);
            assert_eq!(initial, returned);
            assert_eq!(initial.x, 0.0);
            assert_eq!(initial.scale, 1.0);
            assert_eq!(initial.opacity, 1.0);
            assert_eq!(clock_juggle_frame(flight, 0.5).x, flight.x);
            for t in [0.1, 0.25, 0.4] {
                let outbound = clock_juggle_frame(flight, t);
                let inbound = clock_juggle_frame(flight, 1.0 - t);
                assert!((outbound.x - inbound.x).abs() < 1e-9);
                assert!((outbound.y - inbound.y).abs() < 1e-9);
                assert!((outbound.rotation - inbound.rotation).abs() < 1e-9);
                assert!((outbound.scale - inbound.scale).abs() < 1e-9);
                assert!((outbound.opacity - inbound.opacity).abs() < 1e-9);
            }
        }
    }

    #[test]
    fn calendar_grid_forward_surface_settles_and_respects_direction() {
        let initial = calendar_grid_frame(0.0, 1);
        assert_eq!(initial.x, 8.0);
        assert_eq!(initial.y, 2.0);
        assert_eq!(initial.scale, 0.991);
        assert_eq!(initial.opacity, 0.94);
        let end = calendar_grid_frame(1.0, -1);
        assert_eq!(end.x, 0.0);
        assert_eq!(end.y, 0.0);
        assert_eq!(end.scale, 1.0);
        assert_eq!(end.opacity, 1.0);
        for p in [0.0, 0.16, 0.55, 0.9, 1.0] {
            assert_eq!(calendar_grid_frame(p, 1).x, -calendar_grid_frame(p, -1).x);
        }
    }

    #[test]
    fn calendar_grid_inverted_traces_return_to_saved_contact() {
        for direction in [-1, 1] {
            for i in 0..4 {
                let source = calendar_flights(direction)[i];
                let start = calendar_grid_trace_frame(0.0, direction, i);
                let end = calendar_grid_trace_frame(1.0, direction, i);
                assert_eq!(start.x, source.x);
                assert_eq!(start.y, source.y);
                assert_eq!(start.rotation, source.rotation);
                assert_eq!(start.opacity, 0.85);
                assert_eq!(end.x, 0.0);
                assert_eq!(end.y, 0.0);
                assert_eq!(end.opacity, 0.0);
            }
        }
    }

    #[test]
    fn popup_reuses_one_cursor_under_rapid_direction_changes() {
        let mut timeline = PopupTimeline::default();
        timeline.open();
        timeline.advance_ms(150.0);
        assert_eq!(timeline.phase(), PopupPhase::Opening);
        assert!((timeline.progress() - 0.4).abs() < 1e-9);
        let frame_at_turn = timeline.frame(1);
        timeline.close();
        assert_eq!(timeline.frame(1), frame_at_turn);
        timeline.advance_ms(37.5);
        assert!((timeline.progress() - 0.3).abs() < 1e-9);
        timeline.open();
        assert!((timeline.progress() - 0.3).abs() < 1e-9);
        timeline.advance_ms(300.0);
        assert_eq!(timeline.phase(), PopupPhase::Visible);
        assert_eq!(timeline.progress(), 1.0);
        timeline.close();
        timeline.advance_ms(375.0);
        assert_eq!(timeline.phase(), PopupPhase::Hidden);
        assert_eq!(timeline.progress(), 0.0);
    }

    #[test]
    fn popup_sections_have_original_35ms_stagger_and_truthful_endpoints() {
        let first = popup_section_frame(0.0, 0);
        let third = popup_section_frame(0.0, 2);
        assert_eq!(first.translate_y, 9.0);
        assert_eq!(first.scale, 0.988);
        assert_eq!(first.opacity, 0.94);
        assert_eq!(third, first);
        let p = 35.0 / 540.0;
        assert!(popup_section_frame(p, 0).translate_y < 9.0);
        assert_eq!(popup_section_frame(p, 1).translate_y, 9.0);
        assert_eq!(popup_section_frame(p, 2).translate_y, 9.0);
        for index in 0..3 {
            let final_frame = popup_section_frame(1.0, index);
            assert_eq!(final_frame.translate_y, 0.0);
            assert_eq!(final_frame.scale, 1.0);
            assert_eq!(final_frame.opacity, 1.0);
        }
    }

    #[test]
    fn source_fragment_tracks_rewind_to_the_same_contact() {
        for direction in [-1, 1] {
            for index in 0..4 {
                let outward = calendar_flights(direction)[index];
                assert_eq!(popup_trace_frame(0.0, direction, index).x, outward.x);
                assert_eq!(popup_trace_frame(0.0, direction, index).y, outward.y);
                assert_eq!(popup_trace_frame(1.0, direction, index).x, 0.0);
                for progress in [0.05, 0.3, 0.7, 0.95] {
                    let point = popup_trace_frame(progress, direction, index);
                    // A reverse cursor never regenerates a second trajectory.
                    assert_eq!(point, popup_trace_frame(progress, direction, index));
                    assert!((0.0..=0.13).contains(&point.opacity));
                }
            }
        }
    }

    #[test]
    fn popup_frame_keeps_reverse_geometry_and_reduced_motion_endpoints() {
        let f = popup_frame(0.0, 1);
        assert_eq!(f.clip, 0.0);
        assert_eq!(f.section_scale, 0.988);
        assert_eq!(popup_frame(1.0, 1).clip, 1.0);
        for p in [0.0, 0.12, 0.33, 0.61, 1.0] {
            assert!((0.0..=1.0).contains(&popup_frame(p, 1).clip));
            assert!((popup_frame(p, 1).seam + popup_frame(p, -1).seam - 1.0).abs() < 1e-9);
        }
        let mut timeline = PopupTimeline::default();
        timeline.open();
        timeline.settle();
        assert_eq!(timeline.phase(), PopupPhase::Visible);
        timeline.close();
        timeline.settle();
        assert_eq!(timeline.phase(), PopupPhase::Hidden);
    }

    #[test]
    fn easing_finishes_at_truthful_endpoints() {
        assert_eq!(cubic_in_out(0.0), 0.0);
        assert_eq!(cubic_in_out(1.0), 1.0);
        assert_eq!(back_out(0.0, 1.18), 0.0);
        assert_eq!(back_out(1.0, 1.18), 1.0);
    }
}
