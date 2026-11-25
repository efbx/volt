#[cfg(test)]
mod tests;
mod waiting;

use std::cell::RefCell;
use std::collections::HashMap;
use std::panic::{self, AssertUnwindSafe};
use std::rc::Rc;

use serde_json::Value;
use tokio::sync::oneshot::Sender;
use url::Url;
use volt_browser::Cookie;

use crate::Error;
use crate::platform::{Store, Surface};
use crate::script::Script;
pub(crate) use waiting::LoadEvent;
use waiting::Waiting;

/// Where the answer to one call goes.
pub(crate) type Reply<T> = Sender<Result<T, Error>>;

/// How a session's pages open.
#[derive(Debug)]
pub(crate) struct Setup {
    /// Whether new windows are shown, in front of the program's other windows but without the
    /// keyboard focus.
    pub(crate) visible: bool,
    /// A full user agent override; `None` keeps the webview's own.
    pub(crate) user_agent: Option<String>,
}

/// A session's pages by id, on the one thread that may touch their windows. Webview calls can
/// run that thread's event loop while they wait, so load events reach a page while a call on it
/// still runs: no borrow lasts across a webview call, and a page in use outlives its removal.
pub(crate) struct Pages {
    /// The open pages, or `None` once the session is closed; dropped before the store.
    tabs: RefCell<Option<HashMap<u64, Rc<Tab>>>>,
    /// The webview store the session's pages share, where the platform has one.
    store: Store,
    /// How new pages open.
    setup: Setup,
}

impl Pages {
    /// Returns a session's pages, none open yet, which open as `setup` says.
    pub(crate) fn new(setup: Setup) -> Self {
        Self {
            tabs: RefCell::new(Some(HashMap::new())),
            store: Store::new(),
            setup,
        }
    }

    /// Opens page `id` on `about:blank`, answering `reply` once that load has finished; a closed
    /// session opens no window.
    pub(crate) fn open(&self, id: u64, reply: Reply<()>) {
        if self.tabs.borrow().is_none() {
            return Self::refuse(reply);
        }
        let load = Load::new(Waiting::Open(reply));
        match Surface::open(&self.store, &self.setup, load.listener()) {
            Ok(surface) => self.insert(id, Tab { surface, load }),
            Err(error) => load.fail(error),
        }
    }

    /// Starts loading `url` in page `id`, answering `reply` once that load has ended; the goto
    /// still waiting there fails with [`Error::Replaced`].
    pub(crate) fn goto(&self, id: u64, url: &str, reply: Reply<()>) {
        match self.tab(id) {
            Some(tab) => tab.goto(url, reply),
            None => Self::refuse(reply),
        }
    }

    /// Evaluates the wrapped `script` in page `id`, answering `reply` with its value.
    pub(crate) fn evaluate(&self, id: u64, script: &str, reply: Reply<Value>) {
        match self.tab(id) {
            Some(tab) => tab.surface.evaluate(script, move |json| {
                let _ = reply.send(json.and_then(|json| Script::result(&json)));
            }),
            None => Self::refuse(reply),
        }
    }

    /// Reads the cookies page `id`'s store sends to `url`, answering `reply` with them.
    pub(crate) fn cookies(&self, id: u64, url: &Url, reply: Reply<Vec<Cookie>>) {
        match self.tab(id) {
            Some(tab) => tab.surface.cookies(url, move |cookies| {
                let _ = reply.send(cookies);
            }),
            None => Self::refuse(reply),
        }
    }

    /// Adds `cookie` to page `id`'s store, answering `reply` once it is there.
    pub(crate) fn set_cookie(&self, id: u64, cookie: &Cookie, reply: Reply<()>) {
        match self.tab(id) {
            Some(tab) => tab.surface.set_cookie(cookie, move |stored| {
                let _ = reply.send(stored);
            }),
            None => Self::refuse(reply),
        }
    }

    /// Removes `cookie` from page `id`'s store, answering `reply` once it is gone.
    pub(crate) fn delete_cookie(&self, id: u64, cookie: &Cookie, reply: Reply<()>) {
        match self.tab(id) {
            Some(tab) => tab.surface.delete_cookie(cookie, move |removed| {
                let _ = reply.send(removed);
            }),
            None => Self::refuse(reply),
        }
    }

