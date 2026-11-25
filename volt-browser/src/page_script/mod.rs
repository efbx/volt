use serde::de::DeserializeOwned;
use serde::ser::Error as _;
use serde::{Deserialize, Serialize};
#[cfg(test)]
use serde_json::Deserializer;
use serde_json::Value;

use crate::element::Path;
use crate::{Blocker, Error, Rect};

/// The helper script every command runs in the page: a function that takes one command, finds
/// its elements, carries it out and hands back one answer. It travels with each call, so a
/// navigation cannot lose it, and leaves nothing in the page but a slot for each promise a
/// command left, until that command takes the promise's value.
///
/// It is kept as one file per topic and joined into one function when the crate is compiled.
/// The topic files hold function declarations and constants only, so their order here is free;
/// the text after them is the one statement that runs, and it turns whatever a command throws
/// into an answer.
const HELPER: &str = concat!(
    "(function (command, code) {\n",
    include_str!("find.js"),
    include_str!("resolve.js"),
    include_str!("check.js"),
    include_str!("pointer.js"),
    include_str!("focus.js"),
    include_str!("edit.js"),
    include_str!("keys.js"),
    include_str!("form.js"),
    include_str!("page.js"),
    include_str!("read.js"),
    include_str!("act.js"),
    include_str!("run.js"),
    "try {\n",
    "  return run();\n",
    "} catch (error) {\n",
    "  return error instanceof Halt ? error.envelope : fail(\"script\", describe(error));\n",
    "}\n",
    "})\n",
);

/// What the helper script is asked to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Op {
    /// Hands over the settled value of a promise an earlier command left pending.
    Take,
    /// Evaluates the caller's expression.
    Evaluate,
    /// Calls the caller's function, with the element first when the command has one.
    Call,
    /// Asks the caller's function whether to stop waiting.
    Until,
    /// The page's URL.
    Url,
    /// The page's title.
    Title,
    /// The page's HTML.
    Source,
    /// The page's user agent.
    UserAgent,
    /// The document's identity, to tell a later document from it.
    Mark,
    /// Whether the page has left the marked document and finished loading the next.
    Moved,
    /// Whether the document has reached a load state.
    Loaded,
    /// Whether the page's URL fits.
    At,
    /// Reloads the page.
    Reload,
    /// Goes one page back in the history.
    Back,
    /// Goes one page forward in the history.
    Forward,
    /// Stops the load under way.
    Stop,
    /// Replaces the document with new HTML.
    Write,
    /// Reads one stored value.
    StorageGet,
    /// Stores one value.
    StorageSet,
    /// Removes one stored value.
    StorageRemove,
    /// Removes every stored value.
    StorageClear,
    /// Reads every stored pair.
    StorageEntries,
    /// The names of the page's caches.
    CacheNames,
    /// The URLs a cache holds answers for.
    CacheUrls,
    /// One cached answer.
    CacheGet,
    /// Removes a whole cache.
    CacheDelete,
    /// Removes one cached answer.
    CacheRemove,
    /// Whether anything matches.
    Exists,
    /// How many elements match.
    Count,
    /// The visible text of every match.
    Texts,
    /// Whether the element is there and visible.
    IsVisible,
    /// Whether the element has reached a state.
    State,
    /// Whether the element's visible text contains some text.
    HasText,
    /// The element's visible text.
    Text,
    /// The element's raw text.
    TextContent,
    /// The element's inner HTML.
    InnerHtml,
    /// The element's outer HTML.
    OuterHtml,
    /// One attribute.
    Attribute,
    /// One property.
    Property,
    /// A form field's value.
    Value,
    /// One computed style.
    Style,
    /// The element's box.
    Rect,
    /// The element's tag name.
    Tag,
    /// Whether the element is enabled.
    IsEnabled,
    /// Whether the element can be edited.
    IsEditable,
    /// Whether a box is checked.
    IsChecked,
    /// Whether an option or a box is selected.
    IsSelected,
    /// Whether the element has the focus.
    IsFocused,
    /// Clicks.
    Click,
    /// Moves the pointer over the element.
    Hover,
    /// Gives the element the focus.
    Focus,
    /// Takes the focus from the element.
    Blur,
    /// Sets the whole value of a field.
    Fill,
    /// Types text key by key.
    TypeText,
    /// Presses one key.
    Press,
    /// Checks or unchecks a box.
    SetChecked,
    /// Chooses options of a `<select>`.
    Select,
    /// Submits the element's form.
    Submit,
    /// Scrolls the element into view.
    ScrollIntoView,
    /// Scrolls the element's content to a place.
    ScrollTo,
    /// Scrolls the element's content by a distance.
    ScrollBy,
    /// Selects the element's text.
    SelectText,
    /// Sends an event to the element.
    Dispatch,
}

