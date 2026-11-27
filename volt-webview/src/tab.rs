use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use url::Url;
use volt_browser::{Cookie, Engine};

#[cfg(any(test, target_os = "windows"))]
use crate::Error;
#[cfg(any(test, target_os = "windows"))]
use crate::error_page::ErrorPage;
use crate::link::Link;
use crate::script::Script;

/// A window in the system webview and the page it shows, as one page of [`volt_browser`].
/// Dropping it leaves the window open until [`close`](Engine::close) or the end of the session.
/// A person who closes a shown window only hides it: the page goes on, and showing it brings it
/// back.
#[derive(Debug)]
pub(crate) struct Tab {
    /// The way to the session's pages.
    link: Arc<Link>,
    /// The session's id for the page.
    id: u64,
}

impl Tab {
    /// Returns the page the session knows as `id`.
    pub(crate) fn new(link: Arc<Link>, id: u64) -> Self {
        Self { link, id }
    }

    /// Fails with the load's failure when the page a load just ended on is the error page
    /// `WebView2` shows in place of one it could not load.
    #[cfg(any(test, target_os = "windows"))]
    async fn landed(&self) -> Result<(), Error> {
        let id = self.id;
        let shown = self
            .link
            .call(move |pages, reply| pages.evaluate(id, ErrorPage::PROBE, reply))
            .await?;
        ErrorPage::check(shown)
    }
}

#[async_trait]
impl Engine for Tab {
    /// Loads `url` and waits for the end of that load. `WebView2` ends a load it could not make
    /// as a finished one, on its own error page, so on Windows the page is then asked whether it
    /// is that page.
    async fn goto(&self, url: &Url) -> Result<(), volt_browser::Error> {
        let id = self.id;
        let url = url.as_str().to_owned();
        self.link
            .call(move |pages, reply| pages.goto(id, &url, reply))
            .await?;
        #[cfg(any(test, target_os = "windows"))]
        self.landed().await?;
        Ok(())
    }

    /// Evaluates the expression `script` and returns its value as JSON, as the page's own
    /// `JSON.stringify` makes it: `undefined`, a function, a value JSON cannot hold and an
    /// expression that throws give [`Value::Null`].
    async fn evaluate(&self, script: &str) -> Result<Value, volt_browser::Error> {
        let id = self.id;
        let script = Script::wrap(script);
        Ok(self
            .link
            .call(move |pages, reply| pages.evaluate(id, &script, reply))
            .await?)
    }

    async fn cookies(&self, url: &Url) -> Result<Vec<Cookie>, volt_browser::Error> {
        let id = self.id;
        let url = url.clone();
        Ok(self
            .link
            .call(move |pages, reply| pages.cookies(id, &url, reply))
            .await?)
    }

    async fn set_cookie(&self, cookie: &Cookie) -> Result<(), volt_browser::Error> {
        let id = self.id;
        let cookie = cookie.clone();
        Ok(self
            .link
            .call(move |pages, reply| pages.set_cookie(id, &cookie, reply))
            .await?)
    }

    async fn delete_cookie(&self, cookie: &Cookie) -> Result<(), volt_browser::Error> {
        let id = self.id;
        let cookie = cookie.clone();
        Ok(self
            .link
            .call(move |pages, reply| pages.delete_cookie(id, &cookie, reply))
            .await?)
    }

    async fn set_visible(&self, visible: bool) -> Result<(), volt_browser::Error> {
        let id = self.id;
        Ok(self
            .link
            .ask(move |pages| pages.set_visible(id, visible))
            .await?)
    }

