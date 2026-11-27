use std::borrow::Cow;
use std::convert::Infallible;
use std::future::{self, Future, Ready};
use std::marker::PhantomData;
use std::time::Duration;

use url::Url;
use volt_browser::Cookie;

use crate::Error;
use crate::pages::{LoadEvent, Setup};

/// Why no session can start: on Linux and the BSDs the crate was built without its `webkitgtk`
/// feature, and no other target but macOS and Windows has a system webview the crate drives.
const NO_WEBVIEW: &str = cfg_select! {
    any(
        target_os = "linux",
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd"
    ) => "volt-webview was built without its `webkitgtk` feature",
    _ => "this target has no system webview",
};

/// The thread that would run webviews on a target without any, which never starts.
pub(crate) struct UiThread<T> {
    /// The proof that no value exists.
    never: Infallible,
    /// The state its jobs would get.
    state: PhantomData<fn(&T)>,
}

impl<T> UiThread<T> {
    /// Fails with [`Error::Unavailable`].
    pub(crate) fn start(
        _timeout: Duration,
        _make: impl FnOnce() -> T + Send + 'static,
    ) -> Ready<Result<Self, Error>> {
        future::ready(Err(Error::Unavailable(Cow::Borrowed(NO_WEBVIEW))))
    }

    /// Cannot run, since no thread exists.
    pub(crate) fn run(&self, _job: impl FnOnce(&T) + Send + 'static) {
        match self.never {}
    }
}

/// The thread a call is awaited on, which no webview thread needs on a target without any.
pub(crate) struct CallerThread;

impl CallerThread {
    /// Waits for `future`.
    pub(crate) async fn wait<F: Future>(future: F) -> F::Output {
        future.await
    }
}

/// The store of a target without a webview, which holds nothing.
pub(crate) struct Store;

impl Store {
    /// Returns the empty store.
    pub(crate) fn new() -> Self {
        Self
    }
}

/// The window with its webview on a target without any, which never exists.
pub(crate) enum Surface {}

impl Surface {
    /// Fails with [`Error::Unavailable`].
    pub(crate) fn open(
        _store: &Store,
        _setup: &Setup,
        _on_load: impl Fn(LoadEvent) + Clone + 'static,
    ) -> Result<Self, Error> {
        Err(Error::Unavailable(Cow::Borrowed(NO_WEBVIEW)))
    }

    /// Cannot run, since no surface exists.
    pub(crate) fn load(&self, _url: &str, _refused: impl FnOnce(Error)) {
        match *self {}
    }

    /// Cannot run, since no surface exists.
    pub(crate) fn evaluate(
        &self,
        _script: &str,
        _done: impl FnOnce(Result<String, Error>) + Send + 'static,
    ) {
        match *self {}
    }

    /// Cannot run, since no surface exists.
    pub(crate) fn cookies(
        &self,
        _url: &Url,
        _done: impl FnOnce(Result<Vec<Cookie>, Error>) + 'static,
    ) {
        match *self {}
    }

    /// Cannot run, since no surface exists.
    pub(crate) fn set_cookie(
        &self,
        _cookie: &Cookie,
        _done: impl FnOnce(Result<(), Error>) + 'static,
    ) {
        match *self {}
    }

    /// Cannot run, since no surface exists.
    pub(crate) fn delete_cookie(
        &self,
        _cookie: &Cookie,
        _done: impl FnOnce(Result<(), Error>) + 'static,
    ) {
        match *self {}
    }

    /// Cannot run, since no surface exists.
    pub(crate) fn set_visible(&self, _visible: bool) {
        match *self {}
    }
}
