use std::fmt::{self, Debug};
use std::future::Future;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use tokio::sync::Mutex;
use volt_browser::{Page, Session, Settings};
use wreq::cookie::Jar;

use super::solver::{Action, ActionFuture, Browser, Detect, Detector, Launch, Launching};

/// Builds a [`Browser`] around the cookie jar its solved cookies go into and the browser it
/// solves in.
pub struct BrowserBuilder {
    jar: Arc<Jar>,
    cookie_domain: String,
    cookie_host: String,
    recovery_url: Option<String>,
    detect: Detector,
    action: Action,
    launch: Launch,
    headed: bool,
    timeout: Duration,
}

impl Debug for BrowserBuilder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BrowserBuilder")
            .field("cookie_domain", &self.cookie_domain)
            .field("cookie_host", &self.cookie_host)
            .field("recovery_url", &self.recovery_url)
            .field("headed", &self.headed)
            .field("timeout", &self.timeout)
            .finish_non_exhaustive()
    }
}

impl BrowserBuilder {
    /// Returns a builder that adds cookies to `jar` and solves in the browser `launch` starts,
    /// which is any engine of volt-browser: `|settings| Chrome::default().launch(settings)` for
    /// volt-chrome, `|settings| Webview.launch(settings)` for volt-webview. It treats `403`
    /// responses as challenges, runs no action, opens a visible window and allows a solve 120 s.
    pub fn new<L, Fut>(jar: Arc<Jar>, launch: L) -> Self
    where
        L: Fn(Settings) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Session, volt_browser::Error>> + Send + 'static,
    {
        Self {
            jar,
            cookie_domain: String::new(),
            cookie_host: String::new(),
            recovery_url: None,
            detect: Detect::forbidden,
            action: Arc::new(|_page| Box::pin(async { Ok(()) })),
            launch: Arc::new(move |settings| Box::pin(launch(settings)) as Launching),
            headed: true,
            timeout: Duration::from_secs(120),
        }
    }

    /// Sets the text a cookie's domain must contain to be copied, and the `Domain` it is stored
    /// with. A leading dot does not count for the match. Defaults to empty.
    #[must_use]
    pub fn cookie_domain(mut self, domain: impl Into<String>) -> Self {
        self.cookie_domain = domain.into();
        self
    }

    /// Sets the URL the cookies are stored as coming from. Defaults to the scheme and host of the
    /// page the browser solved.
    #[must_use]
    pub fn cookie_host(mut self, host: impl Into<String>) -> Self {
        self.cookie_host = host.into();
        self
    }

    /// Sets the page the browser opens to solve a challenge, instead of the URL of the detected
    /// response. Required by [`Browser::recover_now`].
    #[must_use]
    pub fn recovery_url(mut self, url: impl Into<String>) -> Self {
        self.recovery_url = Some(url.into());
        self
    }

    /// Sets the detector that decides which responses are challenges. Defaults to
    /// [`Detect::forbidden`].
    #[must_use]
    pub fn detect(mut self, detect: Detector) -> Self {
        self.detect = detect;
        self
    }

    /// Sets the steps run on the loaded page before its cookies are read; an error fails the
    /// solve. Defaults to doing nothing.
    #[must_use]
    pub fn action<F, Fut>(mut self, action: F) -> Self
    where
        F: Fn(Page) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<(), volt_browser::Error>> + Send + 'static,
    {
        self.action = Arc::new(move |page| Box::pin(action(page)) as ActionFuture);
        self
    }

    /// Sets whether the browser opens a visible window. Defaults to `true`.
    #[must_use]
    pub fn headed(mut self, headed: bool) -> Self {
        self.headed = headed;
        self
    }

    /// Sets the most the browser's launch may take, and then the most the solve may take, before
    /// it fails. Defaults to 120 s.
    #[must_use]
    pub fn solve_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Returns the configured [`Browser`], with no user agent captured yet.
    pub fn build(self) -> Browser {
        Browser {
            jar: self.jar,
            cookie_domain: self.cookie_domain,
            cookie_host: self.cookie_host,
            recovery_url: self.recovery_url,
            detect: self.detect,
            action: self.action,
            launch: self.launch,
            headed: self.headed,
            timeout: self.timeout,
            user_agent: RwLock::new(None),
            solve_lock: Mutex::new(()),
            generation: AtomicU64::new(0),
        }
    }
}
