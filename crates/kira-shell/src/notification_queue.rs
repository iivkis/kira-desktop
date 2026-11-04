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
    since: Instant,
}

#[derive(Default)]
pub struct NotificationQueue {
    waiting: VecDeque<Waiting>,
    showing: Vec<Showing>, // first element is the bottom card
    history: VecDeque<Notice>,
    last_admission: Option<Instant>,
    next_id: u32,
}

impl NotificationQueue {
    pub fn allocate_id(&mut self) -> u32 {
        loop {
            self.next_id = self.next_id.wrapping_add(1).max(1);
            if !self.history.iter().any(|n| n.id == self.next_id) {
                return self.next_id;
            }
        }
    }

    /// Preserve the FIFO position and the original visible_since on replace.
    pub fn push(&mut self, notice: Notice, now: Instant) {
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
            // The caller receives a D-Bus ID even if UI storage is at capacity.
            // Older queued items have priority; evict historical records first.
            if self.waiting.len() + self.showing.len() >= MAX_STORED {
                self.waiting.pop_front();
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
    }

    pub fn close(&mut self, id: u32) -> bool {
        let before = self.waiting.len() + self.showing.len() + self.history.len();
        self.waiting.retain(|item| item.notice.id != id);
        self.showing.retain(|item| item.notice.id != id);
        self.history.retain(|item| item.id != id);
        before != self.waiting.len() + self.showing.len() + self.history.len()
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
                self.last_admission = None;
                changed = true;
            }
            return changed;
        }

        let before = self.showing.len();
        self.showing.retain(|entry| {
            now.saturating_duration_since(entry.since) < entry.notice.dwell.max(MIN_DWELL)
        });
        changed |= before != self.showing.len();
        if self.showing.len() < MAX_VISIBLE {
            if let Some(head) = self.waiting.front() {
                let interval_ok = self.showing.is_empty()
                    || self
                        .last_admission
                        .is_none_or(|time| now.saturating_duration_since(time) >= ADMISSION_GAP);
                if now >= head.eligible_at && interval_ok {
                    let entry = self.waiting.pop_front().expect("FIFO head");
                    self.showing.push(Showing {
                        notice: entry.notice,
                        since: now,
                    });
                    self.last_admission = Some(now);
                    changed = true;
                }
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
            .map(|item| item.since + item.notice.dwell.max(MIN_DWELL))
            .min();
        let admission = if self.showing.len() < MAX_VISIBLE {
            self.waiting.front().map(|entry| {
                let interval = if self.showing.is_empty() {
                    now
                } else {
                    self.last_admission
                        .map(|at| at + ADMISSION_GAP)
                        .unwrap_or(now)
                };
                entry.eligible_at.max(interval)
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
            dwell: DEFAULT_DWELL,
        }
    }

    #[test]
    fn fifo_three_cards_and_one_point_five_second_spacing() {
        let t = Instant::now();
        let mut q = NotificationQueue::default();
        q.push(notice(1), t);
        assert!(q.step(t, true));
        q.push(notice(2), t);
        q.push(notice(3), t);
        q.push(notice(4), t);
        assert!(!q.step(t + Duration::from_millis(1499), true));
        q.step(t + ADMISSION_GAP, true);
        assert_eq!(q.visible().map(|n| n.id).collect::<Vec<_>>(), vec![1, 2]);
        q.step(t + ADMISSION_GAP * 2, true);
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
        q.push(notice(2), t);
        assert!(q.step(t + Duration::from_millis(2999), false));
        assert_eq!(q.visible_count(), 0);
        q.step(t + Duration::from_secs(4), true);
        assert_eq!(q.visible().next().unwrap().id, 1);
        q.step(t + Duration::from_secs(6), true);
        assert_eq!(q.visible().next().unwrap().id, 1);
        q.step(t + Duration::from_secs(7), true);
        assert!(q.visible().all(|n| n.id != 1));
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