/// The caller's own JavaScript that goes with a command.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Code<'a> {
    /// The command needs none.
    None,
    /// A function.
    Function(&'a str),
    /// An expression, evaluated when the command runs.
    Expression(&'a str),
}

/// One command for the helper script.
#[derive(Debug)]
pub(crate) struct Request<'a, A: ?Sized> {
    /// What to do.
    pub(crate) op: Op,
    /// The element to do it to, for a command that has one.
    pub(crate) path: Option<&'a Path>,
    /// What the command needs to know.
    pub(crate) arg: &'a A,
    /// The caller's JavaScript.
    pub(crate) code: Code<'a>,
}

/// A command as the helper script reads it.
#[derive(Serialize)]
struct Command<'a, A: ?Sized> {
    /// What to do.
    op: Op,
    /// The steps to the element.
    path: Option<&'a Path>,
    /// What the command needs to know.
    arg: &'a A,
    /// The box the element had a moment ago, for a command that needs it to hold still.
    rect: Option<Rect>,
    /// The number a pending promise of this command is kept under.
    token: u64,
}

/// The script text of commands, and the values in their answers.
pub(crate) struct Script;

impl Script {
    /// Returns the script that runs `request` in the page: the helper called with the command as
    /// JSON, and with the caller's code where there is some. Values only ever enter as JSON.
    pub(crate) fn build<A: Serialize + ?Sized>(
        request: &Request<'_, A>,
        rect: Option<Rect>,
        token: u64,
    ) -> Result<String, Error> {
        let command = Command {
            op: request.op,
            path: request.path,
            arg: request.arg,
            rect,
            token,
        };
        let mut script = Vec::with_capacity(HELPER.len() + 256);
        script.extend_from_slice(HELPER.as_bytes());
        script.push(b'(');
        serde_json::to_writer(&mut script, &command)?;
        match request.code {
            Code::None => {}
            Code::Function(function) => {
                script.extend_from_slice(b",(\n");
                script.extend_from_slice(function.as_bytes());
                script.extend_from_slice(b"\n)");
            }
            Code::Expression(expression) => {
                script.extend_from_slice(b",function(){return (\n");
                script.extend_from_slice(Self::bare(expression).as_bytes());
                script.extend_from_slice(b"\n)}");
            }
        }
        script.push(b')');
        String::from_utf8(script).map_err(|error| Error::Json(serde_json::Error::custom(error)))
    }

    /// Returns `expression` without its trailing white space and one trailing `;`, which would
    /// end the expression inside the function that returns it.
    fn bare(expression: &str) -> &str {
        let expression = expression.trim_end();
        expression.strip_suffix(';').unwrap_or(expression)
    }

    /// Reads the value an answer carries as JSON text, `null` where it carries none.
    pub(crate) fn value<T: DeserializeOwned>(json: Option<&str>) -> Result<T, Error> {
        Ok(serde_json::from_str(json.unwrap_or("null"))?)
    }

    /// Returns the command inside a built `script`.
    #[cfg(test)]
    pub(crate) fn command(script: &str) -> Value {
        let command = script
            .strip_prefix(HELPER)
            .unwrap()
            .strip_prefix('(')
            .unwrap();
        Deserializer::from_str(command)
            .into_iter()
            .next()
            .unwrap()
            .unwrap()
    }

