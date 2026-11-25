use std::sync::{Arc, Mutex, PoisonError};

use url::Url;
use volt_browser::Cookie;
use wry::{BackgroundThrottlingPolicy, PageLoadEvent, PermissionResponse, WebView, WebViewBuilder};

use super::{PageWindow, Store};
use crate::Error;
use crate::cookie::Stored;
use crate::pages::{LoadEvent, Setup};

/// The page every window opens on.
const BLANK: &str = "about:blank";

/// A page's window and the wry webview filling it, on macOS and Windows.
pub(crate) struct Surface {
    /// The webview, dropped before the window it fills.
    webview: WebView,
    /// The window.
    window: PageWindow,
}

impl Surface {
    /// Opens a window on `about:blank` in an incognito store, shown without the keyboard focus
    /// when `setup` says so, whose load events go to `on_load`.
    pub(crate) fn open(
        store: &Store,
        setup: &Setup,
        on_load: impl Fn(LoadEvent) + Clone + 'static,
    ) -> Result<Self, Error> {
        let window = PageWindow::new()?;
        let webview = window.attach(store, |builder| Self::configure(builder, setup, on_load))?;
        if setup.visible {
            window.reveal();
        }
        Ok(Self { webview, window })
    }

    /// Starts loading `url`, or calls `refused` with the reason the webview refused it at once; a
    /// URL the webview cannot parse is refused before it reaches the webview.
    pub(crate) fn load(&self, url: &str, refused: impl FnOnce(Error)) {
        if !PageWindow::accepts(url) {
            return refused(Error::InvalidUrl);
        }
        if let Err(error) = self.webview.load_url(url) {
            refused(error.into());
        }
    }

    /// Evaluates `script`, handing `done` its value as JSON text, or the reason the webview
    /// could not start the script.
    pub(crate) fn evaluate(
        &self,
        script: &str,
        done: impl FnOnce(Result<String, Error>) + Send + 'static,
    ) {
        let done = Arc::new(Mutex::new(Some(done)));
        let answer = Arc::clone(&done);
        let started = self
            .webview
            .evaluate_script_with_callback(script, move |json| {
                if let Some(done) = Self::take(&answer) {
                    done(Ok(json));
                }
            });
        if let Err(error) = started
            && let Some(done) = Self::take(&done)
        {
            done(Err(error.into()));
        }
    }

    /// Takes what `slot` holds, whatever a panic left it as.
    fn take<T>(slot: &Mutex<Option<T>>) -> Option<T> {
        slot.lock().unwrap_or_else(PoisonError::into_inner).take()
    }

    /// Reads the cookies the page's store sends to `url`, `HttpOnly` ones included, and hands
    /// them to `done`.
    pub(crate) fn cookies(
        &self,
        url: &Url,
        done: impl FnOnce(Result<Vec<Cookie>, Error>) + 'static,
    ) {
        let cookies = self.webview.cookies().map_err(Error::from);
        done(cookies.map(|cookies| {
            cookies
                .iter()
                .map(Stored::shared)
                .filter(|cookie| cookie.applies_to(url))
                .collect()
        }));
    }

    /// Adds `cookie` to the page's store and tells `done` how that went.
    pub(crate) fn set_cookie(
        &self,
        cookie: &Cookie,
        done: impl FnOnce(Result<(), Error>) + 'static,
    ) {
        let stored = self.webview.set_cookie(&Stored::native(cookie));
        done(stored.map_err(Error::from));
    }

    /// Removes `cookie` from the page's store and tells `done` how that went.
    pub(crate) fn delete_cookie(
        &self,
        cookie: &Cookie,
        done: impl FnOnce(Result<(), Error>) + 'static,
    ) {
        let removed = self.webview.delete_cookie(&Stored::native(cookie));
        done(removed.map_err(Error::from));
    }

    /// Shows the window in front with the page focused, or hides it.
    pub(crate) fn set_visible(&self, visible: bool) {
        if visible {
            self.window.show();
            let _ = self.webview.focus();
        } else {
            self.window.hide();
        }
    }

    /// Sets up the webview: `about:blank` in an incognito store, no developer tools, no focus or
    /// activation of the program when built, no throttling while hidden, every permission and
    /// download refused, the user agent override, and load events, the end of the web content
    /// process included where the platform reports it, to `on_load`.
    fn configure<'a>(
        builder: WebViewBuilder<'a>,
        setup: &Setup,
        on_load: impl Fn(LoadEvent) + Clone + 'static,
    ) -> WebViewBuilder<'a> {
        let crashed = on_load.clone();
        let builder = PageWindow::watch(builder, move || crashed(LoadEvent::Crashed))
            .with_url(BLANK)
            .with_incognito(true)
            .with_devtools(false)
            .with_focused(false)
            .with_accept_first_mouse(true)
            .with_background_throttling(BackgroundThrottlingPolicy::Disabled)
            .with_permission_handler(|_| PermissionResponse::Deny)
            .with_download_started_handler(|_, _| false)
            .with_on_page_load_handler(move |event, url| Self::report(&on_load, event, &url));
        match setup.user_agent.as_deref() {
            Some(user_agent) => builder.with_user_agent(user_agent),
            None => builder,
        }
    }

    /// Hands the start or the end of a load to `on_load`, and after a start at `about:blank`
    /// the blank page the load opened.
    fn report(on_load: &impl Fn(LoadEvent), event: PageLoadEvent, url: &str) {
        let blank = matches!(event, PageLoadEvent::Started) && url == BLANK;
        on_load(LoadEvent::from(event));
        if blank {
            on_load(LoadEvent::Blank);
        }
    }
}

impl From<PageLoadEvent> for LoadEvent {
    fn from(event: PageLoadEvent) -> Self {
        match event {
            PageLoadEvent::Started => Self::Started,
            PageLoadEvent::Finished => Self::Ended(None),
        }
    }
}

impl From<wry::Error> for Error {
    fn from(error: wry::Error) -> Self {
        Self::Webview(Box::new(error))
    }
}
