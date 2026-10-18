//! Blank checks, typographic symbol normalisation, in-place edits and non-blank
//! joins for `str`, `String` and slices of `&str`.
//!
//! [`StrExt`] reads a `str` and returns new values, [`StringExt`] edits a
//! `String` in place without reallocating when it can, and [`StrsExt`] joins a
//! slice of `&str` while skipping the entries a predicate rejects.

#![warn(missing_docs)]

mod normalizer;
mod str_ext;
mod string_ext;
mod strs_ext;

pub use str_ext::StrExt;
pub use string_ext::StringExt;
pub use strs_ext::StrsExt;
