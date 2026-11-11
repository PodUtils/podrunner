//! The clock of podrunner. Each deadline of the core reads a [`Clock`], so
//! that a test with a [`ManualClock`] waits no real time.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// A monotonic clock, in milliseconds from an origin.
pub trait Clock: Send + Sync {
    fn now_ms(&self) -> u64;

    fn now(&self) -> Duration {
        Duration::from_millis(self.now_ms())
    }
}

/// The monotonic clock of the system, from its creation.
#[derive(Debug, Clone)]
pub struct SystemClock(Instant);

impl Default for SystemClock {
    fn default() -> Self {
        SystemClock(Instant::now())
    }
}

impl Clock for SystemClock {
    fn now_ms(&self) -> u64 {
        u64::try_from(self.0.elapsed().as_millis()).unwrap_or(u64::MAX)
    }
}

/// A clock that moves only when a test moves it. Clones share the time.
#[derive(Debug, Clone, Default)]
pub struct ManualClock(Arc<AtomicU64>);

impl ManualClock {
    pub fn advance(&self, by: Duration) {
        let ms = u64::try_from(by.as_millis()).unwrap_or(u64::MAX);
        self.0.fetch_add(ms, Ordering::SeqCst);
    }

    pub fn set_ms(&self, ms: u64) {
        self.0.store(ms, Ordering::SeqCst);
    }
}

impl Clock for ManualClock {
    fn now_ms(&self) -> u64 {
        self.0.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_manual_clock_moves_only_when_moved_and_clones_share_it() {
        let c = ManualClock::default();
        let d = c.clone();
        assert_eq!(c.now_ms(), 0);
        c.advance(Duration::from_secs(3));
        assert_eq!(d.now(), Duration::from_secs(3));
        d.set_ms(10);
        assert_eq!(c.now_ms(), 10);
    }

    #[test]
    fn the_system_clock_does_not_go_back() {
        let c = SystemClock::default();
        let a = c.now_ms();
        assert!(c.now_ms() >= a);
    }
}
