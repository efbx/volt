use std::borrow::Cow;
use std::fmt;

use serde::Serialize;

/// A way to find elements in a page. A literal such as `By::css("input[name=email]")` is
/// borrowed, not copied.
///
/// [`css`](Self::css), [`xpath`](Self::xpath), [`tag`](Self::tag), [`link`](Self::link) and
/// [`link_containing`](Self::link_containing) are the five strategies of the W3C `WebDriver`
/// specification; the others follow Selenium's `By` and Playwright's finders.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct By {
    /// The kind of search.
    strategy: Strategy,
    /// What the search looks for.
    value: Cow<'static, str>,
}

impl By {
    /// Elements matching a CSS selector.
    #[must_use]
    pub fn css(selector: impl Into<Cow<'static, str>>) -> Self {
        Self::new(Strategy::Css, selector)
    }

    /// Elements an `XPath` expression selects. A path that starts with `//` searches the whole
    /// document, wherever the search starts.
    #[must_use]
    pub fn xpath(path: impl Into<Cow<'static, str>>) -> Self {
        Self::new(Strategy::Xpath, path)
    }

    /// Elements with this `id` attribute.
    #[must_use]
    pub fn id(id: impl Into<Cow<'static, str>>) -> Self {
        Self::new(Strategy::Id, id)
    }

    /// Elements with this `name` attribute.
    #[must_use]
    pub fn name(name: impl Into<Cow<'static, str>>) -> Self {
        Self::new(Strategy::Name, name)
    }

    /// Elements with this tag name.
    #[must_use]
    pub fn tag(tag: impl Into<Cow<'static, str>>) -> Self {
        Self::new(Strategy::Tag, tag)
    }

    /// Elements with this class.
    #[must_use]
    pub fn class(class: impl Into<Cow<'static, str>>) -> Self {
        Self::new(Strategy::Class, class)
    }

    /// The innermost elements whose whole text, as written in the page with its hidden parts, is
    /// `text`, runs of white space counting as one space. A button made from an `<input>` matches
    /// by its value.
    #[must_use]
    pub fn text(text: impl Into<Cow<'static, str>>) -> Self {
        Self::new(Strategy::Text, text)
    }

    /// The innermost elements whose text, as written in the page with its hidden parts, contains
    /// `text`, whatever its case.
    #[must_use]
    pub fn text_containing(text: impl Into<Cow<'static, str>>) -> Self {
        Self::new(Strategy::TextContaining, text)
    }

    /// Links whose visible text is `text`.
    #[must_use]
    pub fn link(text: impl Into<Cow<'static, str>>) -> Self {
        Self::new(Strategy::Link, text)
    }

    /// Links whose visible text contains `text`, whatever its case.
    #[must_use]
    pub fn link_containing(text: impl Into<Cow<'static, str>>) -> Self {
        Self::new(Strategy::LinkContaining, text)
    }

    /// Form fields whose label contains `text`, whatever its case: a `<label>` for the field, or
    /// its `aria-label` or `aria-labelledby`.
    #[must_use]
    pub fn label(text: impl Into<Cow<'static, str>>) -> Self {
        Self::new(Strategy::Label, text)
    }

    /// Elements whose `placeholder` contains `text`, whatever its case.
    #[must_use]
    pub fn placeholder(text: impl Into<Cow<'static, str>>) -> Self {
        Self::new(Strategy::Placeholder, text)
    }

    /// Elements whose `alt` text contains `text`, whatever its case.
    #[must_use]
    pub fn alt(text: impl Into<Cow<'static, str>>) -> Self {
        Self::new(Strategy::Alt, text)
    }

    /// Elements whose `title` attribute contains `text`, whatever its case.
    #[must_use]
    pub fn title(text: impl Into<Cow<'static, str>>) -> Self {
        Self::new(Strategy::Title, text)
    }

    /// Elements with this `data-testid` attribute.
    #[must_use]
    pub fn test_id(id: impl Into<Cow<'static, str>>) -> Self {
        Self::new(Strategy::TestId, id)
    }

    /// Returns the search of kind `strategy` for `value`.
    fn new(strategy: Strategy, value: impl Into<Cow<'static, str>>) -> Self {
        Self {
            strategy,
            value: value.into(),
        }
    }
}

impl fmt::Display for By {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {:?}", self.strategy.name(), self.value)
    }
}

/// The kinds of search, named as the page's helper script knows them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Strategy {
    /// A CSS selector.
    Css,
    /// An `XPath` expression.
    Xpath,
    /// An `id` attribute.
    Id,
    /// A `name` attribute.
    Name,
    /// A tag name.
    Tag,
    /// A class.
    Class,
    /// The whole text.
    Text,
    /// A part of the text.
    TextContaining,
    /// A link's whole text.
    Link,
    /// A part of a link's text.
    LinkContaining,
    /// A form field's label.
    Label,
    /// A `placeholder` attribute.
    Placeholder,
    /// An `alt` attribute.
    Alt,
    /// A `title` attribute.
    Title,
    /// A `data-testid` attribute.
    TestId,
}

impl Strategy {
    /// The words an error message uses for the kind of search.
    fn name(self) -> &'static str {
        match self {
            Self::Css => "css",
            Self::Xpath => "xpath",
            Self::Id => "id",
            Self::Name => "name",
            Self::Tag => "tag",
            Self::Class => "class",
            Self::Text => "text",
            Self::TextContaining => "text containing",
            Self::Link => "link",
            Self::LinkContaining => "link containing",
            Self::Label => "label",
            Self::Placeholder => "placeholder",
            Self::Alt => "alt text",
            Self::Title => "title",
            Self::TestId => "test id",
        }
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use serde_json::json;

    use super::By;

    #[test]
    fn a_literal_is_borrowed_and_a_string_is_moved() {
        assert!(matches!(By::css("form").value, Cow::Borrowed("form")));
        assert!(matches!(By::id(String::from("main")).value, Cow::Owned(_)));
    }

    #[test]
    fn a_search_reaches_the_page_as_its_strategy_and_value() {
        assert_eq!(
            serde_json::to_value(By::text_containing("Sign in")).unwrap(),
            json!({"strategy": "text_containing", "value": "Sign in"})
        );
        assert_eq!(
            serde_json::to_value(By::test_id("save")).unwrap(),
            json!({"strategy": "test_id", "value": "save"})
        );
    }

    #[test]
    fn a_search_shows_as_its_kind_and_quoted_value() {
        assert_eq!(By::css("a[href]").to_string(), "css \"a[href]\"");
        assert_eq!(
            By::link_containing("More").to_string(),
            "link containing \"More\""
        );
    }
}
