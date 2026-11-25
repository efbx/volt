use std::cell::Cell;
use std::error;
use std::fmt;
use std::rc::Rc;

use gtk::gio::Cancellable;
use gtk::glib::{self, Propagation, SignalHandlerId};
use gtk::prelude::{Cast, ContainerExt, GtkWindowExt, ObjectExt, WidgetExt, WindowGroupExt};
use gtk::{OffscreenWindow, Widget, Window, WindowGroup, WindowType};
use javascriptcore::{Value, ValueExt};
use webkit2gtk::{
    AuthenticationRequestExt, ColorChooserRequestExt, CookieManager, CookieManagerExt,
    FileChooserRequestExt, NetworkError, PermissionRequestExt, ScriptDialog, ScriptDialogType,
    SettingsExt, WebContextExt, WebView, WebViewExt,
};

use url::Url;
use volt_browser::{Cause, Cookie};

use super::store::Store;
use crate::Error;
use crate::cookie::Stored;
use crate::pages::{LoadEvent, Setup};

/// The page every window opens on.
const BLANK: &str = "about:blank";
/// The title of a page's window.
const TITLE: &str = "volt-webview";
/// The width a page's window opens with, in pixels.
const WIDTH: i32 = 1280;
/// The height a page's window opens with, in pixels.
const HEIGHT: i32 = 800;
/// The failure of a webview without settings, which `WebKitGTK` always gives one.
const NO_SETTINGS: &str = "the webview has no settings";

/// A page's webview with the two windows that may hold it: an offscreen window that gives a
/// hidden page a real size while never reaching the screen, and the window it moves to once
/// shown. Each page's windows form a group of their own, so that no grab elsewhere in the
/// program keeps them from closing.
pub(crate) struct Surface {
    /// The webview.
    webview: WebView,
    /// The window a shown page sits in; a person's close only hides it.
    window: Window,
    /// The window a page sits in until it is first shown.
    offscreen: OffscreenWindow,
    /// The handler that turns a person's close into a hide, removed before the page closes the
    /// window itself.
    hides: Option<SignalHandlerId>,
    /// What the webview's signals remember about the load a goto asked for.
    progress: Rc<Progress>,
}

impl Surface {
    /// Opens a webview in the session's store on `about:blank`, in its window when `setup` says
    /// it is shown, in the offscreen window otherwise, whose load events go to `on_load`.
    pub(crate) fn open(
        store: &Store,
        setup: &Setup,
        on_load: impl Fn(LoadEvent) + Clone + 'static,
    ) -> Result<Self, Error> {
        let webview = WebView::with_context(&store.context);
        Self::configure(&webview, setup)?;
        let progress = Rc::new(Progress::default());
        Self::report(&webview, &progress, on_load);
        let group = WindowGroup::new();
        let window = Self::window(&group);
        let hides = window.connect_delete_event(|window, _| {
            window.hide();
            Propagation::Stop
        });
        let offscreen = OffscreenWindow::new();
        offscreen.set_default_size(WIDTH, HEIGHT);
        group.add_window(&offscreen);
        let surface = Self {
            webview,
            window,
            offscreen,
            hides: Some(hides),
            progress,
        };
        surface.place(setup.visible);
        surface.webview.load_uri(BLANK);
        Ok(surface)
    }

    /// Starts loading `url`. `WebKitGTK` never refuses a load at once, so `_refused` is never
    /// called: a URL it cannot load ends as a failed load.
    pub(crate) fn load(&self, url: &str, _refused: impl FnOnce(Error)) {
        self.progress.ask();
        self.webview.load_uri(url);
    }

    /// Evaluates `script` in the page's own world, handing `done` its value as JSON text, or
    /// nothing for a script that throws.
    pub(crate) fn evaluate(
        &self,
        script: &str,
        done: impl FnOnce(Result<String, Error>) + Send + 'static,
    ) {
        self.webview
            .evaluate_javascript(script, None, None, None::<&Cancellable>, move |result| {
                done(Ok(Self::json(result)));
            });
    }

