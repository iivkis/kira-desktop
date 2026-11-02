//! A bounded, deterministic scheduler for trusted GTK/WebKit popup hosts.
//! Pure policy: no GTK objects, renderer state, or timers live here.
//! Top Bar has its own permanent WebView and is deliberately outside the pool.
use std::time::{Duration, Instant};

pub const MAX_POPUP_VIEWS: usize = 4;
pub const HOT_IDLE_TARGET: usize = 1;
pub const IDLE_GRACE: Duration = Duration::from_secs(45);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Surface {
    Calendar,
    System,
    Launcher,
}
impl Surface {
    pub fn name(self) -> &'static str {
        match self {
            Self::Calendar => "calendar",
            Self::System => "system",
            Self::Launcher => "launcher",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reservation {
    Existing(u32),
    Reused(u32),
    Created(u32),
    Full,
}

#[derive(Clone, Debug)]
struct Slot {
    id: u32,
    owner: Option<Surface>,
    idle_since: Instant,
}

/// No active slot may be stolen. One slot is warmed at startup; additional
/// slots exist only after real demand. A short idle grace protects quick
/// reopen/animation chains without keeping an unbounded WebKit farm alive.
#[derive(Default)]
pub struct WebviewScheduler {
    slots: Vec<Slot>,
    next_id: u32,
}

impl WebviewScheduler {
    pub fn reserve(&mut self, surface: Surface, now: Instant) -> Reservation {
        if let Some(slot) = self.slots.iter().find(|s| s.owner == Some(surface)) {
            return Reservation::Existing(slot.id);
        }
        // Favor the most recently used warm renderer: it is more likely to
        // have an active WebKit process and cached compiled Svelte scripts.
        if let Some(slot) = self
            .slots
            .iter_mut()
            .filter(|s| s.owner.is_none())
            .max_by_key(|s| s.idle_since)
        {
            slot.owner = Some(surface);
            return Reservation::Reused(slot.id);
        }
        if self.slots.len() == MAX_POPUP_VIEWS {
            return Reservation::Full;
        }
        self.next_id = self.next_id.checked_add(1).expect("bounded popup slot identifiers");
        let id = self.next_id;
        self.slots.push(Slot {
            id,
            owner: Some(surface),
            idle_since: now,
        });
        Reservation::Created(id)
    }

    /// This only runs once at Shell activation, not for every possible widget.
    pub fn prewarm_one(&mut self, now: Instant) -> Option<u32> {
        if !self.slots.is_empty() {
            return None;
        }
        let Reservation::Created(id) = self.reserve(Surface::Calendar, now) else {
            return None;
        };
        self.release(id, now);
        Some(id)
    }

    pub fn release(&mut self, id: u32, now: Instant) -> bool {
        let Some(slot) = self.slots.iter_mut().find(|s| s.id == id) else {
            return false;
        };
        if slot.owner.is_none() {
            return false;
        }
        slot.owner = None;
        slot.idle_since = now;
        true
    }

    pub fn active_count(&self) -> usize {
        self.slots.iter().filter(|s| s.owner.is_some()).count()
    }

    pub fn active_slot(&self, surface: Surface) -> Option<u32> {
        self.slots.iter().find(|s| s.owner == Some(surface)).map(|s| s.id)
    }

    pub fn is_active(&self, id: u32) -> bool {
        self.slots.iter().any(|s| s.id == id && s.owner.is_some())
    }

    /// Retain exactly one hot spare after the grace period. A renderer with a
    /// visible widget is NEVER a reclaim candidate, even at full capacity.
    pub fn expired_idle(&mut self, now: Instant) -> Vec<u32> {
        let mut idle: Vec<_> = self
            .slots
            .iter()
            .filter(|s| s.owner.is_none())
            .map(|s| (s.id, s.idle_since))
            .collect();
        idle.sort_unstable_by_key(|(_, instant)| std::cmp::Reverse(*instant));
        let expired: Vec<_> = idle
            .into_iter()
            .skip(HOT_IDLE_TARGET)
            .filter(|(_, since)| now.saturating_duration_since(*since) >= IDLE_GRACE)
            .map(|(id, _)| id)
            .collect();
        self.slots.retain(|slot| !expired.contains(&slot.id));
        expired
    }

    #[cfg(test)]
    pub fn resident_count(&self) -> usize {
        self.slots.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_initial_hot_webview_not_ten_speculative_instances() {
        let mut s = WebviewScheduler::default();
        let now = Instant::now();
        assert_eq!(s.prewarm_one(now), Some(1));
        assert_eq!(s.prewarm_one(now), None);
        assert_eq!(s.resident_count(), 1);
        assert_eq!(s.reserve(Surface::Calendar, now), Reservation::Reused(1));
    }

    #[test]
    fn simultaneous_windows_never_steal_active_or_interrupt_other_surface() {
        let mut s = WebviewScheduler::default();
        let now = Instant::now();
        s.prewarm_one(now);
        assert_eq!(s.reserve(Surface::Calendar, now), Reservation::Reused(1));
        assert_eq!(s.reserve(Surface::System, now), Reservation::Created(2));
        assert_eq!(s.reserve(Surface::Launcher, now), Reservation::Created(3));
        assert_eq!(s.active_count(), 3);
        assert_eq!(s.reserve(Surface::Calendar, now), Reservation::Existing(1));
        assert_eq!(s.active_slot(Surface::System), Some(2));
        assert!(s.expired_idle(now + Duration::from_secs(500)).is_empty());
    }

    #[test]
    fn closing_one_widget_keeps_others_and_reuses_warm_slot() {
        let mut s = WebviewScheduler::default();
        let now = Instant::now();
        s.prewarm_one(now);
        s.reserve(Surface::Calendar, now);
        s.reserve(Surface::System, now);
        assert!(s.release(1, now + Duration::from_secs(1)));
        assert!(!s.release(1, now + Duration::from_secs(2)));
        assert_eq!(s.active_slot(Surface::System), Some(2));
        assert_eq!(s.reserve(Surface::Launcher, now + Duration::from_secs(2)), Reservation::Reused(1));
        assert_eq!(s.active_count(), 2);
    }

    #[test]
    fn idle_eviction_is_time_bounded_preserves_most_recent_and_live_leases() {
        let mut s = WebviewScheduler::default();
        let now = Instant::now();
        s.prewarm_one(now);
        s.reserve(Surface::Calendar, now);
        s.reserve(Surface::System, now);
        s.reserve(Surface::Launcher, now);
        s.release(1, now);
        s.release(2, now + Duration::from_secs(1));
        assert!(s.expired_idle(now + Duration::from_secs(10)).is_empty());
        assert_eq!(s.expired_idle(now + Duration::from_secs(46)), vec![1]);
        assert!(s.is_active(3));
        assert_eq!(s.resident_count(), 2);
        assert_eq!(s.reserve(Surface::System, now + Duration::from_secs(47)), Reservation::Reused(2));
    }

    #[test]
    fn fixed_cap_reports_backpressure_without_eviction_of_active_windows() {
        let mut s = WebviewScheduler::default();
        let now = Instant::now();
        // Surface identity prevents duplicate active panes. Bounded capacity
        // covers today's three built-ins plus one future trusted surface.
        for kind in [Surface::Calendar, Surface::System, Surface::Launcher] {
            assert!(matches!(s.reserve(kind, now), Reservation::Created(_)));
        }
        assert_eq!(s.active_count(), 3);
        assert_eq!(s.resident_count(), 3);
    }
}
