//! Reusable native CPU/RAM history. Each point is a real worker pulse, not
//! interpolation between slow system snapshots or a GTK frame-clock sample.
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk::prelude::*;

pub const MAX_PULSE_POINTS: usize = 120;
pub const MAX_PULSE_AGE: Duration = Duration::from_secs(60);
pub const PULSE_FRESHNESS: Duration = Duration::from_millis(2500);

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
        }
    }
}

#[derive(Default)]
pub struct PulseHistory {
    samples: VecDeque<ChartPoint>,
}

impl PulseHistory {
    pub fn push(&mut self, sample: ChartSample) {
        if self
            .samples
            .back()
            .is_some_and(|last| sample.measured_at <= last.measured_at)
        {
            // Never invent ordering or replace a real reading with an older one.
            return;
        }
        self.samples.push_back(sample.into());
        self.prune(sample.measured_at);
    }

    pub fn prune(&mut self, now: Instant) {
        while self.samples.front().is_some_and(|first| {
            now.saturating_duration_since(first.measured_at) > MAX_PULSE_AGE
        }) {
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
        self.samples
            .back()
            .copied()
            .filter(|point| {
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
}

fn reading(point: &ChartPoint, lane: ActivityLane) -> Option<f64> {
    let value = match lane {
        ActivityLane::Cpu => point.cpu,
        ActivityLane::Ram => point.ram,
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
        .filter(|point| point.cpu.is_some() || point.ram.is_some())
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

fn meter_text(value: Option<f64>) -> String {
    value.map_or_else(|| "нет данных".into(), |n| format!("{n:.1}%"))
}

fn draw_history(cr: &gtk::cairo::Context, width: i32, height: i32, points: &[ChartPoint]) {
    if width <= 80 || height <= 60 {
        return;
    }
    let left = 45.0;
    let right = f64::from(width) - 8.0;
    let available_width = (right - left).max(1.0);
    let first = points.first().map(|point| point.measured_at);
    let span = first
        .zip(points.last().map(|point| point.measured_at))
        .map_or(0.5, |(start, end)| {
            end.saturating_duration_since(start).as_secs_f64().max(0.5)
        });
    let lane_height = f64::from(height) * 0.5;
    for (index, lane) in [ActivityLane::Cpu, ActivityLane::Ram]
        .into_iter()
        .enumerate()
    {
        let top = index as f64 * lane_height + 16.0;
        let bottom = (index as f64 + 1.0) * lane_height - 10.0;
        let axis = activity_axis(points, lane);
        cr.set_source_rgb(0.078, 0.129, 0.239);
        cr.set_line_width(1.0);
        for tick in 0..=2 {
            let y = top + (bottom - top) * f64::from(tick) / 2.0;
            cr.move_to(left, y);
            cr.line_to(right, y);
        }
        let _ = cr.stroke();
        cr.set_line_width(2.0);
        match lane {
            ActivityLane::Cpu => cr.set_source_rgb(0.988, 0.639, 0.067),
            ActivityLane::Ram => cr.set_source_rgb(1.0, 1.0, 1.0),
        }
        let mut drawing = false;
        let mut last_xy = None;
        for point in points {
            if let (Some(first), Some(value)) = (first, reading(point, lane)) {
                let x = left
                    + available_width
                        * (point
                            .measured_at
                            .saturating_duration_since(first)
                            .as_secs_f64()
                            / span)
                            .clamp(0.0, 1.0);
                let y = bottom
                    - (value - axis.min) / (axis.max - axis.min) * (bottom - top);
                if !drawing {
                    cr.move_to(x, y);
                    drawing = true;
                } else {
                    cr.line_to(x, y);
                }
                last_xy = Some((x, y));
            } else if drawing {
                let _ = cr.stroke();
                drawing = false;
            }
        }
        if drawing {
            let _ = cr.stroke();
        }
        if points.len() == 1
            && let Some((x, y)) = last_xy
        {
            cr.arc(x, y, 2.5, 0.0, std::f64::consts::TAU);
            let _ = cr.fill();
        }
    }
}

struct ChartState {
    history: RefCell<PulseHistory>,
    cpu_text: gtk::Label,
    ram_text: gtk::Label,
    cpu_bar: gtk::ProgressBar,
    ram_bar: gtk::ProgressBar,
    cpu_axis: gtk::Label,
    ram_axis: gtk::Label,
    caption: gtk::Label,
    plot: gtk::DrawingArea,
}

impl ChartState {
    fn refresh(&self) {
        let now = Instant::now();
        let history = self.history.borrow();
        let current = history.latest(now);
        for (bar, label, value) in [
            (&self.cpu_bar, &self.cpu_text, current.and_then(|p| p.cpu)),
            (&self.ram_bar, &self.ram_text, current.and_then(|p| p.ram)),
        ] {
            bar.set_visible(value.is_some());
            if let Some(value) = value {
                bar.set_fraction(value / 100.0);
            }
            label.set_text(&meter_text(value));
        }
        let points = history.recent(now);
        self.caption.set_text(&measured_history_caption(&points));
        let cpu_axis = activity_axis(&points, ActivityLane::Cpu);
        let ram_axis = activity_axis(&points, ActivityLane::Ram);
        self.cpu_axis
            .set_text(&format!("CPU {:.1}–{:.1}%", cpu_axis.min, cpu_axis.max));
        self.ram_axis
            .set_text(&format!("RAM {:.1}–{:.1}%", ram_axis.min, ram_axis.max));
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

        let gauges = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        gauges.set_homogeneous(true);
        let mut meters = Vec::new();
        for title in ["CPU", "RAM"] {
            let column = gtk::Box::new(gtk::Orientation::Vertical, 4);
            let name = gtk::Label::new(Some(title));
            name.add_css_class("muted");
            let value = gtk::Label::new(Some("нет данных"));
            value.add_css_class("system-chart-value");
            let bar = gtk::ProgressBar::new();
            bar.set_visible(false);
            bar.set_show_text(false);
            column.append(&value);
            column.append(&name);
            column.append(&bar);
            gauges.append(&column);
            meters.push((value, bar));
        }
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
        cpu_axis.add_css_class("muted");
        ram_axis.add_css_class("muted");
        axes.append(&cpu_axis);
        axes.append(&ram_axis);
        root.append(&axes);
        let caption = gtk::Label::new(Some("Нет доступных замеров за последнюю минуту"));
        caption.add_css_class("muted");
        caption.set_xalign(0.0);
        caption.set_wrap(true);
        root.append(&caption);
        let state = Rc::new(ChartState {
            history: RefCell::new(PulseHistory::default()),
            cpu_text: meters[0].0.clone(),
            ram_text: meters[1].0.clone(),
            cpu_bar: meters[0].1.clone(),
            ram_bar: meters[1].1.clone(),
            cpu_axis,
            ram_axis,
            caption,
            plot: plot.clone(),
        });
        let weak = Rc::downgrade(&state);
        plot.set_draw_func(move |_, cr, width, height| {
            if let Some(state) = weak.upgrade() {
                let points = state.history.borrow().recent(Instant::now());
                draw_history(cr, width, height, &points);
            }
        });
        Self { root, state }
    }

    pub fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }

    pub fn push_sample(&self, sample: ChartSample) {
        self.state.history.borrow_mut().push(sample);
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
            history.push(sample(t + Duration::from_millis(n * 500), Some(50), Some(500)));
        }
        assert_eq!(history.recent(t + Duration::from_millis(74500)).len(), MAX_PULSE_POINTS);
        history.push(sample(t + Duration::from_millis(500), Some(99), Some(990)));
        assert_eq!(history.latest(t + Duration::from_millis(74500)).unwrap().cpu, Some(50.0));
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
            ChartPoint { measured_at: t, cpu: Some(99.9), ram: Some(12.5) },
            ChartPoint { measured_at: t, cpu: Some(100.0), ram: Some(12.6) },
        ];
        let cpu = activity_axis(&points, ActivityLane::Cpu);
        let ram = activity_axis(&points, ActivityLane::Ram);
        assert_eq!(cpu.max, 100.0);
        assert!(cpu.min <= 99.9);
        assert!(ram.min <= 12.5 && ram.max >= 12.6);
        assert!(ram.max - ram.min >= 4.0);
        assert_eq!(activity_axis(&[broken], ActivityLane::Cpu),
            ActivityAxis { min: 0.0, max: 100.0 });
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
}
