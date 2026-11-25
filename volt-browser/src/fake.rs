use std::collections::VecDeque;
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;
use tokio::runtime::Handle;
use url::Url;

use crate::page_script::Script;
use crate::{Cookie, Engine, Error, Page, Settings};

/// What a fake engine answers a script with.
type Reply = Result<Value, Error>;

/// An engine for tests that runs no browser: it answers scripts from a list, keeps cookies in a
/// plain store, and remembers everything it was asked.
pub(crate) struct Fake {
    /// The answers still to give, in order.
    queue: Mutex<VecDeque<Reply>>,
    /// What to answer once the queue is empty; nothing where there is none.
    fallback: Option<Box<dyn Fn() -> Reply + Send + Sync>>,
    /// Every script received.
    scripts: Mutex<Vec<String>>,
    /// Every URL loaded.
    visited: Mutex<Vec<String>>,
    /// The cookie store.
    cookies: Mutex<Vec<Cookie>>,
    /// Whether the window is shown.
    visible: AtomicBool,
    /// Whether the page was closed.
    closed: AtomicBool,
}

impl Fake {
    /// The timeout the fake's pages give their commands.
    pub(crate) const TIMEOUT: Duration = Duration::from_millis(200);

    /// Returns an engine that gives `answers` in order, then none.
    pub(crate) fn answering(answers: impl IntoIterator<Item = Value>) -> Arc<Self> {
        Self::build(answers.into_iter().map(Ok).collect(), None)
    }

    /// Returns an engine that gives `answer` every time.
    pub(crate) fn repeating(answer: Value) -> Arc<Self> {
        Self::build(VecDeque::new(), Some(Box::new(move || Ok(answer.clone()))))
    }

    /// Returns an engine that fails with `errors` in order, then gives `answers` in order.
    pub(crate) fn failing_then(
        errors: impl IntoIterator<Item = Error>,
        answers: impl IntoIterator<Item = Value>,
    ) -> Arc<Self> {
        let queue = errors
            .into_iter()
            .map(Err)
            .chain(answers.into_iter().map(Ok))
            .collect();
        Self::build(queue, None)
    }

    /// Returns an engine that fails every time with the error `failure` makes.
    pub(crate) fn breaking(failure: impl Fn() -> Error + Send + Sync + 'static) -> Arc<Self> {
        Self::build(VecDeque::new(), Some(Box::new(move || Err(failure()))))
    }

    /// Returns a page on this engine with a short timeout and a quick poll; it has to be made
    /// inside a Tokio runtime.
    pub(crate) fn page(self: &Arc<Self>) -> Page {
        let settings = Settings {
            timeout: Self::TIMEOUT,
            poll: Duration::from_millis(5),
            ..Settings::default()
        };
        let engine: Arc<dyn Engine> = Arc::<Self>::clone(self);
        Page::new(engine, Arc::new(settings), Handle::current())
    }

    /// The commands inside the scripts received, in order.
    pub(crate) fn commands(&self) -> Vec<Value> {
        Self::held(&self.scripts)
            .iter()
            .map(|script| Script::command(script))
            .collect()
    }

    /// What follows the command in the script received `index`th: the caller's code and the
    /// closing bracket.
    pub(crate) fn tail(&self, index: usize) -> String {
        Script::tail(&Self::held(&self.scripts)[index]).to_owned()
    }

    /// The URLs loaded, in order.
    pub(crate) fn visited(&self) -> Vec<String> {
        Self::held(&self.visited).clone()
    }

    /// Puts `cookie` into the store.
    pub(crate) fn store(&self, cookie: Cookie) {
        Self::held(&self.cookies).push(cookie);
    }

    /// The cookies in the store.
    pub(crate) fn stored(&self) -> Vec<Cookie> {
        Self::held(&self.cookies).clone()
    }

    /// Whether the window is shown.
    pub(crate) fn is_visible(&self) -> bool {
        self.visible.load(Ordering::Relaxed)
    }

    /// Whether the page was closed.
    pub(crate) fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Relaxed)
    }

    /// Returns an engine that gives what is in `queue`, then what `fallback` makes.
    fn build(
        queue: VecDeque<Reply>,
        fallback: Option<Box<dyn Fn() -> Reply + Send + Sync>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            queue: Mutex::new(queue),
            fallback,
            scripts: Mutex::new(Vec::new()),
            visited: Mutex::new(Vec::new()),
            cookies: Mutex::new(Vec::new()),
            visible: AtomicBool::new(false),
            closed: AtomicBool::new(false),
        })
    }

    /// Locks `mutex`, whatever a panicking test left it as.
    fn held<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
        mutex.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Whether `one` and `other` name the same cookie of the store.
    fn same(one: &Cookie, other: &Cookie) -> bool {
        one.name == other.name && one.domain == other.domain && one.path == other.path
    }
}

impl fmt::Debug for Fake {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Fake").finish_non_exhaustive()
    }
}

#[async_trait]
impl Engine for Fake {
    async fn goto(&self, url: &Url) -> Result<(), Error> {
        Self::held(&self.visited).push(url.as_str().to_owned());
        Ok(())
    }

    async fn evaluate(&self, script: &str) -> Result<Value, Error> {
        Self::held(&self.scripts).push(script.to_owned());
        let queued = Self::held(&self.queue).pop_front();
        match (queued, &self.fallback) {
            (Some(reply), _) => reply,
            (None, Some(fallback)) => fallback(),
            (None, None) => Ok(Value::Null),
        }
    }

    async fn cookies(&self, url: &Url) -> Result<Vec<Cookie>, Error> {
        let cookies = Self::held(&self.cookies);
        Ok(cookies
            .iter()
            .filter(|cookie| cookie.applies_to(url))
            .cloned()
            .collect())
    }

    async fn set_cookie(&self, cookie: &Cookie) -> Result<(), Error> {
        let mut cookies = Self::held(&self.cookies);
        cookies.retain(|stored| !Self::same(stored, cookie));
        cookies.push(cookie.clone());
        Ok(())
    }

    async fn delete_cookie(&self, cookie: &Cookie) -> Result<(), Error> {
        Self::held(&self.cookies).retain(|stored| !Self::same(stored, cookie));
        Ok(())
    }

    async fn set_visible(&self, visible: bool) -> Result<(), Error> {
        self.visible.store(visible, Ordering::Relaxed);
        Ok(())
    }

    async fn close(&self) -> Result<(), Error> {
        self.closed.store(true, Ordering::Relaxed);
        Ok(())
    }
}