    /// Reads the cookies the store sends to `url`, `HttpOnly` ones included, and hands them to
    /// `done`.
    pub(crate) fn cookies(
        &self,
        url: &Url,
        done: impl FnOnce(Result<Vec<Cookie>, Error>) + 'static,
    ) {
        let Some(manager) = self.manager() else {
            return done(Err(Error::Closed));
        };
        manager.cookies(url.as_str(), None::<&Cancellable>, move |found| {
            done(
                found
                    .map(|cookies| cookies.into_iter().map(Stored::shared).collect())
                    .map_err(Error::from),
            );
        });
    }

    /// Adds `cookie` to the store and tells `done` how that went.
    pub(crate) fn set_cookie(
        &self,
        cookie: &Cookie,
        done: impl FnOnce(Result<(), Error>) + 'static,
    ) {
        let Some(manager) = self.manager() else {
            return done(Err(Error::Closed));
        };
        let mut native = Stored::native(cookie);
        manager.add_cookie(&mut native, None::<&Cancellable>, move |added| {
            done(added.map_err(Error::from));
        });
    }

    /// Removes the cookie with `cookie`'s name, domain and path from the store, whatever its
    /// value, and tells `done` how that went. The store only removes a cookie shown to it exactly
    /// as it holds it, value and spelling of the domain included, so the stored one is looked up
    /// first.
    pub(crate) fn delete_cookie(
        &self,
        cookie: &Cookie,
        done: impl FnOnce(Result<(), Error>) + 'static,
    ) {
        let Some(manager) = self.manager() else {
            return done(Err(Error::Closed));
        };
        let mut named = Stored::native(cookie);
        let address = Self::address(&mut named);
        let store = manager.clone();
        manager.cookies(&address, None::<&Cancellable>, move |found| match found {
            Ok(found) => Self::delete_all(&store, Self::matching(found, named), done),
            Err(error) => done(Err(error.into())),
        });
    }

    /// Shows the page in its window in front, with the keyboard focus, moving it there from the
    /// offscreen window the first time; or hides the window.
    pub(crate) fn set_visible(&self, visible: bool) {
        if !visible {
            return self.window.hide();
        }
        if self.offscreen_holds_it() {
            self.offscreen.remove(&self.webview);
            self.window.add(&self.webview);
        }
        self.window.show_all();
        self.window.present();
    }

    /// Puts the webview in its window, shown without the keyboard focus, or in the offscreen
    /// window.
    fn place(&self, visible: bool) {
        if visible {
            self.window.add(&self.webview);
            self.window.show_all();
        } else {
            self.offscreen.add(&self.webview);
            self.offscreen.show_all();
        }
    }

    /// Sets the user agent override and answers in a person's place whatever a page could ask
    /// of one, without showing anything: permissions, files and colours are refused, script
    /// dialogs dismissed and printing dropped.
    fn configure(webview: &WebView, setup: &Setup) -> Result<(), Error> {
        let settings =
            WebViewExt::settings(webview).ok_or_else(|| Error::Webview(Box::from(NO_SETTINGS)))?;
        settings.set_enable_developer_extras(false);
        settings.set_user_agent(setup.user_agent.as_deref());
        webview.connect_permission_request(|_, request| {
            request.deny();
            true
        });
        webview.connect_run_file_chooser(|_, request| {
            request.cancel();
            true
        });
        webview.connect_run_color_chooser(|_, request| {
            request.cancel();
            true
        });
        webview.connect_script_dialog(|_, dialog| Self::dismiss(dialog));
        webview.connect_print(|_, _| true);
        Ok(())
    }

    /// Answers a script dialog in a person's place without showing it: the question before
    /// unloading with "leave", so that a page can always be left, and every other kind with a
    /// dismissal.
    fn dismiss(dialog: &ScriptDialog) -> bool {
        if dialog.dialog_type() == ScriptDialogType::BeforeUnloadConfirm {
            dialog.confirm_set_confirmed(true);
        }
        true
    }

