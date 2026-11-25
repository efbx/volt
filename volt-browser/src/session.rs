use std::future::Future;
use std::sync::Arc;

use tokio::runtime::Handle;
use tokio::time;

use crate::{Browser, Error, Page, Settings};

/// A running browser of any engine, and the settings its pages follow. An engine's crate hands
/// one out from its own launch call; dropping it leaves the cleaning up to the engine.
#[derive(Debug)]
pub struct Session {
    /// The browser.
    browser: Box<dyn Browser>,
    /// What every command of the session's pages follows.
    settings: Arc<Settings>,
    /// The runtime the session was made in, whose timers its waiting commands use.
    runtime: Handle,
}

impl Session {
    /// Returns a session over the launched `browser`, whose pages follow `settings`. It has to
    /// be made inside a Tokio runtime, and fails with [`Error::NoRuntime`] outside one.
    pub fn new(browser: impl Browser + 'static, settings: Settings) -> Result<Self, Error> {
        let runtime = Handle::try_current().map_err(|_| Error::NoRuntime)?;
        Ok(Self {
            browser: Box::new(browser),
            settings: Arc::new(settings),
            runtime,
        })
    }

    /// What every command of the session's pages follows.
    #[must_use]
    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// Opens a new page on `about:blank`.
    pub async fn new_page(&self) -> Result<Page, Error> {
        let engine = self.browser.open().await?;
        Ok(Page::new(
            engine,
            Arc::clone(&self.settings),
            self.runtime.clone(),
        ))
    }

    /// Hands one new page to `job`, closes the browser and returns what `job` returned.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Timeout`] when opening the page and running `job` together take longer
    /// than [`Settings::timeout`], besides any error of the page or of `job`. After a job that
    /// succeeded the call waits for [`close`](Self::close); after one that failed or timed out,
    /// which may mean a browser too busy to answer, it returns at once and leaves the closing to
    /// the engine.
    pub async fn run<T, F, Fut>(self, job: F) -> Result<T, Error>
    where
        F: FnOnce(Page) -> Fut,
        Fut: Future<Output = Result<T, Error>>,
    {
        let timeout = self.settings.timeout;
        let work = async {
            let page = self.new_page().await?;
            job(page).await
        };
        let limited = {
            let _entered = self.runtime.enter();
            time::timeout(timeout, work)
        };
        match limited.await {
            Ok(Ok(value)) => {
                self.close().await;
                Ok(value)
            }
            Ok(Err(error)) => Err(error),
            Err(_) => Err(Error::Timeout(timeout)),
        }
    }

    /// Closes every page and the browser.
    pub async fn close(self) {
        self.browser.close().await;
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::time::Duration;

    use async_trait::async_trait;
    use serde_json::json;

    use super::Session;
    use crate::fake::Fake;
    use crate::{Browser, By, Engine, Error, Settings};

    /// A browser whose pages are fake engines, and which notes what was asked of it.
    #[derive(Debug, Default)]
    struct Stage {
        /// How many pages were opened.
        opened: Arc<AtomicUsize>,
        /// Whether the browser was closed.
        closed: Arc<AtomicBool>,
        /// Whether the browser was dropped.
        dropped: Arc<AtomicBool>,
    }

    impl Drop for Stage {
        fn drop(&mut self) {
            self.dropped.store(true, Ordering::Relaxed);
        }
    }

    #[async_trait]
    impl Browser for Stage {
        async fn open(&self) -> Result<Arc<dyn Engine>, Error> {
            self.opened.fetch_add(1, Ordering::Relaxed);
            Ok(Fake::repeating(
                json!({"status": "ok", "json": "\"Stage\""}),
            ))
        }

        async fn close(&self) {
            self.closed.store(true, Ordering::Relaxed);
        }
    }

    #[tokio::test]
    async fn a_session_opens_pages_that_follow_its_settings() {
        let stage = Stage::default();
        let opened = Arc::clone(&stage.opened);
        let settings = Settings {
            timeout: Duration::from_millis(250),
            ..Settings::default()
        };
        let session = Session::new(stage, settings).unwrap();
        assert_eq!(session.settings().timeout, Duration::from_millis(250));
        let page = session.new_page().await.unwrap();
        assert_eq!(page.settings().timeout, Duration::from_millis(250));
        assert_eq!(page.title().await.unwrap(), "Stage");
        session.new_page().await.unwrap();
        assert_eq!(opened.load(Ordering::Relaxed), 2);
    }

    #[tokio::test]
    async fn run_hands_over_one_page_and_closes_the_browser() {
        let stage = Stage::default();
        let closed = Arc::clone(&stage.closed);
        let session = Session::new(stage, Settings::default()).unwrap();
        let title = session
            .run(|page| async move { page.find(By::tag("h1")).text().await })
            .await
            .unwrap();
        assert_eq!(title, "Stage");
        assert!(closed.load(Ordering::Relaxed));
    }

    #[tokio::test]
    async fn a_job_that_takes_too_long_times_out_and_the_browser_is_let_go_at_once() {
        let stage = Stage::default();
        let closed = Arc::clone(&stage.closed);
        let dropped = Arc::clone(&stage.dropped);
        let settings = Settings {
            timeout: Duration::from_millis(30),
            ..Settings::default()
        };
        let session = Session::new(stage, settings).unwrap();
        let outcome = session
            .run(|_page| async {
                tokio::time::sleep(Duration::from_secs(5)).await;
                Ok(())
            })
            .await;
        assert!(matches!(
            outcome,
            Err(Error::Timeout(timeout)) if timeout == Duration::from_millis(30)
        ));
        assert!(!closed.load(Ordering::Relaxed));
        assert!(dropped.load(Ordering::Relaxed));
    }

    #[tokio::test]
    async fn a_failing_job_gives_its_error_and_the_browser_is_let_go_at_once() {
        let stage = Stage::default();
        let closed = Arc::clone(&stage.closed);
        let dropped = Arc::clone(&stage.dropped);
        let session = Session::new(stage, Settings::default()).unwrap();
        let outcome: Result<(), Error> =
            session.run(|_page| async { Err(Error::InvalidUrl) }).await;
        assert!(matches!(outcome, Err(Error::InvalidUrl)));
        assert!(!closed.load(Ordering::Relaxed));
        assert!(dropped.load(Ordering::Relaxed));
    }

    #[test]
    fn a_session_outside_a_runtime_is_refused() {
        assert!(matches!(
            Session::new(Stage::default(), Settings::default()),
            Err(Error::NoRuntime)
        ));
    }
}
