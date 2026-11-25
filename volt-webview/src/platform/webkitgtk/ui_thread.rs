use std::borrow::Cow;
use std::future::Future;
use std::io::{self, ErrorKind, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
use std::panic::{self, AssertUnwindSafe};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use gtk::glib::{self, ControlFlow, IOCondition};
use tokio::sync::oneshot;
use tokio::time;

use super::gtk_thread::GtkThread;
use crate::Error;

/// Why no session can start without the socket pair that wakes the GTK thread.
const NO_DOORBELL: &str = "the session's wake-up socket could not be made";
/// Why a session got no answer from the thread that runs GTK.
const NOT_SERVED: &str = "the thread that runs GTK dropped the session";

/// A job for the thread that runs GTK.
type Job<T> = Box<dyn FnOnce(&T) + Send>;

/// A session's state on the thread that runs GTK, which owns `GLib`'s default main context: the
/// program's own GTK thread when GTK runs through gtk-rs, otherwise one this crate starts once
/// for the rest of the process. Jobs go through a channel and wake that thread through a socket
/// that its main loop watches, so that the thread never polls a future of this crate.
pub(crate) struct UiThread<T: 'static> {
    /// Where jobs go.
    jobs: Sender<Job<T>>,
    /// The end of the socket pair this side writes a byte to for every job; dropping it tells
    /// the GTK thread that the session's last handle is gone.
    bell: UnixStream,
}

impl<T: 'static> UiThread<T> {
    /// Hands the session to the thread that runs GTK, starting that thread when nothing runs GTK
    /// yet, creates the state there with `make` and returns once it exists, waiting at most
    /// `timeout`.
    pub(crate) async fn start(
        timeout: Duration,
        make: impl FnOnce() -> T + Send + 'static,
    ) -> Result<Self, Error> {
        let (jobs, inbox) = mpsc::channel();
        let (bell, ringing) =
            Self::doorbell().map_err(|_| Error::Unavailable(Cow::Borrowed(NO_DOORBELL)))?;
        let (ready, started) = oneshot::channel();
        Start {
            inbox,
            ringing,
            make,
            ready,
        }
        .launch()?;
        let ui = Self { jobs, bell };
        match time::timeout(timeout, started).await {
            Ok(Ok(Ok(()))) => Ok(ui),
            Ok(Ok(Err(error))) => Err(error),
            Ok(Err(_)) => Err(Error::Unavailable(Cow::Borrowed(NOT_SERVED))),
            Err(_) => Err(Error::Timeout(timeout)),
        }
    }

    /// Runs `job` with the state on the GTK thread, after the jobs queued before it.
    pub(crate) fn run(&self, job: impl FnOnce(&T) + Send + 'static) {
        if self.jobs.send(Box::new(job)).is_ok() {
            let _ = (&self.bell).write_all(&[1]);
        }
    }

    /// Returns the two ends of a socket pair, neither of which ever blocks: a full socket already
    /// holds a byte that wakes the GTK thread.
    fn doorbell() -> io::Result<(UnixStream, UnixStream)> {
        let (bell, ringing) = UnixStream::pair()?;
        bell.set_nonblocking(true)?;
        ringing.set_nonblocking(true)?;
        Ok((bell, ringing))
    }
}

/// The thread a call is awaited on, which the GTK thread never needs.
pub(crate) struct CallerThread;

impl CallerThread {
    /// Waits for `future`.
    pub(crate) async fn wait<F: Future>(future: F) -> F::Output {
        future.await
    }
}

/// A session waiting to be served on the thread that runs GTK.
pub(super) struct Start<T, M> {
    /// Where the session's jobs arrive.
    inbox: Receiver<Job<T>>,
    /// The end of the socket pair the GTK thread watches.
    ringing: UnixStream,
    /// Creates the session's state on the GTK thread.
    make: M,
    /// Told whether the session is ready.
    ready: oneshot::Sender<Result<(), Error>>,
}

impl<T: 'static, M: FnOnce() -> T + Send + 'static> Start<T, M> {
    /// Hands the session to the thread that runs GTK through gtk-rs, or starts the thread that
    /// finds or becomes the one that runs GTK.
    fn launch(self) -> Result<(), Error> {
        if gtk::is_initialized() {
            self.post();
            Ok(())
        } else {
            GtkThread::start(self)
        }
    }

    /// Queues the session on `GLib`'s default main context, never run inline, so that the thread
    /// owning that context serves it.
    pub(super) fn post(self) {
        let _ = glib::idle_add_once(move || Guard::run(|| self.serve()));
    }

    /// Starts serving the session on this thread, which owns the default context, unless its
    /// caller stopped waiting, and tells the caller how that went.
    pub(super) fn serve(self) {
        let Self {
            inbox,
            ringing,
            make,
            ready,
        } = self;
        if ready.is_closed() {
            return;
        }
        let served = GtkThread::adopt().map(|()| Served::attach(make(), inbox, ringing));
        let _ = ready.send(served);
    }

    /// Whether the caller stopped waiting for the session.
    pub(super) fn abandoned(&self) -> bool {
        self.ready.is_closed()
    }

    /// Tells the caller that GTK could not start, for the reason `error` gives.
    pub(super) fn fail(self, error: Error) {
        let _ = self.ready.send(Err(error));
    }
}

/// A session served on the GTK thread: its state, and where its jobs and wake-ups arrive.
struct Served<T> {
    /// The state, until the session's last handle is gone.
    state: Option<T>,
    /// Where jobs arrive.
    inbox: Receiver<Job<T>>,
    /// The end of the socket pair the main loop watches.
    ringing: UnixStream,
}

impl<T: 'static> Served<T> {
    /// Has the default main context run the session's jobs whenever a byte arrives on `ringing`,
    /// with `state`; the source goes once the other end closes.
    fn attach(state: T, inbox: Receiver<Job<T>>, ringing: UnixStream) {
        let fd = ringing.as_raw_fd();
        let mut served = Self {
            state: Some(state),
            inbox,
            ringing,
        };
        let _ = glib_unix::unix_fd_add_local(
            fd,
            IOCondition::IN | IOCondition::HUP | IOCondition::ERR,
            move |_, _| {
                panic::catch_unwind(AssertUnwindSafe(|| served.answer()))
                    .unwrap_or(ControlFlow::Break)
            },
        );
    }

    /// Runs the jobs that arrived, and drops the state and the source once the session's last
    /// handle is gone.
    fn answer(&mut self) -> ControlFlow {
        let open = self.heard();
        self.drain();
        if open {
            ControlFlow::Continue
        } else {
            self.state = None;
            ControlFlow::Break
        }
    }

    /// Reads the bytes that woke the thread and returns whether the session's handle still
    /// exists; at its end the read finds the socket closed.
    fn heard(&self) -> bool {
        let mut rings = [0; 64];
        match (&self.ringing).read(&mut rings) {
            Ok(0) => false,
            Ok(_) => true,
            Err(error) => matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::Interrupted),
        }
    }

    /// Runs every job in the channel with the state.
    fn drain(&self) {
        let Some(state) = &self.state else {
            return;
        };
        for job in self.inbox.try_iter() {
            job(state);
        }
    }
}

/// Stops panics at the edge of a `GLib` callback, where unwinding would abort the program.
pub(super) struct Guard;

impl Guard {
    /// Runs `work`, stopping a panic in it.
    pub(super) fn run(work: impl FnOnce()) {
        let _ = panic::catch_unwind(AssertUnwindSafe(work));
    }
}
