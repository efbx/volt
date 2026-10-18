use std::future::Future;
use std::path::Path;

use futures::StreamExt;
use tokio::task::JoinHandle;

use super::profile::TempProfile;
use super::{Browser, Error, Options, Page};

/// A running browser and the task driving its event loop, which stops when the session
/// is closed or dropped.
#[derive(Debug)]
pub struct Session {
    browser: Browser,
    events: JoinHandle<()>,
}

impl Session {
    /// Launches the browser described by `options` and starts its event loop on the tokio
    /// runtime.
    pub async fn launch(options: Options) -> Result<Self, Error> {
        Self::launch_with_profile(&options, None).await
    }

    /// Launches a browser with a temporary profile, hands one blank page to `extractor`,
    /// closes the browser and returns what `extractor` returned.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Timeout`] when opening the page and running `extractor` together
    /// take longer than `options.timeout`, besides any launch or page error.
    pub async fn run<T, F, Fut>(options: Options, extractor: F) -> Result<T, Error>
    where
        F: FnOnce(Page) -> Fut,
        Fut: Future<Output = Result<T, Error>>,
    {
        let profile = TempProfile::create();
        let session = Self::launch_with_profile(&options, Some(profile.path())).await?;
        let outcome = tokio::time::timeout(options.timeout, async {
            let page = session.new_page().await?;
            extractor(page).await
        })
        .await;
        session.close().await;
        outcome.map_err(|_| Error::Timeout(options.timeout))?
    }

    /// Launches the browser with `profile` as its user data directory when given, and
    /// spawns the task that drives its events.
    async fn launch_with_profile(options: &Options, profile: Option<&Path>) -> Result<Self, Error> {
        let (browser, mut handler) = Browser::launch(options.build_config(profile)?).await?;
        let events = tokio::spawn(async move { while handler.next().await.is_some() {} });
        Ok(Self { browser, events })
    }

    /// Returns the chromiumoxide browser handle.
    #[must_use]
    pub fn browser(&self) -> &Browser {
        &self.browser
    }

    /// Opens a new tab on `about:blank`.
    pub async fn new_page(&self) -> Result<Page, Error> {
        Ok(self.browser.new_page("about:blank").await?)
    }

    /// Stops the event loop, waits for it to end and shuts the browser down.
    pub async fn close(mut self) {
        self.events.abort();
        let _ = (&mut self.events).await;
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.events.abort();
    }
}
