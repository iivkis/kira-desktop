//! Bounded, deterministic FIFO admission and toast lifetime policy.
//! These rules are independent from GTK, D-Bus, Niri and wall-clock changes.
use std::collections::VecDeque;
use std::time::{Duration, Instant};

pub const MAX_VISIBLE: usize = 3;
pub const MAX_STORED: usize = 100;
pub const ADMISSION_GAP: Duration = Duration::from_millis(1500);
pub const MIN_DWELL: Duration = Duration::from_millis(3000);
pub const DEFAULT_DWELL: Duration = Duration::from_millis(5000);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    pub id: u32,
    pub app: String,
    pub summary: String,
    pub body: String,
    pub actions: Vec<(String, String)>,
    pub critical: bool,
    /// D-Bus timeout=0 and critical hints must never auto-close.
    pub persistent: bool,
    /// Visual dwell, not the D-Bus logical lifetime.
    pub dwell: Duration,
}

#[derive(Clone, Debug)]
struct Waiting {
    notice: Notice,
    eligible_at: Instant,
}

#[derive(Clone, Debug)]
struct Showing {
    notice: Notice,
    /// Starts only after the Svelte host confirms a painted frame.
    since: Option<Instant>,
}

#[derive(Default)]
pub struct NotificationQueue {
    waiting: VecDeque<Waiting>,
    showing: Vec<Showing>, // first element is the bottom card
    // Persistent events can leave the toast without closing their D-Bus ID.
    // CloseNotification or a future replacement can still address them.
    parked: VecDeque<Notice>,
    history: VecDeque<Notice>,
    next_id: u32,
}

impl NotificationQueue {
    pub fn contains_active(&self, id: u32) -> bool {
        self.waiting.iter().any(|item| item.notice.id == id)
            || self.showing.iter().any(|item| item.notice.id == id)
            || self.parked.iter().any(|item| item.id == id)
    }

    pub fn allocate_id(&mut self) -> u32 {
        loop {
            self.next_id = self.next_id.wrapping_add(1).max(1);
            if !self.contains_active(self.next_id)
                && !self.history.iter().any(|n| n.id == self.next_id)
            {
                return self.next_id;
            }
        }
    }

    /// Preserve the FIFO position and the original visible_since on replace.
    /// Returns false on overload, without discarding earlier FIFO entries.
    pub fn push(&mut self, notice: Notice, now: Instant) -> bool {
        if let Some(entry) = self
            .waiting
            .iter_mut()
            .find(|item| item.notice.id == notice.id)
        {
            entry.notice = notice.clone();
        } else if let Some(entry) = self
            .showing
            .iter_mut()
            .find(|item| item.notice.id == notice.id)
        {
            entry.notice = notice.clone();
        } else {
            // Replacing a parked persistent ID creates a fresh toast request
            // at the tail of FIFO, without closing/reallocating that D-Bus ID.
            let parked_position = self.parked.iter().position(|item| item.id == notice.id);
            if parked_position.is_none() && self.active_count() >= MAX_STORED {
                return false;
            }
            if let Some(index) = parked_position {
                self.parked.remove(index);
            }
            self.waiting.push_back(Waiting {
                eligible_at: if self.showing.is_empty() {
                    now
                } else {
                    now + ADMISSION_GAP
                },
                notice: notice.clone(),
            });
        }
        if let Some(item) = self.history.iter_mut().find(|item| item.id == notice.id) {
            *item = notice;
        } else {
            if self.history.len() >= MAX_STORED {
                self.history.pop_front();
            }
            self.history.push_back(notice);
        }
        true
    }

    pub fn close(&mut self, id: u32) -> bool {
        let before = self.active_count();
        self.waiting.retain(|item| item.notice.id != id);
        self.showing.retain(|item| item.notice.id != id);
        self.parked.retain(|item| item.id != id);
        // Session history is a separate, bounded audit of past notifications.
        // Closing a toast must not erase it from history or make its ID active
        // again for Freedesktop's replaces_id semantics.
        before != self.active_count()
    }

    pub fn active_count(&self) -> usize {
        self.waiting.len() + self.showing.len() + self.parked.len()
    }