    /// Returns what follows the command inside a built `script`: the caller's code and the
    /// closing bracket.
    #[cfg(test)]
    pub(crate) fn tail(script: &str) -> &str {
        let command = script
            .strip_prefix(HELPER)
            .unwrap()
            .strip_prefix('(')
            .unwrap();
        let mut values = Deserializer::from_str(command).into_iter::<Value>();
        values.next().unwrap().unwrap();
        &command[values.byte_offset()..]
    }
}

/// What the helper script answered.
#[derive(Debug, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub(crate) enum Answer {
    /// The command is done; its value is JSON text, or absent.
    Ok {
        /// The value.
        json: Option<String>,
    },
    /// The page is not ready for the command yet.
    Wait {
        /// What is in the way.
        blocker: Obstacle,
        /// The element's box now, when the command wants to see it hold still.
        rect: Option<Rect>,
    },
    /// The command left a promise that has not settled yet.
    Pending,
    /// The command failed for good.
    Error {
        /// The kind of failure.
        kind: Fault,
        /// The page's words for it.
        message: String,
        /// How many elements matched, for an ambiguous selector.
        count: usize,
    },
}

impl Answer {
    /// Reads the engine's value as an answer; `None` where the page gave none, as during a
    /// navigation.
    pub(crate) fn read(value: Value) -> Result<Option<Self>, Error> {
        if value.is_null() {
            return Ok(None);
        }
        Ok(Some(serde_json::from_value(value)?))
    }
}

/// What keeps a command waiting.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Obstacle {
    /// Nothing matches yet.
    Missing,
    /// The element is not visible.
    Hidden,
    /// The element is not where it was a moment ago.
    Moving,
    /// The element is disabled.
    Disabled,
    /// The element cannot be edited.
    ReadOnly,
    /// Another element lies over it.
    Covered,
    /// The awaited condition does not hold yet.
    Unmet,
}

impl Obstacle {
    /// The failure a caller is told when the obstacle outlasts the timeout, `None` for the two
    /// that are no state of an element.
    pub(crate) fn blocker(self) -> Option<Blocker> {
        match self {
            Self::Missing | Self::Unmet => None,
            Self::Hidden => Some(Blocker::Hidden),
            Self::Moving => Some(Blocker::Moving),
            Self::Disabled => Some(Blocker::Disabled),
            Self::ReadOnly => Some(Blocker::ReadOnly),
            Self::Covered => Some(Blocker::Covered),
        }
    }
}

/// The kinds of failure the helper script reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Fault {
    /// Several elements match a command for one.
    Ambiguous,
    /// The page rejected the selector.
    Selector,
    /// A frame or shadow root cannot be entered.
    Frame,
    /// The command does not fit the element.
    Wrong,
    /// A script threw or a promise was rejected.
    Script,
    /// The document a promise was pending in is gone.
    Detached,
}

