use std::fmt::{self, Debug};

use super::Extractor;

/// One part of a compiled template: literal text or a token to fill.
pub(super) enum Piece<T> {
    /// Text copied into the output as it is.
    Text(String),
    /// A token replaced by what its extractor returns for the input.
    Token(Extractor<T>),
}

/// A template already split into text and tokens, ready to fill for any number of
/// inputs.
pub struct CompiledTemplate<T> {
    pub(super) pieces: Vec<Piece<T>>,
    pub(super) literal_len: usize,
}

impl<T> Debug for CompiledTemplate<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CompiledTemplate")
            .field("pieces", &self.pieces.len())
            .field("literal_len", &self.literal_len)
            .finish()
    }
}

impl<T> CompiledTemplate<T> {
    /// Returns the template with every token replaced by its value for `input`.
    pub fn execute(&self, input: &T) -> String {
        let mut out = String::with_capacity(self.literal_len);
        for piece in &self.pieces {
            match piece {
                Piece::Text(text) => out.push_str(text),
                Piece::Token(extract) => out.push_str(&extract(input)),
            }
        }
        out
    }
}
