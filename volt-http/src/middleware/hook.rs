use std::error::Error;

use wreq::{Request, Response};

/// The result of a middleware hook. The client reports a failure as
/// [`Error::Middleware`](crate::Error::Middleware).
pub type MiddlewareResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// A hook into every attempt a [`Client`](crate::Client) makes, retries, redirects and replays
/// included. Every method has a pass-through default, so an implementation overrides only the
/// stages it needs.
#[async_trait::async_trait]
pub trait Middleware: Send + Sync + 'static {
    /// Runs before the request is sent, in registration order, and may edit it. Returning a
    /// response or an error skips the transport and the remaining `before` hooks, and that becomes
    /// the result. The default does nothing.
    async fn before(&self, _request: &mut Request) -> MiddlewareResult<Option<Response>> {
        Ok(None)
    }

    /// Runs on the result of every attempt, in reverse registration order, and returns the result
    /// the next hook or the caller sees. The default passes it through.
    async fn after(&self, result: MiddlewareResult<Response>) -> MiddlewareResult<Response> {
        result
    }

    /// Runs on each response, not on errors, before redirects and retries are considered; every
    /// middleware is asked, and the request is replayed without a delay when any returns `true`.
    /// Replays are capped by the client's `max_recoveries`, and the default returns `false`.
    async fn recover(&self, _request: &Request, _response: &Response) -> MiddlewareResult<bool> {
        Ok(false)
    }
}
