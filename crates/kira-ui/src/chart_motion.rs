//! Presentation-only, interruptible chart animation. Values are *painted*
//! geometry, never worker samples. CPU/RAM labels always use the real pulse.
//! The time cursor merely clips the line between existing measured points.
use std::time::Instant;

/// Frozen main ECharts initial and update motion durations.
pub(crate) const CHART_INITIAL_MS: u64 = 350;
pub(crate) const CHART_UPDATE_MS: u64 = 180;

fn cubic_out(progress: f64) -> f64 {
    let t = if progress.is_finite() {
        progress.clamp(0.0, 1.0)
    } else {
        0.0
    };
    1.0 - (1.0 - t).powi(3)
}

fn valid_percent(value: Option<f64>) -> Option<f64> {
    value.filter(|n| n.is_finite() && (0.0..=100.0).contains(n))
}

#[derive(Clone, Copy, Debug, Default)]
struct GaugeTrack {
    from: Option<f64>,
    painted: Option<f64>,
    target: Option<f64>,
}

impl GaugeTrack {
    fn retarget(&mut self, target: Option<f64>, animated: bool) {
        let target = valid_percent(target);
        // A missing sensor must disappear immediately, not animate to zero.
        let from = target.map(|_| self.painted.unwrap_or(0.0));
        self.target = target;
        self.from = from;
        self.painted = if animated { from } else { target };
    }

    fn step(&mut self, progress: f64) {
        self.painted = self.target.map(|end| {
            let start = self.from.unwrap_or(0.0);
            (start + (end - start) * cubic_out(progress)).clamp(0.0, 100.0)
        });
    }

    fn clear(&mut self) {
        *self = Self::default();
    }
}

/// A new real timestamp retargets the painted time boundary without
/// re-generating old paths. In-flight updates begin at the last painted
/// position, including if a worker delivers more than one pulse per frame.
#[derive(Clone, Debug, Default)]
pub(crate) struct ChartMotion {
    cpu: GaugeTrack,
    ram: GaugeTrack,
    from_time: Option<Instant>,
    painted_time: Option<Instant>,
    to_time: Option<Instant>,
    has_sample: bool,
}

impl ChartMotion {
    /// Only call for an accepted strictly increasing real worker pulse.
    pub(crate) fn begin(
        &mut self,
        at: Instant,
        previous_at: Option<Instant>,
        cpu: Option<f64>,
        ram: Option<f64>,
        animated: bool,
    ) -> u64 {
        let duration = if self.has_sample {
            CHART_UPDATE_MS
        } else {
            CHART_INITIAL_MS
        };
        self.has_sample = true;
        self.cpu.retarget(cpu, animated);
        self.ram.retarget(ram, animated);
        let from = self.painted_time.or(previous_at).unwrap_or(at).min(at);
        self.from_time = Some(from);
        self.to_time = Some(at);
        self.painted_time = Some(if animated { from } else { at });
        duration
    }

    pub(crate) fn step(&mut self, progress: f64) {
        self.cpu.step(progress);
        self.ram.step(progress);
        if let (Some(from), Some(to)) = (self.from_time, self.to_time) {
            let elapsed = to.saturating_duration_since(from);
            self.painted_time = Some(from + elapsed.mul_f64(cubic_out(progress)));
        }
    }

    pub(crate) fn gauges(&self) -> (Option<f64>, Option<f64>) {
        (self.cpu.painted, self.ram.painted)
    }

    /// 0..1 visible fraction of the *real measured timestamp range*.
    /// No extra points are appended or interpolated into the series.
    pub(crate) fn trace_extent(&self, first: Instant, last: Instant) -> f64 {
        if last <= first {
            return 1.0;
        }
        self.painted_time.map_or(1.0, |painted| {
            (painted.saturating_duration_since(first).as_secs_f64()
                / last.saturating_duration_since(first).as_secs_f64())
            .clamp(0.0, 1.0)
        })
    }

    /// A freshness timeout invalidates just the gauge presentation, not
    /// historical measured samples. The history can still be shown for 60s.
    pub(crate) fn expire_gauges(&mut self) {
        self.cpu.clear();
        self.ram.clear();
        self.step(1.0);
    }

    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn initial_update_and_interrupt_keep_painted_value() {
        let t = Instant::now();
        let mut animation = ChartMotion::default();
        assert_eq!(animation.begin(t, None, Some(80.0), None, true), CHART_INITIAL_MS);
        assert_eq!(animation.gauges(), (Some(0.0), None));
        animation.step(0.5);
        assert_eq!(animation.gauges().0, Some(70.0));
        assert_eq!(
            animation.begin(t + Duration::from_millis(50), Some(t), Some(20.0), None, true),
            CHART_UPDATE_MS
        );
        assert_eq!(animation.gauges().0, Some(70.0));
        animation.step(0.5);
        assert_eq!(animation.gauges().0, Some(26.25));
        animation.step(1.0);
        assert_eq!(animation.gauges().0, Some(20.0));
    }

    #[test]
    fn missing_invalid_and_reduced_motion_settle_without_fake_zero() {
        let t = Instant::now();
        let mut animation = ChartMotion::default();
        animation.begin(t, None, Some(50.0), Some(55.0), false);
        assert_eq!(animation.gauges(), (Some(50.0), Some(55.0)));
        animation.begin(t + Duration::from_secs(1), Some(t), None, Some(101.0), true);
        assert_eq!(animation.gauges(), (None, None));
        animation.begin(t + Duration::from_secs(2), None, Some(60.0), Some(f64::NAN), false);
        assert_eq!(animation.gauges(), (Some(60.0), None));
        animation.expire_gauges();
        assert_eq!(animation.gauges(), (None, None));
        animation.reset();
        assert_eq!(animation.begin(t, None, Some(10.0), None, true), CHART_INITIAL_MS);
    }

    #[test]
    fn curve_has_no_overshoot_on_interrupt() {
        let t = Instant::now();
        let mut animation = ChartMotion::default();
        animation.begin(t, None, Some(100.0), None, true);
        for step in 0..=100 {
            animation.step(f64::from(step) / 100.0);
            assert!((0.0..=100.0).contains(&animation.gauges().0.unwrap()));
        }
        animation.begin(t + Duration::from_secs(1), Some(t), Some(0.0), None, true);
        for step in 0..=100 {
            animation.step(f64::from(step) / 100.0);
            assert!((0.0..=100.0).contains(&animation.gauges().0.unwrap()));
        }
    }

    #[test]
    fn recorded_time_cursor_retargets_from_current_frame_not_old_endpoint() {
        let t = Instant::now();
        let mut animation = ChartMotion::default();
        animation.begin(t, None, Some(10.0), None, false);
        let next = t + Duration::from_millis(500);
        animation.begin(next, Some(t), Some(20.0), None, true);
        animation.step(0.5);
        assert_eq!(animation.painted_time, Some(t + Duration::from_micros(437_500)));
        let third = t + Duration::from_millis(1000);
        animation.begin(third, Some(next), Some(30.0), None, true);
        assert_eq!(animation.painted_time, Some(t + Duration::from_millis(437)));
        animation.step(1.0);
        assert_eq!(animation.painted_time, Some(third));
    }
}