impl Fault {
    /// Returns the error for a failure of this kind on the element `path` leads to.
    pub(crate) fn error(self, path: Option<&Path>, message: String, count: usize) -> Error {
        match self {
            Self::Ambiguous => Error::Ambiguous(Path::label(path), count),
            Self::Selector => Error::InvalidSelector(Path::label(path), message),
            Self::Frame => Error::NoSuchFrame(Path::label(path), message),
            Self::Wrong => Error::WrongElement(Path::label(path), message),
            Self::Script => Error::Script(message),
            Self::Detached => Error::Detached,
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::{Answer, Code, Fault, Obstacle, Op, Request, Script};
    use crate::{Blocker, Error, Rect};

    #[test]
    fn a_command_enters_the_script_as_json_after_the_helper() {
        let request = Request {
            op: Op::StorageGet,
            path: None,
            arg: "it's \"quoted\"\u{2028}",
            code: Code::None,
        };
        let script = Script::build(&request, None, 7).unwrap();
        assert_eq!(
            Script::command(&script),
            json!({"op": "storage_get", "path": null, "arg": "it's \"quoted\"\u{2028}", "rect": null, "token": 7})
        );
        assert_eq!(Script::tail(&script), ")");
    }

    #[test]
    fn the_callers_code_follows_the_command_on_lines_of_its_own() {
        let function = Request {
            op: Op::Call,
            path: None,
            arg: &[1, 2],
            code: Code::Function("function (pair) { return pair[0] + pair[1]; } // sum"),
        };
        assert_eq!(
            Script::tail(&Script::build(&function, None, 0).unwrap()),
            ",(\nfunction (pair) { return pair[0] + pair[1]; } // sum\n))"
        );
        let expression = Request {
            op: Op::Evaluate,
            path: None,
            arg: &(),
            code: Code::Expression("document.title // the title"),
        };
        assert_eq!(
            Script::tail(&Script::build(&expression, None, 0).unwrap()),
            ",function(){return (\ndocument.title // the title\n)})"
        );
    }

    #[test]
    fn one_trailing_semicolon_and_trailing_white_space_are_left_out_of_an_expression() {
        let expression = Request {
            op: Op::Evaluate,
            path: None,
            arg: &(),
            code: Code::Expression("document.title; \n"),
        };
        assert_eq!(
            Script::tail(&Script::build(&expression, None, 0).unwrap()),
            ",function(){return (\ndocument.title\n)})"
        );
        assert_eq!(Script::bare("a;;"), "a;");
    }

    #[test]
    fn the_box_to_hold_still_travels_with_the_command() {
        let request = Request {
            op: Op::Click,
            path: None,
            arg: &(),
            code: Code::None,
        };
        let rect = Rect {
            x: 1.0,
            y: 2.0,
            width: 30.0,
            height: 4.5,
        };
        let script = Script::build(&request, Some(rect), 0).unwrap();
        assert_eq!(
            Script::command(&script)["rect"],
            json!({"x": 1.0, "y": 2.0, "width": 30.0, "height": 4.5})
        );
    }

    #[test]
    fn each_answer_reads_as_its_kind() {
        let done = Answer::read(json!({"status": "ok", "json": "[1,2]"})).unwrap();
        assert!(matches!(done, Some(Answer::Ok { json: Some(json) }) if json == "[1,2]"));
        let empty = Answer::read(json!({"status": "ok", "json": null})).unwrap();
        assert!(matches!(empty, Some(Answer::Ok { json: None })));
        let waiting =
            Answer::read(json!({"status": "wait", "blocker": "read_only", "rect": null})).unwrap();
        assert!(matches!(
            waiting,
            Some(Answer::Wait {
                blocker: Obstacle::ReadOnly,
                rect: None
            })
        ));
        let pending = Answer::read(json!({"status": "pending"})).unwrap();
        assert!(matches!(pending, Some(Answer::Pending)));
        let failed = Answer::read(
            json!({"status": "error", "kind": "ambiguous", "message": "", "count": 3}),
        )
        .unwrap();
        assert!(matches!(
            failed,
            Some(Answer::Error {
                kind: Fault::Ambiguous,
                count: 3,
                ..
            })
        ));
    }

    #[test]
    fn no_answer_is_none_and_a_strange_one_fails() {
        assert!(Answer::read(Value::Null).unwrap().is_none());
        assert!(matches!(
            Answer::read(json!({"status": "unheard of"})),
            Err(Error::Json(_))
        ));
    }

    #[test]
    fn a_value_is_read_from_its_json_text() {
        assert_eq!(Script::value::<Vec<u8>>(Some("[1,2]")).unwrap(), [1, 2]);
        assert_eq!(Script::value::<Option<String>>(None).unwrap(), None);
        Script::value::<()>(None).unwrap();
        assert!(matches!(
            Script::value::<bool>(Some("{")),
            Err(Error::Json(_))
        ));
    }

    #[test]
    fn obstacles_and_faults_become_the_callers_errors() {
        assert_eq!(Obstacle::Covered.blocker(), Some(Blocker::Covered));
        assert_eq!(Obstacle::Missing.blocker(), None);
        assert_eq!(Obstacle::Unmet.blocker(), None);
        assert!(matches!(
            Fault::Ambiguous.error(None, String::new(), 2),
            Error::Ambiguous(selector, 2) if selector == "the page"
        ));
        assert!(matches!(
            Fault::Script.error(None, "boom".to_owned(), 0),
            Error::Script(message) if message == "boom"
        ));
        assert!(matches!(
            Fault::Detached.error(None, String::new(), 0),
            Error::Detached
        ));
    }
}
