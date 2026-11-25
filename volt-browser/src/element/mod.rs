mod act;
mod read;
mod wait;

use std::borrow::Cow;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::ser::SerializeSeq;
use serde::{Serialize, Serializer};

use crate::page::{Page, Patience};
use crate::page_script::{Code, Op, Request};
use crate::{By, Error};
pub use wait::State;

/// A description of an element in a page: how to find it, not a handle on it. Every command
/// finds the element afresh when it runs, so an `Element` never goes stale, and it can be made
/// before the element exists.
///
/// The narrowing methods cost no trip to the page. Commands that act wait until the element is
/// ready for them; commands that read wait until it exists. A command for one element fails
/// with [`Error::Ambiguous`] when several match, unless the description ends in
/// [`first`](Self::first), [`last`](Self::last) or [`nth`](Self::nth).
#[derive(Clone, Debug)]
pub struct Element {
    /// The page the element is looked for in.
    page: Page,
    /// The steps that lead to the element.
    path: Arc<Path>,
    /// The most a command on this element waits, where it differs from the session's.
    timeout: Option<Duration>,
}

impl Element {
    /// Returns the element `step` leads to from the top of `page`.
    pub(crate) fn new(page: Page, step: Step) -> Self {
        Self {
            page,
            path: Arc::new(Path { parent: None, step }),
            timeout: None,
        }
    }

    /// The elements `by` finds inside this one.
    #[must_use]
    pub fn find(self, by: By) -> Self {
        self.then(Step::Find(by))
    }

    /// The match at `index`, counting from zero in document order.
    #[must_use]
    pub fn nth(self, index: usize) -> Self {
        self.then(Step::Nth(index))
    }

    /// The first match in document order.
    #[must_use]
    pub fn first(self) -> Self {
        self.then(Step::First)
    }

    /// The last match in document order.
    #[must_use]
    pub fn last(self) -> Self {
        self.then(Step::Last)
    }

    /// Only the matches whose visible text, as [`text`](Self::text) gives it, contains `text`,
    /// whatever its case.
    #[must_use]
    pub fn with_text(self, text: impl Into<Cow<'static, str>>) -> Self {
        self.then(Step::WithText(text.into()))
    }

    /// Only the matches whose visible text, as [`text`](Self::text) gives it, does not contain
    /// `text`, whatever its case.
    #[must_use]
    pub fn without_text(self, text: impl Into<Cow<'static, str>>) -> Self {
        self.then(Step::WithoutText(text.into()))
    }

    /// Only the matches that have something `by` finds inside them.
    #[must_use]
    pub fn having(self, by: By) -> Self {
        self.then(Step::Having(by))
    }

    /// Only the matches that have nothing `by` finds inside them.
    #[must_use]
    pub fn not_having(self, by: By) -> Self {
        self.then(Step::NotHaving(by))
    }

    /// Only the matches that are visible.
    #[must_use]
    pub fn visible(self) -> Self {
        self.then(Step::Visible)
    }

    /// The matches of this description and of `other` together.
    #[must_use]
    pub fn or(self, other: Self) -> Self {
        self.then(Step::Or(Branch(other.path)))
    }

    /// The parent of each match.
    #[must_use]
    pub fn parent(self) -> Self {
        self.then(Step::Parent)
    }

    /// For each match, the nearest of itself and its ancestors that `by` finds.
    #[must_use]
    pub fn closest(self, by: By) -> Self {
        self.then(Step::Closest(by))
    }

    /// The document inside each matching `<iframe>`, to go on finding in. Only a frame from the
    /// page's own site can be entered; another fails with [`Error::NoSuchFrame`].
    #[must_use]
    pub fn frame(self) -> Self {
        self.then(Step::Frame)
    }

    /// The open shadow root of each match, to go on finding in.
    #[must_use]
    pub fn shadow(self) -> Self {
        self.then(Step::Shadow)
    }

    /// The same element, with `timeout` as the most its commands wait.
    #[must_use]
    pub fn timeout(self, timeout: Duration) -> Self {
        Self {
            timeout: Some(timeout),
            ..self
        }
    }

    /// Returns the description with `step` added at its end.
    fn then(self, step: Step) -> Self {
        Self {
            path: Arc::new(Path {
                parent: Some(self.path),
                step,
            }),
            ..self
        }
    }

    /// Runs the command `op` on the element and reads its answer.
    pub(crate) async fn ask<T, A>(&self, op: Op, arg: &A, patience: Patience) -> Result<T, Error>
    where
        T: DeserializeOwned,
        A: Serialize + ?Sized,
    {
        self.run(op, arg, Code::None, patience).await
    }

    /// Runs the command `op` on the element with the caller's `code` and reads its answer.
    pub(crate) async fn run<T, A>(
        &self,
        op: Op,
        arg: &A,
        code: Code<'_>,
        patience: Patience,
    ) -> Result<T, Error>
    where
        T: DeserializeOwned,
        A: Serialize + ?Sized,
    {
        let request = Request {
            op,
            path: Some(&self.path),
            arg,
            code,
        };
        let timeout = self.timeout.unwrap_or_else(|| self.page.settings().timeout);
        self.page.perform(&request, timeout, patience).await
    }
}

impl fmt::Display for Element {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.path.fmt(f)
    }
}

/// The steps that lead to an element, each holding the ones before it, so that descriptions
/// built from one another share them.
#[derive(Debug)]
pub(crate) struct Path {
    /// The steps before this one.
    parent: Option<Arc<Path>>,
    /// The last step.
    step: Step,
}

impl Path {
    /// The words an error uses for the element `path` leads to, or for no element at all.
    pub(crate) fn label(path: Option<&Self>) -> String {
        path.map_or_else(|| "the page".to_owned(), ToString::to_string)
    }

