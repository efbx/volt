use std::borrow::Cow;
use std::future::{Future, poll_fn};
use std::mem;
use std::panic::{self, AssertUnwindSafe};
use std::pin::pin;
use std::sync::Arc;
use std::time::Duration;

use dispatch2::{DispatchQueue, MainThreadBound};
use objc2::MainThreadMarker;
use objc2::rc::autoreleasepool;
use objc2_core_foundation::{CFRunLoop, CFString};
use tokio::sync::oneshot::{self, Sender};
use tokio::time;

use crate::Error;

/// Why no session can start: nothing serves the main thread, where every window lives.
pub(super) const NO_EVENT_LOOP: &str = "the main thread runs no event loop; await the call on the main thread, or run an event loop there";
/// Why a launch awaited by blocking the main thread from inside its event loop cannot start.
const BLOCKED: &str =
    "the main thread is blocked waiting for this call; await it from a task instead";
/// How long a launch from another thread looks for the main thread's event loop.
const GRACE: Duration = Duration::from_secs(1);
/// How often a launch from another thread looks at the main thread's run loop meanwhile.
const SAMPLE: Duration = Duration::from_millis(20);
/// The longest the main thread's run loop runs, in seconds, each time a call polled on the main
/// thread outside any event loop is still waiting.
const SLICE: f64 = 0.05;
/// The name of the run loop's default mode, `kCFRunLoopDefaultMode`, which serves the main
/// dispatch queue.
const DEFAULT_MODE: &str = "kCFRunLoopDefaultMode";

/// A session's state on the main thread, reached from any thread through jobs on the main
/// dispatch queue, which the main thread's run loop serves.
pub(crate) struct UiThread<T: 'static> {
    /// The state, shared with the jobs still queued; `None` only while it is dropped.
    state: Option<Arc<MainThreadBound<T>>>,
}

impl<T: 'static> UiThread<T> {
    /// Creates the state on the main thread with `make` and returns once it exists, waiting at
    /// most `timeout`. From another thread it first looks for the main thread's run loop for a
    /// second at most, and fails when it never sees it run.
    pub(crate) async fn start(
        timeout: Duration,
        make: impl FnOnce() -> T + Send + 'static,
    ) -> Result<Self, Error> {
        MainThread::reachable(timeout).await?;
        let (ready, started) = oneshot::channel();
        DispatchQueue::main().exec_async(move || {
            MainThread::on_main(|mtm| Self::create(mtm, make, ready));
        });
        match CallerThread::wait(time::timeout(timeout, started)).await {
            Ok(Ok(ui)) => Ok(ui),
            Ok(Err(_)) => Err(Error::Unavailable(Cow::Borrowed(NO_EVENT_LOOP))),
            Err(_) => Err(MainThread::stalled(timeout)),
        }
    }

    /// Runs `job` with the state on the main thread, after the jobs queued before it.
    pub(crate) fn run(&self, job: impl FnOnce(&T) + Send + 'static) {
        if let Some(state) = &self.state {
            let state = Arc::clone(state);
            DispatchQueue::main().exec_async(move || MainThread::with(state, job));
        }
    }

    /// Creates the state with `make` on the main thread and hands its handle to `ready`; a caller
    /// that stopped waiting drops the handle here, which queues the state's drop.
    fn create(mtm: MainThreadMarker, make: impl FnOnce() -> T, ready: Sender<Self>) {
        let state = MainThreadBound::new(make(), mtm);
        let _ = ready.send(Self {
            state: Some(Arc::new(state)),
        });
    }
}

impl<T: 'static> Drop for UiThread<T> {
    fn drop(&mut self) {
        if let Some(state) = self.state.take() {
            DispatchQueue::main().exec_async(move || MainThread::with(state, |_| {}));
        }
    }
}

/// The thread a call is awaited on. When that is the main thread and no event loop runs there,
/// as in a terminal program that awaits the call in `main`, it runs the main thread's run loop
/// while it waits, which serves the call.
pub(crate) struct CallerThread;

impl CallerThread {
    /// Waits for `future`, running the main thread's run loop between polls when this is the
    /// main thread and nothing else runs that loop.
    pub(crate) async fn wait<F: Future>(future: F) -> F::Output {
        let mut future = pin!(future);
        poll_fn(|context| {
            let polled = future.as_mut().poll(context);
            if polled.is_pending() && MainThread::idle_here() {
                MainThread::run_once();
                context.waker().wake_by_ref();
            }
            polled
        })
        .await
    }
}

/// The main thread, where every window and webview lives.
struct MainThread;

impl MainThread {
    /// Returns once the main thread can be reached: at once on the main thread, otherwise once
    /// its run loop is seen running, which fails after a second, or `timeout` when shorter.
    async fn reachable(timeout: Duration) -> Result<(), Error> {
        if MainThreadMarker::new().is_some() {
            return Ok(());
        }
        time::timeout(GRACE.min(timeout), Self::running())
            .await
            .map_err(|_| Error::Unavailable(Cow::Borrowed(NO_EVENT_LOOP)))
    }

    /// Returns once the main thread's run loop is seen running.
    async fn running() {
        while !Self::loop_running() {
            time::sleep(SAMPLE).await;
        }
    }

    /// Whether the main thread's run loop runs in some mode right now; it does not in a terminal
    /// program, a server, a test under libtest, or while `main` blocks outside any event loop.
    fn loop_running() -> bool {
        CFRunLoop::main().is_some_and(|main| main.current_mode().is_some())
    }

    /// Whether this is the main thread, and no run loop runs there.
    fn idle_here() -> bool {
        MainThreadMarker::new().is_some() && !Self::loop_running()
    }

    /// Runs the main thread's run loop in its default mode until one source has been handled,
    /// the main dispatch queue included, or for a short slice at most.
    fn run_once() {
        autoreleasepool(|_| {
            let mode = CFString::from_static_str(DEFAULT_MODE);
            let _ = CFRunLoop::run_in_mode(Some(&mode), SLICE, true);
        });
    }

    /// Returns why a launch got no answer in `timeout`: a caller on the main thread whose run
    /// loop runs is blocking that thread from inside its event loop; otherwise the main thread
    /// was too busy.
    fn stalled(timeout: Duration) -> Error {
        if MainThreadMarker::new().is_some() && Self::loop_running() {
            Error::Unavailable(Cow::Borrowed(BLOCKED))
        } else {
            Error::Timeout(timeout)
        }
    }

    /// Runs `work` with the main thread's marker, or drops it unrun where the main dispatch queue
    /// runs on another thread, as it does after `dispatch_main`, where no window can exist.
    fn on_main(work: impl FnOnce(MainThreadMarker)) {
        if let Some(mtm) = MainThreadMarker::new() {
            Self::guarded(|| work(mtm));
        }
    }

    /// Runs `work` with the state on the main thread, then lets go of the state there; where the
    /// main dispatch queue runs on another thread, the state is leaked instead, since
    /// `MainThreadBound` would wait for the main thread forever to drop it.
    fn with<T>(state: Arc<MainThreadBound<T>>, work: impl FnOnce(&T)) {
        match MainThreadMarker::new() {
            Some(mtm) => Self::guarded(|| {
                work(state.get(mtm));
                drop(state);
            }),
            None => mem::forget(state),
        }
    }

    /// Runs `work` in an autorelease pool and stops a panic from unwinding into libdispatch,
    /// which would abort the program.
    fn guarded(work: impl FnOnce()) {
        let _ = panic::catch_unwind(AssertUnwindSafe(|| autoreleasepool(|_| work())));
    }
}
