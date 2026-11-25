use std::fmt::{self, Debug};
use std::future::Future;
use std::panic::{self, AssertUnwindSafe};
use std::time::Duration;

use tokio::runtime::Handle;
use tokio::sync::oneshot;
use tokio::time::{self, Timeout};
use volt_browser::Settings;

use crate::Error;
use crate::pages::{Pages, Reply, Setup};
use crate::platform::{CallerThread, UiThread};

/// The way from a session's handles to its pages on the thread that may touch them, with the most
/// any call waits and the Tokio runtime whose timers the calls use.
pub(crate) struct Link {
    /// The thread the pages live on.
    ui: UiThread<Pages>,
    /// The most any call waits for its answer.
    timeout: Duration,
    /// The runtime the session was launched in, whose timers bound every call, so that a page
    /// can be used from any executor while that runtime runs.
    runtime: Handle,
}

impl Debug for Link {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Link")
            .field("timeout", &self.timeout)
            .finish_non_exhaustive()
    }
}

impl Link {
    /// Reaches the thread that may touch webviews and puts a session's pages there, as `settings`
    /// say, waiting at most `settings.timeout`.
    pub(crate) async fn start(settings: &Settings) -> Result<Self, Error> {
        let runtime = Handle::try_current().map_err(|_| Error::NoRuntime)?;
        let timeout = settings.timeout;
        let setup = Setup {
            visible: settings.visible,
            user_agent: settings.user_agent.clone(),
        };
        let ui = UiThread::start(timeout, move || Pages::new(setup)).await?;
        Ok(Self {
            ui,
            timeout,
            runtime,
        })
    }

    /// Runs `job` with the pages on their thread and waits, at most the link's timeout, for the
    /// answer it sends to its reply.
    pub(crate) async fn call<T, F>(&self, job: F) -> Result<T, Error>
    where
        T: Send + 'static,
        F: FnOnce(&Pages, Reply<T>) + Send + 'static,
    {
        self.call_within(self.timeout, job).await
    }

    /// Runs `job` with the pages on their thread and waits, at most `timeout`, for the answer it
    /// sends to its reply. A job whose caller stopped waiting before the job's turn came, as
    /// after a timeout while the thread was busy, never runs, so that it neither opens a window
    /// nobody waits for nor replaces a live caller's goto. A job that panics drops its reply, so
    /// the call fails with [`Error::Closed`].
    pub(crate) async fn call_within<T, F>(&self, timeout: Duration, job: F) -> Result<T, Error>
    where
        T: Send + 'static,
        F: FnOnce(&Pages, Reply<T>) + Send + 'static,
    {
        let (reply, answer) = oneshot::channel();
        self.ui.run(move |pages| {
            if !reply.is_closed() {
                Self::guarded(|| job(pages, reply));
            }
        });
        match CallerThread::wait(self.timed(timeout, answer)).await {
            Ok(Ok(outcome)) => outcome,
            Ok(Err(_)) => Err(Error::Closed),
            Err(_) => Err(Error::Timeout(timeout)),
        }
    }

    /// Runs `job` with the pages on their thread and waits, at most the link's timeout, for what
    /// it returns.
    pub(crate) async fn ask<T, F>(&self, job: F) -> Result<T, Error>
    where
        T: Send + 'static,
        F: FnOnce(&Pages) -> Result<T, Error> + Send + 'static,
    {
        self.call(move |pages, reply| {
            let _ = reply.send(job(pages));
        })
        .await
    }

    /// Closes every page of the session on their thread, without waiting.
    pub(crate) fn close_all(&self) {
        self.ui.run(|pages| Self::guarded(|| pages.close_all()));
    }

    /// Returns `future` bounded by `timeout` on the runtime's timers, whichever executor polls
    /// it.
    fn timed<F: Future>(&self, timeout: Duration, future: F) -> Timeout<F> {
        let _entered = self.runtime.enter();
        time::timeout(timeout, future)
    }

    /// Runs `job`, stopping a panic in it at the end of the job, which drops whatever reply the
    /// job still held.
    fn guarded(job: impl FnOnce()) {
        let _ = panic::catch_unwind(AssertUnwindSafe(job));
    }
}
