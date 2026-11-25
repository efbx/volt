use std::path::Path;
use std::time::Duration;

use chromiumoxide::Page;
use chromiumoxide::browser::Browser;
use futures::StreamExt;
use tokio::task::JoinHandle;
use tokio::time;

use super::error::Error;
use super::options::Options;

/// How long quitting waits for the browser's process to end.
const QUIT_GRACE: Duration = Duration::from_secs(5);

/// A running browser and the task driving its event loop, which stops when the session
/// is quit or dropped.
#[derive(Debug)]
pub(super) struct Session {
    browser: Browser,
    events: JoinHandle<()>,
}

impl Session {
    /// Launches the browser described by `options` with `profile` as its user data directory,
    /// and spawns the task that drives its events on the tokio runtime.
    pub(super) async fn launch(options: &Options, profile: &Path) -> Result<Self, Error> {
        let config = options.build_config(profile)?;
        let (browser, mut handler) = Browser::launch(config).await?;
        let events = tokio::spawn(async move { while handler.next().await.is_some() {} });
        Ok(Self { browser, events })
    }

    /// Opens a new tab on `about:blank`.
    pub(super) async fn new_page(&self) -> Result<Page, Error> {
        Ok(self.browser.new_page("about:blank").await?)
    }

    /// Asks the browser to quit and waits, a few seconds at most, until its process has ended,
    /// so that nothing writes to its profile any more; then stops the event loop.
    pub(super) async fn quit(mut self) {
        let _ = self.browser.close().await;
        let _ = time::timeout(QUIT_GRACE, self.browser.wait()).await;
        self.events.abort();
        let _ = (&mut self.events).await;
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.events.abort();
    }
}
