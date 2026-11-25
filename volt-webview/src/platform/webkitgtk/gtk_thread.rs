use std::borrow::Cow;
use std::env;
use std::fs;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::Builder;
use std::time::Duration;

use gtk::glib::{self, MainContext, MainContextAcquireGuard, MainLoop, Type};

use super::ui_thread::{Guard, Start};
use crate::Error;

/// The name of the thread this crate runs GTK on when nothing else in the process does.
const THREAD_NAME: &str = "volt-webview-gtk";
/// The types GDK registers only once it has opened a display, on X11 or Wayland. Broadway's is
/// left out: GDK registers it before it tries to connect, so a start that failed leaves it
/// behind, and a display on Broadway cannot be told from none.
const DISPLAY_TYPES: [&str; 2] = ["GdkX11Display", "GdkWaylandDisplay"];
/// The type GDK registers the first time a start of GTK is tried, whether a display opens or
/// not.
const START_TYPE: &str = "GdkDisplayManager";
/// How long a session waits for a thread that already serves the default main context to pick
/// it up, each round, before it tries to become that thread.
const PICKUP: Duration = Duration::from_millis(250);
/// Why no session can start without a display.
const NO_DISPLAY: &str = "no display: neither DISPLAY nor WAYLAND_DISPLAY is set";
/// Why no session can start in a setuid or setgid program, which GTK ends at once.
const SET_ID: &str = "GTK refuses to run in a setuid or setgid program";
/// Why a session cannot start where gtk-rs runs GTK on a thread that does not serve it.
const OTHER_THREAD: &str = "GTK runs on another thread, which does not serve the default context";
/// Why no session can start when its GTK thread cannot.
const NO_THREAD: &str = "the GTK thread could not start";

/// The thread that runs GTK: the one that owns `GLib`'s default main context. This crate starts
/// one only when no thread serves that context; it runs for the rest of the process and serves
/// every later session too.
pub(super) struct GtkThread;

impl GtkThread {
    /// Starts a thread that hands `start` to the thread serving the default context, or becomes
    /// that thread, after checking that GTK can run here at all.
    pub(super) fn start<T, M>(start: Start<T, M>) -> Result<(), Error>
    where
        T: 'static,
        M: FnOnce() -> T + Send + 'static,
    {
        Display::require()?;
        SetId::refuse()?;
        Builder::new()
            .name(THREAD_NAME.to_owned())
            .spawn(move || Self::run(start))
            .map(drop)
            .map_err(|_| Error::Unavailable(Cow::Borrowed(NO_THREAD)))
    }

    /// Makes GTK usable through gtk-rs on this thread, the one serving the default context:
    /// GTK may already run here, or C code or a plain `GLib` loop may own the context without
    /// gtk-rs knowing.
    pub(super) fn adopt() -> Result<(), Error> {
        if gtk::is_initialized_main_thread() {
            return Ok(());
        }
        if gtk::is_initialized() {
            return Err(Error::Unavailable(Cow::Borrowed(OTHER_THREAD)));
        }
        SetId::refuse()?;
        Self::init()
    }

    /// Offers `start` to whichever thread serves the default context, and serves it here when
    /// none does.
    fn run<T, M>(start: Start<T, M>)
    where
        T: 'static,
        M: FnOnce() -> T + Send + 'static,
    {
        let context = MainContext::default();
        let handoff = Handoff::offer(start);
        if let Some((start, claim)) = handoff.turn(&context) {
            Self::serve(start, claim);
        }
    }

