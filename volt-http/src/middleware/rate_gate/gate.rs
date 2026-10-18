use std::fmt::{self, Debug};
use std::sync::atomic::Ordering;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use super::state::State;
use crate::headers;
use crate::middleware::{Middleware, MiddlewareResult};
use crate::{Request, Response, StatusCode};

/// Middleware that spaces requests apart and closes after a `429` for the response's
/// `Retry-After`. Repeated trips widen the spacing until a quiet spell resets it, and clones
/// share one gate, so every client holding a clone is paced together.
#[derive(Clone)]
pub struct RateGate {
    gap: Duration,
    jitter: Duration,
    first_gap: Duration,
    max_gap: Duration,
    calm_after: Duration,
    fallback_wait: Duration,
    max_wait: Duration,
    wake_jitter: Duration,
    state: Arc<State>,
}

impl Debug for RateGate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RateGate")
            .field("gap", &self.gap)
            .field("jitter", &self.jitter)
            .field("first_gap", &self.first_gap)
            .field("max_gap", &self.max_gap)
            .field("calm_after", &self.calm_after)
            .field("fallback_wait", &self.fallback_wait)
            .field("max_wait", &self.max_wait)
            .field("wake_jitter", &self.wake_jitter)
            .finish_non_exhaustive()
    }
}

impl RateGate {
    /// Returns a gate with no fixed gap or jitter, which only slows down after a trip. Trips pace
    /// at 250 ms, doubling up to 2 s, and calm down after 30 s; a `429` without `Retry-After`
    /// closes it for 15 s, no `429` closes it for over 120 s, and wakes get up to 250 ms jitter.
    #[must_use]
    pub fn new() -> Self {
        Self {
            gap: Duration::ZERO,
            jitter: Duration::ZERO,
            first_gap: Duration::from_millis(250),
            max_gap: Duration::from_secs(2),
            calm_after: Duration::from_secs(30),
            fallback_wait: Duration::from_secs(15),
            max_wait: Duration::from_secs(120),
            wake_jitter: Duration::from_millis(250),
            state: Arc::default(),
        }
    }

    /// Sets the minimum spacing between requests, applied whether or not the gate has tripped.
    /// Defaults to zero.
    #[must_use]
    pub fn gap(mut self, gap: Duration) -> Self {
        self.gap = gap;
        self
    }

    /// Sets the most random time added to the gap for each request. Defaults to zero.
    #[must_use]
    pub fn jitter(mut self, jitter: Duration) -> Self {
        self.jitter = jitter;
        self
    }

    /// Sets the spacing the first trip opens, `first_gap`, and the cap that later trips double it
    /// up to, `max_gap`. Defaults to 250 ms and 2 s.
    #[must_use]
    pub fn pacing(mut self, first_gap: Duration, max_gap: Duration) -> Self {
        self.first_gap = first_gap;
        self.max_gap = max_gap;
        self
    }

    /// Sets how long without a trip before the trip spacing resets to zero. Defaults to 30 s.
    #[must_use]
    pub fn calm_after(mut self, calm_after: Duration) -> Self {
        self.calm_after = calm_after;
        self
    }

    /// Sets how long a `429` without a readable `Retry-After` closes the gate. Defaults to 15 s.
    #[must_use]
    pub fn fallback_wait(mut self, wait: Duration) -> Self {
        self.fallback_wait = wait;
        self
    }

    /// Sets the longest a `429` closes the gate, whatever its `Retry-After` says. Defaults to
    /// 120 s.
    #[must_use]
    pub fn max_wait(mut self, wait: Duration) -> Self {
        self.max_wait = wait;
        self
    }

    /// Sets the most random time added each time a waiting caller sleeps, so waiters do not all
    /// wake at once. Defaults to 250 ms.
    #[must_use]
    pub fn wake_jitter(mut self, jitter: Duration) -> Self {
        self.wake_jitter = jitter;
        self
    }

