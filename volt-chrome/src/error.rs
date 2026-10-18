use std::time::Duration;

use chromiumoxide::error::CdpError;

/// A failure to configure, launch or drive the browser.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The launch configuration could not be built, for example when no Chrome binary
    /// is found.
    #[error("browser config: {0}")]
    Config(String),
    /// Launching the browser or a `DevTools` protocol call failed.
    #[error("{0}")]
    Cdp(#[from] CdpError),
    /// The job given to [`Session::run`](crate::Session::run) did not finish within the
    /// timeout it carries.
    #[error("operation timed out after {0:?}")]
    Timeout(Duration),
}
