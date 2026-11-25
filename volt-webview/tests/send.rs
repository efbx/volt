//! The launcher and the future of a whole job cross threads; nothing here opens a window.

use std::future::Future;

use volt_browser::Settings;
use volt_webview::Webview;

/// Compile-time checks on thread safety.
struct Assert;

impl Assert {
    /// Compiles only when `T` can be sent to and shared between threads.
    fn send_sync<T: Send + Sync>() {}

    /// Compiles only when `future` can move between threads.
    fn send<T: Future + Send>(future: T) -> T {
        future
    }
}

#[test]
fn the_launcher_and_a_jobs_future_cross_threads() {
    Assert::send_sync::<Webview>();
    drop(Assert::send(async {
        let session = Webview.launch(Settings::default()).await?;
        session
            .run(|page| async move {
                page.goto("about:blank").await?;
                page.title().await?;
                page.cookies().await
            })
            .await
    }));
}
