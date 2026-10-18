//! Integer timestamp newtypes, one per unit, that convert into each other and parse the
//! common wire formats.
//!
//! [`Nano`], [`Micro`], [`Milli`] and [`Seconds`] each wrap an `i64` and convert into
//! one another with `From`, truncating toward coarser units and saturating toward finer
//! ones. The [`Timestamp`] trait turns any of them into a UTC time, formats it and
//! parses RFC 3339 or a custom chrono format; [`DISPLAY_FORMAT`] is a day-first display
//! format. Each serialises as the plain integer and reads leniently inside a
//! `#[coerce]` struct.

#![warn(missing_docs)]

mod micro;
mod milli;
mod nano;
mod seconds;
mod time;

pub use micro::Micro;
pub use milli::Milli;
pub use nano::Nano;
pub use seconds::Seconds;
pub use time::{DISPLAY_FORMAT, Timestamp};
