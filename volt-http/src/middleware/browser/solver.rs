use std::borrow::Cow;
use std::fmt::{self, Debug, Write};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, PoisonError, RwLock};
use std::time::Duration;

use tokio::sync::Mutex;
use volt_browser::{Page, Session, Settings};
use wreq::cookie::Jar;
use wreq::header::{HeaderValue, USER_AGENT};

use crate::middleware::{Middleware, MiddlewareResult};
use crate::{Request, Response, StatusCode, Uri};

/// Decides from a response whether it is a challenge page for [`Browser`] to solve.
pub type Detector = fn(&Response) -> bool;
/// The boxed future an [`Action`] returns.
pub(super) type ActionFuture =
    Pin<Box<dyn Future<Output = Result<(), volt_browser::Error>> + Send>>;
/// The steps run on the loaded page before its cookies are read.
pub(super) type Action = Arc<dyn Fn(Page) -> ActionFuture + Send + Sync>;
/// The boxed future a [`Launch`] returns.
pub(super) type Launching =
    Pin<Box<dyn Future<Output = Result<Session, volt_browser::Error>> + Send>>;
/// Starts the browser a solve runs in, as the settings say.
pub(super) type Launch = Arc<dyn Fn(Settings) -> Launching + Send + Sync>;

/// Ready-made [`Detector`]s.
#[derive(Debug)]
pub struct Detect;

impl Detect {
    /// Whether `response` is a `403 Forbidden`; the default detector.
    pub fn forbidden(response: &Response) -> bool {
        response.status() == StatusCode::FORBIDDEN
    }
}

/// Middleware that solves challenge pages in a browser of volt-browser. When the detector
/// matches a response, it launches the browser, opens the page, runs the action, copies the
/// matching cookies into the cookie jar and has the request replayed; later requests carry the
/// browser's user agent. Built with [`BrowserBuilder`](crate::middleware::BrowserBuilder).
pub struct Browser {
    /// The cookie jar the solved cookies are added to.
    pub(super) jar: Arc<Jar>,
    /// The text a cookie's domain must contain to be copied, and the `Domain` it is stored with.
    pub(super) cookie_domain: String,
    /// The URL the cookies are stored as coming from; empty means the origin of the solved page.
    pub(super) cookie_host: String,
    /// The page to solve, instead of the URL of the detected response.
    pub(super) recovery_url: Option<String>,
    /// Decides which responses are challenges.
    pub(super) detect: Detector,
    /// The steps run on the page before its cookies are read.
    pub(super) action: Action,
    /// Starts the browser of each solve.
    pub(super) launch: Launch,
    /// Whether the browser opens a visible window.
    pub(super) headed: bool,
    /// The most the browser's launch may take, and then the most the solve may take.
    pub(super) timeout: Duration,
    /// The browser's user agent from the last solve, set on every later request.
    pub(super) user_agent: RwLock<Option<HeaderValue>>,
    /// Lets one solve run at a time.
    pub(super) solve_lock: Mutex<()>,
    /// The number of completed solves.
    pub(super) generation: AtomicU64,
}

impl Debug for Browser {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Browser")
            .field("cookie_domain", &self.cookie_domain)
            .field("cookie_host", &self.cookie_host)
            .field("recovery_url", &self.recovery_url)
            .field("headed", &self.headed)
            .field("timeout", &self.timeout)
            .finish_non_exhaustive()
    }
}

impl Browser {
    /// Solves the challenge at the recovery URL now, without waiting for a detected response.
    ///
    /// # Errors
    ///
    /// Fails when no `recovery_url` was set or the solve in the browser fails.
    pub async fn recover_now(&self) -> MiddlewareResult<()> {
        let url = self
            .recovery_url
            .as_deref()
            .ok_or("browser: recovery_url is required for recover_now")?;
        self.recover_at(url).await
    }

    /// Solves the challenge at `url`, adds its cookies to the jar and keeps the browser's user
    /// agent. A caller that waited while another solve finished returns without solving again.
    async fn recover_at(&self, url: &str) -> MiddlewareResult<()> {
        let observed = self.generation.load(Ordering::Acquire);
        let _solving = self.solve_lock.lock().await;
        if self.generation.load(Ordering::Acquire) != observed {
            return Ok(());
        }

        let (cookies, user_agent) = self.solve(url).await?;
        let cookie_host = self.cookie_host_for(url);
        let mut cookie = String::new();
        for (name, value) in cookies {
            cookie.clear();
            write!(
                cookie,
                "{name}={value}; Domain={}; Path=/",
                self.cookie_domain
            )?;
            self.jar.add(cookie.as_str(), &*cookie_host);
        }
        if !user_agent.is_empty() {
            *self
                .user_agent
                .write()
                .unwrap_or_else(PoisonError::into_inner) = HeaderValue::try_from(user_agent).ok();
        }
        self.generation.fetch_add(1, Ordering::Release);
        Ok(())
    }

