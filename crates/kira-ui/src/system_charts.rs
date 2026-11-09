//! Reusable native CPU/RAM/GPU history. Each point is a real worker pulse, not
//! interpolation between slow system snapshots or a GTK frame-clock sample.
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk::prelude::*;

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
            format!("{count} реальных замеров · {span:.1} с истории")
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
    lane: ActivityLane,
    left: f64,
    right: f64,
    top: f64,
    bottom: f64,
) -> Vec<Vec<(f64, f64)>> {
    let Some(first) = points.first().map(|p| p.measured_at) else {
        return Vec::new();
    };
    let span = points
        .last()
        .map(|p| p.measured_at.saturating_duration_since(first).as_secs_f64())
        .unwrap_or_default()
        .max(0.5);
    let axis = activity_axis(points, lane);
    let mut paths = Vec::new();
    let mut path = Vec::new();
    for point in points {
        if let Some(value) = reading(point, lane) {
            let elapsed = point
                .measured_at
                .saturating_duration_since(first)
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
    show_gpu: bool,
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
        for path in history_paths(points, lane, left, right, top, bottom) {
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
    }
}

struct ChartState {
    history: RefCell<PulseHistory>,
    gpu: RefCell<GpuPulseCoupler>,
    cpu_text: gtk::Label,
    ram_text: gtk::Label,
    gauge_canvas: gtk::DrawingArea,
    cpu_axis: gtk::Label,
    ram_axis: gtk::Label,
    gpu_axis: gtk::Label,
    caption: gtk::Label,
    plot: gtk::DrawingArea,
}

impl ChartState {
    fn refresh(&self) {
        let now = Instant::now();
        let history = self.history.borrow();
        let current = history.latest(now);
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
        self.plot.set_tooltip_text(Some(&format!(
            "История нагрузки с адаптивными шкалами. {}",
            measured_history_caption(&points)
        )));
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
        plot.set_can_target(false);
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
        let state = Rc::new(ChartState {
            history: RefCell::new(PulseHistory::default()),
            gpu: RefCell::new(GpuPulseCoupler::default()),
            cpu_text: meters[0].clone(),
            ram_text: meters[1].clone(),
            gauge_canvas: gauge_canvas.clone(),
            cpu_axis,
            ram_axis,
            gpu_axis,
            caption,
            plot: plot.clone(),
        });
        let weak = Rc::downgrade(&state);
        gauge_canvas.set_draw_func(move |_, cr, width, height| {
            if let Some(state) = weak.upgrade() {
                let current = state.history.borrow().latest(Instant::now());
                draw_gauges(
                    cr,
                    width,
                    height,
                    current.and_then(|p| p.cpu),
                    current.and_then(|p| p.ram),
                );
            }
        });
        let weak = Rc::downgrade(&state);
        plot.set_draw_func(move |_, cr, width, height| {
            if let Some(state) = weak.upgrade() {
                let now = Instant::now();
                let points = state.history.borrow().recent(now);
                draw_history(cr, width, height, &points, state.gpu.borrow().is_fresh(now));
            }
        });
        let weak = Rc::downgrade(&state);
        root.connect_unmap(move |_| {
            if let Some(state) = weak.upgrade() {
                state.history.borrow_mut().clear();
                state.gpu.borrow_mut().reset();
                state.refresh();
            }
        });
        Self { root, state }
    }

    pub fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }

    pub fn push_sample(&self, sample: ChartSample) {
        let gpu = self
            .state
            .gpu
            .borrow_mut()
            .take_for_pulse(sample.measured_at);
        self.state.history.borrow_mut().push_with_gpu(sample, gpu);
        self.state.refresh();
        // One-shot expiry callbacks, not an always-ticking UI timer. Older
        // callbacks cannot forge new readings; they only re-evaluate time.
        for after in [PULSE_FRESHNESS, MAX_PULSE_AGE] {
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
        let cpu = history_paths(&points, ActivityLane::Cpu, 41.0, 341.0, 28.0, 84.0);
        assert_eq!(cpu.len(), 2);
        assert_eq!(cpu[0].len(), 1);
        assert_eq!(cpu[1].len(), 1);
        assert_eq!(cpu[0][0].0, 41.0);
        assert_eq!(cpu[1][0].0, 341.0);
        let gpu = history_paths(&points, ActivityLane::Gpu, 41.0, 341.0, 28.0, 84.0);
        assert_eq!(gpu.len(), 1);
        assert_eq!(gpu[0].len(), 2);
        assert_eq!(gpu[0][0].0, 41.0);
        assert_eq!(gpu[0][1].0, 341.0);
        let ram = history_paths(&points, ActivityLane::Ram, 41.0, 341.0, 28.0, 84.0);
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
