use super::RedirectContext;

/// A function that decides from a [`RedirectContext`] whether to follow a redirect; declining
/// returns the 3xx response to the caller.
pub type RedirectClassifier = fn(&RedirectContext<'_>) -> bool;

/// The built-in redirect classifiers, for
/// [`RedirectPolicy::classifier`](super::RedirectPolicy::classifier).
#[derive(Debug)]
pub struct Classifier;

impl Classifier {
    /// Follows every redirect; the default.
    #[must_use]
    pub fn follow(_context: &RedirectContext<'_>) -> bool {
        true
    }

    /// Follows no redirect, so the 3xx response is returned.
    #[must_use]
    pub fn stop(_context: &RedirectContext<'_>) -> bool {
        false
    }

    /// Follows a redirect only when its host is the original request's host; scheme and port are
    /// not compared.
    #[must_use]
    pub fn same_host(context: &RedirectContext<'_>) -> bool {
        context
            .history
            .first()
            .is_some_and(|origin| origin.host() == context.destination.host())
    }
}
