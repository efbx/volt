use std::error::Error;

use wreq::{Request, Response};

/// What a [`RetryClassifier`](super::RetryClassifier) sees about one finished send.
#[derive(Debug)]
pub struct RetryContext<'a> {
    /// The number of the send that just finished, from 1; redirect hops and middleware replays do
    /// not advance it.
    pub attempt: u32,
    /// The request as it stood before the middleware chain ran.
    pub request: &'a Request,
    /// The response after the middleware chain, or `None` when the send failed.
    pub response: Option<&'a Response>,
    /// The error when the send failed, or `None` when a response came back; transport failures
    /// downcast to `wreq::Error`.
    pub error: Option<&'a (dyn Error + Send + Sync + 'static)>,
}
