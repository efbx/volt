use serde::de::DeserializeOwned;

use super::Element;
use crate::page::Patience;
use crate::page_script::Op;
use crate::{Error, Rect};

impl Element {
    /// Whether anything matches right now.
    pub async fn exists(&self) -> Result<bool, Error> {
        self.ask(Op::Exists, &(), Patience::Now).await
    }

    /// How many elements match right now.
    pub async fn count(&self) -> Result<usize, Error> {
        self.ask(Op::Count, &(), Patience::Now).await
    }

    /// One description for each element that matches right now, in document order.
    pub async fn all(&self) -> Result<Vec<Self>, Error> {
        let count = self.count().await?;
        Ok((0..count).map(|index| self.clone().nth(index)).collect())
    }

    /// The visible text of the element, as the browser renders it for a person, without white
    /// space at its ends: hidden parts inside the element are left out, an `<input>` has none
    /// whatever its value, and an element that is not rendered at all, as a hidden one, gives all
    /// its text.
    pub async fn text(&self) -> Result<String, Error> {
        self.ask(Op::Text, &(), Patience::Wait).await
    }

    /// The visible text, as [`text`](Self::text) gives it, of every element that matches right
    /// now.
    pub async fn texts(&self) -> Result<Vec<String>, Error> {
        self.ask(Op::Texts, &(), Patience::Now).await
    }

    /// The text of the element's nodes as written, hidden parts included.
    pub async fn text_content(&self) -> Result<String, Error> {
        self.ask(Op::TextContent, &(), Patience::Wait).await
    }

    /// The HTML inside the element.
    pub async fn html(&self) -> Result<String, Error> {
        self.ask(Op::InnerHtml, &(), Patience::Wait).await
    }

    /// The HTML of the element, its own tag included.
    pub async fn outer_html(&self) -> Result<String, Error> {
        self.ask(Op::OuterHtml, &(), Patience::Wait).await
    }

    /// The attribute `name` as written in the HTML, or `None` where the element has none.
    pub async fn attribute(&self, name: &str) -> Result<Option<String>, Error> {
        self.ask(Op::Attribute, name, Patience::Wait).await
    }

    /// The JavaScript property `name` of the element, read as `T`.
    pub async fn property<T: DeserializeOwned>(&self, name: &str) -> Result<T, Error> {
        self.ask(Op::Property, name, Patience::Wait).await
    }

    /// The current value of a form field.
    pub async fn value(&self) -> Result<String, Error> {
        self.ask(Op::Value, &(), Patience::Wait).await
    }

    /// The computed value of the CSS property `property`.
    pub async fn style(&self, property: &str) -> Result<String, Error> {
        self.ask(Op::Style, property, Patience::Wait).await
    }

    /// The box the element takes in the viewport.
    pub async fn rect(&self) -> Result<Rect, Error> {
        self.ask(Op::Rect, &(), Patience::Wait).await
    }

    /// The element's tag name in lower case.
    pub async fn tag(&self) -> Result<String, Error> {
        self.ask(Op::Tag, &(), Patience::Wait).await
    }

    /// Whether the element is there and visible right now: it has a size and its style does not
    /// hide it.
    pub async fn is_visible(&self) -> Result<bool, Error> {
        self.ask(Op::IsVisible, &(), Patience::Now).await
    }

    /// Whether the element is enabled right now.
    pub async fn is_enabled(&self) -> Result<bool, Error> {
        self.ask(Op::IsEnabled, &(), Patience::Now).await
    }

    /// Whether the element takes text right now: a field that is enabled and not read-only.
    pub async fn is_editable(&self) -> Result<bool, Error> {
        self.ask(Op::IsEditable, &(), Patience::Now).await
    }

    /// Whether a checkbox or radio button is checked right now.
    pub async fn is_checked(&self) -> Result<bool, Error> {
        self.ask(Op::IsChecked, &(), Patience::Now).await
    }

    /// Whether an `<option>` is selected, or a checkbox or radio button checked, right now.
    pub async fn is_selected(&self) -> Result<bool, Error> {
        self.ask(Op::IsSelected, &(), Patience::Now).await
    }

    /// Whether the element has the keyboard focus right now.
    pub async fn is_focused(&self) -> Result<bool, Error> {
        self.ask(Op::IsFocused, &(), Patience::Now).await
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::fake::Fake;
    use crate::{By, Error, Rect};

    #[tokio::test]
    async fn each_read_asks_for_its_own_command_and_reads_the_answer() {
        let fake = Fake::answering([
            json!({"status": "ok", "json": "\"Sign in\""}),
            json!({"status": "ok", "json": "[\"a\",\"b\"]"}),
            json!({"status": "ok", "json": "null"}),
            json!({"status": "ok", "json": "7"}),
            json!({"status": "ok", "json": "{\"x\":1,\"y\":2,\"width\":3,\"height\":4.5}"}),
            json!({"status": "ok", "json": "false"}),
        ]);
        let element = fake.page().find(By::css("button"));

        assert_eq!(element.text().await.unwrap(), "Sign in");
        assert_eq!(element.texts().await.unwrap(), ["a", "b"]);
        assert_eq!(element.attribute("href").await.unwrap(), None);
        assert_eq!(element.property::<u32>("tabIndex").await.unwrap(), 7);
        assert_eq!(
            element.rect().await.unwrap(),
            Rect {
                x: 1.0,
                y: 2.0,
                width: 3.0,
                height: 4.5
            }
        );
        assert!(!element.is_checked().await.unwrap());

        let commands = fake.commands();
        let asked: Vec<_> = commands.iter().map(|command| &command["op"]).collect();
        assert_eq!(
            asked,
            [
                "text",
                "texts",
                "attribute",
                "property",
                "rect",
                "is_checked"
            ]
        );
        assert_eq!(commands[2]["arg"], "href");
        assert_eq!(commands[3]["arg"], "tabIndex");
    }

    #[tokio::test]
    async fn all_gives_one_description_per_match() {
        let fake = Fake::answering([json!({"status": "ok", "json": "3"})]);
        let items = fake.page().find(By::tag("li")).all().await.unwrap();
        let shown: Vec<_> = items.iter().map(ToString::to_string).collect();
        assert_eq!(
            shown,
            [
                "tag \"li\" > nth 0",
                "tag \"li\" > nth 1",
                "tag \"li\" > nth 2"
            ]
        );
    }

    #[tokio::test]
    async fn a_question_about_now_does_not_wait_for_a_missing_element() {
        let fake = Fake::answering([json!({"status": "wait", "blocker": "missing", "rect": null})]);
        let element = fake.page().find(By::id("gone"));
        assert!(matches!(
            element.is_enabled().await,
            Err(Error::NotFound(selector)) if selector == "id \"gone\""
        ));
        assert_eq!(fake.commands().len(), 1);
    }

    #[tokio::test]
    async fn a_value_of_the_wrong_shape_fails_as_json() {
        let fake = Fake::answering([json!({"status": "ok", "json": "\"text\""})]);
        let element = fake.page().find(By::id("box"));
        assert!(matches!(element.count().await, Err(Error::Json(_))));
    }
}
