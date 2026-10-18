//! Lenient JSON decoding on top of serde: a field that arrives as junk, the wrong type or
//! `null` falls to its default instead of failing the whole document.
//!
//! [`coerce`] on a struct derives `Deserialize` with every field read through the [`Coerce`]
//! trait, which decides for each JSON shape whether a value reads or counts as junk.
//! [`Reader`] runs those rules from any serde deserializer, [`Json`] decodes a JSON document
//! stored inside a string, and [`AfterOutcome`] is what a `#[coerce(after = …)]` hook returns.

#![warn(missing_docs)]
#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "coercion turns a number of one type into another with `as`, which rounds, drops fractions and saturates by design"
)]

extern crate self as volt_coerce;

mod after;
mod boxed;
mod capacity;
mod float;
mod integer;
mod json;
mod list;
mod map;
mod option;
mod reader;
mod rule;
#[cfg(test)]
mod tests;
mod text;
mod truth;

pub use after::AfterOutcome;
pub use json::Json;
pub use reader::Reader;
pub use rule::Coerce;
#[doc(hidden)]
pub use serde_json;
pub use volt_coerce_macros::SerdeAttrs as __SerdeAttrs;
pub use volt_coerce_macros::coerce;
