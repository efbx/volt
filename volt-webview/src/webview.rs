use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use volt_browser::{Browser, Engine, Session, Settings};

use crate::Error;
use crate::link::Link;
use crate::tab::Tab;

/// How long closing waits for the session's windows to close.
const CLOSE_GRACE: Duration = Duration::from_secs(3);

/// The system webview as a browser for the commands of [`volt_browser`]: launch it and get a
/// [`Session`], whose pages take every selector, action and storage command.
///
/// Each page is a window of its own inside this program, opened from any thread or tokio task
/// and hidden unless [`Settings::visible`] is set. Every page opens in an incognito store. On
/// macOS each page has a store of its own and on Linux and the BSDs the session's pages share one,
/// so no cookie outlives the session there; on Windows every session of the program shares one
/// for as long as `WebView2`'s browser process runs, so a cookie can outlive its session, and
/// `WebView2`'s data folder, named after the program, stays in `%TEMP%` for its next run.
/// Dropping the session closes its pages without waiting.
///
/// On macOS every window lives on the main thread, so a call from another task is served only
/// while the main thread runs an event loop or is itself inside one of this crate's awaits. A
/// program that runs no event loop there, as a terminal program is, can show a page to a person
/// all the same: its first shown window makes it an accessory program, without a Dock icon or a
/// menu bar, and comes in front of the other programs' windows. Such a window is redrawn and
/// takes the mouse and the keyboard only while the main thread is inside one of this crate's
/// awaits, so a wait on the page keeps it responding, the more smoothly the shorter
/// [`Settings::poll`] is.
///
/// Every failure is a [`volt_browser::Error`]; what only a webview can fail with is its
/// [`Engine`](volt_browser::Error::Engine) kind, with the reason as its message. A failed load
/// ends as the platform reports it: `WKWebView` reports no end, so on macOS the `goto` fails with
/// [`volt_browser::Error::Timeout`]; `WebKitGTK` reports the failure, so on Linux and the BSDs
/// the `goto` fails with the `Engine` kind; `WebView2` shows its own error page instead, which
/// the `goto` finds, so on Windows it fails with the `Engine` kind too, naming the page's error
/// code. A later `goto` on the same page replaces one still waiting, which then fails with the
/// `Engine` kind as well, and so does, on macOS, Linux and the BSDs, a `goto` whose page loses
/// its web content process.
#[derive(Clone, Copy, Debug, Default)]
pub struct Webview;

impl Webview {
    /// Reaches the thread that may touch webviews here and returns the session over it, waiting
    /// at most [`Settings::timeout`]; on macOS a launch from another thread can take up to a
    /// second longer, because it first looks for the main thread's event loop for up to a second.
    /// It must be awaited inside a Tokio runtime with timers; the session's pages then work from
    /// any executor while that runtime runs.
    ///
    /// # Errors
    ///
    /// A failure of the webview itself comes back as [`volt_browser::Error::Engine`], with the
    /// reason as its message: on a target without a system webview, which Linux and the BSDs are
    /// without the `webkitgtk` feature; when that thread cannot be reached, which on macOS is when
    /// the main thread runs no event loop and the call is not awaited on the main thread, as in a
    /// terminal program's tasks or a test under the default harness, and on Linux and the BSDs is
    /// without a display; and on macOS when the call blocks the main thread from inside its event
    /// loop. Outside a Tokio runtime it fails with [`volt_browser::Error::NoRuntime`], and with
    /// [`volt_browser::Error::Timeout`] when that thread does not answer in time. On Windows a
    /// missing `WebView2` Runtime shows only when opening a page fails.
    pub async fn launch(self, settings: Settings) -> Result<Session, volt_browser::Error> {
        let launched = Launched::start(&settings).await?;
        Session::new(launched, settings)
    }
}

/// The pages of one launched session, on the thread that may touch their windows.
#[derive(Debug)]
pub(crate) struct Launched {
    /// The way to the session's pages, shared with every page.
    link: Arc<Link>,
    /// The id the next page gets.
    next_page: AtomicU64,
}

impl Launched {
    /// Reaches the thread that may touch webviews and puts the session's pages there, waiting at
    /// most `settings.timeout`.
    pub(crate) async fn start(settings: &Settings) -> Result<Self, Error> {
        let link = Link::start(settings).await?;
        Ok(Self {
            link: Arc::new(link),
            next_page: AtomicU64::new(1),
        })
    }
}

#[async_trait]
impl Browser for Launched {
    async fn open(&self) -> Result<Arc<dyn Engine>, volt_browser::Error> {
        let id = self.next_page.fetch_add(1, Ordering::Relaxed);
        self.link
            .call(move |pages, reply| pages.open(id, reply))
            .await?;
        Ok(Arc::new(Tab::new(Arc::clone(&self.link), id)))
    }

    /// Closes every page of the session and waits, a few seconds at most, until their windows are
    /// gone; calls through its pages then fail with [`Error::Closed`].
    async fn close(&self) {
        let _ = self
            .link
            .call_within(CLOSE_GRACE, |pages, reply| {
                pages.close_all();
                let _ = reply.send(Ok(()));
            })
            .await;
    }
}

