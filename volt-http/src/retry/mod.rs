//! The retry policy a [`Client`](crate::Client) applies between sends: a [`RetryPolicy`] with an
//! attempt limit and backoff waits, and a [`RetryClassifier`] that decides from a [`RetryContext`]
//! which failures are worth retrying.

mod retry_classifier;
mod retry_context;
mod retry_policy;

pub use retry_classifier::{Classifier, RetryClassifier};
pub use retry_context::RetryContext;
pub use retry_policy::RetryPolicy;
