use std::error;
use std::fmt;
use std::time::Duration;

/// The engine's own error behind a failure, kept as that failure's source.
pub type Cause = Box<dyn error::Error + Send + Sync>;

/// A failure to find, read or act on something in a page, or to reach the page at all.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Nothing in the page matched the selector within the timeout.
    #[error("no element matches {0}")]
    NotFound(String),
    /// A command for one element matched this many; narrow the selector with `first`, `last` or
    /// `nth`.
    #[error("{1} elements match {0}; narrow it with first, last or nth")]
    Ambiguous(String, usize),
    /// The element stayed in a state the command cannot act on until the timeout.
    #[error("the element matching {0} is {1}")]
    NotActionable(String, Blocker),
    /// The page rejected the selector, with its reason.
    #[error("the selector {0} is not valid: {1}")]
    InvalidSelector(String, String),
    /// The element is not a frame of the same site, or has no open shadow root to enter.
    #[error("the element matching {0} cannot be entered: {1}")]
    NoSuchFrame(String, String),
    /// The command does not apply to this kind of element, as filling a link would.
    #[error("the command does not fit the element matching {0}: {1}")]
    WrongElement(String, String),
    /// A script threw, a promise was rejected, or a value could not be turned into JSON.
    #[error("script: {0}")]
    Script(String),
    /// The page gave no answer: it left its document while a script or a promise was awaited,
    /// or it has replaced the JSON functions an answer travels through.
    #[error("the page gave the script no answer")]
    Detached,
    /// A command, or the job given to [`Session::run`](crate::Session::run), did not finish
    /// within the timeout it carries.
    #[error("operation timed out after {0:?}")]
    Timeout(Duration),
    /// The URL is not an absolute URL.
    #[error("the URL is not an absolute URL")]
    InvalidUrl,
    /// A script's value is not the JSON the command expects.
    #[error("unexpected script value: {0}")]
    Json(#[from] serde_json::Error),
    /// The engine cannot do this, as showing a browser that runs without a window.
    #[error("{0} is not supported by this engine")]
    Unsupported(&'static str),
    /// A [`Session`](crate::Session) was created outside a Tokio runtime, whose timers every
    /// waiting command uses.
    #[error("volt-browser needs a Tokio runtime with timers")]
    NoRuntime,
    /// The page or its session is closed.
    #[error("the page or its session is closed")]
    Closed,
    /// The engine failed, for its own reason.
    #[error("{0}")]
    Engine(#[source] Cause),
}

impl Error {
    /// Whether waiting longer cannot help: every failure but one of the engine, which a
    /// navigation under way also causes.
    pub(crate) fn is_final(&self) -> bool {
        !matches!(self, Self::Engine(_))
    }
}

/// What keeps an element from being acted on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blocker {
    /// It has no size, or is hidden by its style.
    Hidden,
    /// It keeps changing its place or size.
    Moving,
    /// It is disabled, or inside something that is.
    Disabled,
    /// It cannot be edited.
    ReadOnly,
    /// Another element lies over the point the command acts on.
    Covered,
}

impl fmt::Display for Blocker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Hidden => "hidden",
            Self::Moving => "moving",
            Self::Disabled => "disabled",
            Self::ReadOnly => "read-only",
            Self::Covered => "covered by another element",
        })
    }
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::time::Duration;

    use super::{Blocker, Error};

    #[test]
    fn only_an_engine_failure_is_worth_waiting_out() {
        let engine = Error::Engine(Box::new(io::Error::other("context destroyed")));
        assert!(!engine.is_final());
        assert!(Error::Closed.is_final());
        assert!(Error::Timeout(Duration::from_secs(1)).is_final());
        assert!(Error::Unsupported("showing").is_final());
    }

    #[test]
    fn messages_name_the_selector_and_the_reason() {
        assert_eq!(
            Error::NotActionable("css \"button\"".to_owned(), Blocker::Covered).to_string(),
            "the element matching css \"button\" is covered by another element"
        );
        assert_eq!(
            Error::Ambiguous("tag \"li\"".to_owned(), 3).to_string(),
            "3 elements match tag \"li\"; narrow it with first, last or nth"
        );
    }
}
