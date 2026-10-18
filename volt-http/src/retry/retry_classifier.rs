use wreq::StatusCode;

use super::RetryContext;

/// A function that decides from a [`RetryContext`] whether a send is worth retrying; the
/// [`RetryPolicy`](super::RetryPolicy) still applies its attempt limit and waits.
pub type RetryClassifier = fn(&RetryContext<'_>) -> bool;

/// The built-in retry classifiers, for [`RetryPolicy::classifier`](super::RetryPolicy::classifier)
/// or for combining in a classifier of your own.
#[derive(Debug)]
pub struct Classifier;

impl Classifier {
    /// Whether the send failed in transport, was rate limited or got a server error; the default
    /// classifier.
    #[must_use]
    pub fn transient(context: &RetryContext<'_>) -> bool {
        Self::transport(context) || Self::rate_limited(context) || Self::server_errors(context)
    }

    /// Whether the send failed with a wreq error while building the request, resolving DNS,
    /// connecting directly or through a proxy, during TLS, on a reset connection, on a timeout or
    /// while reading the body. A middleware error that is not a wreq error never counts.
    #[must_use]
    pub fn transport(context: &RetryContext<'_>) -> bool {
        context
            .error
            .and_then(|error| error.downcast_ref::<wreq::Error>())
            .is_some_and(|error| {
                error.is_request()
                    || error.is_connect()
                    || error.is_proxy_connect()
                    || error.is_connection_reset()
                    || error.is_dns()
                    || error.is_timeout()
                    || error.is_body()
                    || error.is_tls()
            })
    }

    /// Whether the response status is 429 Too Many Requests.
    #[must_use]
    pub fn rate_limited(context: &RetryContext<'_>) -> bool {
        context
            .response
            .is_some_and(|response| response.status() == StatusCode::TOO_MANY_REQUESTS)
    }

    /// Whether the response status is 5xx.
    #[must_use]
    pub fn server_errors(context: &RetryContext<'_>) -> bool {
        context
            .response
            .is_some_and(|response| response.status().is_server_error())
    }
}
