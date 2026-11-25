//! Chrome as an engine of [`volt_browser`], over chromiumoxide and launched without announcing
//! itself as automated.
//!
//! [`Chrome`] launches the browser and returns a [`volt_browser::Session`], whose pages take
//! every selector, action and storage command of that crate. Every failure is a
//! [`volt_browser::Error`]; what only Chrome can fail with, as a launch that finds no binary or
//! a `DevTools` protocol call chromiumoxide gives up on, is its
//! [`Engine`](volt_browser::Error::Engine) kind, with the reason as its message.

#![cfg_attr(doctest, doc = include_str!("../README.md"))]
#![warn(missing_docs)]

mod chrome;
mod error;
mod options;
mod profile;
mod session;
mod tab;

pub use chrome::Chrome;