    /// How many steps there are.
    fn len(&self) -> usize {
        1 + self.parent.as_deref().map_or(0, Self::len)
    }

    /// Writes the steps into `steps`, the first one first.
    fn write<S: SerializeSeq>(&self, steps: &mut S) -> Result<(), S::Error> {
        if let Some(parent) = &self.parent {
            parent.write(steps)?;
        }
        steps.serialize_element(&self.step)
    }
}

impl Serialize for Path {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut steps = serializer.serialize_seq(Some(self.len()))?;
        self.write(&mut steps)?;
        steps.end()
    }
}

impl fmt::Display for Path {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(parent) = &self.parent {
            write!(f, "{parent} > ")?;
        }
        self.step.fmt(f)
    }
}

/// One step on the way to an element, named as the page's helper script knows it.
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Step {
    /// What a search finds inside the matches so far.
    Find(By),
    /// The element that has the focus.
    Focused,
    /// The match at a place.
    Nth(usize),
    /// The first match.
    First,
    /// The last match.
    Last,
    /// The matches whose text contains this.
    WithText(Cow<'static, str>),
    /// The matches whose text does not contain this.
    WithoutText(Cow<'static, str>),
    /// The matches with something a search finds inside.
    Having(By),
    /// The matches with nothing a search finds inside.
    NotHaving(By),
    /// The visible matches.
    Visible,
    /// The matches together with those of another description.
    Or(Branch),
    /// The parents of the matches.
    Parent,
    /// The nearest ancestors a search finds.
    Closest(By),
    /// The documents inside matching frames.
    Frame,
    /// The shadow roots of the matches.
    Shadow,
}

impl fmt::Display for Step {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Find(by) => by.fmt(f),
            Self::Focused => f.write_str("the focused element"),
            Self::Nth(index) => write!(f, "nth {index}"),
            Self::First => f.write_str("first"),
            Self::Last => f.write_str("last"),
            Self::WithText(text) => write!(f, "with text {text:?}"),
            Self::WithoutText(text) => write!(f, "without text {text:?}"),
            Self::Having(by) => write!(f, "having {by}"),
            Self::NotHaving(by) => write!(f, "not having {by}"),
            Self::Visible => f.write_str("visible"),
            Self::Or(branch) => write!(f, "or ({})", branch.0),
            Self::Parent => f.write_str("parent"),
            Self::Closest(by) => write!(f, "closest {by}"),
            Self::Frame => f.write_str("frame"),
            Self::Shadow => f.write_str("shadow"),
        }
    }
}

/// Another description's steps, inside a step.
#[derive(Debug)]
pub(crate) struct Branch(Arc<Path>);

impl Serialize for Branch {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.as_ref().serialize(serializer)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use serde_json::json;

    use crate::fake::Fake;
    use crate::{By, Error};

    #[tokio::test]
    async fn the_steps_reach_the_page_in_the_order_they_were_added() {
        let fake = Fake::answering([json!({"status": "ok", "json": "true"})]);
        let element = fake
            .page()
            .find(By::css("form"))
            .find(By::name("email"))
            .with_text("Mail")
            .having(By::tag("b"))
            .nth(2)
            .visible()
            .parent()
            .closest(By::css("section"))
            .frame()
            .shadow()
            .first();

        assert!(element.exists().await.unwrap());
        assert_eq!(
            fake.commands()[0]["path"],
            json!([
                {"find": {"strategy": "css", "value": "form"}},
                {"find": {"strategy": "name", "value": "email"}},
                {"with_text": "Mail"},
                {"having": {"strategy": "tag", "value": "b"}},
                {"nth": 2},
                "visible",
                "parent",
                {"closest": {"strategy": "css", "value": "section"}},
                "frame",
                "shadow",
                "first"
            ])
        );
    }

    #[tokio::test]
    async fn another_description_travels_inside_its_step() {
        let fake = Fake::answering([json!({"status": "ok", "json": "2"})]);
        let page = fake.page();
        let either = page
            .find(By::id("a"))
            .or(page.find(By::id("b")).last())
            .without_text("x")
            .not_having(By::tag("i"));

        assert_eq!(either.count().await.unwrap(), 2);
        assert_eq!(
            fake.commands()[0]["path"],
            json!([
                {"find": {"strategy": "id", "value": "a"}},
                {"or": [{"find": {"strategy": "id", "value": "b"}}, "last"]},
                {"without_text": "x"},
                {"not_having": {"strategy": "tag", "value": "i"}}
            ])
        );
    }

    #[tokio::test]
    async fn a_description_shows_as_its_steps() {
        let fake = Fake::answering([]);
        let page = fake.page();
        let element = page
            .find(By::css("ul"))
            .find(By::tag("li"))
            .with_text("Tea")
            .or(page.focused())
            .first();
        assert_eq!(
            element.to_string(),
            "css \"ul\" > tag \"li\" > with text \"Tea\" > or (the focused element) > first"
        );
    }

    #[tokio::test]
    async fn a_shared_start_is_not_changed_by_what_is_built_on_it() {
        let fake = Fake::answering([]);
        let teas = fake.page().find(By::tag("ul"));
        let first = teas.clone().first();
        let last = teas.clone().last();
        assert_eq!(teas.to_string(), "tag \"ul\"");
        assert_eq!(first.to_string(), "tag \"ul\" > first");
        assert_eq!(last.to_string(), "tag \"ul\" > last");
    }

    #[tokio::test]
    async fn an_elements_own_timeout_replaces_the_sessions() {
        let fake = Fake::answering([]);
        let element = fake
            .page()
            .find(By::id("late"))
            .timeout(Duration::from_millis(30));
        assert!(matches!(
            element.text().await,
            Err(Error::NotFound(selector)) if selector == "id \"late\""
        ));
    }
}
