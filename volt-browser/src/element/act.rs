use serde::Serialize;
use serde::de::DeserializeOwned;

use super::Element;
use crate::page::Patience;
use crate::page_script::{Code, Op};
use crate::{Button, Choice, Click, Error, Keystroke, Point};

/// An event to send to an element.
#[derive(Serialize)]
struct Occasion<'a, I: ?Sized> {
    /// The event's type, such as `mouseover`.
    #[serde(rename = "type")]
    kind: &'a str,
    /// The properties the event is made with.
    init: &'a I,
}

impl Element {
    /// Clicks the middle of the element once with the left button. It waits until the element
    /// is visible, holds still, is enabled and is not covered, and scrolls it into view.
    pub async fn click(&self) -> Result<(), Error> {
        self.click_with(Click::default()).await
    }

    /// Clicks as `click` says: another button, several times, with modifier keys held, or at a
    /// place inside the element.
    pub async fn click_with(&self, click: Click) -> Result<(), Error> {
        self.act(Op::Click, &click).await
    }

    /// Clicks twice in a row, which a page hears as a double click.
    pub async fn double_click(&self) -> Result<(), Error> {
        self.click_with(Click {
            count: 2,
            ..Click::default()
        })
        .await
    }

    /// Clicks with the right button, which a page hears as a request for its context menu.
    pub async fn right_click(&self) -> Result<(), Error> {
        self.click_with(Click {
            button: Button::Right,
            ..Click::default()
        })
        .await
    }

    /// Sends the element the events of a pointer moving over its middle; no pointer moves, so
    /// CSS `:hover` does not apply.
    pub async fn hover(&self) -> Result<(), Error> {
        self.act(Op::Hover, &()).await
    }

    /// Gives the element the keyboard focus.
    pub async fn focus(&self) -> Result<(), Error> {
        self.act(Op::Focus, &()).await
    }

    /// Takes the keyboard focus from the element.
    pub async fn blur(&self) -> Result<(), Error> {
        self.act(Op::Blur, &()).await
    }

    /// Replaces the whole value of a text field with `text`, as one edit. It waits until the
    /// field is visible, enabled and not read-only.
    pub async fn fill(&self, text: &str) -> Result<(), Error> {
        self.act(Op::Fill, text).await
    }

    /// Empties a text field.
    pub async fn clear(&self) -> Result<(), Error> {
        self.fill("").await
    }

    /// Types `text` into the element key by key, for pages that react to each key; a line break
    /// presses Enter. It waits until the element is visible and enabled. [`fill`](Self::fill) is
    /// the quicker way to set a value.
    pub async fn type_text(&self, text: &str) -> Result<(), Error> {
        self.act(Op::TypeText, text).await
    }

    /// Presses one key on the element, with any modifier keys held: `press(NamedKey::Enter)`,
    /// `press(Keystroke::with(Modifiers::CONTROL, 'a'))`. Enter, Space, Tab, Backspace, Delete,
    /// the left and right arrows, Home, End and select-all have their usual effect; the up and
    /// down arrows only send their events. It waits until the element is visible and enabled.
    pub async fn press(&self, keystroke: impl Into<Keystroke>) -> Result<(), Error> {
        let keystroke = keystroke.into();
        self.act(Op::Press, &keystroke).await
    }

    /// Checks a checkbox or radio button, clicking it when it is not checked yet.
    pub async fn check(&self) -> Result<(), Error> {
        self.set_checked(true).await
    }

    /// Unchecks a checkbox, clicking it when it is checked.
    pub async fn uncheck(&self) -> Result<(), Error> {
        self.set_checked(false).await
    }

    /// Checks or unchecks a checkbox or radio button, clicking it when its state differs.
    pub async fn set_checked(&self, checked: bool) -> Result<(), Error> {
        self.act(Op::SetChecked, &checked).await
    }

    /// Chooses one option of a `<select>`.
    pub async fn select(&self, choice: Choice<'_>) -> Result<(), Error> {
        self.select_all(&[choice]).await
    }