    async fn close(&self) -> Result<(), volt_browser::Error> {
        let id = self.id;
        Ok(self.link.ask(move |pages| pages.close(id)).await?)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use serde_json::{Value, json};
    use url::Url;
    use volt_browser::{Browser, Cookie, Engine, Settings};

    use crate::Error;
    use crate::platform::Fake;
    use crate::webview::Launched;

    /// A launched fake webview with one open page, and the timeout its calls get.
    struct Opened;

    impl Opened {
        /// How long calls on the fake webview wait.
        const TIMEOUT: Duration = Duration::from_millis(200);

        /// Returns a launched webview with a short timeout and its first page.
        async fn page() -> (Launched, Arc<dyn Engine>) {
            Self::page_within(Self::TIMEOUT).await
        }

        /// Returns a launched webview whose calls wait `timeout`, and its first page.
        async fn page_within(timeout: Duration) -> (Launched, Arc<dyn Engine>) {
            let settings = Settings {
                timeout,
                ..Settings::default()
            };
            let launched = Launched::start(&settings).await.unwrap();
            let page = launched.open().await.unwrap();
            (launched, page)
        }

        /// Returns the URL the fake store's cookie belongs to.
        fn site() -> Url {
            Url::parse("https://example.com/").unwrap()
        }

        /// Returns `url`, one of the fake's own.
        fn url(url: &str) -> Url {
            Url::parse(url).unwrap()
        }

        /// Whether `outcome` failed with the webview's own error that `is` picks.
        fn failed_with<T>(
            outcome: &Result<T, volt_browser::Error>,
            is: fn(&Error) -> bool,
        ) -> bool {
            matches!(
                outcome,
                Err(volt_browser::Error::Engine(cause)) if cause.downcast_ref().is_some_and(is)
            )
        }
    }

    #[tokio::test]
    async fn each_call_reaches_the_page_and_reads_its_answer() {
        let (launched, page) = Opened::page().await;

        page.goto(&Opened::site()).await.unwrap();
        assert_eq!(page.evaluate("2").await.unwrap(), 2);
        assert_eq!(
            page.evaluate(r#"{"title":"volt"}"#).await.unwrap(),
            json!({"title": "volt"})
        );
        assert_eq!(page.evaluate("undefined").await.unwrap(), Value::Null);
        assert_eq!(
            page.cookies(&Opened::site()).await.unwrap(),
            [Fake::cookie()]
        );
        page.set_visible(true).await.unwrap();
        page.set_visible(false).await.unwrap();
        launched.close().await;
    }

    #[tokio::test]
    async fn the_store_gives_the_cookies_of_a_url_and_takes_and_drops_them() {
        let (_launched, page) = Opened::page().await;
        let elsewhere = Url::parse("https://example.org/").unwrap();
        assert_eq!(page.cookies(&elsewhere).await.unwrap().len(), 0);

        let added = Cookie::new("theme", "dark", "example.org");
        page.set_cookie(&added).await.unwrap();
        assert_eq!(
            page.cookies(&elsewhere).await.unwrap(),
            std::slice::from_ref(&added)
        );
        page.delete_cookie(&added).await.unwrap();
        assert_eq!(page.cookies(&elsewhere).await.unwrap().len(), 0);
        assert_eq!(
            page.cookies(&Opened::site()).await.unwrap(),
            [Fake::cookie()]
        );
    }

    #[tokio::test]
    async fn a_goto_that_never_ends_times_out_and_the_next_one_works() {
        let (_launched, page) = Opened::page().await;

        assert!(matches!(
            page.goto(&Opened::url(Fake::STUCK_URL)).await,
            Err(volt_browser::Error::Timeout(timeout)) if timeout == Opened::TIMEOUT
        ));
        page.goto(&Opened::site()).await.unwrap();
    }

    #[tokio::test]
    async fn a_goto_still_waiting_fails_when_another_replaces_it() {
        let (_launched, page) = Opened::page().await;
        let stuck = Arc::clone(&page);
        let waiting = tokio::spawn(async move { stuck.goto(&Opened::url(Fake::STUCK_URL)).await });
        tokio::time::sleep(Duration::from_millis(20)).await;

        page.goto(&Opened::site()).await.unwrap();
        assert!(Opened::failed_with(&waiting.await.unwrap(), |error| {
            matches!(error, Error::Replaced)
        }));
    }

    #[tokio::test]
    async fn a_refused_url_fails_the_goto_and_leaves_the_page_usable() {
        let (_launched, page) = Opened::page().await;

        let refused = page.goto(&Opened::url(Fake::REFUSED_URL)).await;
        assert!(Opened::failed_with(&refused, |error| matches!(
            error,
            Error::Webview(_)
        )));
        page.goto(&Opened::site()).await.unwrap();
    }

    #[tokio::test]
    async fn a_goto_that_ends_on_the_webviews_error_page_fails_with_its_error_code() {
        let (_launched, page) = Opened::page().await;

        let failed = page.goto(&Opened::url(Fake::ERROR_PAGE_URL)).await;
        assert!(Opened::failed_with(&failed, |error| matches!(
            error,
            Error::LoadFailed(code) if code.to_string() == Fake::ERROR_CODE
        )));
        page.goto(&Opened::site()).await.unwrap();
    }

    #[tokio::test]
    async fn an_expression_never_answered_times_out() {
        let (_launched, page) = Opened::page().await;

        assert!(matches!(
            page.evaluate(Fake::SILENT).await,
            Err(volt_browser::Error::Timeout(_))
        ));
        assert_eq!(page.evaluate("2").await.unwrap(), 2);
    }

    #[tokio::test]
    async fn a_value_that_is_not_json_fails() {
        let (_launched, page) = Opened::page().await;

        assert!(matches!(
            page.evaluate("{").await,
            Err(volt_browser::Error::Json(_))
        ));
    }

    #[tokio::test]
    async fn a_panicking_webview_call_fails_and_the_page_keeps_working() {
        let (_launched, page) = Opened::page().await;

        assert!(matches!(
            page.evaluate(Fake::PANICKING).await,
            Err(volt_browser::Error::Closed)
        ));
        assert_eq!(page.evaluate("2").await.unwrap(), 2);
    }

    #[tokio::test]
    async fn a_closed_page_fails_every_later_call_through_its_clones() {
        let (_launched, page) = Opened::page().await;
        let same_page = Arc::clone(&page);

        page.close().await.unwrap();
        assert!(matches!(
            same_page.goto(&Opened::site()).await,
            Err(volt_browser::Error::Closed)
        ));
        assert!(matches!(
            same_page.cookies(&Opened::site()).await,
            Err(volt_browser::Error::Closed)
        ));
        assert!(matches!(
            same_page.close().await,
            Err(volt_browser::Error::Closed)
        ));
    }

    #[tokio::test]
    async fn a_cancelled_goto_leaves_the_page_usable() {
        let (_launched, page) = Opened::page().await;

        let stuck = Opened::url(Fake::STUCK_URL);
        let cancelled = tokio::time::timeout(Duration::from_millis(10), page.goto(&stuck)).await;
        assert!(cancelled.is_err());
        page.goto(&Opened::site()).await.unwrap();
    }

    #[tokio::test]
    async fn a_goto_whose_caller_gave_up_before_its_turn_never_replaces_a_live_one() {
        let (_launched, page) = Opened::page_within(Duration::from_secs(1)).await;
        let live = Arc::clone(&page);
        let waiting = tokio::spawn(async move { live.goto(&Opened::url(Fake::STUCK_URL)).await });
        tokio::time::sleep(Duration::from_millis(20)).await;
        let labored = Arc::clone(&page);
        let busy = tokio::spawn(async move { labored.evaluate(Fake::LABORED).await });
        tokio::time::sleep(Duration::from_millis(20)).await;

        let site = Opened::site();
        let abandoned = tokio::time::timeout(Duration::from_millis(50), page.goto(&site)).await;
        assert!(abandoned.is_err());
        assert_eq!(busy.await.unwrap().unwrap(), 1);
        assert!(matches!(
            waiting.await.unwrap(),
            Err(volt_browser::Error::Timeout(_))
        ));
    }
}