    /// Returns the spacing trips currently add, zero before the first trip and after a calm spell.
    #[must_use]
    pub fn spacing(&self) -> Duration {
        Duration::from_millis(self.trip_spacing())
    }

    /// Closes the gate for `wait` from now, unless it is already closed for longer, and widens the
    /// trip spacing: the first gap on the first trip, then doubling up to the cap.
    pub fn trip(&self, wait: Duration) {
        let now = Self::now_ms();
        let state = &self.state;
        state
            .closed_until
            .fetch_max(now + Self::millis(wait), Ordering::Relaxed);
        state.last_trip.store(now, Ordering::Relaxed);

        let widened = match state.trip_gap.load(Ordering::Relaxed) {
            0 => Self::millis(self.first_gap),
            current => current.saturating_mul(2),
        };
        state
            .trip_gap
            .fetch_max(widened.min(Self::millis(self.max_gap)), Ordering::Relaxed);
    }

    /// Waits until the gate is open and, while it is spacing requests, until one interval after
    /// the previous caller. A closure extended by a later trip is honoured while waiting.
    pub async fn pass(&self) {
        let mut release = self.reserve();
        loop {
            release = release.max(self.state.closed_until.load(Ordering::Relaxed));
            let now = Self::now_ms();
            if now >= release {
                return;
            }
            let wait = Duration::from_millis(release - now)
                + rand::random_range(Duration::ZERO..=self.wake_jitter);
            tokio::time::sleep(wait).await;
        }
    }

    /// Claims the next release time, no earlier than now or the reopening, and moves the following
    /// one an interval later. With no spacing it returns the reopening without claiming a slot.
    fn reserve(&self) -> u64 {
        let spacing = self.interval();
        let state = &self.state;
        if spacing == 0 {
            return state.closed_until.load(Ordering::Relaxed);
        }
        let mut observed = state.next_release.load(Ordering::Relaxed);
        loop {
            let release = observed
                .max(Self::now_ms())
                .max(state.closed_until.load(Ordering::Relaxed));
            match state.next_release.compare_exchange_weak(
                observed,
                release + spacing,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => return release,
                Err(current) => observed = current,
            }
        }
    }

    /// Returns this caller's spacing in milliseconds: the gap plus random jitter, or the trip
    /// spacing when that is wider.
    fn interval(&self) -> u64 {
        let fixed = Self::millis(self.gap + rand::random_range(Duration::ZERO..=self.jitter));
        fixed.max(self.trip_spacing())
    }

    /// Returns the trip spacing in milliseconds, resetting it to zero once `calm_after` has passed
    /// since the last trip.
    fn trip_spacing(&self) -> u64 {
        let state = &self.state;
        let gap = state.trip_gap.load(Ordering::Relaxed);
        if gap == 0 {
            return 0;
        }
        let quiet = Self::now_ms().saturating_sub(state.last_trip.load(Ordering::Relaxed));
        if quiet >= Self::millis(self.calm_after) {
            state.trip_gap.store(0, Ordering::Relaxed);
            return 0;
        }
        gap
    }

    /// Returns `duration` in whole milliseconds.
    fn millis(duration: Duration) -> u64 {
        u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
    }

    /// Returns the milliseconds since the first call in the process, on a monotonic clock.
    fn now_ms() -> u64 {
        /// The instant every gate measures time from.
        static ANCHOR: OnceLock<Instant> = OnceLock::new();
        Self::millis(ANCHOR.get_or_init(Instant::now).elapsed())
    }
}

impl Default for RateGate {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Middleware for RateGate {
    async fn before(&self, _request: &mut Request) -> MiddlewareResult<Option<Response>> {
        self.pass().await;
        Ok(None)
    }

    async fn after(&self, result: MiddlewareResult<Response>) -> MiddlewareResult<Response> {
        if let Ok(response) = &result
            && response.status() == StatusCode::TOO_MANY_REQUESTS
        {
            let wait = headers::retry_after(response.headers())
                .unwrap_or(self.fallback_wait)
                .min(self.max_wait);
            self.trip(wait);
        }
        result
    }
}
