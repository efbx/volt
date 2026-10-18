//! Text templates with typed tokens, compiled once and filled in a single Aho-Corasick pass.
//!
//! [`Templater`] holds the program's fixed set of [`Token`]s, each a key paired with an
//! [`Extractor`] that reads its value from the input. [`Templater::compile`] splits a
//! user-written template into text and tokens once, and the resulting [`CompiledTemplate`]
//! fills them for each input; text that is not a token is copied through unchanged.

#![warn(missing_docs)]

mod compiled;
mod template;
mod token;

pub use compiled::CompiledTemplate;
pub use template::Templater;
pub use token::{Extractor, Token};
