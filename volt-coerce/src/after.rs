use std::convert::Infallible;
use std::fmt::Display;

/// The return value of a `#[coerce(after = …)]` hook: `()` always succeeds, and the `Err` of a
/// `Result<(), E>` fails the document with the error's text.
pub trait AfterOutcome {
    /// The error a rejecting hook reports, turned into a serde error through its `Display` text.
    type Error: Display;

    /// Returns `Ok` when the hook accepted the decoded value, or the error that rejects it.
    fn finish(self) -> Result<(), Self::Error>;
}

impl AfterOutcome for () {
    type Error = Infallible;

    fn finish(self) -> Result<(), Infallible> {
        Ok(())
    }
}

impl<E: Display> AfterOutcome for Result<(), E> {
    type Error = E;

    fn finish(self) -> Self {
        self
    }
}