    /// Hands the webview's load starts, ends, failures and the end of its web content process to
    /// `on_load`, with what `progress` remembers between its signals.
    fn report(
        webview: &WebView,
        progress: &Rc<Progress>,
        on_load: impl Fn(LoadEvent) + Clone + 'static,
    ) {
        Self::report_changes(webview, Rc::clone(progress), on_load.clone());
        Self::report_failures(webview, Rc::clone(progress), on_load.clone());
        Self::report_moves(webview, Rc::clone(progress), on_load.clone());
        Self::refuse_passwords(webview, Rc::clone(progress));
        webview.connect_web_process_terminated(move |_, _| on_load(LoadEvent::Crashed));
    }

    /// Hands the start and the end of each load to `on_load`.
    fn report_changes(
        webview: &WebView,
        progress: Rc<Progress>,
        on_load: impl Fn(LoadEvent) + 'static,
    ) {
        webview.connect_load_changed(move |webview, event| {
            if event == webkit2gtk::LoadEvent::Started {
                progress.begin();
            }
            if let Some(event) = Self::event(webview, event) {
                on_load(event);
            }
        });
    }

    /// Hands each failed load to `on_load` with its cause, and keeps `WebKitGTK` from loading
    /// its error page: that page would replace the document, and its own load would end the
    /// goto that follows a failed one.
    fn report_failures(
        webview: &WebView,
        progress: Rc<Progress>,
        on_load: impl Fn(LoadEvent) + 'static,
    ) {
        webview.connect_load_failed(move |_, _, _, error| {
            if let Some(cause) = progress.failure(error) {
                on_load(LoadEvent::Ended(Some(cause)));
            }
            true
        });
    }

    /// Hands `on_load` the start and the end of a goto that `WebKitGTK` carried out inside the
    /// document, as when only the fragment changes: it reports no load for one and only stops
    /// loading.
    fn report_moves(
        webview: &WebView,
        progress: Rc<Progress>,
        on_load: impl Fn(LoadEvent) + 'static,
    ) {
        webview.connect_is_loading_notify(move |webview| {
            if !webview.is_loading() && progress.settle() {
                on_load(LoadEvent::Started);
                on_load(LoadEvent::Ended(None));
            }
        });
    }

    /// Refuses every password request, which cancels the load that asked.
    fn refuse_passwords(webview: &WebView, progress: Rc<Progress>) {
        webview.connect_authenticate(move |_, request| {
            progress.refuse();
            request.cancel();
            true
        });
    }

    /// Returns the start or the end of a load as this crate's event, and the commit of
    /// `about:blank` in `webview` as the blank page the load opened; the other steps of a load
    /// change nothing.
    fn event(webview: &WebView, event: webkit2gtk::LoadEvent) -> Option<LoadEvent> {
        match event {
            webkit2gtk::LoadEvent::Started => Some(LoadEvent::Started),
            webkit2gtk::LoadEvent::Committed if webview.uri().as_deref() == Some(BLANK) => {
                Some(LoadEvent::Blank)
            }
            webkit2gtk::LoadEvent::Finished => Some(LoadEvent::Ended(None)),
            _ => None,
        }
    }

    /// Returns a page's window: titled, never focused when it appears, in `group`.
    fn window(group: &WindowGroup) -> Window {
        let window = Window::new(WindowType::Toplevel);
        window.set_title(TITLE);
        window.set_default_size(WIDTH, HEIGHT);
        window.set_focus_on_map(false);
        group.add_window(&window);
        window
    }

    /// Whether the webview still sits in the offscreen window.
    fn offscreen_holds_it(&self) -> bool {
        self.webview.parent().as_ref() == Some(self.offscreen.upcast_ref::<Widget>())
    }

    /// The cookie manager of the page's store.
    fn manager(&self) -> Option<CookieManager> {
        self.webview.context()?.cookie_manager()
    }

