use std::borrow::Cow;
use std::time::Duration;

use volt_browser::Cause;

/// A failure to reach, open or drive a page in the system webview, as the crate words it inside.
/// A caller gets the [`volt_browser::Error`] it turns into: the kind of the same name for
/// `NoRuntime`, `InvalidUrl`, `Closed`, `Json` and `Timeout`, and
/// [`Engine`](volt_browser::Error::Engine), holding this failure, for the rest.
#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    /// The system webview cannot run in this process, for the reason given: on macOS no event
    /// loop serves the main thread and the call is not awaited there, or the call blocks the main
    /// thread from inside its event loop; on Linux and the BSDs there is no display, GTK refused
    /// to start, GTK belongs to a thread that does not serve the crate, or the crate was built
    /// without its `webkitgtk` feature; on Windows the session's window thread could not start;
    /// every other target has no system webview.
    #[error("the system webview cannot run here: {0}")]
    Unavailable(Cow<'static, str>),
    /// [`Webview::launch`](crate::Webview::launch) was awaited outside a Tokio runtime, whose
    /// timers every call of the session uses.
    #[error("volt-webview needs a Tokio runtime with timers")]
    NoRuntime,
    /// The webview or its window failed: it could not be built, refused a call, or could not read
    /// cookies.
    #[error("webview: {0}")]
    Webview(#[source] Cause),
    /// The webview cannot parse the URL. Only the webviews of macOS and Windows say so before a
    /// load, so the kind exists there; `WebKitGTK` fails such a load instead.
    #[cfg(any(test, target_os = "macos", target_os = "windows"))]
    #[error("the webview cannot load this URL")]
    InvalidUrl,
    /// The load failed. `WebKitGTK` reports failed loads, and `WebView2` shows its error page
    /// for one, whose error code is then the reason, so most of these happen on Linux, the BSDs
    /// and Windows; a webview that opens `about:blank` in place of an address it refuses fails
    /// the load on any platform.
    #[error("the load failed: {0}")]
    LoadFailed(#[source] Cause),
    /// The page's web content process ended, by a crash or for lack of memory, while a load was
    /// awaited; reported on macOS, Linux and the BSDs.
    #[error("the page's web content process ended")]
    Crashed,
    /// A later [`goto`](volt_browser::Page::goto) on the same page started before this one's load
    /// ended.
    #[error("a later goto on the same page replaced this one")]
    Replaced,
    /// The page or its session is closed, or the webview went away without answering.
    #[error("the page or its session is closed")]
    Closed,
    /// A script's value is not the JSON the call expects.
    #[error("unexpected script value: {0}")]
    Json(#[from] serde_json::Error),
    /// A call did not finish within the timeout it carries.
    #[error("operation timed out after {0:?}")]
    Timeout(Duration),
}

impl From<Error> for volt_browser::Error {
    fn from(error: Error) -> Self {
        match error {
            Error::NoRuntime => Self::NoRuntime,
            #[cfg(any(test, target_os = "macos", target_os = "windows"))]
            Error::InvalidUrl => Self::InvalidUrl,
            Error::Closed => Self::Closed,
            Error::Json(error) => Self::Json(error),
            Error::Timeout(timeout) => Self::Timeout(timeout),
            error @ (Error::Unavailable(_)
            | Error::Webview(_)
            | Error::LoadFailed(_)
            | Error::Crashed
            | Error::Replaced) => Self::Engine(Box::new(error)),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;
    use std::time::Duration;

    use super::Error;

    #[test]
    fn a_webview_that_cannot_run_says_why_inside_the_engine_error() {
        let unavailable = Error::Unavailable(Cow::Borrowed("no display"));
        assert!(matches!(
            volt_browser::Error::from(unavailable),
            volt_browser::Error::Engine(cause)
                if cause.to_string() == "the system webview cannot run here: no display"
        ));
    }

    #[test]
    fn failures_the_shared_commands_know_keep_their_kind() {
        let timeout = Duration::from_secs(2);
        assert!(matches!(
            volt_browser::Error::from(Error::Timeout(timeout)),
            volt_browser::Error::Timeout(kept) if kept == timeout
        ));
        assert!(matches!(
            volt_browser::Error::from(Error::Closed),
            volt_browser::Error::Closed
        ));
        assert!(matches!(
            volt_browser::Error::from(Error::InvalidUrl),
            volt_browser::Error::InvalidUrl
        ));
        assert!(matches!(
            volt_browser::Error::from(Error::NoRuntime),
            volt_browser::Error::NoRuntime
        ));
    }

    #[test]
    fn the_webviews_own_failures_travel_inside_the_engine_error() {
        let replaced = volt_browser::Error::from(Error::Replaced);
        assert!(matches!(
            &replaced,
            volt_browser::Error::Engine(cause)
                if matches!(cause.downcast_ref(), Some(Error::Replaced))
        ));
        assert_eq!(
            replaced.to_string(),
            "a later goto on the same page replaced this one"
        );
    }
}
