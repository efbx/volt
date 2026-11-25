use std::cell::{Cell, RefCell};
use std::future::Future;
use std::mem;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use tokio::sync::oneshot;
use tokio::time;
use url::Url;
use volt_browser::Cookie;

use crate::Error;
use crate::pages::{LoadEvent, Setup};
use crate::script::Script;

/// The inputs that make the fake webview of unit tests misbehave.
pub(crate) struct Fake;

impl Fake {
    /// A URL whose load never reports an end, like a failed load in `WKWebView`.
    pub(crate) const STUCK_URL: &str = "https://fake.invalid/stuck";
    /// A URL the webview refuses to load.
    pub(crate) const REFUSED_URL: &str = "https://fake.invalid/refused";
    /// A URL whose load starts, fails and finishes, as in `WebKitGTK`.
    pub(crate) const FAILING_URL: &str = "https://fake.invalid/failing";
    /// A URL in whose place the webview opens `about:blank`, as newer `WebKit` does with an
    /// address it refuses.
    pub(crate) const BLANKED_URL: &str = "https://fake.invalid/blanked";
    /// The blank page in capitals, which the webview opens as it does `about:blank`.
    pub(crate) const BLANK_URL: &str = "ABOUT:blank";
    /// A URL whose load ends the web content process.
    pub(crate) const CRASHING_URL: &str = "https://fake.invalid/crashing";
    /// A URL whose load events arrive only with the next call on the page.
    pub(crate) const LATE_URL: &str = "https://fake.invalid/late";
    /// A URL whose load keeps the UI thread busy for [`Fake::BUSY`].
    pub(crate) const BUSY_URL: &str = "https://fake.invalid/busy";
    /// How long a load of [`Fake::BUSY_URL`] keeps the UI thread busy.
    pub(crate) const BUSY: Duration = Duration::from_millis(400);
    /// A user agent the webview refuses to open a page with.
    pub(crate) const REFUSED_AGENT: &str = "fake:refused";
    /// A user agent that opens a page whose store fails every read.
    pub(crate) const BROKEN_AGENT: &str = "fake:broken";
    /// An expression the webview never answers.
    pub(crate) const SILENT: &str = "fake:silent";
    /// An expression the webview drops without answering.
    pub(crate) const DROPPED: &str = "fake:dropped";
    /// An expression the webview refuses to start, as `WebView2` may.
    pub(crate) const REFUSED: &str = "fake:refused-script";
    /// An expression whose evaluation panics, as a webview call may.
    pub(crate) const PANICKING: &str = "fake:panic";
    /// An expression whose evaluation keeps the UI thread busy for [`Fake::BUSY`], then gives 1.
    pub(crate) const LABORED: &str = "fake:labored";

    /// The cookie every fake store starts with: `cf_clearance`, `HttpOnly`, for `example.com`.
    pub(crate) fn cookie() -> Cookie {
        Cookie {
            secure: true,
            http_only: true,
            ..Cookie::new("cf_clearance", "abc", "example.com")
        }
    }

    /// Whether `one` and `other` name the same cookie of a store.
    fn same(one: &Cookie, other: &Cookie) -> bool {
        one.name == other.name && one.domain == other.domain && one.path == other.path
    }
}

/// A job for the fake UI thread.
type Job<T> = Box<dyn FnOnce(&T) + Send>;

/// An answer the fake webview holds back forever.
type Held = Box<dyn FnOnce(Result<String, Error>) + Send>;

/// The UI thread of unit tests: a thread of its own that runs jobs in order with the state they
/// share, and drops that state there once every handle is gone.
pub(crate) struct UiThread<T> {
    /// Where jobs go.
    jobs: mpsc::Sender<Job<T>>,
}

impl<T: 'static> UiThread<T> {
    /// Starts the thread, creates its state there with `make` and waits, at most `timeout`, until
    /// it has.
    pub(crate) async fn start(
        timeout: Duration,
        make: impl FnOnce() -> T + Send + 'static,
    ) -> Result<Self, Error> {
        let (jobs, inbox) = mpsc::channel();
        let (ready, started) = oneshot::channel();
        thread::spawn(move || Self::serve(make, ready, &inbox));
        time::timeout(timeout, started)
            .await
            .map_err(|_| Error::Timeout(timeout))?
            .map_err(|_| Error::Closed)?;
        Ok(Self { jobs })
    }

    /// Runs `job` on the thread after the jobs before it, or drops it once the thread is gone.
    pub(crate) fn run(&self, job: impl FnOnce(&T) + Send + 'static) {
        let _ = self.jobs.send(Box::new(job));
    }

    /// Creates the state, reports that it exists, then runs every job until no handle is left.
    fn serve(make: impl FnOnce() -> T, ready: oneshot::Sender<()>, inbox: &mpsc::Receiver<Job<T>>) {
        let state = make();
        let _ = ready.send(());
        for job in inbox {
            job(&state);
        }
    }
}

/// The thread a call is awaited on, which the fake UI thread never needs.
pub(crate) struct CallerThread;

impl CallerThread {
    /// Waits for `future`.
    pub(crate) async fn wait<F: Future>(future: F) -> F::Output {
        future.await
    }
}

/// The session's store in unit tests, which holds nothing.
pub(crate) struct Store;

impl Store {
    /// Returns the empty store.
    pub(crate) fn new() -> Self {
        Self
    }
}

/// The webview of unit tests: it loads at once, answers an expression with the expression's own
/// text as the value's JSON, and keeps a store that starts with one `HttpOnly` cookie.
pub(crate) struct Surface {
    /// Where its load events go.
    on_load: Box<dyn Fn(LoadEvent)>,
    /// The cookies in its store.
    cookies: RefCell<Vec<Cookie>>,
    /// Whether its store fails every read.
    broken: bool,
    /// Whether the window is shown.
    visible: Cell<bool>,
    /// The answers it holds back forever.
    held: RefCell<Vec<Held>>,
    /// The load events it reports with the next call.
    pending: RefCell<Vec<LoadEvent>>,
}