    /// Returns the URL the solved cookies are stored as coming from: the configured cookie host,
    /// or the scheme and authority of `url`, the page the browser solved, when none was set.
    fn cookie_host_for<'a>(&'a self, url: &'a str) -> Cow<'a, str> {
        if !self.cookie_host.is_empty() {
            return Cow::Borrowed(&self.cookie_host);
        }
        if let Ok(uri) = url.parse::<Uri>()
            && let (Some(scheme), Some(authority)) = (uri.scheme_str(), uri.authority())
        {
            return Cow::Owned(format!("{scheme}://{authority}"));
        }
        Cow::Borrowed(url)
    }

    /// Whether a cookie of `domain` is copied: the domain contains the cookie domain, whose
    /// leading dot does not count, since a browser's cookies come without one.
    fn copies(&self, domain: &str) -> bool {
        domain.contains(self.cookie_domain.trim_start_matches('.'))
    }

    /// Launches a fresh browser, opens `url`, runs the action and returns the cookies to copy,
    /// with the browser's user agent.
    async fn solve(
        &self,
        url: &str,
    ) -> Result<(Vec<(String, String)>, String), volt_browser::Error> {
        let settings = Settings {
            timeout: self.timeout,
            visible: self.headed,
            ..Settings::default()
        };
        let session = (self.launch)(settings).await?;
        let action = &*self.action;
        session
            .run(|page| async move {
                page.goto(url).await?;
                action(page.clone()).await?;
                let cookies = page
                    .cookies()
                    .await?
                    .into_iter()
                    .filter(|cookie| self.copies(&cookie.domain))
                    .map(|cookie| (cookie.name, cookie.value))
                    .collect();
                let user_agent = page.user_agent().await?;
                Ok((cookies, user_agent))
            })
            .await
    }
}

#[async_trait::async_trait]
impl Middleware for Browser {
    async fn before(&self, request: &mut Request) -> MiddlewareResult<Option<Response>> {
        let user_agent = self
            .user_agent
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        if let Some(value) = user_agent {
            request.headers_mut().insert(USER_AGENT, value);
        }
        Ok(None)
    }

    async fn recover(&self, _request: &Request, response: &Response) -> MiddlewareResult<bool> {
        if !(self.detect)(response) {
            return Ok(false);
        }
        let url = self
            .recovery_url
            .as_deref()
            .map_or_else(|| Cow::Owned(response.uri().to_string()), Cow::Borrowed);
        self.recover_at(&url).await?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use wreq::cookie::Jar;

    use crate::middleware::BrowserBuilder;

    /// Builders for tests that never launch a browser.
    struct Unlaunched;

    impl Unlaunched {
        /// Returns a builder over `jar` whose browser cannot be launched.
        fn builder(jar: Arc<Jar>) -> BrowserBuilder {
            BrowserBuilder::new(jar, |_settings| async { Err(volt_browser::Error::Closed) })
        }
    }

    #[test]
    fn cookie_host_defaults_to_the_solved_origin() {
        let jar = Arc::new(Jar::default());

        let derived = Unlaunched::builder(jar.clone()).build();
        assert_eq!(
            derived.cookie_host_for("https://gateway.chub.ai/api/characters?page=2"),
            "https://gateway.chub.ai"
        );
        assert_eq!(derived.cookie_host_for("not a url"), "not a url");

        let explicit = Unlaunched::builder(jar)
            .cookie_host("https://chub.ai")
            .build();
        assert_eq!(
            explicit.cookie_host_for("https://gateway.chub.ai/api/characters"),
            "https://chub.ai"
        );
    }

    #[test]
    fn a_derived_host_keeps_the_cookie_in_the_jar() {
        let jar = Arc::new(Jar::default());
        let browser = Unlaunched::builder(jar.clone()).build();
        let host = browser.cookie_host_for("https://chub.ai/characters/odessa");

        jar.add("cf_clearance=abc; Domain=; Path=/", &*host);

        let sent: Vec<String> = jar
            .matches("https://chub.ai/api/characters")
            .map(|cookie| cookie.name().to_owned())
            .collect();
        assert_eq!(sent, ["cf_clearance"]);
    }

    #[test]
    fn a_leading_dot_in_the_cookie_domain_does_not_count() {
        let jar = Arc::new(Jar::default());

        let dotted = Unlaunched::builder(jar.clone())
            .cookie_domain(".chub.ai")
            .build();
        assert!(dotted.copies("chub.ai"));
        assert!(dotted.copies("gateway.chub.ai"));
        assert!(!dotted.copies("example.com"));

        let every = Unlaunched::builder(jar).build();
        assert!(every.copies("example.com"));
    }
}
