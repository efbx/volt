//! A Chrome session over chromiumoxide that launches without announcing itself as
//! automated.
//!
//! [`Session`] starts the browser from [`Options`], drives its event loop on the tokio
//! runtime, opens pages and shuts it down; [`Session::run`] does all of that around one
//! job. Failures come back as [`Error`], and chromiumoxide's [`Browser`], [`Page`] and
//! [`Cookie`] are re-exported so callers need not depend on it directly.

#![warn(missing_docs)]

mod error;
mod options;
mod profile;
mod session;

pub use chromiumoxide::Page;
pub use chromiumoxide::browser::Browser;
pub use chromiumoxide::cdp::browser_protocol::network::Cookie;
pub use error::Error;
pub use options::{DEFAULT_TIMEOUT, Options};
pub use session::Session;