    /// Chooses exactly these options of a `<select>` that takes several.
    pub async fn select_all(&self, choices: &[Choice<'_>]) -> Result<(), Error> {
        self.act(Op::Select, choices).await
    }

    /// Submits the form the element is, or is in, running the form's own checks and handlers.
    pub async fn submit(&self) -> Result<(), Error> {
        self.act(Op::Submit, &()).await
    }

    /// Scrolls the page until the element is in the middle of the view.
    pub async fn scroll_into_view(&self) -> Result<(), Error> {
        self.act(Op::ScrollIntoView, &()).await
    }

    /// Scrolls the element's own content to `point`.
    pub async fn scroll_to(&self, point: Point) -> Result<(), Error> {
        self.act(Op::ScrollTo, &point).await
    }

    /// Scrolls the element's own content by `delta`.
    pub async fn scroll_by(&self, delta: Point) -> Result<(), Error> {
        self.act(Op::ScrollBy, &delta).await
    }

    /// Selects all the text of the element.
    pub async fn select_text(&self) -> Result<(), Error> {
        self.act(Op::SelectText, &()).await
    }

    /// Sends the element an event of type `event`, made with the properties in `init`.
    pub async fn dispatch<I: Serialize + ?Sized>(
        &self,
        event: &str,
        init: &I,
    ) -> Result<(), Error> {
        let occasion = Occasion { kind: event, init };
        self.act(Op::Dispatch, &occasion).await
    }

    /// Calls the JavaScript `function` with the element and `arg`, and reads what it returns as
    /// `T`, waiting for a promise. This is the way out for what no command covers. A function
    /// that throws fails with [`Error::Script`], and so does one that does not parse on Chrome;
    /// on the web view that one gives no answer and fails with [`Error::NotFound`] at the timeout.
    pub async fn call<T, A>(&self, function: &str, arg: &A) -> Result<T, Error>
    where
        T: DeserializeOwned,
        A: Serialize + ?Sized,
    {
        self.run(Op::Call, arg, Code::Function(function), Patience::Wait)
            .await
    }

    /// Runs the command `op`, which answers nothing, once the element is ready for it.
    async fn act<A: Serialize + ?Sized>(&self, op: Op, arg: &A) -> Result<(), Error> {
        self.ask(op, arg, Patience::Wait).await
    }
}

#[cfg(test)]
mod tests {
    use keyboard_types::{Modifiers, NamedKey};
    use serde_json::{Value, json};

    use crate::fake::Fake;
    use crate::{Blocker, By, Choice, Error, Keystroke, Point};

    /// The answer of a command that is done and gives nothing back.
    struct Done;

    impl Done {
        /// Returns the answer.
        fn answer() -> Value {
            json!({"status": "ok", "json": null})
        }
    }

    #[tokio::test]
    async fn each_action_sends_its_command_with_what_it_needs() {
        let fake = Fake::answering((0..9).map(|_| Done::answer()));
        let element = fake.page().find(By::name("q"));

        element.double_click().await.unwrap();
        element.right_click().await.unwrap();
        element.fill("tea").await.unwrap();
        element.clear().await.unwrap();
        element.press(NamedKey::Enter).await.unwrap();
        element
            .press(Keystroke::with(Modifiers::META, 'a'))
            .await
            .unwrap();
        element.uncheck().await.unwrap();
        element
            .select_all(&[Choice::Value("a"), Choice::Index(1)])
            .await
            .unwrap();
        element.scroll_by(Point { x: 0.0, y: 40.0 }).await.unwrap();

        let commands = fake.commands();
        assert_eq!(commands[0]["op"], "click");
        assert_eq!(commands[0]["arg"]["count"], 2);
        assert_eq!(commands[1]["arg"]["button"], "right");
        assert_eq!(
            (&commands[2]["op"], &commands[2]["arg"]),
            (&json!("fill"), &json!("tea"))
        );
        assert_eq!(commands[3]["arg"], "");
        assert_eq!(commands[4]["arg"]["key"], "Enter");
        assert_eq!(commands[5]["arg"]["held"]["meta"], true);
        assert_eq!(
            (&commands[6]["op"], &commands[6]["arg"]),
            (&json!("set_checked"), &json!(false))
        );
        assert_eq!(commands[7]["arg"], json!([{"value": "a"}, {"index": 1}]));
        assert_eq!(commands[8]["arg"], json!({"x": 0.0, "y": 40.0}));
    }

