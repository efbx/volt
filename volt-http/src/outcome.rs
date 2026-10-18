use std::time::Duration;

use super::{StatusCode, Uri};

/// What [`Client::execute`](crate::Client::execute) does after one send.
pub(super) enum Outcome {
    /// Send the same request again because a middleware recovered.
    Replay,
    /// Follow a redirect.
    Redirect {
        /// The redirect status, which decides whether the method and body are kept.
        status: StatusCode,
        /// The resolved URI to request next.
        destination: Uri,
    },
    /// Wait the given delay, then send the request again.
    Retry(Duration),
    /// Hand the result to the caller.
    Return,
}
