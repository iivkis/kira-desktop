//! Reusable native CPU/RAM/GPU history. Each point is a real worker pulse, not
//! interpolation between slow system snapshots or a GTK frame-clock sample.
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk::prelude::*;

use crate::chart_motion::{ChartMotion, ChartTimeDomain};
use crate::motion::{MotionToken, animations_enabled};

pub const MAX_PULSE_POINTS: usize = 120;
pub const MAX_PULSE_AGE: Duration = Duration::from_secs(60);
pub const PULSE_FRESHNESS: Duration = Duration::from_millis(2500);
pub const GPU_FRESHNESS: Duration = Duration::from_secs(8);
pub const GPU_ATTACH_LIMIT: Duration = Duration::from_millis(2500);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct GpuObservation {
    measured_at: Instant,
    peak_percent: u8,
}

/// A slow GPU measurement belongs to exactly one subsequent *real* pulse.
/// No synthetic intermediate GPU readings and no reuse after hide/reopen.
#[derive(Default)]
pub struct GpuPulseCoupler {
    pending: Option<GpuObservation>,
    last_observed_at: Option<Instant>,
    last_valid_at: Option<Instant>,
}

impl GpuPulseCoupler {
    pub fn observe(&mut self, measured_at: Instant, peak_percent: Option<u8>) {
        if self
            .last_observed_at
            .is_some_and(|last| measured_at <= last)
        {
            return;
        }
        self.last_observed_at = Some(measured_at);
        self.pending = peak_percent
            .filter(|value| *value <= 100)
            .map(|value| GpuObservation {
                measured_at,
                peak_percent: value,
            });
        self.last_valid_at = self.pending.map(|point| point.measured_at);
    }

    pub fn take_for_pulse(&mut self, pulse_at: Instant) -> Option<u8> {
        let pending = self.pending?;
        if pulse_at < pending.measured_at {
            return None;
        }
        self.pending = None;
        (pulse_at.saturating_duration_since(pending.measured_at) <= GPU_ATTACH_LIMIT)
            .then_some(pending.peak_percent)
    }

