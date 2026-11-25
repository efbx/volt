use std::borrow::Cow;
use std::future::Future;
use std::ops::ControlFlow;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread::Builder;
use std::time::Duration;

use tokio::sync::oneshot;
use tokio::time;

use super::win32::{Doorbell, Queue};
use crate::Error;

/// The name of every session's window thread.
const THREAD_NAME: &str = "volt-webview";
/// Why no session can start when its window thread cannot.
const NO_THREAD: &str = "the window thread could not start";

/// A job for a session's window thread.
type Job<T> = Box<dyn FnOnce(&T) + Send>;

/// A session's state on a window thread of its own, which runs a Win32 message loop for the
/// session's windows and webviews, so that nothing depends on the program's own threads.
pub(crate) struct UiThread<T: 'static> {
    /// Where jobs go.
    jobs: Sender<Job<T>>,
    /// Set once the session's last handle is gone, so that the thread starts no further job.
    ending: Arc<AtomicBool>,
    /// Wakes the thread from any thread.
    doorbell: Doorbell,
}

impl<T: 'static> UiThread<T> {
    /// Starts the session's window thread, creates the state there with `make` and returns once
    /// it exists, waiting at most `timeout`.
    pub(crate) async fn start(
        timeout: Duration,
        make: impl FnOnce() -> T + Send + 'static,
    ) -> Result<Self, Error> {
        let (jobs, inbox) = mpsc::channel();
        let (ready, started) = oneshot::channel();
        let ending = Arc::new(AtomicBool::new(false));
        let runner = Runner {
            inbox,
            ending: Arc::clone(&ending),
        };
        let thread = Builder::new()
            .name(THREAD_NAME.to_owned())
            .spawn(move || runner.start(make, ready))
            .map_err(|_| Error::Unavailable(Cow::Borrowed(NO_THREAD)))?;
        let ui = Self {
            jobs,
            ending,
            doorbell: Doorbell::new(thread),
        };
        match time::timeout(timeout, started).await {
            Ok(Ok(())) => Ok(ui),
            Ok(Err(_)) => Err(Error::Unavailable(Cow::Borrowed(NO_THREAD))),
            Err(_) => Err(Error::Timeout(timeout)),
        }
    }

    /// Runs `job` with the state on the window thread, after the jobs queued before it.
    pub(crate) fn run(&self, job: impl FnOnce(&T) + Send + 'static) {
        if self.jobs.send(Box::new(job)).is_ok() {
            self.doorbell.ring();
        }
    }
}

impl<T: 'static> Drop for UiThread<T> {
    fn drop(&mut self) {
        self.ending.store(true, Ordering::Release);
        self.doorbell.quit();
    }
}

/// The thread a call is awaited on, which the window thread never needs.
pub(crate) struct CallerThread;

impl CallerThread {
    /// Waits for `future`.
    pub(crate) async fn wait<F: Future>(future: F) -> F::Output {
        future.await
    }
}

/// The body of a session's window thread: the jobs it receives, and the flag that ends it.
struct Runner<T> {
    /// Where jobs arrive.
    inbox: Receiver<Job<T>>,
    /// Set once the session's last handle is gone.
    ending: Arc<AtomicBool>,
}

impl<T> Runner<T> {
    /// Creates the thread's message queue, creates the state with `make`, reports through
    /// `ready`, then serves jobs until the session ends.
    fn start(self, make: impl FnOnce() -> T, ready: oneshot::Sender<()>) {
        let mut queue = Queue::create();
        let state = make();
        if ready.send(()).is_ok() {
            self.serve(&state, &mut queue);
        }
    }

    /// Runs every job that arrived before each wait for a window message, so that a doorbell
    /// that a nested message loop of the webview swallowed never strands a job, until the
    /// session ends or a quit message arrives.
    fn serve(&self, state: &T, queue: &mut Queue) {
        while self.drain(state).is_continue() && queue.wait() {
            queue.dispatch();
        }
    }

    /// Runs every job in the channel with `state`, stopping once the session ends.
    fn drain(&self, state: &T) -> ControlFlow<()> {
        while let Some(job) = self.next()? {
            job(state);
        }
        ControlFlow::Continue(())
    }

    /// Returns the next job, none when the channel is empty, or the end of the session.
    fn next(&self) -> ControlFlow<(), Option<Job<T>>> {
        if self.ending.load(Ordering::Acquire) {
            return ControlFlow::Break(());
        }
        match self.inbox.try_recv() {
            Ok(job) => ControlFlow::Continue(Some(job)),
            Err(TryRecvError::Empty) => ControlFlow::Continue(None),
            Err(TryRecvError::Disconnected) => ControlFlow::Break(()),
        }
    }
}
