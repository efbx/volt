//! A call awaited on the main thread of a program that runs no event loop there, as in a
//! terminal program's `#[tokio::main]`: on macOS the call runs the main thread's run loop while
//! it waits, which serves it. The test opens a webview window, so it runs only when asked for
//! with `--ignored` or `--include-ignored`; other platforms bring their own window thread and
//! have nothing to test here.

use std::env;
use std::process::ExitCode;

use tokio::runtime::Builder;
use volt_browser::{Error, Settings};
use volt_webview::Webview;

/// The test's name.
const NAME: &str = "a_call_awaited_on_an_idle_main_thread_is_served";
/// A page titled `volt`, as a data URL.
const TITLED_PAGE: &str = "data:text/html,%3Ctitle%3Evolt%3C%2Ftitle%3E";

/// The one test of this binary.
struct IdleMainThread;

impl IdleMainThread {
    /// Whether the command line asks for ignored tests, as libtest's flags do.
    fn asked() -> bool {
        env::args().any(|arg| arg == "--ignored" || arg == "--include-ignored")
    }

    /// Opens a session, loads the titled page in one of its pages and reads the title.
    async fn title() -> Result<String, Error> {
        let session = Webview.launch(Settings::default()).await?;
        session
            .run(|page| async move {
                page.goto(TITLED_PAGE).await?;
                page.title().await
            })
            .await
    }

    /// Runs a job from the main thread, inside a runtime's `block_on` and outside any event
    /// loop, and returns whether it read the page's title.
    fn passes() -> bool {
        let runtime = Builder::new_multi_thread().enable_all().build().unwrap();
        let title = runtime.block_on(Self::title());
        matches!(&title, Ok(title) if title == "volt")
    }
}

fn main() -> ExitCode {
    if !IdleMainThread::asked() || cfg!(not(target_os = "macos")) {
        println!("\nrunning 1 test\ntest {NAME} ... ignored, opens a webview window on macOS\n");
        return ExitCode::SUCCESS;
    }
    let passed = IdleMainThread::passes();
    println!("test {NAME} ... {}", if passed { "ok" } else { "FAILED" });
    if passed {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
