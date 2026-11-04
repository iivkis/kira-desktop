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
    /// Unique session-bus sender. Not forwarded to the WebView.
    pub owner: String,
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
    history: VecDeque<Notice>,
    next_id: u32,
}

impl NotificationQueue {
    pub fn contains_active(&self, id: u32) -> bool {
        self.active_owner(id).is_some()
    }

    /// An arbitrary D-Bus application must not overwrite or close another
    /// application's active notification by guessing the integer ID.
    pub fn active_owner(&self, id: u32) -> Option<&str> {
        self.waiting.iter().map(|entry| &entry.notice)
            .chain(self.showing.iter().map(|entry| &entry.notice))
            .find(|notice| notice.id == id)
            .map(|notice| notice.owner.as_str())
    }

    pub fn allocate_id(&mut self) -> u32 {
        loop {
            self.next_id = self.next_id.wrapping_add(1).max(1);
            if !self.history.iter().any(|n| n.id == self.next_id) {
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
            // Reject the newest event when full. Evicting the oldest waiting
            // entry would silently violate FIFO and lose an acknowledged notice.
            if self.waiting.len() + self.showing.len() >= MAX_STORED {
                return false;
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
        let before = self.waiting.len() + self.showing.len();
        self.waiting.retain(|item| item.notice.id != id);
        self.showing.retain(|item| item.notice.id != id);
        // Session history is a separate, bounded audit of past notifications.
        // Closing a toast must not erase it from history or make its ID active
        // again for Freedesktop's replaces_id semantics.
        before != self.waiting.len() + self.showing.len()
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

        let before = self.showing.len();
        self.showing.retain(|entry| {
            entry.notice.persistent
                || entry.since.is_none_or(|since| {
                    now.saturating_duration_since(since) < entry.notice.dwell.max(MIN_DWELL)
                })
        });
        changed |= before != self.showing.len();
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

    /// Exact next wakeup, never a permanent UI poll.
    pub fn next_deadline(&self, now: Instant) -> Option<Instant> {
        let expiry = self
            .showing
            .iter()
            .filter_map(|item| {
                if item.notice.persistent {
                    None
                } else {
                    item.since
                        .map(|since| since + item.notice.dwell.max(MIN_DWELL))
                }
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
            owner: ":1.101".into(),
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
    fn timeout_zero_is_persistent_and_replacement_requires_active_id() {
        let t = Instant::now();
        let mut q = NotificationQueue::default();
        let mut n = notice(1);
        n.persistent = true;
        assert!(q.push(n, t));
        q.step(t, true);
        q.mark_painted(t, &[1]);
        assert_eq!(q.next_deadline(t), None);
        q.step(t + Duration::from_secs(500), true);
        assert!(q.contains_active(1));
        assert!(q.close(1));
        assert!(!q.contains_active(1));
        assert!(q.history().any(|item| item.id == 1));
        assert!(!q.close(1));
    }

    #[test]
    fn d_bus_sender_ownership_is_active_only() {
        let t = Instant::now();
        let mut q = NotificationQueue::default();
        q.push(notice(1), t);
        assert_eq!(q.active_owner(1), Some(":1.101"));
        assert_ne!(q.active_owner(1), Some(":1.999"));
        assert!(q.close(1));
        assert_eq!(q.active_owner(1), None);
        assert!(q.history().any(|item| item.id == 1));
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
