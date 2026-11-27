use serde_json::Value;
use volt_browser::Cause;

use crate::Error;

/// What a load that ended on the error page failed for when the page names no error code.
const NO_CODE: &str = "the webview showed its error page";

/// The page `WebView2` shows in place of one it could not load. That load ends as a finished one,
/// so only the page it ended on tells that it failed.
pub(crate) struct ErrorPage;

impl ErrorPage {
    /// The expression that is `null` on every other page and, on the error page, the error code
    /// the page names as JSON text, empty when it names none.
    pub(crate) const PROBE: &str = "location.protocol===\"chrome-error:\"?JSON.stringify((((document.documentElement||{}).innerText||\"\").match(/ERR_[A-Z0-9_]+/)||[\"\"])[0]):null";

    /// Reads what [`ErrorPage::PROBE`] gave after a load: nothing is wrong on any other page,
    /// and on the error page the load failed, for the error code the page names when it names
    /// one.
    pub(crate) fn check(shown: Value) -> Result<(), Error> {
        match shown {
            Value::Null => Ok(()),
            Value::String(code) if !code.is_empty() => Err(Error::LoadFailed(Cause::from(code))),
            _ => Err(Error::LoadFailed(Cause::from(NO_CODE))),
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::ErrorPage;
    use crate::Error;

    #[test]
    fn a_load_that_ended_on_another_page_is_fine() {
        assert!(ErrorPage::check(Value::Null).is_ok());
    }

    #[test]
    fn a_load_that_ended_on_the_error_page_failed_for_the_code_it_names() {
        assert!(matches!(
            ErrorPage::check(json!("ERR_CONNECTION_REFUSED")),
            Err(Error::LoadFailed(code)) if code.to_string() == "ERR_CONNECTION_REFUSED"
        ));
    }

    #[test]
    fn an_error_page_that_names_no_code_still_fails_the_load() {
        assert!(matches!(
            ErrorPage::check(json!("")),
            Err(Error::LoadFailed(reason)) if reason.to_string() == "the webview showed its error page"
        ));
    }
}
