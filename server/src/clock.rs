//! The clock is injected into the app state so tests can control time
//! (session expiry now; the 30-day Trash later) instead of sleeping.

use std::sync::{Arc, Mutex};

use time::{Duration, OffsetDateTime};

pub trait Clock: Send + Sync + 'static {
    fn now(&self) -> OffsetDateTime;
}

/// Shared handle to the clock, as stored in the app state.
pub type SharedClock = Arc<dyn Clock>;

/// Wall-clock time; used in production.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> OffsetDateTime {
        OffsetDateTime::now_utc()
    }
}

/// A clock that only moves when told to. Cloning it yields a handle to the
/// same time, so a test can keep one handle and give another to the app.
#[derive(Debug, Clone)]
pub struct ManualClock {
    now: Arc<Mutex<OffsetDateTime>>,
}

impl ManualClock {
    pub fn new(start: OffsetDateTime) -> Self {
        Self {
            now: Arc::new(Mutex::new(start)),
        }
    }

    pub fn set(&self, to: OffsetDateTime) {
        *self.now.lock().unwrap() = to;
    }

    pub fn advance(&self, by: Duration) {
        let mut now = self.now.lock().unwrap();
        *now += by;
    }
}

impl Clock for ManualClock {
    fn now(&self) -> OffsetDateTime {
        *self.now.lock().unwrap()
    }
}
