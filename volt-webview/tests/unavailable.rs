//! Where no webview can run, a session fails at once with a clear error; nothing here opens a
//! window. Under the default harness every test runs on a thread of its own while the main
//! thread waits for the results, so on macOS no event loop serves the main thread.

#[cfg(target_os = "macos")]
use std::time::{Duration, Instant};

#[cfg(any(
    target_os = "macos",
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd"
))]
use volt_browser::{Session, Settings};
#[cfg(any(
    target_os = "macos",
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd"
))]
use volt_webview::Webview;

/// What a launch gave where no webview can run.
#[cfg(any(
    target_os = "macos",
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd"
))]
struct Launch;

#[cfg(any(
    target_os = "macos",
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd"
))]
impl Launch {
    /// Whether `launched` failed because the webview is unavailable here.
    fn was_unavailable(launched: &Result<Session, volt_browser::Error>) -> bool {
        matches!(
            launched,
            Err(volt_browser::Error::Engine(cause))
                if cause.to_string().starts_with("the system webview cannot run here")
        )
    }
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn a_session_without_an_event_loop_on_the_main_thread_is_unavailable_after_a_second() {
    let start = Instant::now();
    let launched = Webview.launch(Settings::default()).await;

    assert!(Launch::was_unavailable(&launched), "{launched:?}");
    assert!(
        start.elapsed() < Duration::from_millis(1500),
        "{:?}",
        start.elapsed()
    );
}

#[cfg(any(
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd"
))]
mod without_a_display {
    use std::env;
    use std::process::Command;
    use std::time::{Duration, Instant};

    use super::{Launch, Settings, Webview};

    /// The variable that tells a process it is the one its parent test started.
    const CHILD: &str = "VOLT_WEBVIEW_CHILD_CASE";

    #[test]
    fn a_session_in_a_process_without_a_display_is_unavailable_at_once() {
        let ended = Command::new(env::current_exe().unwrap())
            .args(["--ignored", "--exact"])
            .arg("without_a_display::child_a_session_without_a_display")
            .env(CHILD, "1")
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY")
            .status()
            .unwrap();
        assert!(ended.success(), "{ended}");
    }

    #[tokio::test]
    #[ignore = "runs in the process its parent test starts"]
    async fn child_a_session_without_a_display() {
        if env::var_os(CHILD).is_none() {
            return;
        }
        let start = Instant::now();
        let launched = Webview.launch(Settings::default()).await;
        assert!(Launch::was_unavailable(&launched), "{launched:?}");
        assert!(
            start.elapsed() < Duration::from_secs(1),
            "{:?}",
            start.elapsed()
        );
    }
}