    /// Only the host may start the visual clock. A covered/unmapped window must
    /// not count as having displayed anything.
    pub fn step(&mut self, now: Instant, can_show: bool) -> bool {
        let mut changed = false;
        if !can_show {
            if !self.showing.is_empty() {
                let items: Vec<_> = self.showing.drain(..).collect();
                for item in items.into_iter().rev() {
                    self.waiting.push_front(Waiting {
                        notice: item.notice,
                        eligible_at: now,
                    });
                }
                changed = true;
            }
            return changed;
        }

        // Separate visual dwell from logical Freedesktop lifetime: a
        // timeout=0 or critical toast still vacates its slot after dwell,
        // but stays logically active until CloseNotification/replace.
        let mut kept = Vec::with_capacity(self.showing.len());
        for entry in self.showing.drain(..) {
            let elapsed = entry.since.is_some_and(|since| {
                now.saturating_duration_since(since) >= entry.notice.dwell.max(MIN_DWELL)
            });
            if elapsed {
                if entry.notice.persistent {
                    self.parked.push_back(entry.notice);
                }
                changed = true;
            } else {
                kept.push(entry);
            }
        }
        self.showing = kept;
        if self.showing.len() < MAX_VISIBLE {
            if let Some(head) = self.waiting.front() {
                // The 1.5s gap begins when the previous card actually paints,
                // not when an invisible GTK/WebKit host admits it.
                let interval_ok = self.showing.last().is_none_or(|item| {
                    item.since
                        .is_some_and(|since| now.saturating_duration_since(since) >= ADMISSION_GAP)
                });
                if now >= head.eligible_at && interval_ok {
                    let entry = self.waiting.pop_front().expect("FIFO head");
                    self.showing.push(Showing {
                        notice: entry.notice,
                        since: None,
                    });
                    changed = true;
                }
            }
        }
        changed
    }

    /// A notification is not considered displayed until the trusted renderer
    /// has applied the snapshot and a frame was presented. A stale ID or a
    /// repeated acknowledgement never resets the minimum dwell time.
    pub fn mark_painted(&mut self, now: Instant, ids: &[u32]) -> bool {
        let mut changed = false;
        for item in &mut self.showing {
            if item.since.is_none() && ids.contains(&item.notice.id) {
                item.since = Some(now);
                changed = true;
            }
        }
        changed
    }

    pub fn visible(&self) -> impl Iterator<Item = &Notice> {
        self.showing.iter().map(|entry| &entry.notice)
    }

    pub fn history(&self) -> impl Iterator<Item = &Notice> {
        self.history.iter()
    }

    pub fn has_pending(&self) -> bool {
        !self.waiting.is_empty()
    }
    pub fn visible_count(&self) -> usize {
        self.showing.len()
    }

    /// True only after the trusted Svelte renderer acknowledges a visual
    /// frame. Prevents exposing an outdated DOM when the host was blank.
    pub fn painted_count(&self) -> usize {
        self.showing
            .iter()
            .filter(|entry| entry.since.is_some())
            .count()
    }

    pub fn has_painted(&self) -> bool {
        self.painted_count() != 0
    }

