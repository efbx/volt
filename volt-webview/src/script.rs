use serde_json::Value;

use crate::Error;

/// The JavaScript a page evaluates for a caller's expression: a call that hands the expression's
/// value to the page's `JSON.stringify` and gives back the text only when it is a string, so that
/// a webview only ever hands back a string or `null`, whatever the page did to its own objects.
/// `WKWebView` would otherwise hand wry a value JSON cannot hold, such as a `Date` or `NaN`, which
/// wry's conversion throws on inside the program.
pub(crate) struct Script;

impl Script {
    /// The text before the expression: the function that keeps only string text, taking the
    /// expression's value as its argument so that its own names never hide the page's.
    pub(crate) const OPEN: &str =
        "(function(v){var s=JSON.stringify(v);return typeof s===\"string\"?s:null})((\n";
    /// The text after the expression; its line break first ends a trailing `//` comment.
    pub(crate) const CLOSE: &str = "\n))";

    /// Returns `expression`, without its trailing white space and one trailing `;`, wrapped so
    /// that the page hands back its value as JSON text.
    pub(crate) fn wrap(expression: &str) -> String {
        let expression = Self::bare(expression);
        let mut script =
            String::with_capacity(Self::OPEN.len() + expression.len() + Self::CLOSE.len());
        script.push_str(Self::OPEN);
        script.push_str(expression);
        script.push_str(Self::CLOSE);
        script
    }

    /// Reads what the webview handed back for a wrapped expression: nothing or `null` for
    /// `undefined`, a function, a value JSON cannot hold and an expression that throws, all of
    /// which give [`Value::Null`], otherwise the value's JSON text as a JSON string.
    pub(crate) fn result(json: &str) -> Result<Value, Error> {
        if json.is_empty() {
            return Ok(Value::Null);
        }
        let text: Option<String> = serde_json::from_str(json)?;
        text.map_or(Ok(Value::Null), |text| Ok(serde_json::from_str(&text)?))
    }

    /// Returns `expression` without its trailing white space and one trailing `;`, which would
    /// end the expression inside the call.
    fn bare(expression: &str) -> &str {
        let expression = expression.trim_end();
        expression.strip_suffix(';').unwrap_or(expression)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::Script;
    use crate::Error;

    #[test]
    fn an_expression_is_wrapped_on_lines_of_its_own() {
        assert_eq!(
            Script::wrap("document.title // the title"),
            "(function(v){var s=JSON.stringify(v);return typeof s===\"string\"?s:null})((\ndocument.title // the title\n))"
        );
    }

    #[test]
    fn one_trailing_semicolon_and_trailing_white_space_are_left_out() {
        assert_eq!(
            Script::wrap("document.title; \n"),
            Script::wrap("document.title")
        );
        assert!(Script::wrap("a;;").contains("\na;\n"));
    }

    #[test]
    fn nothing_and_null_are_null() {
        assert_eq!(Script::result("").unwrap(), Value::Null);
        assert_eq!(Script::result("null").unwrap(), Value::Null);
    }

    #[test]
    fn the_json_text_inside_the_string_is_the_value() {
        assert_eq!(Script::result(r#""2""#).unwrap(), 2);
        assert_eq!(
            Script::result(r#""{\"title\":\"volt\"}""#).unwrap(),
            json!({"title": "volt"})
        );
        assert_eq!(Script::result(r#""true""#).unwrap(), true);
    }

    #[test]
    fn a_result_that_is_not_json_text_fails() {
        assert!(matches!(Script::result("2"), Err(Error::Json(_))));
        assert!(matches!(Script::result(r#""{""#), Err(Error::Json(_))));
        assert!(matches!(Script::result("{"), Err(Error::Json(_))));
    }
}
