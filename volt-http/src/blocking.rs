use std::future::Future;
use std::sync::OnceLock;

use tokio::runtime::{Builder, Handle, Runtime, RuntimeFlavor};

/// Runs `future` to completion from synchronous code, inside or outside a Tokio runtime.
///
/// A multi-thread runtime the calling thread has entered drives the request itself, blocking in
/// place so the worker is handed off first; that runtime must have its IO and time drivers
/// enabled. Only without a runtime, or on a current-thread runtime that cannot be blocked, does
/// the request run on the shared [`runtime`].
pub(crate) fn block_on<F>(future: F) -> F::Output
where
    F: Future + Send,
    F::Output: Send,
{
    match Handle::try_current() {
        Err(_) => runtime().block_on(future),
        Ok(handle) if handle.runtime_flavor() == RuntimeFlavor::MultiThread => {
            tokio::task::block_in_place(|| handle.block_on(future))
        }
        Ok(_) => std::thread::scope(|scope| {
            scope
                .spawn(|| runtime().block_on(future))
                .join()
                .expect("blocking request thread panicked")
        }),
    }
}

/// Returns the shared two-worker runtime for blocking calls, started on first use.
fn runtime() -> &'static Runtime {
    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("volt-http")
            .enable_all()
            .build()
            .expect("failed to start the volt_http runtime")
    })
}
