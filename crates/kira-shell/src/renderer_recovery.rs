//! Bounded Top Bar WebKit renderer recovery policy, independent of GTK.
//! Retrying immediately in the termination signal risks a crash/reload loop.

use std::time::{Duration, Instant};

const MAX_CONSECUTIVE_CRASHES: u8 = 5;
const RESET_AFTER: Duration = Duration::from_secs(60);
const FIRST_RETRY_MS: u64 = 250;

#[derive(Debug, PartialEq, Eq)]
pub enum RetryDecision {
    After(Duration),
    AlreadyPending,
    Exhausted,
}

#[derive(Default)]
pub struct RendererRecovery {
    crashes: u8,
    last_crash: Option<Instant>,
    pending: bool,
}

impl RendererRecovery {
    /// Retry at most five times (250ms, 500ms, 1s, 2s, 4s).
    /// A process that stayed alive for 60 seconds gets a fresh retry budget.
    pub fn on_terminated(&mut self, now: Instant) -> RetryDecision {
        if self.pending {
            return RetryDecision::AlreadyPending;
        }
        if self
            .last_crash
            .is_none_or(|last| now.saturating_duration_since(last) >= RESET_AFTER)
        {
            self.crashes = 0;
        }
        self.last_crash = Some(now);
        if self.crashes == MAX_CONSECUTIVE_CRASHES {
            return RetryDecision::Exhausted;
        }
        let delay = Duration::from_millis(FIRST_RETRY_MS << self.crashes);
        self.crashes += 1;
        self.pending = true;
        RetryDecision::After(delay)
    }

    /// Called when the GTK timer fires, immediately before a new load_html().
    pub fn retry_started(&mut self) {
        self.pending = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retries_are_bounded_and_exponential() {
        let mut recovery = RendererRecovery::default();
        let start = Instant::now();
        for (index, millis) in [250, 500, 1000, 2000, 4000].into_iter().enumerate() {
            let now = start + Duration::from_secs(index as u64);
            assert_eq!(
                recovery.on_terminated(now),
                RetryDecision::After(Duration::from_millis(millis))
            );
            assert_eq!(
                recovery.on_terminated(now),
                RetryDecision::AlreadyPending,
                "Only one restart timer is allowed per crash"
            );
            recovery.retry_started();
        }
        assert_eq!(
            recovery.on_terminated(start + Duration::from_secs(5)),
            RetryDecision::Exhausted
        );
    }

    #[test]
    fn a_stable_renderer_restores_retry_budget() {
        let mut recovery = RendererRecovery::default();
        let start = Instant::now();
        assert_eq!(
            recovery.on_terminated(start),
            RetryDecision::After(Duration::from_millis(250))
        );
        recovery.retry_started();
        assert_eq!(
            recovery.on_terminated(start + Duration::from_secs(1)),
            RetryDecision::After(Duration::from_millis(500))
        );
        recovery.retry_started();
        assert_eq!(
            recovery.on_terminated(start + Duration::from_secs(61)),
            RetryDecision::After(Duration::from_millis(250))
        );
    }

    #[test]
    fn retry_budget_does_not_reset_during_crash_loop() {
        let mut recovery = RendererRecovery::default();
        let start = Instant::now();
        for second in 0..5 {
            assert!(matches!(
                recovery.on_terminated(start + Duration::from_secs(second)),
                RetryDecision::After(_)
            ));
            recovery.retry_started();
        }
        assert_eq!(
            recovery.on_terminated(start + Duration::from_secs(59)),
            RetryDecision::Exhausted
        );
    }
}
