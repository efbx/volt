use std::fmt::{self, Debug};
use std::sync::Arc;

use wreq::cookie::Jar;

use super::Client;
use super::middleware::Middleware;
use super::redirect::RedirectPolicy;
use super::retry::RetryPolicy;

/// A builder for a child [`Client`] that shares its parent's connection pool, cookie jar and
/// middleware instances while changing the middleware chain or policies.
pub struct ClientDerive {
    inner: wreq::Client,
    jar: Option<Arc<Jar>>,
    middleware: Vec<Arc<dyn Middleware>>,
    retry_policy: RetryPolicy,
    redirect_policy: RedirectPolicy,
    max_recoveries: u32,
}

impl Debug for ClientDerive {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClientDerive")
            .field("cookies", &self.jar.is_some())
            .field("middleware", &self.middleware.len())
            .field("retry_policy", &self.retry_policy)
            .field("redirect_policy", &self.redirect_policy)
            .field("max_recoveries", &self.max_recoveries)
            .finish_non_exhaustive()
    }
}

impl ClientDerive {
    /// Starts from `client`'s transport, cookie jar, middleware chain and policies.
    pub(super) fn from_client(client: &Client) -> Self {
        Self {
            inner: client.inner.clone(),
            jar: client.jar.clone(),
            middleware: client.middleware.to_vec(),
            retry_policy: client.retry_policy,
            redirect_policy: client.redirect_policy,
            max_recoveries: client.max_recoveries,
        }
    }

    /// Appends `middleware` after the inherited chain.
    #[must_use]
    pub fn middleware(mut self, middleware: impl Middleware) -> Self {
        self.middleware.push(Arc::new(middleware));
        self
    }

    /// Removes every middleware, inherited or added so far.
    #[must_use]
    pub fn clear_middleware(mut self) -> Self {
        self.middleware.clear();
        self
    }

    /// Replaces the inherited retry policy.
    #[must_use]
    pub fn retry(mut self, retry_policy: RetryPolicy) -> Self {
        self.retry_policy = retry_policy;
        self
    }

    /// Replaces the inherited redirect policy.
    #[must_use]
    pub fn redirect(mut self, redirect_policy: RedirectPolicy) -> Self {
        self.redirect_policy = redirect_policy;
        self
    }

    /// Replaces the inherited limit on middleware replays per request.
    #[must_use]
    pub fn max_recoveries(mut self, max_recoveries: u32) -> Self {
        self.max_recoveries = max_recoveries;
        self
    }

    /// Builds the child [`Client`]; the parent is unchanged.
    #[must_use]
    pub fn build(self) -> Client {
        Client::from_parts(
            self.inner,
            self.jar,
            self.middleware,
            self.retry_policy,
            self.redirect_policy,
            self.max_recoveries,
        )
    }
}
