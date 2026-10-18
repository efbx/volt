use std::time::Duration;

use super::{Classifier, RetryClassifier, RetryContext};
use crate::headers;

/// How many times a request is resent and how long to wait between sends, set on
/// [`ClientBuilder::retry`](crate::ClientBuilder::retry) or per request with
/// [`RequestBuilder::retry`](crate::RequestBuilder::retry).
#[derive(Debug, Clone, Copy)]
pub struct RetryPolicy {
    max_retries: u32,
    initial_wait: Duration,
    max_wait: Duration,
    classifier: RetryClassifier,
}

impl RetryPolicy {
    /// Returns the default policy: up to 5 retries, waits from 500 milliseconds up to 2 minutes,
    /// and [`Classifier::transient`].
    #[must_use]
    pub fn new() -> Self {
        Self {
            max_retries: 5,
            initial_wait: Duration::from_millis(500),
            max_wait: Duration::from_secs(120),
            classifier: Classifier::transient,
        }
    }

    /// Returns a policy that never retries.
    #[must_use]
    pub fn none() -> Self {
        Self::new().max_retries(0)
    }

    /// Sets the most retries after the first send; 0 never retries.
    #[must_use]
    pub fn max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }

    /// Sets the backoff before the first retry; each later retry doubles it.
    #[must_use]
    pub fn initial_wait(mut self, initial_wait: Duration) -> Self {
        self.initial_wait = initial_wait;
        self
    }

    /// Sets the cap on the backoff. A `Retry-After` longer than this is not waited for: the
    /// response is returned instead.
    #[must_use]
    pub fn max_wait(mut self, max_wait: Duration) -> Self {
        self.max_wait = max_wait;
        self
    }

    /// Sets the function that decides which sends are worth retrying.
    #[must_use]
    pub fn classifier(mut self, classifier: RetryClassifier) -> Self {
        self.classifier = classifier;
        self
    }

    /// Returns how long to wait before resending, or `None` once `max_retries` are spent, when the
    /// classifier declines, or when the `Retry-After` wait exceeds `max_wait`. A `Retry-After`
    /// within `max_wait` is used as is, otherwise the backoff applies.
    pub(crate) fn delay(&self, context: &RetryContext<'_>) -> Option<Duration> {
        if context.attempt > self.max_retries || !(self.classifier)(context) {
            return None;
        }
        let retry_after = context
            .response
            .and_then(|response| headers::retry_after(response.headers()));
        match retry_after {
            Some(wait) if wait > self.max_wait => None,
            Some(wait) => Some(wait),
            None => Some(self.backoff_delay(context.attempt)),
        }
    }

    /// Returns `initial_wait` doubled for each earlier retry and capped at `max_wait`, jittered to
    /// between half and the full value.
    fn backoff_delay(&self, attempt: u32) -> Duration {
        let exponential = self
            .initial_wait
            .saturating_mul(2u32.saturating_pow(attempt.saturating_sub(1)))
            .min(self.max_wait);
        let half = exponential / 2;
        half + rand::random_range(Duration::ZERO..=half)
    }
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self::new()
    }
}
