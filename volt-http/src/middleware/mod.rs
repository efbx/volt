//! Hooks that see every request a [`Client`](crate::Client) sends and every result it gets back.
//!
//! [`Middleware`] runs `before` in registration order, `after` in reverse and `recover` to ask
//! for a replay; [`Bearer`], [`RateGate`] and, with the `chrome` feature, `Browser` are the
//! built-in implementations.

mod bearer;
#[cfg(feature = "chrome")]
mod browser;
mod hook;
mod rate_gate;

pub use bearer::{Anonymous, Bearer, Token, TokenSource};
#[cfg(feature = "chrome")]
pub use browser::{Browser, BrowserBuilder, Detect, Detector};
pub use hook::{Middleware, MiddlewareResult};
pub use rate_gate::RateGate;