    /// Starts GTK under `claim`, then serves `start` and every later session until the process
    /// ends; a start that fails leaves the thread, and the next session tries again.
    fn serve<T, M>(start: Start<T, M>, claim: MainContextAcquireGuard<'_>)
    where
        T: 'static,
        M: FnOnce() -> T + Send + 'static,
    {
        let started = Self::init();
        drop(claim);
        match started {
            Ok(()) => {
                Guard::run(|| start.serve());
                MainLoop::new(None, false).run();
            }
            Err(error) => start.fail(error),
        }
    }

    /// Starts GTK on this thread. The first start in the process leaves the process's locale
    /// alone; a later one finds that choice made, by an earlier try or by C code, and GTK warns
    /// when it is made again.
    fn init() -> Result<(), Error> {
        if !Self::start_tried() {
            gtk::disable_setlocale();
        }
        gtk::init().map_err(|error| Error::Unavailable(error.message))
    }

    /// Whether GDK has already opened a display, which means that C code started GTK without
    /// gtk-rs.
    fn display_opened() -> bool {
        DISPLAY_TYPES
            .into_iter()
            .any(|name| Type::from_name(name).is_some())
    }

    /// Whether a start of GTK was already tried in this process, by this crate or by C code.
    fn start_tried() -> bool {
        Type::from_name(START_TYPE).is_some()
    }
}

/// A session offered to whichever thread serves the default context first.
struct Handoff<T, M> {
    /// The session, until a thread takes it.
    start: Arc<Mutex<Option<Start<T, M>>>>,
    /// Told when a thread has run the offer.
    pickup: Receiver<()>,
}

/// What the offering thread does next.
enum Turn<'a, T, M> {
    /// Nothing more: a thread took the session, or its caller stopped waiting.
    Done,
    /// Wait another round.
    Wait,
    /// Start GTK under the claim on the unused default context and serve the session here.
    Serve(Start<T, M>, MainContextAcquireGuard<'a>),
}

impl<T, M> Handoff<T, M>
where
    T: 'static,
    M: FnOnce() -> T + Send + 'static,
{
    /// Queues a job on the default context that takes `start` and serves it on the thread that
    /// runs the job.
    fn offer(start: Start<T, M>) -> Self {
        let start = Arc::new(Mutex::new(Some(start)));
        let (picked, pickup) = mpsc::channel();
        let offered = Arc::clone(&start);
        let _ = glib::idle_add_once(move || {
            let start = Self::take_from(&offered);
            let _ = picked.send(());
            if let Some(start) = start {
                Guard::run(|| start.serve());
            }
        });
        Self { start, pickup }
    }

    /// Waits until the offer was taken or its caller stopped waiting, or until this thread may
    /// claim the unused default context for it.
    fn turn<'a>(
        &self,
        context: &'a MainContext,
    ) -> Option<(Start<T, M>, MainContextAcquireGuard<'a>)> {
        loop {
            match self.next(context) {
                Turn::Done => return None,
                Turn::Wait => {}
                Turn::Serve(start, claim) => return Some((start, claim)),
            }
        }
    }

    /// Decides the next step: done once a thread ran the offer within [`PICKUP`] or the caller
    /// stopped waiting; serve here once this thread holds the default context while nothing runs
    /// GTK and no display is open; otherwise wait another round.
    fn next<'a>(&self, context: &'a MainContext) -> Turn<'a, T, M> {
        if self.picked() || self.abandoned() {
            return Turn::Done;
        }
        let Ok(claim) = context.acquire() else {
            return Turn::Wait;
        };
        if gtk::is_initialized() || GtkThread::display_opened() {
            return Turn::Wait;
        }
        match Self::take_from(&self.start) {
            Some(start) => Turn::Serve(start, claim),
            None => Turn::Done,
        }
    }

    /// Whether a thread ran the offer within [`PICKUP`].
    fn picked(&self) -> bool {
        !matches!(
            self.pickup.recv_timeout(PICKUP),
            Err(RecvTimeoutError::Timeout)
        )
    }

    /// Whether the caller stopped waiting for the session still offered.
    fn abandoned(&self) -> bool {
        self.start
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .is_some_and(Start::abandoned)
    }

    /// Takes the session out of `start`, unless a thread took it already.
    fn take_from(start: &Mutex<Option<Start<T, M>>>) -> Option<Start<T, M>> {
        start.lock().unwrap_or_else(PoisonError::into_inner).take()
    }
}

/// The display server GTK connects to.
struct Display;

impl Display {
    /// Fails unless `DISPLAY` or `WAYLAND_DISPLAY` names a display.
    fn require() -> Result<(), Error> {
        let named = ["DISPLAY", "WAYLAND_DISPLAY"]
            .into_iter()
            .filter_map(env::var_os)
            .any(|name| !name.is_empty());
        if named {
            Ok(())
        } else {
            Err(Error::Unavailable(Cow::Borrowed(NO_DISPLAY)))
        }
    }
}

/// Whether this program runs setuid or setgid, which GTK answers by ending the process.
struct SetId;

impl SetId {
    /// Fails when the process's real and effective user or group ids differ, as far as Linux's
    /// `/proc` tells; where it cannot be read, as on the BSDs, nothing is checked.
    fn refuse() -> Result<(), Error> {
        let status = fs::read_to_string("/proc/self/status").unwrap_or_default();
        if status.lines().any(Self::differs) {
            Err(Error::Unavailable(Cow::Borrowed(SET_ID)))
        } else {
            Ok(())
        }
    }

    /// Whether `line` is the `Uid:` or `Gid:` line of a process whose real and effective ids
    /// differ.
    fn differs(line: &str) -> bool {
        let Some(ids) = line
            .strip_prefix("Uid:")
            .or_else(|| line.strip_prefix("Gid:"))
        else {
            return false;
        };
        let mut ids = ids.split_whitespace();
        ids.next() != ids.next()
    }
}