    /// Exact next wakeup, never a permanent UI poll.
    pub fn next_deadline(&self, now: Instant) -> Option<Instant> {
        let expiry = self
            .showing
            .iter()
            .filter_map(|item| {
                item.since
                    .map(|since| since + item.notice.dwell.max(MIN_DWELL))
            })
            .min();
        let admission = if self.showing.len() < MAX_VISIBLE {
            self.waiting.front().and_then(|entry| {
                // No retry timer until WebKit confirms a frame, otherwise
                // delayed painting could admit all three at once.
                let interval = self.showing.last().map_or(Some(now), |item| {
                    item.since.map(|since| since + ADMISSION_GAP)
                });
                interval.map(|at| entry.eligible_at.max(at))
            })
        } else {
            None
        };
        expiry.into_iter().chain(admission).min()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notice(id: u32) -> Notice {
        Notice {
            id,
            app: "Test".into(),
            summary: format!("{id}"),
            body: String::new(),
            actions: vec![],
            critical: false,
            persistent: false,
            dwell: DEFAULT_DWELL,
        }
    }

    #[test]
    fn fifo_three_cards_and_one_point_five_second_spacing() {
        let t = Instant::now();
        let mut q = NotificationQueue::default();
        q.push(notice(1), t);
        assert!(q.step(t, true));
        assert!(q.mark_painted(t, &[1]));
        q.push(notice(2), t);
        q.push(notice(3), t);
        q.push(notice(4), t);
        assert!(!q.step(t + Duration::from_millis(1499), true));
        q.step(t + ADMISSION_GAP, true);
        q.mark_painted(t + ADMISSION_GAP, &[2]);
        assert_eq!(q.visible().map(|n| n.id).collect::<Vec<_>>(), vec![1, 2]);
        q.step(t + ADMISSION_GAP * 2, true);
        q.mark_painted(t + ADMISSION_GAP * 2, &[3]);
        assert_eq!(q.visible().map(|n| n.id).collect::<Vec<_>>(), vec![1, 2, 3]);
        q.step(t + Duration::from_secs(5), true);
        assert_eq!(q.visible().map(|n| n.id).collect::<Vec<_>>(), vec![2, 3, 4]);
        assert_eq!(q.visible_count(), MAX_VISIBLE);
    }

    #[test]
    fn dwell_is_at_least_three_seconds_and_visual_pause_preserves_fifo() {
        let t = Instant::now();
        let mut q = NotificationQueue::default();
        let mut a = notice(1);
        a.dwell = Duration::from_millis(100);
        q.push(a, t);
        q.step(t, true);
        q.mark_painted(t, &[1]);
        q.push(notice(2), t);
        assert!(q.step(t + Duration::from_millis(2999), false));
        assert_eq!(q.visible_count(), 0);
        q.step(t + Duration::from_secs(4), true);
        q.mark_painted(t + Duration::from_secs(4), &[1]);
        assert_eq!(q.visible().next().unwrap().id, 1);
        q.step(t + Duration::from_secs(6), true);
        assert_eq!(q.visible().next().unwrap().id, 1);
        q.step(t + Duration::from_secs(7), true);
        assert!(q.visible().all(|n| n.id != 1));
    }

    #[test]
    fn input_region_grows_only_after_each_painted_ack() {
        let t = Instant::now();
        let mut q = NotificationQueue::default();
        q.push(notice(1), t);
        q.step(t, true);
        assert_eq!(q.painted_count(), 0);
        q.mark_painted(t, &[1]);
        assert_eq!(q.painted_count(), 1);
        q.push(notice(2), t);
        q.step(t + ADMISSION_GAP, true);
        assert_eq!(q.visible_count(), 2);
        assert_eq!(q.painted_count(), 1);
        q.mark_painted(t + ADMISSION_GAP, &[2]);
        assert_eq!(q.painted_count(), 2);
    }

    #[test]
    fn no_expiry_before_render_ack_even_when_renderer_is_delayed() {
        let t = Instant::now();
        let mut q = NotificationQueue::default();
        q.push(notice(1), t);
        q.step(t, true);
        q.step(t + Duration::from_secs(30), true);
        assert_eq!(q.visible_count(), 1);
        assert!(q.mark_painted(t + Duration::from_secs(30), &[1]));
        assert!(!q.mark_painted(t + Duration::from_secs(32), &[1]));
        q.step(t + Duration::from_secs(34), true);
        assert_eq!(q.visible_count(), 1);
        q.step(t + Duration::from_secs(35), true);
        assert_eq!(q.visible_count(), 0);
    }

    #[test]
    fn incoming_cards_wait_for_previous_real_paint() {
        let t = Instant::now();
        let mut q = NotificationQueue::default();
        q.push(notice(1), t);
        q.step(t, true);
        q.push(notice(2), t);
        assert_eq!(q.next_deadline(t), None);
        assert!(!q.step(t + Duration::from_secs(4), true));
        assert_eq!(q.visible_count(), 1);
        assert!(q.mark_painted(t + Duration::from_secs(4), &[1]));
        assert_eq!(
            q.next_deadline(t + Duration::from_secs(4)),
            Some(t + Duration::from_millis(5500))
        );
        assert!(!q.step(t + Duration::from_millis(5499), true));
        assert!(q.step(t + Duration::from_millis(5500), true));
        assert_eq!(
            q.visible().map(|item| item.id).collect::<Vec<_>>(),
            vec![1, 2]
        );
    }

    #[test]
    fn persistent_toast_rotates_out_but_dbus_id_remains_live() {
        let t = Instant::now();
        let mut q = NotificationQueue::default();
        let mut persistent = notice(1);
        persistent.persistent = true;
        assert!(q.push(persistent, t));
        assert!(q.step(t, true));
        assert!(q.mark_painted(t, &[1]));
        assert_eq!(q.next_deadline(t), Some(t + DEFAULT_DWELL));
        q.push(notice(2), t);
        q.step(t + ADMISSION_GAP, true);
        q.mark_painted(t + ADMISSION_GAP, &[2]);
        q.step(t + DEFAULT_DWELL, true);
        assert_eq!(q.visible().map(|n| n.id).collect::<Vec<_>>(), vec![2]);
        assert!(q.contains_active(1));
        assert_eq!(q.parked.len(), 1);
        q.step(t + DEFAULT_DWELL + ADMISSION_GAP, true);
        assert!(!q.contains_active(2));
        assert!(q.contains_active(1));
        assert!(q.close(1));
        assert!(!q.contains_active(1));
        assert!(q.history().any(|item| item.id == 1));
        assert!(!q.close(1));
    }

    #[test]
    fn three_persistent_toasts_do_not_block_the_next_fifo_event() {
        let t = Instant::now();
        let mut q = NotificationQueue::default();
        for index in 0..3 {
            let mut n = notice(index + 1);
            n.persistent = true;
            q.push(n, t);
            let now = t + ADMISSION_GAP * index;
            q.step(now, true);
            q.mark_painted(now, &[index + 1]);
        }
        q.push(notice(4), t + ADMISSION_GAP * 2);
        q.step(t + DEFAULT_DWELL, true);
        assert_eq!(q.visible().map(|n| n.id).collect::<Vec<_>>(), vec![2, 3, 4]);
        assert!(q.contains_active(1)); // logically active, no toast slot
        assert!(q.contains_active(4));
        assert_eq!(q.parked.len(), 1);
    }

    #[test]
    fn replacement_of_parked_persistent_id_reenters_fifo_with_same_id() {
        let t = Instant::now();
        let mut q = NotificationQueue::default();
        let mut n = notice(77);
        n.persistent = true;
        q.push(n.clone(), t);
        q.step(t, true);
        q.mark_painted(t, &[77]);
        q.step(t + DEFAULT_DWELL, true);
        assert_eq!(q.visible_count(), 0);
        assert!(q.contains_active(77));
        n.summary = "updated".into();
        assert!(q.push(n, t + DEFAULT_DWELL));
        assert_eq!(q.parked.len(), 0);
        q.step(t + DEFAULT_DWELL, true);
        assert_eq!(q.visible().next().unwrap().summary, "updated");
        assert!(q.contains_active(77));
        assert_eq!(q.history().filter(|n| n.id == 77).count(), 1);
    }

    #[test]
    fn allocator_skips_known_ids_even_after_history_overflows() {
        let t = Instant::now();
        let mut q = NotificationQueue::default();
        q.next_id = 41;
        let mut n = notice(42);
        n.persistent = true;
        q.push(n, t);
        q.step(t, true);
        q.mark_painted(t, &[42]);
        q.step(t + DEFAULT_DWELL, true);
        for id in 100..201 {
            q.push(notice(id), t);
            q.close(id);
        }
        assert!(!q.history().any(|n| n.id == 42));
        assert_eq!(q.allocate_id(), 43);
        q.next_id = 41;
        assert_eq!(q.allocate_id(), 43);
    }

    #[test]
    fn overloaded_queue_rejects_newest_without_eviction() {
        let t = Instant::now();
        let mut q = NotificationQueue::default();
        for id in 1..=MAX_STORED as u32 {
            assert!(q.push(notice(id), t));
        }
        assert!(!q.push(notice(101), t));
        q.step(t, true);
        assert_eq!(q.visible().next().unwrap().id, 1);
        assert_eq!(q.history().count(), MAX_STORED);
        assert!(!q.history().any(|n| n.id == 101));
    }

    #[test]
    fn replace_keeps_position_and_close_is_immediate() {
        let t = Instant::now();
        let mut q = NotificationQueue::default();
        q.push(notice(1), t);
        q.step(t, true);
        q.push(notice(2), t);
        let mut replacement = notice(1);
        replacement.summary = "changed".into();
        q.push(replacement, t + Duration::from_secs(1));
        assert_eq!(q.visible().next().unwrap().summary, "changed");
        assert!(q.close(1));
        q.step(t + ADMISSION_GAP, true);
        assert_eq!(q.visible().next().unwrap().id, 2);
    }
}