impl Drop for Launched {
    fn drop(&mut self) {
        self.link.close_all();
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use futures::executor;
    use tokio::runtime::Builder;
    use tokio::sync::oneshot;
    use url::Url;
    use volt_browser::{Browser, Settings};

    use super::{Launched, Webview};
    use crate::Error;
    use crate::platform::Fake;

    /// Settings for sessions on the fake webview, with a short timeout.
    struct Quick;

    impl Quick {
        /// How long calls on the fake webview wait.
        const TIMEOUT: Duration = Duration::from_millis(200);

        /// Returns settings with the short timeout.
        fn settings() -> Settings {
            Settings {
                timeout: Self::TIMEOUT,
                ..Settings::default()
            }
        }

        /// Returns `url` parsed.
        fn url(url: &str) -> Url {
            Url::parse(url).unwrap()
        }
    }

    #[tokio::test]
    async fn a_launched_webview_opens_a_page_that_has_loaded() {
        let launched = Launched::start(&Quick::settings()).await.unwrap();
        let page = launched.open().await.unwrap();

        page.goto(&Quick::url("https://example.com/"))
            .await
            .unwrap();
        launched.close().await;
    }

    #[test]
    fn a_webview_outside_a_tokio_runtime_fails_to_launch() {
        let started = executor::block_on(Launched::start(&Quick::settings()));
        assert!(matches!(started, Err(Error::NoRuntime)));

        let launched = executor::block_on(Webview.launch(Quick::settings()));
        assert!(matches!(launched, Err(volt_browser::Error::NoRuntime)));
    }

    #[tokio::test]
    async fn a_page_that_cannot_open_fails_with_the_webviews_reason() {
        let settings = Settings {
            user_agent: Some(Fake::REFUSED_AGENT.to_owned()),
            ..Quick::settings()
        };
        let session = Webview.launch(settings).await.unwrap();
        let opened = session.new_page().await;
        assert!(matches!(
            &opened,
            Err(volt_browser::Error::Engine(cause))
                if matches!(cause.downcast_ref(), Some(Error::Webview(_)))
        ));
    }

    #[tokio::test]
    async fn a_session_drives_its_pages_through_the_shared_commands() {
        let session = Webview.launch(Quick::settings()).await.unwrap();
        let page = session.new_page().await.unwrap();

        page.goto("https://example.com/").await.unwrap();
        let cookies = page.cookies_for("https://example.com/").await.unwrap();
        assert_eq!(cookies, [Fake::cookie()]);
        page.show().await.unwrap();
        page.hide().await.unwrap();
        session.close().await;
        assert!(matches!(
            page.goto("https://example.com/").await,
            Err(volt_browser::Error::Closed)
        ));
    }

    #[tokio::test]
    async fn run_hands_its_job_a_page_and_returns_the_result() {
        let session = Webview.launch(Quick::settings()).await.unwrap();
        let cookies = session
            .run(|page| async move {
                page.goto("https://example.com/").await?;
                page.cookies_for("https://example.com/").await
            })
            .await;

        assert_eq!(cookies.unwrap(), [Fake::cookie()]);
    }

    #[tokio::test]
    async fn a_job_that_outlasts_the_timeout_fails_and_closes_the_session() {
        let (smuggle, smuggled) = oneshot::channel();
        let session = Webview.launch(Quick::settings()).await.unwrap();
        let result = session
            .run(|page| async move {
                let _ = smuggle.send(page);
                tokio::time::sleep(Duration::from_secs(5)).await;
                Ok(())
            })
            .await;

        assert!(matches!(
            result,
            Err(volt_browser::Error::Timeout(timeout)) if timeout == Quick::TIMEOUT
        ));
        let page = smuggled.await.unwrap();
        assert!(matches!(
            page.goto("https://example.com/").await,
            Err(volt_browser::Error::Closed)
        ));
    }

    #[tokio::test]
    async fn run_returns_at_its_timeout_while_the_ui_thread_is_busy() {
        let start = Instant::now();
        let session = Webview.launch(Quick::settings()).await.unwrap();
        let result = session
            .run(|page| async move { page.goto(Fake::BUSY_URL).await })
            .await;

        assert!(matches!(result, Err(volt_browser::Error::Timeout(_))));
        assert!(start.elapsed() < Fake::BUSY, "{:?}", start.elapsed());
    }

    #[tokio::test]
    async fn dropping_the_launched_webview_closes_its_pages() {
        let launched = Launched::start(&Quick::settings()).await.unwrap();
        let page = launched.open().await.unwrap();

        drop(launched);
        assert!(matches!(
            page.goto(&Quick::url("https://example.com/")).await,
            Err(volt_browser::Error::Closed)
        ));
    }

    #[tokio::test]
    async fn closing_fails_a_goto_still_waiting() {
        let launched = Launched::start(&Quick::settings()).await.unwrap();
        let stuck = launched.open().await.unwrap();
        let waiting = tokio::spawn(async move { stuck.goto(&Quick::url(Fake::STUCK_URL)).await });
        tokio::time::sleep(Duration::from_millis(20)).await;

        launched.close().await;
        assert!(matches!(
            waiting.await.unwrap(),
            Err(volt_browser::Error::Closed)
        ));
    }

    #[test]
    fn a_page_works_from_another_executor_while_its_runtime_runs() {
        let runtime = Builder::new_multi_thread().enable_all().build().unwrap();
        let launched = runtime
            .block_on(Launched::start(&Quick::settings()))
            .unwrap();
        let page = runtime.block_on(launched.open()).unwrap();

        assert_eq!(executor::block_on(page.evaluate("2")).unwrap(), 2);
        assert!(matches!(
            executor::block_on(page.evaluate(Fake::SILENT)),
            Err(volt_browser::Error::Timeout(_))
        ));
    }
}
