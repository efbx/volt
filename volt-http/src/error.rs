use std::error;
use std::result;

/// The error of a request: a transport failure, a middleware failure or a refused redirect.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The request could not be built or sent, or its 4xx or 5xx status was turned into an error by
    /// [`RequestBuilder::send_for_status`](crate::RequestBuilder::send_for_status).
    #[error(transparent)]
    Transport(#[from] wreq::Error),
    /// A middleware failed with an error of its own.
    #[error("middleware: {0}")]
    Middleware(Box<dyn error::Error + Send + Sync>),
    /// The redirect chain went past the redirect limit.
    #[error("redirect: {0}")]
    Redirect(Box<dyn error::Error + Send + Sync>),
}

/// The result of a request, failing with [`Error`].
pub type Result<T> = result::Result<T, Error>;

impl From<Box<dyn error::Error + Send + Sync>> for Error {
    fn from(error: Box<dyn error::Error + Send + Sync>) -> Self {
        match error.downcast::<wreq::Error>() {
            Ok(transport) => Self::Transport(*transport),
            Err(other) => Self::Middleware(other),
        }
    }
}