    /// Returns a URL the store sends `cookie` to: its host and path over HTTPS, which a secure
    /// cookie needs.
    fn address(cookie: &mut soup::Cookie) -> String {
        let domain = cookie.domain();
        let path = cookie.path();
        format!(
            "https://{}{}",
            Self::host(domain.as_deref()),
            path.as_deref().unwrap_or("/")
        )
    }

    /// Returns those of `found` that are the cookie `named` names: the same name and path, and
    /// the same domain whichever way the store spells it.
    fn matching(
        found: Vec<soup::Cookie>,
        mut named: soup::Cookie,
    ) -> impl Iterator<Item = soup::Cookie> {
        let (name, path, domain) = (named.name(), named.path(), named.domain());
        found.into_iter().filter_map(move |mut stored| {
            let same = stored.name() == name
                && stored.path() == path
                && Self::host(stored.domain().as_deref()) == Self::host(domain.as_deref());
            same.then_some(stored)
        })
    }

    /// Returns `domain` without the leading dot the store gives a cookie for a whole domain.
    fn host(domain: Option<&str>) -> &str {
        let domain = domain.unwrap_or_default();
        domain.strip_prefix('.').unwrap_or(domain)
    }

    /// Removes `cookies` from `store` one after the other and tells `done` how that went.
    fn delete_all(
        store: &CookieManager,
        mut cookies: impl Iterator<Item = soup::Cookie> + 'static,
        done: impl FnOnce(Result<(), Error>) + 'static,
    ) {
        let Some(mut cookie) = cookies.next() else {
            return done(Ok(()));
        };
        let rest = store.clone();
        store.delete_cookie(
            &mut cookie,
            None::<&Cancellable>,
            move |removed| match removed {
                Ok(()) => Self::delete_all(&rest, cookies, done),
                Err(error) => done(Err(error.into())),
            },
        );
    }

    /// Returns a script's value as JSON text, or nothing for a script that threw.
    fn json(result: Result<Value, glib::Error>) -> String {
        result
            .ok()
            .and_then(|value| value.to_json(0))
            .map(String::from)
            .unwrap_or_default()
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        if let Some(hides) = self.hides.take() {
            self.window.disconnect(hides);
        }
        if self.offscreen_holds_it() {
            self.offscreen.remove(&self.webview);
        } else {
            self.window.remove(&self.webview);
        }
        self.window.realize();
        self.window.close();
        self.offscreen.close();
    }
}

/// What a webview's signals remember between each other about the load a goto asked for.
#[derive(Default)]
struct Progress {
    /// Whether a goto asked for a load that has not started yet.
    awaited: Cell<bool>,
    /// Whether a password request was refused since the load under way started.
    refused: Cell<bool>,
}

impl Progress {
    /// Notes that a goto asked for a load.
    fn ask(&self) {
        self.awaited.set(true);
        self.refused.set(false);
    }

    /// Notes that a load started.
    fn begin(&self) {
        self.awaited.set(false);
        self.refused.set(false);
    }

    /// Notes that a password request was refused.
    fn refuse(&self) {
        self.refused.set(true);
    }

    /// Ends the wait for a goto's load and returns whether one was still awaited.
    fn settle(&self) -> bool {
        self.awaited.replace(false)
    }

    /// Returns why a load that ended with `error` failed, or nothing when it was only cancelled:
    /// by a later load, by the page itself or by a stop. A load cancelled because its password
    /// request was refused did fail.
    fn failure(&self, error: &glib::Error) -> Option<Cause> {
        if !error.matches(NetworkError::Cancelled) {
            return Some(Box::new(error.clone()));
        }
        self.refused.get().then(|| Cause::from(PasswordAsked))
    }
}

/// Why a load failed whose page asked for a password.
#[derive(Debug)]
struct PasswordAsked;

impl fmt::Display for PasswordAsked {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the page asks for a password, and no one is there to give one")
    }
}

impl error::Error for PasswordAsked {}

impl From<glib::Error> for Error {
    fn from(error: glib::Error) -> Self {
        Self::Webview(Box::new(error))
    }
}
