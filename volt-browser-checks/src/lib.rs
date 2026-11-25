//! The checks every engine of [`volt_browser`] has to pass: each command run for real against
//! pages of a small site on this machine. An engine's crate calls [`Checks::run`] with a session
//! of its browser from its own tests, so that all engines are held to the same behaviour.

#![cfg_attr(doctest, doc = include_str!("../README.md"))]
#![warn(missing_docs)]

mod checks;
mod elements;
mod site;
mod state;

pub use checks::{Checks, Failure};