impl Surface {
    /// Opens a page, whose `about:blank` loads at once, unless the user agent is
    /// [`Fake::REFUSED_AGENT`].
    pub(crate) fn open(
        _store: &Store,
        setup: &Setup,
        on_load: impl Fn(LoadEvent) + Clone + 'static,
    ) -> Result<Self, Error> {
        let user_agent = setup.user_agent.as_deref();
        if user_agent == Some(Fake::REFUSED_AGENT) {
            return Err(Error::Webview(Box::from(
                "the fake webview refused to open",
            )));
        }
        let surface = Self {
            on_load: Box::new(on_load),
            cookies: RefCell::new(vec![Fake::cookie()]),
            broken: user_agent == Some(Fake::BROKEN_AGENT),
            visible: Cell::new(setup.visible),
            held: RefCell::new(Vec::new()),
            pending: RefCell::new(Vec::new()),
        };
        surface.complete_load();
        Ok(surface)
    }

    /// Starts loading `url`, which loads at once unless it is one of [`Fake`]'s URLs; a refused
    /// URL goes to `refused`.
    pub(crate) fn load(&self, url: &str, refused: impl FnOnce(Error)) {
        self.report_pending();
        match url {
            Fake::STUCK_URL => {}
            Fake::REFUSED_URL => {
                refused(Error::Webview(Box::from(
                    "the fake webview refused the URL",
                )));
            }
            Fake::FAILING_URL => {
                (self.on_load)(LoadEvent::Started);
                (self.on_load)(LoadEvent::Ended(Some(Box::from("the fake load failed"))));
                (self.on_load)(LoadEvent::Ended(None));
            }
            Fake::BLANKED_URL | Fake::BLANK_URL => {
                (self.on_load)(LoadEvent::Started);
                (self.on_load)(LoadEvent::Blank);
                (self.on_load)(LoadEvent::Ended(None));
            }
            Fake::CRASHING_URL => (self.on_load)(LoadEvent::Crashed),
            Fake::LATE_URL => self
                .pending
                .borrow_mut()
                .extend([LoadEvent::Started, LoadEvent::Ended(None)]),
            Fake::BUSY_URL => {
                thread::sleep(Fake::BUSY);
                self.complete_load();
            }
            _ => self.complete_load(),
        }
    }

    /// Evaluates the wrapped `script`, answering `done` the way wry does: with the JSON text of
    /// the string the page made.
    pub(crate) fn evaluate(
        &self,
        script: &str,
        done: impl FnOnce(Result<String, Error>) + Send + 'static,
    ) {
        self.report_pending();
        let expression = script
            .strip_prefix(Script::OPEN)
            .and_then(|rest| rest.strip_suffix(Script::CLOSE))
            .unwrap_or(script);
        match expression {
            Fake::SILENT => self.held.borrow_mut().push(Box::new(done)),
            Fake::DROPPED => drop(done),
            Fake::REFUSED => done(Err(Error::Webview(Box::from(
                "the fake refused the script",
            )))),
            Fake::PANICKING => panic!("the fake webview panicked"),
            Fake::LABORED => {
                thread::sleep(Fake::BUSY);
                done(Ok(Self::json_text("1")));
            }
            "undefined" => done(Ok("null".to_owned())),
            _ => done(Ok(Self::json_text(expression))),
        }
    }

    /// Reads the cookies the store sends to `url`, after the load events that are due.
    pub(crate) fn cookies(
        &self,
        url: &Url,
        done: impl FnOnce(Result<Vec<Cookie>, Error>) + 'static,
    ) {
        self.report_pending();
        if self.broken {
            return done(Err(Error::Webview(Box::from("the fake store is broken"))));
        }
        let cookies = self.cookies.borrow();
        done(Ok(cookies
            .iter()
            .filter(|cookie| cookie.applies_to(url))
            .cloned()
            .collect()));
    }

    /// Adds `cookie` to the store, in place of the one it names.
    pub(crate) fn set_cookie(
        &self,
        cookie: &Cookie,
        done: impl FnOnce(Result<(), Error>) + 'static,
    ) {
        self.report_pending();
        let mut cookies = self.cookies.borrow_mut();
        cookies.retain(|stored| !Fake::same(stored, cookie));
        cookies.push(cookie.clone());
        done(Ok(()));
    }

    /// Removes the cookie `cookie` names from the store.
    pub(crate) fn delete_cookie(
        &self,
        cookie: &Cookie,
        done: impl FnOnce(Result<(), Error>) + 'static,
    ) {
        self.report_pending();
        self.cookies
            .borrow_mut()
            .retain(|stored| !Fake::same(stored, cookie));
        done(Ok(()));
    }

    /// Shows or hides the window.
    pub(crate) fn set_visible(&self, visible: bool) {
        self.report_pending();
        self.visible.set(visible);
    }

    /// Whether the window is shown.
    pub(crate) fn is_visible(&self) -> bool {
        self.visible.get()
    }

    /// Reports a load that starts, then finishes.
    fn complete_load(&self) {
        (self.on_load)(LoadEvent::Started);
        (self.on_load)(LoadEvent::Ended(None));
    }

    /// Reports the load events held for the next call.
    fn report_pending(&self) {
        let pending = mem::take(&mut *self.pending.borrow_mut());
        for event in pending {
            (self.on_load)(event);
        }
    }

    /// Returns `text` as a JSON string.
    fn json_text(text: &str) -> String {
        serde_json::to_string(text).expect("text always serializes")
    }
}
