use std::error::Error;

use super::{Classifier, RedirectClassifier, RedirectContext};

/// How many redirects a request follows and which ones, set on
/// [`ClientBuilder::redirect`](crate::ClientBuilder::redirect) or per request with
/// [`RequestBuilder::redirect`](crate::RequestBuilder::redirect).
#[derive(Debug, Clone, Copy)]
pub struct RedirectPolicy {
    max_redirects: usize,
    classifier: RedirectClassifier,
}

impl RedirectPolicy {
    /// Returns the default policy: follow every redirect, up to 15.
    #[must_use]
    pub fn new() -> Self {
        Self {
            max_redirects: 15,
            classifier: Classifier::follow,
        }
    }

    /// Returns a policy that follows no redirect, so the 3xx response is returned.
    pub fn none() -> Self {
        Self::new().classifier(Classifier::stop)
    }

    /// Sets the most redirects followed; one more fails the request with
    /// [`Error::Redirect`](crate::Error::Redirect).
    #[must_use]
    pub fn max_redirects(mut self, max_redirects: usize) -> Self {
        self.max_redirects = max_redirects;
        self
    }

    /// Sets the function that decides whether each redirect is followed.
    #[must_use]
    pub fn classifier(mut self, classifier: RedirectClassifier) -> Self {
        self.classifier = classifier;
        self
    }

    /// Whether to follow the redirect in `context`, failing once `context.hop` passes the redirect
    /// limit.
    pub(crate) fn follow(
        &self,
        context: &RedirectContext<'_>,
    ) -> Result<bool, Box<dyn Error + Send + Sync>> {
        if context.hop > self.max_redirects {
            return Err("too many redirects".into());
        }
        Ok((self.classifier)(context))
    }
}

impl Default for RedirectPolicy {
    fn default() -> Self {
        Self::new()
    }
}