    #[tokio::test]
    async fn an_action_waits_out_what_blocks_it() {
        let fake = Fake::answering([
            json!({"status": "wait", "blocker": "missing", "rect": null}),
            json!({"status": "wait", "blocker": "covered", "rect": null}),
            Done::answer(),
        ]);
        fake.page().find(By::id("go")).click().await.unwrap();
        assert_eq!(fake.commands().len(), 3);
    }

    #[tokio::test]
    async fn a_click_is_sent_again_with_the_box_it_must_hold() {
        let seen = json!({"x": 10.0, "y": 20.0, "width": 80.0, "height": 24.0});
        let fake = Fake::answering([
            json!({"status": "wait", "blocker": "moving", "rect": seen}),
            Done::answer(),
        ]);
        fake.page().find(By::id("go")).click().await.unwrap();
        let commands = fake.commands();
        assert_eq!(commands[0]["rect"], json!(null));
        assert_eq!(commands[1]["rect"], seen);
    }

    #[tokio::test]
    async fn what_still_blocks_at_the_timeout_is_the_error() {
        let fake = Fake::repeating(json!({"status": "wait", "blocker": "disabled", "rect": null}));
        let outcome = fake.page().find(By::id("go")).click().await;
        assert!(matches!(
            outcome,
            Err(Error::NotActionable(selector, Blocker::Disabled)) if selector == "id \"go\""
        ));
        assert!(fake.commands().len() > 1);
    }

    #[tokio::test]
    async fn a_failure_ends_the_command_at_once() {
        let fake = Fake::answering([
            json!({"status": "error", "kind": "ambiguous", "message": "", "count": 4}),
            json!({"status": "error", "kind": "wrong", "message": "it is not a select element", "count": 0}),
            json!({"status": "error", "kind": "selector", "message": "bad", "count": 0}),
            json!({"status": "error", "kind": "frame", "message": "it has no open shadow root", "count": 0}),
        ]);
        let element = fake.page().find(By::css("li"));
        assert!(matches!(element.click().await, Err(Error::Ambiguous(_, 4))));
        assert!(matches!(
            element.select(Choice::Index(0)).await,
            Err(Error::WrongElement(_, reason)) if reason == "it is not a select element"
        ));
        assert!(matches!(
            element.hover().await,
            Err(Error::InvalidSelector(_, _))
        ));
        assert!(matches!(
            element.focus().await,
            Err(Error::NoSuchFrame(_, _))
        ));
        assert_eq!(fake.commands().len(), 4);
    }

    #[tokio::test]
    async fn the_callers_function_travels_as_code_and_its_argument_as_json() {
        let fake = Fake::answering([json!({"status": "ok", "json": "\"A-1\""})]);
        let element = fake.page().find(By::id("row"));
        let answer: String = element
            .call("function (row, suffix) { return row.id + suffix; }", "-1")
            .await
            .unwrap();
        assert_eq!(answer, "A-1");
        assert_eq!(fake.commands()[0]["arg"], "-1");
        assert_eq!(
            fake.tail(0),
            ",(\nfunction (row, suffix) { return row.id + suffix; }\n))"
        );
    }

    #[tokio::test]
    async fn an_event_carries_its_type_and_properties() {
        let fake = Fake::answering([Done::answer()]);
        fake.page()
            .find(By::id("zone"))
            .dispatch("drop", &json!({"detail": 3}))
            .await
            .unwrap();
        assert_eq!(
            fake.commands()[0]["arg"],
            json!({"type": "drop", "init": {"detail": 3}})
        );
    }
}
