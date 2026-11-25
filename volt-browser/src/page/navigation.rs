use serde::{Deserialize, Serialize};
use url::Url;

use super::Page;
use crate::Error;
use crate::page_script::Op;

/// How far a document has loaded.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Load {
    /// The HTML is read and the page's scripts can run; images may still be loading.
    Interactive,
    /// Everything the document asked for has loaded.
    Complete,
}

/// A way to recognise the page's URL.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Match<'a> {
    /// The URL is exactly this.
    Exact(&'a str),
    /// The URL starts with this.
    Prefix(&'a str),
    /// The URL contains this.
    Containing(&'a str),
}

/// What tells one document of a page from the next.
#[derive(Debug, Serialize, Deserialize)]
struct Mark {
    /// The number the page keeps on the marked document, and on no other.
    token: u64,
    /// The document's URL.
    url: String,
}

impl Page {
    /// Loads `url` and returns once that load has ended.
    pub async fn goto(&self, url: &str) -> Result<(), Error> {
        let url = Url::parse(url).map_err(|_| Error::InvalidUrl)?;
        self.engine().goto(&url).await
    }

    /// Reloads the page and returns once it has loaded again.
    pub async fn reload(&self) -> Result<(), Error> {
        self.travel(Op::Reload).await
    }

    /// Goes one step back in the history and returns once the page it leads to has loaded; from
    /// the first page a new page loaded, that is `about:blank`. Where the page can see that
    /// there is no step back, as on a new page that has loaded nothing, nothing happens. A page
    /// cannot see the history of other sites, so elsewhere the step is taken, and one that does
    /// not change the page, because nothing is behind it or because it only takes a frame back,
    /// ends with [`Error::Timeout`].
    pub async fn back(&self) -> Result<(), Error> {
        self.travel(Op::Back).await
    }

    /// Goes one step forward in the history and returns once the page it leads to has loaded.
    /// Where the page can see that there is no step forward, as on a new page that has loaded
    /// nothing, nothing happens. A page cannot see the history of other sites, so elsewhere the
    /// step is taken, and one that does not change the page, because nothing is ahead of it or
    /// because it only takes a frame forward, ends with [`Error::Timeout`].
    pub async fn forward(&self) -> Result<(), Error> {
        self.travel(Op::Forward).await
    }

    /// Stops the load under way.
    pub async fn stop(&self) -> Result<(), Error> {
        self.ask(Op::Stop, &()).await
    }

    /// The page's current URL.
    pub async fn url(&self) -> Result<Url, Error> {
        let url: String = self.ask(Op::Url, &()).await?;
        Url::parse(&url).map_err(|_| Error::InvalidUrl)
    }

    /// The page's title.
    pub async fn title(&self) -> Result<String, Error> {
        self.ask(Op::Title, &()).await
    }

    /// The page's HTML as it stands now.
    pub async fn html(&self) -> Result<String, Error> {
        self.ask(Op::Source, &()).await
    }

    /// Replaces the page's document with `html` and returns once it has loaded.
    pub async fn set_html(&self, html: &str) -> Result<(), Error> {
        self.ask::<(), _>(Op::Write, html).await?;
        self.wait_for_load(Load::Complete).await
    }

    /// Waits until the document has loaded as far as `state`.
    pub async fn wait_for_load(&self, state: Load) -> Result<(), Error> {
        self.until(Op::Loaded, &state).await
    }

    /// Waits until the page's URL fits `url`, as after a click that leads somewhere.
    pub async fn wait_for_url(&self, url: Match<'_>) -> Result<(), Error> {
        self.until(Op::At, &url).await
    }

    /// Has the page leave its document by `op` and waits for the next one to load. A call the
    /// leaving cuts off counts as having left.
    async fn travel(&self, op: Op) -> Result<(), Error> {
        let mark: Mark = self.ask(Op::Mark, &()).await?;
        let leaving = match self.ask(op, &()).await {
            Ok(leaving) => leaving,
            Err(Error::Engine(_) | Error::Detached) => true,
            Err(error) => return Err(error),
        };
        if leaving {
            self.until(Op::Moved, &mark).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::{Load, Match};
    use crate::Error;
    use crate::fake::Fake;

    #[tokio::test]
    async fn a_url_that_does_not_parse_fails_without_reaching_the_engine() {
        let fake = Fake::answering([]);
        let page = fake.page();
        assert!(matches!(
            page.goto("example.com").await,
            Err(Error::InvalidUrl)
        ));
        assert_eq!(fake.visited(), [""; 0]);
        page.goto("https://example.com/a?b=c").await.unwrap();
        assert_eq!(fake.visited(), ["https://example.com/a?b=c"]);
    }

    #[tokio::test]
    async fn the_pages_own_facts_are_read_by_their_commands() {
        let fake = Fake::answering([
            json!({"status": "ok", "json": "\"https://example.com/shelf\""}),
            json!({"status": "ok", "json": "\"Shelf\""}),
            json!({"status": "ok", "json": "\"<html></html>\""}),
        ]);
        let page = fake.page();
        assert_eq!(page.url().await.unwrap().path(), "/shelf");
        assert_eq!(page.title().await.unwrap(), "Shelf");
        assert_eq!(page.html().await.unwrap(), "<html></html>");
        let commands = fake.commands();
        let asked: Vec<_> = commands.iter().map(|command| &command["op"]).collect();
        assert_eq!(asked, ["url", "title", "source"]);
    }

    #[tokio::test]
    async fn a_reload_waits_until_the_marked_document_is_left() {
        let mark = json!({"token": 3, "url": "https://example.com/"});
        let fake = Fake::answering([
            json!({"status": "ok", "json": mark.to_string()}),
            json!({"status": "ok", "json": "true"}),
            json!({"status": "wait", "blocker": "unmet", "rect": null}),
            Value::Null,
            json!({"status": "ok", "json": null}),
        ]);
        fake.page().reload().await.unwrap();
        let commands = fake.commands();
        let asked: Vec<_> = commands.iter().map(|command| &command["op"]).collect();
        assert_eq!(asked, ["mark", "reload", "moved", "moved", "moved"]);
        assert_eq!(commands[2]["arg"], mark);
    }

    #[tokio::test]
    async fn going_back_with_nowhere_to_go_does_not_wait() {
        let fake = Fake::answering([
            json!({"status": "ok", "json": "{\"token\":1,\"url\":\"about:blank\"}"}),
            json!({"status": "ok", "json": "false"}),
        ]);
        fake.page().back().await.unwrap();
        assert_eq!(fake.commands().len(), 2);
    }

    #[tokio::test]
    async fn a_leaving_call_that_was_cut_off_still_waits_for_the_next_document() {
        let fake = Fake::answering([
            json!({"status": "ok", "json": "{\"token\":1,\"url\":\"https://example.com/b\"}"}),
            Value::Null,
            json!({"status": "ok", "json": null}),
        ]);
        fake.page().forward().await.unwrap();
        let commands = fake.commands();
        let asked: Vec<_> = commands.iter().map(|command| &command["op"]).collect();
        assert_eq!(asked, ["mark", "forward", "moved"]);
    }

    #[tokio::test]
    async fn new_html_is_written_and_its_load_awaited() {
        let fake = Fake::answering([
            json!({"status": "ok", "json": null}),
            json!({"status": "ok", "json": null}),
        ]);
        fake.page().set_html("<p>hi</p>").await.unwrap();
        let commands = fake.commands();
        assert_eq!(
            (&commands[0]["op"], &commands[0]["arg"]),
            (&json!("write"), &json!("<p>hi</p>"))
        );
        assert_eq!(
            (&commands[1]["op"], &commands[1]["arg"]),
            (&json!("loaded"), &json!("complete"))
        );
    }

    #[tokio::test]
    async fn waits_send_the_state_and_the_url_to_recognise() {
        let fake = Fake::answering([
            json!({"status": "ok", "json": null}),
            json!({"status": "ok", "json": null}),
        ]);
        let page = fake.page();
        page.wait_for_load(Load::Interactive).await.unwrap();
        page.wait_for_url(Match::Prefix("https://example.com/in"))
            .await
            .unwrap();
        let commands = fake.commands();
        assert_eq!(commands[0]["arg"], "interactive");
        assert_eq!(
            commands[1]["arg"],
            json!({"prefix": "https://example.com/in"})
        );
    }
}