    /// Shows page `id`'s window in front with the keyboard focus, or hides it.
    pub(crate) fn set_visible(&self, id: u64, visible: bool) -> Result<(), Error> {
        self.tab(id)
            .ok_or(Error::Closed)?
            .surface
            .set_visible(visible);
        Ok(())
    }

    /// Closes page `id`, whose window goes once no running call uses it; the load its caller
    /// waits for fails with [`Error::Closed`].
    pub(crate) fn close(&self, id: u64) -> Result<(), Error> {
        let tab = self
            .tabs
            .borrow_mut()
            .as_mut()
            .and_then(|tabs| tabs.remove(&id));
        tab.ok_or(Error::Closed)?.load.fail(Error::Closed);
        Ok(())
    }

    /// Closes every page and refuses new ones; the loads their callers wait for fail with
    /// [`Error::Closed`].
    pub(crate) fn close_all(&self) {
        let tabs = self.tabs.borrow_mut().take();
        for tab in tabs.into_iter().flat_map(HashMap::into_values) {
            tab.load.fail(Error::Closed);
        }
    }

    /// Adds `tab` as page `id`, or closes it when the session closed while it opened.
    fn insert(&self, id: u64, tab: Tab) {
        let refused = match self.tabs.borrow_mut().as_mut() {
            Some(tabs) => tabs.insert(id, Rc::new(tab)),
            None => Some(Rc::new(tab)),
        };
        if let Some(tab) = refused {
            tab.load.fail(Error::Closed);
        }
    }

    /// Returns page `id`, shared, so that it outlives its removal by a call that runs while it is
    /// in use.
    fn tab(&self, id: u64) -> Option<Rc<Tab>> {
        self.tabs.borrow().as_ref()?.get(&id).map(Rc::clone)
    }

    /// Fails `reply` for a page that does not exist, or no longer does.
    fn refuse<T>(reply: Reply<T>) {
        let _ = reply.send(Err(Error::Closed));
    }
}

impl Drop for Pages {
    fn drop(&mut self) {
        let tabs = self.tabs.get_mut().take();
        let _ = panic::catch_unwind(AssertUnwindSafe(|| drop(tabs)));
    }
}

/// One page: the window with its webview, and the load its caller waits for.
struct Tab {
    /// The window and the webview in it.
    surface: Surface,
    /// The load its caller waits for, shared with the webview's load events.
    load: Load,
}

impl Tab {
    /// Starts loading `url`, answering `reply` once that load has ended; the goto it replaces
    /// fails with [`Error::Replaced`].
    fn goto(&self, url: &str, reply: Reply<()>) {
        self.load.goto(reply, Self::asks_about(url));
        self.surface.load(url, |error| self.load.fail(error));
    }

    /// Whether `url` asks for a page of the webview's own, as `about:blank` is.
    fn asks_about(url: &str) -> bool {
        url.get(..6)
            .is_some_and(|scheme| scheme.eq_ignore_ascii_case("about:"))
    }
}

/// The load a page's caller waits for, shared by the page and its webview's load events.
#[derive(Clone)]
struct Load(Rc<RefCell<Waiting<Reply<()>>>>);

impl Load {
    /// Returns a load that starts as `waiting`.
    fn new(waiting: Waiting<Reply<()>>) -> Self {
        Self(Rc::new(RefCell::new(waiting)))
    }

    /// Returns the handler for the webview's load events, which answers the caller whose load
    /// ended.
    fn listener(&self) -> impl Fn(LoadEvent) + Clone + 'static {
        let load = self.clone();
        move |event| load.advance(event)
    }

    /// Applies `event`, answering the caller whose load it ends.
    fn advance(&self, event: LoadEvent) {
        let ended = self.0.borrow_mut().advance(event);
        if let Some((reply, ended)) = ended {
            let _ = reply.send(ended.outcome());
        }
    }

    /// Waits for the load a goto starts next, which asked for a page of the webview's own when
    /// `about` says so, failing the goto it replaces with [`Error::Replaced`].
    fn goto(&self, reply: Reply<()>, about: bool) {
        let replaced = self.0.borrow_mut().goto(reply, about);
        if let Some(replaced) = replaced {
            let _ = replaced.send(Err(Error::Replaced));
        }
    }

    /// Fails the caller still waiting with `error`.
    fn fail(&self, error: Error) {
        let pending = self.0.borrow_mut().abandon();
        if let Some(reply) = pending {
            let _ = reply.send(Err(error));
        }
    }
}
