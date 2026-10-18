use std::borrow::Cow;
use std::fmt::{self, Debug};

/// The function that reads a token's value from the input, borrowing it when it is already
/// there as text.
pub type Extractor<T> = fn(&T) -> Cow<'_, str>;

/// A key that may appear in a template, paired with the function that fills it.
pub struct Token<T> {
    /// The exact text that marks the token in a template, such as `{name}`.
    pub key: Cow<'static, str>,
    /// Returns the text the key is replaced with.
    pub extractor: Extractor<T>,
    /// Help text describing the token to the user, if any.
    pub description: Option<Cow<'static, str>>,
}

impl<T> Debug for Token<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Token")
            .field("key", &self.key)
            .field("description", &self.description)
            .finish_non_exhaustive()
    }
}

impl<T> Token<T> {
    /// Returns a token for `key` filled by `extractor`, without a description.
    pub fn new(key: impl Into<Cow<'static, str>>, extractor: Extractor<T>) -> Self {
        Self {
            key: key.into(),
            extractor,
            description: None,
        }
    }

    /// Returns a token for `key` filled by `extractor`, with `description` as its help
    /// text.
    pub fn with_description(
        key: impl Into<Cow<'static, str>>,
        extractor: Extractor<T>,
        description: impl Into<Cow<'static, str>>,
    ) -> Self {
        Self {
            key: key.into(),
            extractor,
            description: Some(description.into()),
        }
    }
}
