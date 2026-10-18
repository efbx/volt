//! An HTTP client on wreq with browser emulation and a middleware chain for bearer logins, rate
//! gates, retries, redirects and browser cookie solving.
//!
//! [`Client::builder`] returns a [`ClientBuilder`] that configures the wreq transport and builds a
//! [`Client`], and [`Client::derive`] returns a [`ClientDerive`] for a child client that shares the
//! connection pool and cookies while changing middleware or policies. Each request is a
//! [`RequestBuilder`] with async and `_sync` senders. The [`middleware`] chain sees every request
//! and response, [`retry`] and [`redirect`] hold the policies the client applies between sends, and
//! every failure is one [`Error`]. The `chrome` feature adds the `Browser` middleware, which solves
//! challenge pages in Chrome and copies the cookies into the client.

#![warn(missing_docs)]
#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a JWT `exp` given as a float is read with `as`, dropping its fraction and saturating negatives to the epoch"
)]

mod blocking;
mod client;
mod client_builder;
mod client_derive;
mod error;
pub mod headers;
pub mod middleware;
mod outcome;
pub mod redirect;
mod request_builder;
pub mod retry;
#[cfg(test)]
mod tests;

pub use client::Client;
pub use client_builder::ClientBuilder;
pub use client_derive::ClientDerive;
pub use error::{Error, Result};
pub use request_builder::RequestBuilder;
pub use wreq::{
    Body, Group, IntoEmulation, IntoUri, Method, Proxy, Request, Response, StatusCode, Uri,
    Version, multipart,
};
pub use wreq_util::Emulation;