    pub fn is_fresh(&self, now: Instant) -> bool {
        self.last_valid_at
            .is_some_and(|at| now >= at && now.saturating_duration_since(at) < GPU_FRESHNESS)
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ChartSample {
    pub measured_at: Instant,
    pub cpu_percent: Option<u8>,
    pub memory_total_kib: Option<u64>,
    pub memory_available_kib: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChartPoint {
    pub measured_at: Instant,
    pub cpu: Option<f64>,
    pub ram: Option<f64>,
    pub gpu: Option<f64>,
}

fn memory_percent(total: Option<u64>, available: Option<u64>) -> Option<f64> {
    let total = total?;
    let available = available?;
    if total == 0 || available > total {
        return None;
    }
    Some((total - available) as f64 * 100.0 / total as f64)
}

impl From<ChartSample> for ChartPoint {
    fn from(sample: ChartSample) -> Self {
        Self {
            measured_at: sample.measured_at,
            cpu: sample.cpu_percent.filter(|n| *n <= 100).map(f64::from),
            ram: memory_percent(sample.memory_total_kib, sample.memory_available_kib),
            gpu: None,
        }
    }
}

#[derive(Default)]
pub struct PulseHistory {
    samples: VecDeque<ChartPoint>,
}

impl PulseHistory {
    pub fn push(&mut self, sample: ChartSample) {
        self.push_with_gpu(sample, None);
    }

    pub fn push_with_gpu(&mut self, sample: ChartSample, gpu_percent: Option<u8>) {
        if self
            .samples
            .back()
            .is_some_and(|last| sample.measured_at <= last.measured_at)
        {
            // Never invent ordering or replace a real reading with an older one.
            return;
        }
        let mut point = ChartPoint::from(sample);
        point.gpu = gpu_percent.filter(|value| *value <= 100).map(f64::from);
        self.samples.push_back(point);
        self.prune(sample.measured_at);
    }

    pub fn clear(&mut self) {
        self.samples.clear();
    }

    pub fn prune(&mut self, now: Instant) {
        while self
            .samples
            .front()
            .is_some_and(|first| now.saturating_duration_since(first.measured_at) > MAX_PULSE_AGE)
        {
            self.samples.pop_front();
        }
        while self.samples.len() > MAX_PULSE_POINTS {
            self.samples.pop_front();
        }
    }

    pub fn recent(&self, now: Instant) -> Vec<ChartPoint> {
        let points: Vec<_> = self
            .samples
            .iter()
            .filter(|point| {
                now >= point.measured_at
                    && now.saturating_duration_since(point.measured_at) <= MAX_PULSE_AGE
            })
            .copied()
            .collect();
        // Both lanes share one measured time domain. The first CPU sample
        // requires a delta baseline and can initially be unavailable.
        let first_complete = points
            .iter()
            .position(|point| point.cpu.is_some() && point.ram.is_some());
        match first_complete {
            Some(index) => points[index..].to_vec(),
            None => points,
        }
    }

    pub fn latest(&self, now: Instant) -> Option<ChartPoint> {
        self.samples.back().copied().filter(|point| {
            now >= point.measured_at
                && now.saturating_duration_since(point.measured_at) < PULSE_FRESHNESS
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ActivityAxis {
    pub min: f64,
    pub max: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActivityLane {
    Cpu,
    Ram,
    Gpu,
}

fn reading(point: &ChartPoint, lane: ActivityLane) -> Option<f64> {
    let value = match lane {
        ActivityLane::Cpu => point.cpu,
        ActivityLane::Ram => point.ram,
        ActivityLane::Gpu => point.gpu,
    }?;
    (value.is_finite() && (0.0..=100.0).contains(&value)).then_some(value)
}

/// Main's bounded adaptive axis. Bounds always include every measured value
/// and remain within physical utilization limits.
pub fn activity_axis(points: &[ChartPoint], lane: ActivityLane) -> ActivityAxis {
    let readings: Vec<_> = points
        .iter()
        .filter_map(|point| reading(point, lane))
        .collect();
    if readings.is_empty() {
        return ActivityAxis {
            min: 0.0,
            max: 100.0,
        };
    }
    let low = readings.iter().copied().fold(100.0_f64, f64::min);
    let high = readings.iter().copied().fold(0.0_f64, f64::max);
    let padding = 2.0_f64.max((high - low) * 0.25);
    let mut min = (10.0 * (low - padding)).floor() / 10.0;
    let mut max = (10.0 * (high + padding)).ceil() / 10.0;
    min = min.max(0.0);
    max = max.min(100.0);
    if max - min < 4.0 {
        min = (10.0 * (low - 2.0)).floor() / 10.0;
        min = min.max(0.0);
        max = ((10.0 * (high + 2.0)).ceil() / 10.0)
            .max(min + 4.0)
            .min(100.0);
        min = min.min(max - 4.0).max(0.0);
    }
    ActivityAxis { min, max }
}

/// A pulse that contains no usable reading is still part of the measured
/// time axis, but it must not masquerade as inspectable sensor evidence.
pub fn inspectable_history(points: &[ChartPoint]) -> Vec<ChartPoint> {
    points
        .iter()
        .copied()
        .filter(|point| {
            [ActivityLane::Cpu, ActivityLane::Ram, ActivityLane::Gpu]
                .into_iter()
                .any(|lane| reading(point, lane).is_some())
        })
        .collect()
}

/// Historical inspection is a view of actual retained pulses, never a
/// request to sample or interpolate telemetry. None means follow the latest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InspectionAction {
    Earlier,
    Later,
    First,
    Live,
}

pub fn inspection_target(
    points: &[ChartPoint],
    selected: Option<Instant>,
    action: InspectionAction,
) -> Option<Instant> {
    if points.is_empty() || action == InspectionAction::Live {
        return None;
    }
    if action == InspectionAction::First {
        return Some(points[0].measured_at);
    }
    let current = selected
        .and_then(|stamp| points.iter().position(|point| point.measured_at == stamp))
        .unwrap_or(points.len() - 1);
    match action {
        InspectionAction::Earlier if current > 0 => Some(points[current - 1].measured_at),
        InspectionAction::Earlier => Some(points[current].measured_at),
        InspectionAction::Later if current + 1 < points.len() - 1 => {
            Some(points[current + 1].measured_at)
        }
        InspectionAction::Later => None,
        InspectionAction::First | InspectionAction::Live => None,
    }
}

pub fn inspected_chart_point(
    points: &[ChartPoint],
    selected: Option<Instant>,
) -> Option<ChartPoint> {
    match selected {
        Some(at) => points.iter().find(|point| point.measured_at == at).copied(),
        None => points.last().copied(),
    }
}

/// Mouse inspection projects to the SAME measured axis as the Cairo trace.
fn chart_sample_at_x(
    points: &[ChartPoint],
    domain: ChartTimeDomain,
    x: f64,
    width: i32,
) -> Option<Instant> {
    let left = 41.0_f64;
    let right = f64::from(width) - 8.0;
    if points.is_empty() || !x.is_finite() || right <= left {
        return None;
    }
    let span = domain
        .end
        .saturating_duration_since(domain.start)
        .as_secs_f64();
    if span <= 0.0 {
        return None;
    }
    let fraction = ((x - left) / (right - left)).clamp(0.0, 1.0);
    points
        .iter()
        .min_by(|a, b| {
            let delta = |point: &ChartPoint| {
                (point
                    .measured_at
                    .saturating_duration_since(domain.start)
                    .as_secs_f64()
                    / span
                    - fraction)
                    .abs()
            };
            delta(a).total_cmp(&delta(b))
        })
        .map(|point| point.measured_at)
}

pub fn chart_inspection_text(
    points: &[ChartPoint],
    selected: Option<Instant>,
    now: Instant,
) -> String {
    let Some(point) = inspected_chart_point(points, selected) else {
        return "НЕТ ЗАМЕРОВ · график ожидает реальные показания".into();
    };
    let age = now
        .saturating_duration_since(point.measured_at)
        .as_secs_f64();
    let lag = points.last().map_or(0.0, |last| {
        last.measured_at
            .saturating_duration_since(point.measured_at)
            .as_secs_f64()
    });
    let status = if selected.is_some() {
        format!("ВЫБРАН ЗАМЕР · −{lag:.1} с от последнего")
    } else if now.saturating_duration_since(point.measured_at) < PULSE_FRESHNESS {
        "ТЕКУЩИЙ ЗАМЕР".to_owned()
    } else {
        "ПОСЛЕДНИЙ ЗАМЕР УСТАРЕЛ".to_owned()
    };
    format!(
        "{status} · {age:.1} с назад · CPU {} · RAM {} · GPU {}",
        meter_text(point.cpu, true),
        meter_text(point.ram, true),
        meter_text(point.gpu, true)
    )
}

pub fn chart_inspection_report(
    points: &[ChartPoint],
    selected: Option<Instant>,
    now: Instant,
) -> Option<String> {
    let point = inspected_chart_point(points, selected)?;
    let position = points
        .iter()
        .position(|row| row.measured_at == point.measured_at)?;
    let lag = points
        .last()?
        .measured_at
        .saturating_duration_since(point.measured_at);
    let age = now.saturating_duration_since(point.measured_at);
    Some(format!(
        "Kira · измеренная история системы (снимок)\nЗамер: {} из {}\nОт последнего: {:.1} с\nВозраст: {:.1} с\nCPU: {}\nRAM: {}\nGPU: {}\n",
        position + 1,
        points.len(),
        lag.as_secs_f64(),
        age.as_secs_f64(),
        meter_text(point.cpu, true),
        meter_text(point.ram, true),
        meter_text(point.gpu, true),
    ))
}

pub fn measured_history_caption(points: &[ChartPoint]) -> String {
    let measured: Vec<_> = points
        .iter()
        .filter(|point| point.cpu.is_some() || point.ram.is_some() || point.gpu.is_some())
        .collect();
    match measured.len() {
        0 => "Нет доступных замеров за последнюю минуту".into(),
        1 => "1 реальный замер · ожидание истории".into(),
        count => {
            let span = measured[count - 1]
                .measured_at
                .saturating_duration_since(measured[0].measured_at)
                .as_secs_f64();
            let form = match count % 100 {
                11..=14 => "реальных замеров",
                _ => match count % 10 {
                    1 => "реальный замер",
                    2..=4 => "реальных замера",
                    _ => "реальных замеров",
                },
            };
            format!("{count} {form} · {span:.1} с истории")
        }
    }
}

fn meter_text(value: Option<f64>, precise: bool) -> String {
    value.map_or_else(
        || "—".into(),
        |n| {
            if precise {
                format!("{n:.1}%")
            } else {
                format!("{n:.0}%")
            }
        },
    )
}

/// Only actual finite utilization readings are permitted to paint a gauge.
/// A missing value leaves its track visible without implying zero utilization.
fn gauge_sweep(value: Option<f64>) -> Option<f64> {
    value
        .filter(|v| v.is_finite() && (0.0..=100.0).contains(v))
        .map(|v| std::f64::consts::TAU * v / 100.0)
}

fn draw_gauges(
    cr: &gtk::cairo::Context,
    width: i32,
    height: i32,
    cpu: Option<f64>,
    ram: Option<f64>,
) {
    if width < 80 || height < 40 {
        return;
    }
    let half = f64::from(width) / 2.0;
    // Frozen main: two silent full-circle ECharts gauges, 73% radius, 8px
    // rounded tracks, centered at 25% and 75% within a 182px-high stage.
    let radius = (half.min(f64::from(height)) * 0.73 / 2.0 - 4.0).max(1.0);
    let center_y = f64::from(height) / 2.0;
    let start = -std::f64::consts::FRAC_PI_2;
    cr.set_line_width(8.0);
    cr.set_line_cap(gtk::cairo::LineCap::Round);
    for (index, value, color) in [(0, cpu, (0.988, 0.639, 0.067)), (1, ram, (1.0, 1.0, 1.0))] {
        let center_x = (f64::from(index) + 0.5) * half;
        cr.set_source_rgb(0.078, 0.129, 0.239);
        cr.arc(
            center_x,
            center_y,
            radius,
            start,
            start + std::f64::consts::TAU,
        );
        let _ = cr.stroke();
        if let Some(sweep) = gauge_sweep(value)
            && sweep > 0.0
        {
            cr.set_source_rgb(color.0, color.1, color.2);
            cr.arc(center_x, center_y, radius, start, start + sweep);
            let _ = cr.stroke();
        }
    }
}

/// Project only measured points onto the shared monotonic time axis. CPU/RAM
/// gaps break their traces; GPU connects sparse genuine readings as main does,
/// but never inserts a fake intermediate reading.
fn history_paths(
    points: &[ChartPoint],
    domain: ChartTimeDomain,
    lane: ActivityLane,
    left: f64,
    right: f64,
    top: f64,
    bottom: f64,
) -> Vec<Vec<(f64, f64)>> {
    let span = domain
        .end
        .saturating_duration_since(domain.start)
        .as_secs_f64();
    let axis = activity_axis(points, lane);
    let mut paths = Vec::new();
    let mut path = Vec::new();
    for point in points {
        if let Some(value) = reading(point, lane) {
            let elapsed = point
                .measured_at
                .saturating_duration_since(domain.start)
                .as_secs_f64();
            let x = left + (right - left) * (elapsed / span).clamp(0.0, 1.0);
            let y = bottom - ((value - axis.min) / (axis.max - axis.min)) * (bottom - top);
            path.push((x, y));
        } else if lane != ActivityLane::Gpu && !path.is_empty() {
            paths.push(std::mem::take(&mut path));
        }
    }
    if !path.is_empty() {
        paths.push(path);
    }
    paths
}

/// Source's 0.18 line smoothing with monotone horizontal tangents. Control
/// points stay between measured Y values, never visually overshooting 0–100%.
fn trace_curve(cr: &gtk::cairo::Context, path: &[(f64, f64)]) {
    if let Some(&(x, y)) = path.first() {
        cr.move_to(x, y);
        for segment in path.windows(2) {
            let (prev_x, prev_y) = segment[0];
            let (next_x, next_y) = segment[1];
            let bend = (next_x - prev_x) * 0.18;
            cr.curve_to(prev_x + bend, prev_y, next_x - bend, next_y, next_x, next_y);
        }
    }
}

fn draw_history(
    cr: &gtk::cairo::Context,
    width: i32,
    height: i32,
    points: &[ChartPoint],
    domain: Option<ChartTimeDomain>,
    show_gpu: bool,
    trace_extent: f64,
) {
    if width <= 80 || height <= 60 {
        return;
    }
    // Frozen main uses 96px stride, 28px plot offset and 56px chart height.
    let left = 41.0;
    let right = f64::from(width) - 8.0;
    let lanes: &[ActivityLane] = if show_gpu {
        &[ActivityLane::Cpu, ActivityLane::Ram, ActivityLane::Gpu]
    } else {
        &[ActivityLane::Cpu, ActivityLane::Ram]
    };
    for (index, &lane) in lanes.iter().enumerate() {
        let top = index as f64 * 96.0 + 28.0;
        let bottom = top + 56.0;
        if bottom > f64::from(height) {
            break;
        }
        let axis = activity_axis(points, lane);
        let (red, green, blue, alpha, name) = match lane {
            ActivityLane::Cpu => (0.988, 0.639, 0.067, 0.18, "CPU"),
            ActivityLane::Ram => (1.0, 1.0, 1.0, 0.09, "RAM"),
            ActivityLane::Gpu => (0.898, 0.898, 0.898, 0.09, "GPU"),
        };
        cr.set_font_size(10.0);
        cr.set_source_rgb(red, green, blue);
        cr.move_to(36.0, top - 17.0);
        let _ = cr.show_text(name);
        cr.set_font_size(11.0);
        for tick in 0..=2 {
            let frac = f64::from(tick) / 2.0;
            let y = top + (bottom - top) * frac;
            cr.set_source_rgb(0.078, 0.129, 0.239);
            cr.set_line_width(1.0);
            cr.move_to(left, y);
            cr.line_to(right, y);
            let _ = cr.stroke();
            let limit = axis.max - (axis.max - axis.min) * frac;
            cr.set_source_rgb(0.898, 0.898, 0.898);
            cr.move_to(1.0, y + 3.0);
            let _ = cr.show_text(&format!("{limit:.1}%"));
        }
        // The path is still built solely from actual readings. Retargeting
        // only reveals its recorded geometry up to the current painted time.
        let clipped_right = left + (right - left) * trace_extent.clamp(0.0, 1.0);
        let _ = cr.save();
        cr.rectangle(
            left - 3.0,
            top - 3.0,
            (clipped_right - left + 6.0).max(0.0),
            62.0,
        );
        cr.clip();
        for path in domain.map_or_else(Vec::new, |domain| {
            history_paths(points, domain, lane, left, right, top, bottom)
        }) {
            if path.len() >= 2 {
                trace_curve(cr, &path);
                if let (Some(first), Some(last)) = (path.first(), path.last()) {
                    cr.line_to(last.0, bottom);
                    cr.line_to(first.0, bottom);
                    cr.close_path();
                    cr.set_source_rgba(red, green, blue, alpha);
                    let _ = cr.fill();
                }
                trace_curve(cr, &path);
                cr.set_source_rgb(red, green, blue);
                cr.set_line_width(2.5);
                let _ = cr.stroke();
            }
            // Main shows 5px symbols until there are three aligned points.
            if points.len() < 3 || path.len() == 1 {
                cr.set_source_rgb(red, green, blue);
                for (x, y) in path {
                    cr.arc(x, y, 2.5, 0.0, std::f64::consts::TAU);
                    let _ = cr.fill();
                }
            }
        }
        let _ = cr.restore();
    }
}

/// A pin is drawn only for a genuine point already painted on the
/// measured time axis, never for a synthetic interpolated CPU/RAM/GPU value.
fn draw_inspection_marker(
    cr: &gtk::cairo::Context,
    width: i32,
    height: i32,
    points: &[ChartPoint],
    domain: Option<ChartTimeDomain>,
    extent: f64,
    selected: Option<Instant>,
) {
    let Some((domain, stamp)) = domain.zip(selected) else {
        return;
    };
    if !points.iter().any(|point| point.measured_at == stamp) {
        return;
    }
    let span = domain
        .end
        .saturating_duration_since(domain.start)
        .as_secs_f64();
    if span <= 0.0 || width <= 80 || height <= 60 {
        return;
    }
    let fraction = stamp.saturating_duration_since(domain.start).as_secs_f64() / span;
    if !(0.0..=extent.clamp(0.0, 1.0)).contains(&fraction) {
        return;
    }
    let x = 41.0 + (f64::from(width) - 49.0) * fraction;
    cr.set_source_rgba(0.988, 0.639, 0.067, 0.84);
    cr.set_line_width(1.5);
    cr.move_to(x, 10.0);
    cr.line_to(x, f64::from(height) - 7.0);
    let _ = cr.stroke();
    cr.arc(x, 10.0, 3.0, 0.0, std::f64::consts::TAU);
    let _ = cr.fill();
}

type CopyInspection = Box<dyn Fn(String) -> Result<(), String>>;

struct ChartState {
    history: RefCell<PulseHistory>,
    gpu: RefCell<GpuPulseCoupler>,
    animation: RefCell<ChartMotion>,
    motion: MotionToken,
    cpu_text: gtk::Label,
    ram_text: gtk::Label,
    gauge_canvas: gtk::DrawingArea,
    cpu_axis: gtk::Label,
    ram_axis: gtk::Label,
    gpu_axis: gtk::Label,
    caption: gtk::Label,
    plot: gtk::DrawingArea,
    selected_at: Cell<Option<Instant>>,
    inspector_text: gtk::Label,
    inspector_previous: gtk::Button,
    inspector_next: gtk::Button,
    inspector_live: gtk::Button,
    inspector_copy: gtk::Button,
    inspector_feedback: gtk::Label,
    on_copy: RefCell<Option<CopyInspection>>,
}

impl ChartState {
    fn inspect(&self, action: InspectionAction) {
        let points = inspectable_history(&self.history.borrow().recent(Instant::now()));
        self.selected_at
            .set(inspection_target(&points, self.selected_at.get(), action));
        self.inspector_feedback.set_text("");
        self.refresh();
    }

    fn inspect_pointer(&self, x: f64, width: i32) {
        let now = Instant::now();
        let points = self.history.borrow().recent(now);
        let visible = inspectable_history(&points);
        let Some((first, last)) = points.first().zip(points.last()) else {
            return;
        };
        let domain = self
            .animation
            .borrow()
            .display_domain(first.measured_at, last.measured_at);
        if let Some(stamp) = chart_sample_at_x(&visible, domain, x, width) {
            self.selected_at.set(Some(stamp));
            self.inspector_feedback.set_text("");
            self.refresh();
        }
    }

    fn copy_inspection(&self) {
        let now = Instant::now();
        let points = inspectable_history(&self.history.borrow().recent(now));
        let report = chart_inspection_report(&points, self.selected_at.get(), now);
        let result = match report {
            Some(report) => self.on_copy.borrow().as_ref().map_or_else(
                || Err("Копирование недоступно".to_owned()),
                |copy| copy(report),
            ),
            None => Err("Нет доступного замера для копирования".to_owned()),
        };
        self.inspector_feedback.set_text(match result {
            Ok(()) => "Замер скопирован",
            Err(ref error) => error,
        });
    }

    fn animate_sample(
        self: &Rc<Self>,
        view: &gtk::Widget,
        point: ChartPoint,
        first_visible_at: Instant,
        previous: Option<Instant>,
    ) {
        // Hidden or reduced-motion windows settle immediately. Re-showing a
        // detached chart must never revive an old GTK frame callback.
        let animate = view.is_mapped() && animations_enabled();
        self.motion.cancel();
        let duration = self.animation.borrow_mut().begin(
            point.measured_at,
            first_visible_at,
            previous,
            point.cpu,
            point.ram,
            animate,
        );
        if !animate {
            return;
        }
        let weak = Rc::downgrade(self);
        self.motion.run(
            view,
            duration,
            move |cursor| {
                if let Some(state) = weak.upgrade() {
                    state.animation.borrow_mut().step(cursor);
                    state.gauge_canvas.queue_draw();
                    state.plot.queue_draw();
                }
            },
            || {},
        );
    }

    fn refresh(&self) {
        let now = Instant::now();
        let history = self.history.borrow();
        let current = history.latest(now);
        if current.is_none() {
            self.motion.cancel();
            self.animation.borrow_mut().expire_gauges();
        }
        let cpu = current.and_then(|point| point.cpu);
        let ram = current.and_then(|point| point.ram);
        self.cpu_text.set_text(&meter_text(cpu, false));
        self.ram_text.set_text(&meter_text(ram, true));
        self.cpu_text.set_tooltip_text(Some(&format!(
            "Загрузка CPU: {}",
            cpu.map_or_else(|| "нет данных".into(), |n| format!("{n:.0}%"))
        )));
        self.ram_text.set_tooltip_text(Some(&format!(
            "Использование RAM: {}",
            ram.map_or_else(|| "нет данных".into(), |n| format!("{n:.1}%"))
        )));
        self.gauge_canvas.queue_draw();
        let points = history.recent(now);
        let inspectable = inspectable_history(&points);
        // A pinned Instant can fall out of the 60s window. Never silently
        // claim that the next sample was the user's old selection.
        if self
            .selected_at
            .get()
            .is_some_and(|at| !inspectable.iter().any(|point| point.measured_at == at))
        {
            self.selected_at.set(None);
            self.inspector_feedback
                .set_text("Выбранный замер вышел из истории");
        }
        let selected = self.selected_at.get();
        let index = selected
            .and_then(|at| inspectable.iter().position(|point| point.measured_at == at))
            .unwrap_or_else(|| inspectable.len().saturating_sub(1));
        self.inspector_previous
            .set_sensitive(!inspectable.is_empty() && index > 0);
        self.inspector_next.set_sensitive(selected.is_some());
        self.inspector_live.set_sensitive(selected.is_some());
        self.inspector_copy
            .set_sensitive(!inspectable.is_empty() && self.on_copy.borrow().is_some());
        self.inspector_text
            .set_text(&chart_inspection_text(&inspectable, selected, now));
        let show_gpu = self.gpu.borrow().is_fresh(now);
        self.gpu_axis.set_visible(show_gpu);
        self.plot
            .set_content_height(if show_gpu { 296 } else { 202 });
        self.caption.set_text(&measured_history_caption(&points));
        let cpu_axis = activity_axis(&points, ActivityLane::Cpu);
        let ram_axis = activity_axis(&points, ActivityLane::Ram);
        let gpu_axis = activity_axis(&points, ActivityLane::Gpu);
        self.gpu_axis
            .set_text(&format!("GPU {:.1}–{:.1}%", gpu_axis.min, gpu_axis.max));
        self.cpu_axis
            .set_text(&format!("CPU {:.1}–{:.1}%", cpu_axis.min, cpu_axis.max));
        self.ram_axis
            .set_text(&format!("RAM {:.1}–{:.1}%", ram_axis.min, ram_axis.max));
        let plot_label = format!(
            "История нагрузки с адаптивными шкалами. {}. CPU: {}, RAM: {}. {}. Стрелки влево/вправо выбирают замеры, Home/End переходят к началу/концу.",
            measured_history_caption(&points),
            meter_text(cpu, false),
            meter_text(ram, true),
            chart_inspection_text(&inspectable, selected, now)
        );
        self.plot.set_tooltip_text(Some(&plot_label));
        self.plot
            .update_property(&[gtk::accessible::Property::Label(&plot_label)]);
        self.plot.queue_draw();
    }
}

/// Presentation-only GTK component. The host supplies real timestamped pulses.
pub struct SystemCharts {
    root: gtk::Box,
    state: Rc<ChartState>,
}

impl SystemCharts {
    pub fn new() -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 8);
        root.add_css_class("system-charts");

        let gauges = gtk::Overlay::new();
        gauges.add_css_class("system-chart-gauges");
        let gauge_canvas = gtk::DrawingArea::new();
        gauge_canvas.set_content_width(320);
        gauge_canvas.set_content_height(182);
        gauge_canvas.set_hexpand(true);
        gauge_canvas.set_can_target(false);
        gauges.set_child(Some(&gauge_canvas));
        let meter_row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        meter_row.set_homogeneous(true);
        let mut meters = Vec::new();
        for (title, css_class) in [("CPU", "system-chart-cpu"), ("RAM", "system-chart-ram")] {
            let column = gtk::Box::new(gtk::Orientation::Vertical, 8);
            column.add_css_class("system-chart-meter");
            column.set_valign(gtk::Align::Center);
            column.set_hexpand(true);
            let value = gtk::Label::new(Some("—"));
            value.add_css_class("system-chart-value");
            value.add_css_class(css_class);
            let name = gtk::Label::new(Some(title));
            name.add_css_class("system-chart-label");
            name.add_css_class("muted");
            column.append(&value);
            column.append(&name);
            meter_row.append(&column);
            meters.push(value);
        }
        gauges.add_overlay(&meter_row);
        root.append(&gauges);

        let heading = gtk::Label::new(Some("ИСТОРИЯ НАГРУЗКИ"));
        heading.add_css_class("section-title");
        heading.set_xalign(0.0);
        root.append(&heading);
        let plot = gtk::DrawingArea::new();
        plot.set_content_width(320);
        plot.set_content_height(200);
        plot.set_hexpand(true);
        plot.set_can_target(true);
        plot.set_focusable(true);
        plot.add_css_class("system-chart-inspectable");
        root.append(&plot);

        let axes = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        let cpu_axis = gtk::Label::new(Some("CPU 0–100%"));
        let ram_axis = gtk::Label::new(Some("RAM 0–100%"));
        let gpu_axis = gtk::Label::new(Some("GPU 0–100%"));
        gpu_axis.set_visible(false);
        for label in [&cpu_axis, &ram_axis, &gpu_axis] {
            label.add_css_class("muted");
            axes.append(label);
        }
        root.append(&axes);
        let caption = gtk::Label::new(Some("Нет доступных замеров за последнюю минуту"));
        caption.add_css_class("muted");
        caption.set_xalign(0.0);
        caption.set_wrap(true);
        root.append(&caption);

        // A focusable, accessible workbench for genuine measured points.
        // The chosen Instant is independent of visual gauge interpolation.
        let inspector = gtk::Box::new(gtk::Orientation::Vertical, 7);
        inspector.add_css_class("system-chart-inspector");
        let controls = gtk::FlowBox::new();
        controls.set_selection_mode(gtk::SelectionMode::None);
        controls.set_min_children_per_line(1);
        controls.set_max_children_per_line(4);
        controls.set_column_spacing(6);
        controls.set_row_spacing(6);
        controls.add_css_class("system-chart-inspector-controls");
        let previous = gtk::Button::with_label("◀");
        previous.set_tooltip_text(Some("Предыдущий реальный замер"));
        previous.update_property(&[gtk::accessible::Property::Label(
            "Предыдущий замер нагрузки",
        )]);
        let next = gtk::Button::with_label("▶");
        next.set_tooltip_text(Some("Следующий реальный замер"));
        next.update_property(&[gtk::accessible::Property::Label("Следующий замер нагрузки")]);
        let live = gtk::Button::with_label("СЕЙЧАС");
        live.set_tooltip_text(Some("Вернуться к последнему полученному замеру"));
        live.update_property(&[gtk::accessible::Property::Label(
            "Вернуться к последнему замеру",
        )]);
        let copy = gtk::Button::with_label("КОПИРОВАТЬ");
        copy.set_tooltip_text(Some("Скопировать фактические значения выбранного замера"));
        copy.update_property(&[gtk::accessible::Property::Label(
            "Скопировать выбранный измеренный снимок",
        )]);
        for button in [&previous, &next, &live, &copy] {
            button.add_css_class("system-chart-inspector-button");
            controls.insert(button, -1);
        }
        inspector.append(&controls);
        let inspector_text = gtk::Label::new(Some("НЕТ ЗАМЕРОВ"));
        inspector_text.set_xalign(0.0);
        inspector_text.set_wrap(true);
        inspector_text.set_selectable(true);
        inspector_text.add_css_class("system-chart-inspector-text");
        inspector.append(&inspector_text);
        let feedback = gtk::Label::new(None);
        feedback.set_xalign(0.0);
        feedback.set_wrap(true);
        feedback.add_css_class("system-chart-inspector-feedback");
        inspector.append(&feedback);
        root.append(&inspector);

        let state = Rc::new(ChartState {
            history: RefCell::new(PulseHistory::default()),
            gpu: RefCell::new(GpuPulseCoupler::default()),
            animation: RefCell::new(ChartMotion::default()),
            motion: MotionToken::default(),
            cpu_text: meters[0].clone(),
            ram_text: meters[1].clone(),
            gauge_canvas: gauge_canvas.clone(),
            cpu_axis,
            ram_axis,
            gpu_axis,
            caption,
            plot: plot.clone(),
            selected_at: Cell::new(None),
            inspector_text,
            inspector_previous: previous.clone(),
            inspector_next: next.clone(),
            inspector_live: live.clone(),
            inspector_copy: copy.clone(),
            inspector_feedback: feedback,
            on_copy: RefCell::new(None),
        });
        let weak = Rc::downgrade(&state);
        gauge_canvas.set_draw_func(move |_, cr, width, height| {
            if let Some(state) = weak.upgrade() {
                let current = state.history.borrow().latest(Instant::now());
                let painted = state.animation.borrow().gauges();
                draw_gauges(
                    cr,
                    width,
                    height,
                    current.and_then(|p| p.cpu).and(painted.0),
                    current.and_then(|p| p.ram).and(painted.1),
                );
            }
        });
        let weak = Rc::downgrade(&state);
        plot.set_draw_func(move |_, cr, width, height| {
            if let Some(state) = weak.upgrade() {
                let now = Instant::now();
                let points = state.history.borrow().recent(now);
                let (domain, extent) = {
                    let animation = state.animation.borrow();
                    let endpoints = points.first().zip(points.last());
                    (
                        endpoints.map(|(first, last)| {
                            animation.display_domain(first.measured_at, last.measured_at)
                        }),
                        endpoints.map_or(1.0, |(first, last)| {
                            animation.trace_extent(first.measured_at, last.measured_at)
                        }),
                    )
                };
                draw_history(
                    cr,
                    width,
                    height,
                    &points,
                    domain,
                    state.gpu.borrow().is_fresh(now),
                    extent,
                );
                draw_inspection_marker(
                    cr,
                    width,
                    height,
                    &points,
                    domain,
                    extent,
                    state.selected_at.get(),
                );
            }
        });
        for (button, action) in [
            (&previous, InspectionAction::Earlier),
            (&next, InspectionAction::Later),
            (&live, InspectionAction::Live),
        ] {
            let weak = Rc::downgrade(&state);
            button.connect_clicked(move |_| {
                if let Some(state) = weak.upgrade() {
                    state.inspect(action);
                }
            });
        }
        let weak = Rc::downgrade(&state);
        copy.connect_clicked(move |_| {
            if let Some(state) = weak.upgrade() {
                state.copy_inspection();
            }
        });
        let weak = Rc::downgrade(&state);
        let chart_pointer = gtk::GestureClick::new();
        chart_pointer.connect_pressed(move |gesture, _, x, _| {
            if let Some(state) = weak.upgrade() {
                let width = gesture.widget().map_or(0, |widget| widget.width());
                state.inspect_pointer(x, width);
                state.plot.grab_focus();
            }
        });
        plot.add_controller(chart_pointer);
        let key = gtk::EventControllerKey::new();
        let weak = Rc::downgrade(&state);
        key.connect_key_pressed(move |_, pressed, _, modifiers| {
            if modifiers.intersects(
                gtk::gdk::ModifierType::CONTROL_MASK
                    | gtk::gdk::ModifierType::ALT_MASK
                    | gtk::gdk::ModifierType::SUPER_MASK,
            ) {
                return gtk::glib::Propagation::Proceed;
            }
            let action = match pressed {
                gtk::gdk::Key::Left => InspectionAction::Earlier,
                gtk::gdk::Key::Right => InspectionAction::Later,
                gtk::gdk::Key::Home => InspectionAction::First,
                gtk::gdk::Key::End => InspectionAction::Live,
                _ => return gtk::glib::Propagation::Proceed,
            };
            if let Some(state) = weak.upgrade() {
                state.inspect(action);
            }
            gtk::glib::Propagation::Stop
        });
        plot.add_controller(key);
        let weak = Rc::downgrade(&state);
        root.connect_unmap(move |_| {
            if let Some(state) = weak.upgrade() {
                state.motion.cancel();
                state.animation.borrow_mut().reset();
                state.history.borrow_mut().clear();
                state.gpu.borrow_mut().reset();
                state.selected_at.set(None);
                state.inspector_feedback.set_text("");
                state.refresh();
            }
        });
        state.refresh();
        Self { root, state }
    }

    /// Clipboard transport is owned by the embedding shell, not by the
    /// reusable chart or the process sampler.
    pub fn set_copy_handler(&self, on_copy: impl Fn(String) -> Result<(), String> + 'static) {
        self.state.on_copy.replace(Some(Box::new(on_copy)));
        self.state.refresh();
    }

    pub fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }

    pub fn push_sample(&self, sample: ChartSample) {
        let previous = self
            .state
            .history
            .borrow()
            .samples
            .back()
            .map(|p| p.measured_at);
        // Duplicate/out-of-order samples are neither measurements nor motion.
        if previous.is_some_and(|at| sample.measured_at <= at) {
            return;
        }
        let gpu = self
            .state
            .gpu
            .borrow_mut()
            .take_for_pulse(sample.measured_at);
        self.state.history.borrow_mut().push_with_gpu(sample, gpu);
        let point = ChartPoint {
            gpu: gpu.map(f64::from),
            ..ChartPoint::from(sample)
        };
        let first_visible_at = self
            .state
            .history
            .borrow()
            .recent(sample.measured_at)
            .first()
            .map_or(sample.measured_at, |p| p.measured_at);
        self.state.animate_sample(
            &self.root.clone().upcast(),
            point,
            first_visible_at,
            previous,
        );
        self.state.refresh();
        // One-shot expiry callbacks, not an always-ticking UI timer. Older
        // callbacks cannot forge new readings; they only re-evaluate time.
        // recent() includes a point at EXACTLY 60s. Schedule the one-shot
        // final repaint beyond that inclusive boundary: otherwise a stopped
        // sensor can leave the last history point pinned indefinitely.
        for after in [PULSE_FRESHNESS, MAX_PULSE_AGE + Duration::from_millis(1)] {
            let weak = Rc::downgrade(&self.state);
            gtk::glib::timeout_add_local_once(after, move || {
                if let Some(state) = weak.upgrade() {
                    state.refresh();
                }
            });
        }
    }

    /// The host may submit one actual GPU peak from its slower telemetry worker.
    /// It is consumed only by the next time-ordered 500ms CPU/RAM pulse.
    pub fn observe_gpu(&self, measured_at: Instant, peak_percent: Option<u8>) {
        self.state
            .gpu
            .borrow_mut()
            .observe(measured_at, peak_percent);
        self.state.refresh();
        let weak = Rc::downgrade(&self.state);
        gtk::glib::timeout_add_local_once(GPU_FRESHNESS, move || {
            if let Some(state) = weak.upgrade() {
                state.refresh();
            }
        });
    }
}

impl Default for SystemCharts {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(at: Instant, cpu: Option<u8>, used: Option<u64>) -> ChartSample {
        ChartSample {
            measured_at: at,
            cpu_percent: cpu,
            memory_total_kib: Some(1000),
            memory_available_kib: used.map(|value| 1000 - value),
        }
    }

    #[test]
    fn inspector_moves_only_between_actual_pulses_and_returns_to_live() {
        let t = Instant::now();
        let points = [0_u64, 500, 1000]
            .into_iter()
            .map(|ms| ChartPoint {
                measured_at: t + Duration::from_millis(ms),
                cpu: Some(10.0 + ms as f64 / 100.0),
                ram: Some(50.0),
                gpu: None,
            })
            .collect::<Vec<_>>();
        let second = t + Duration::from_millis(500);
        assert_eq!(
            inspection_target(&points, None, InspectionAction::Earlier),
            Some(second)
        );
        assert_eq!(
            inspection_target(&points, Some(second), InspectionAction::Earlier),
            Some(t)
        );
        assert_eq!(
            inspection_target(&points, Some(t), InspectionAction::Earlier),
            Some(t)
        );
        assert_eq!(
            inspection_target(&points, Some(t), InspectionAction::Later),
            Some(second)
        );
        assert_eq!(
            inspection_target(&points, Some(second), InspectionAction::Later),
            None
        );
        assert_eq!(
            inspection_target(&points, None, InspectionAction::First),
            Some(t)
        );
        assert_eq!(
            inspection_target(&points, Some(t), InspectionAction::Live),
            None
        );
        assert_eq!(
            inspection_target(&[], None, InspectionAction::Earlier),
            None
        );
        assert_eq!(
            inspected_chart_point(&points, Some(second)).unwrap().cpu,
            Some(15.0)
        );
        assert_eq!(
            inspected_chart_point(&points[1..], Some(t)),
            None,
            "an expired selected Instant cannot silently impersonate another sample"
        );
    }

    #[test]
    fn empty_worker_pulses_are_not_misrepresented_as_measurements() {
        let t = Instant::now();
        let points = [
            ChartPoint {
                measured_at: t,
                cpu: None,
                ram: None,
                gpu: None,
            },
            ChartPoint {
                measured_at: t + Duration::from_millis(500),
                cpu: Some(0.0),
                ram: None,
                gpu: None,
            },
            ChartPoint {
                measured_at: t + Duration::from_millis(1000),
                cpu: None,
                ram: None,
                gpu: Some(70.0),
            },
        ];
        let usable = inspectable_history(&points);
        assert_eq!(usable.len(), 2);
        assert_eq!(usable[0].cpu, Some(0.0));
        assert_eq!(usable[1].gpu, Some(70.0));
        let empty = inspectable_history(&points[..1]);
        assert_eq!(inspection_target(&empty, None, InspectionAction::First), None);
        assert!(chart_inspection_report(&empty, None, t).is_none());
    }

    #[test]
    fn pointer_hit_tests_share_the_measured_time_axis_and_reject_invalid_geometry() {
        let t = Instant::now();
        let points = [0_u64, 500, 1000]
            .into_iter()
            .map(|ms| ChartPoint {
                measured_at: t + Duration::from_millis(ms),
                cpu: Some(50.0),
                ram: None,
                gpu: None,
            })
            .collect::<Vec<_>>();
        let domain = ChartTimeDomain::measured(t, t + Duration::from_millis(1000));
        assert_eq!(chart_sample_at_x(&points, domain, 41.0, 349), Some(t));
        assert_eq!(
            chart_sample_at_x(&points, domain, 191.0, 349),
            Some(t + Duration::from_millis(500))
        );
        assert_eq!(
            chart_sample_at_x(&points, domain, 341.0, 349),
            Some(t + Duration::from_millis(1000))
        );
        assert_eq!(chart_sample_at_x(&points, domain, f64::NAN, 349), None);
        assert_eq!(chart_sample_at_x(&points, domain, 191.0, 30), None);
        assert_eq!(chart_sample_at_x(&[], domain, 191.0, 349), None);
    }

    #[test]
    fn historical_copy_preserves_missing_gpu_and_explicit_snapshot_age() {
        let t = Instant::now();
        let points = vec![
            ChartPoint {
                measured_at: t,
                cpu: Some(10.0),
                ram: Some(33.3),
                gpu: None,
            },
            ChartPoint {
                measured_at: t + Duration::from_secs(2),
                cpu: Some(25.0),
                ram: None,
                gpu: Some(72.0),
            },
        ];
        let now = t + Duration::from_secs(3);
        let history = chart_inspection_report(&points, Some(t), now).unwrap();
        assert!(history.contains("Замер: 1 из 2"));
        assert!(history.contains("От последнего: 2.0 с"));
        assert!(history.contains("Возраст: 3.0 с"));
        assert!(history.contains("RAM: 33.3%"));
        assert!(history.contains("GPU: —"));
        assert!(chart_inspection_text(&points, None, now).contains("ТЕКУЩИЙ ЗАМЕР"));
        assert!(chart_inspection_text(&points, Some(t), now).contains("ВЫБРАН ЗАМЕР"));
        assert!(
            chart_inspection_text(&points, None, now + Duration::from_secs(3))
                .contains("ПОСЛЕДНИЙ ЗАМЕР УСТАРЕЛ")
        );
        assert!(chart_inspection_report(&points, Some(t - Duration::from_secs(1)), now).is_none());
        assert!(chart_inspection_report(&[], None, now).is_none());
    }

    #[test]
    fn stores_only_real_pulses_and_aligns_first_concurrent_sample() {
        let t = Instant::now();
        let mut history = PulseHistory::default();
        history.push(sample(t, None, Some(300)));
        history.push(sample(t + Duration::from_millis(500), Some(20), Some(310)));
        history.push(sample(t + Duration::from_millis(1000), Some(25), Some(320)));
        let points = history.recent(t + Duration::from_millis(1000));
        assert_eq!(points.len(), 2);
        assert_eq!(points[0].measured_at, t + Duration::from_millis(500));
        assert_eq!(points[0].ram, Some(31.0));
    }

    #[test]
    fn limits_count_elapsed_time_and_out_of_order_data() {
        let t = Instant::now();
        let mut history = PulseHistory::default();
        for n in 0..150 {
            history.push(sample(
                t + Duration::from_millis(n * 500),
                Some(50),
                Some(500),
            ));
        }
        assert_eq!(
            history.recent(t + Duration::from_millis(74500)).len(),
            MAX_PULSE_POINTS
        );
        history.push(sample(t + Duration::from_millis(500), Some(99), Some(990)));
        assert_eq!(
            history
                .latest(t + Duration::from_millis(74500))
                .unwrap()
                .cpu,
            Some(50.0)
        );
        assert!(history.recent(t + Duration::from_secs(140)).is_empty());
        assert!(history.latest(t + Duration::from_secs(140)).is_none());
    }

    #[test]
    fn sixty_second_inclusive_boundary_requires_one_millisecond_expiry_repaint() {
        let at = Instant::now();
        let mut history = PulseHistory::default();
        history.push(sample(at, Some(37), Some(250)));
        assert_eq!(history.recent(at + MAX_PULSE_AGE).len(), 1);
        assert!(
            history
                .recent(at + MAX_PULSE_AGE + Duration::from_millis(1))
                .is_empty()
        );
        assert!(
            history
                .latest(at + MAX_PULSE_AGE + Duration::from_millis(1))
                .is_none()
        );
    }

    #[test]
    fn unknown_values_stay_unknown_and_axes_are_bounded() {
        let t = Instant::now();
        let broken = ChartPoint::from(ChartSample {
            measured_at: t,
            cpu_percent: Some(200),
            memory_total_kib: Some(10),
            memory_available_kib: Some(11),
        });
        assert_eq!(broken.cpu, None);
        assert_eq!(broken.ram, None);
        let points = [
            ChartPoint {
                measured_at: t,
                cpu: Some(99.9),
                ram: Some(12.5),
                gpu: None,
            },
            ChartPoint {
                measured_at: t,
                cpu: Some(100.0),
                ram: Some(12.6),
                gpu: None,
            },
        ];
        let cpu = activity_axis(&points, ActivityLane::Cpu);
        let ram = activity_axis(&points, ActivityLane::Ram);
        assert_eq!(cpu.max, 100.0);
        assert!(cpu.min <= 99.9);
        assert!(ram.min <= 12.5 && ram.max >= 12.6);
        assert!(ram.max - ram.min >= 4.0);
        assert_eq!(
            activity_axis(&[broken], ActivityLane::Cpu),
            ActivityAxis {
                min: 0.0,
                max: 100.0
            }
        );
    }

    #[test]
    fn freshness_does_not_turn_stopped_sensor_into_zero() {
        let t = Instant::now();
        let mut history = PulseHistory::default();
        history.push(sample(t, Some(0), Some(0)));
        assert_eq!(history.latest(t).unwrap().cpu, Some(0.0));
        assert!(history.latest(t + PULSE_FRESHNESS).is_none());
        assert_eq!(memory_percent(Some(0), Some(0)), None);
        assert_eq!(memory_percent(Some(100), Some(101)), None);
        assert_eq!(memory_percent(Some(200), Some(100)), Some(50.0));
    }

    #[test]
    fn gpu_attaches_once_to_a_real_later_pulse_and_never_fills_gaps() {
        let t = Instant::now();
        let mut coupler = GpuPulseCoupler::default();
        let mut history = PulseHistory::default();
        coupler.observe(t, Some(42));
        assert!(coupler.is_fresh(t));
        assert_eq!(coupler.take_for_pulse(t - Duration::from_millis(1)), None);
        history.push_with_gpu(
            sample(t + Duration::from_millis(500), Some(10), Some(300)),
            coupler.take_for_pulse(t + Duration::from_millis(500)),
        );
        history.push_with_gpu(
            sample(t + Duration::from_millis(1000), Some(11), Some(310)),
            coupler.take_for_pulse(t + Duration::from_millis(1000)),
        );
        let data = history.recent(t + Duration::from_millis(1000));
        assert_eq!(data.len(), 2);
        assert_eq!(data[0].gpu, Some(42.0));
        assert_eq!(data[1].gpu, None);
        assert!(activity_axis(&data, ActivityLane::Gpu).max >= 42.0);
        assert!(!coupler.is_fresh(t + GPU_FRESHNESS));
    }

    #[test]
    fn gauge_displays_unknown_without_inventing_zero_and_bounds_sweep() {
        assert_eq!(gauge_sweep(None), None);
        assert_eq!(gauge_sweep(Some(f64::NAN)), None);
        assert_eq!(gauge_sweep(Some(-0.1)), None);
        assert_eq!(gauge_sweep(Some(101.0)), None);
        assert_eq!(gauge_sweep(Some(0.0)), Some(0.0));
        assert_eq!(gauge_sweep(Some(100.0)), Some(std::f64::consts::TAU));
        assert_eq!(gauge_sweep(Some(25.0)), Some(std::f64::consts::FRAC_PI_2));
        assert_eq!(meter_text(None, true), "—");
        assert_eq!(meter_text(Some(13.5), true), "13.5%");
        assert_eq!(meter_text(Some(13.0), false), "13%");
    }

    #[test]
    fn timestamp_paths_break_cpu_gaps_but_connect_only_real_gpu_points() {
        let t = Instant::now();
        let points = [
            ChartPoint {
                measured_at: t,
                cpu: Some(10.0),
                ram: Some(50.0),
                gpu: Some(20.0),
            },
            ChartPoint {
                measured_at: t + Duration::from_millis(500),
                cpu: None,
                ram: Some(60.0),
                gpu: None,
            },
            ChartPoint {
                measured_at: t + Duration::from_millis(1500),
                cpu: Some(70.0),
                ram: Some(70.0),
                gpu: Some(80.0),
            },
        ];
        let cpu = history_paths(
            &points,
            ChartTimeDomain::measured(t, t + Duration::from_millis(1500)),
            ActivityLane::Cpu,
            41.0,
            341.0,
            28.0,
            84.0,
        );
        assert_eq!(cpu.len(), 2);
        assert_eq!(cpu[0].len(), 1);
        assert_eq!(cpu[1].len(), 1);
        assert_eq!(cpu[0][0].0, 41.0);
        assert_eq!(cpu[1][0].0, 341.0);
        let gpu = history_paths(
            &points,
            ChartTimeDomain::measured(t, t + Duration::from_millis(1500)),
            ActivityLane::Gpu,
            41.0,
            341.0,
            28.0,
            84.0,
        );
        assert_eq!(gpu.len(), 1);
        assert_eq!(gpu[0].len(), 2);
        assert_eq!(gpu[0][0].0, 41.0);
        assert_eq!(gpu[0][1].0, 341.0);
        let ram = history_paths(
            &points,
            ChartTimeDomain::measured(t, t + Duration::from_millis(1500)),
            ActivityLane::Ram,
            41.0,
            341.0,
            28.0,
            84.0,
        );
        assert_eq!(ram.len(), 1);
        assert_eq!(ram[0].len(), 3);
        assert_eq!(ram[0][1].0, 141.0);
        for path in [cpu, gpu, ram] {
            for (x, y) in path.into_iter().flatten() {
                assert!((41.0..=341.0).contains(&x));
                assert!((28.0..=84.0).contains(&y));
            }
        }
    }

    #[test]
    fn russian_count_agrees_with_frozen_main_for_short_and_long_histories() {
        let t = Instant::now();
        let points = (0..=25)
            .map(|i| ChartPoint {
                measured_at: t + Duration::from_millis(i * 500),
                cpu: Some(1.0),
                ram: None,
                gpu: None,
            })
            .collect::<Vec<_>>();
        for (n, expected) in [
            (2, "2 реальных замера"),
            (3, "3 реальных замера"),
            (5, "5 реальных замеров"),
            (11, "11 реальных замеров"),
            (21, "21 реальный замер"),
            (22, "22 реальных замера"),
        ] {
            assert!(measured_history_caption(&points[..n]).starts_with(expected));
        }
    }

    #[test]
    fn projected_x_uses_painted_axis_instead_of_new_real_endpoint() {
        let t = Instant::now();
        let points = [
            ChartPoint {
                measured_at: t,
                cpu: Some(10.0),
                ram: None,
                gpu: None,
            },
            ChartPoint {
                measured_at: t + Duration::from_millis(500),
                cpu: Some(20.0),
                ram: None,
                gpu: None,
            },
            ChartPoint {
                measured_at: t + Duration::from_millis(1000),
                cpu: Some(30.0),
                ram: None,
                gpu: None,
            },
        ];
        let old = ChartTimeDomain::measured(t, t + Duration::from_millis(500));
        let new = ChartTimeDomain::measured(t, t + Duration::from_millis(1000));
        let old_path = history_paths(&points, old, ActivityLane::Cpu, 41.0, 341.0, 28.0, 84.0);
        let new_path = history_paths(&points, new, ActivityLane::Cpu, 41.0, 341.0, 28.0, 84.0);
        assert_eq!(old_path[0][1].0, 341.0);
        assert_eq!(new_path[0][1].0, 191.0);
        assert_eq!(new_path[0][2].0, 341.0);
    }

    #[test]
    fn gpu_rejects_missing_invalid_stale_and_out_of_order_snapshots() {
        let t = Instant::now();
        let mut coupler = GpuPulseCoupler::default();
        coupler.observe(t, Some(55));
        coupler.observe(t - Duration::from_millis(50), Some(99));
        assert_eq!(coupler.take_for_pulse(t + Duration::from_secs(3)), None);
        coupler.observe(t + Duration::from_secs(4), Some(101));
        assert!(!coupler.is_fresh(t + Duration::from_secs(4)));
        coupler.observe(t + Duration::from_secs(6), Some(0));
        assert_eq!(coupler.take_for_pulse(t + Duration::from_secs(6)), Some(0));
        coupler.observe(t + Duration::from_secs(8), None);
        assert!(!coupler.is_fresh(t + Duration::from_secs(8)));
        coupler.reset();
        assert_eq!(coupler.take_for_pulse(t + Duration::from_secs(9)), None);
    }
}
