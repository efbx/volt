use serde::Serialize;

use super::Element;
use crate::Error;
use crate::page::Patience;
use crate::page_script::Op;

/// A state an element can be waited into.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    /// It is in the page.
    Attached,
    /// Nothing matches any more.
    Detached,
    /// It has a size and its style does not hide it.
    Visible,
    /// It is not visible, or not in the page.
    Hidden,
    /// It is enabled.
    Enabled,
    /// It is disabled.
    Disabled,
    /// It is a field that takes text, enabled and not read-only.
    Editable,
    /// It is a checked checkbox or radio button.
    Checked,
    /// It is an unchecked checkbox or radio button.
    Unchecked,
    /// It has the keyboard focus.
    Focused,
}

impl Element {
    /// Waits until the element is in the page.
    pub async fn wait(&self) -> Result<(), Error> {
        self.wait_for(State::Attached).await
    }

    /// Waits until the element is in `state`, failing with [`Error::Timeout`] when it does not
    /// get there in time, or with [`Error::NotFound`] when a state of a present element is
    /// awaited and nothing ever matches.
    pub async fn wait_for(&self, state: State) -> Result<(), Error> {
        self.ask(Op::State, &state, Patience::Wait).await
    }

    /// Waits until the element's visible text, as [`text`](Self::text) gives it, contains `text`,
    /// whatever its case.
    pub async fn wait_for_text(&self, text: &str) -> Result<(), Error> {
        self.ask(Op::HasText, text, Patience::Wait).await
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::State;
    use crate::fake::Fake;
    use crate::{By, Error};

    #[tokio::test]
    async fn a_wait_asks_again_until_the_state_holds() {
        let fake = Fake::answering([
            json!({"status": "wait", "blocker": "unmet", "rect": null}),
            json!({"status": "wait", "blocker": "unmet", "rect": null}),
            json!({"status": "ok", "json": null}),
        ]);
        fake.page()
            .find(By::id("spinner"))
            .wait_for(State::Hidden)
            .await
            .unwrap();
        let commands = fake.commands();
        assert_eq!(commands.len(), 3);
        assert_eq!(
            (&commands[0]["op"], &commands[0]["arg"]),
            (&json!("state"), &json!("hidden"))
        );
    }

    #[tokio::test]
    async fn a_state_never_reached_times_out() {
        let fake = Fake::repeating(json!({"status": "wait", "blocker": "unmet", "rect": null}));
        let outcome = fake
            .page()
            .find(By::id("box"))
            .wait_for(State::Checked)
            .await;
        assert!(matches!(outcome, Err(Error::Timeout(timeout)) if timeout == Fake::TIMEOUT));
    }

    #[tokio::test]
    async fn waiting_for_an_element_that_never_comes_is_not_found() {
        let fake = Fake::repeating(json!({"status": "wait", "blocker": "missing", "rect": null}));
        let outcome = fake.page().find(By::id("late")).wait().await;
        assert!(matches!(outcome, Err(Error::NotFound(_))));
    }

    #[tokio::test]
    async fn awaited_text_travels_as_the_argument() {
        let fake = Fake::answering([json!({"status": "ok", "json": null})]);
        fake.page()
            .find(By::id("status"))
            .wait_for_text("Saved")
            .await
            .unwrap();
        let commands = fake.commands();
        assert_eq!(
            (&commands[0]["op"], &commands[0]["arg"]),
            (&json!("has_text"), &json!("Saved"))
        );
    }
}
