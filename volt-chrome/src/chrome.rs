use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::runtime::Handle;
use tokio::sync::Mutex;
use volt_browser::{Browser, Engine, Settings};

use super::options::Options;
use super::profile::TempProfile;
use super::session::Session;
use super::tab::Tab;

/// Chrome as a browser for the commands of [`volt_browser`]: launch it and get a
/// [`volt_browser::Session`], whose pages take every selector, action and storage command.
///
/// The browser launches without the switches that announce automation, in a temporary profile
/// that goes away with it. [`Settings::visible`] decides between a window and none, and
/// [`Settings::user_agent`] overrides the user agent. [`Settings::timeout`] is the time the
/// launch gets, and chromiumoxide gives up on a protocol call after one to two times as long; a
/// load keeps chromiumoxide's own 30 seconds.
///
/// Every failure is a [`volt_browser::Error`]; what only Chrome can fail with, as a launch that
/// finds no binary or runs out of time, or a protocol call chromiumoxide gives up on, is its
/// [`Engine`](volt_browser::Error::Engine) kind, with the reason as its message.
#[derive(Clone, Debug, Default)]
pub struct Chrome {
    /// The Chrome or Chromium binary to run, or `None` to let chromiumoxide find one.
    pub path: Option<PathBuf>,
    /// Further command-line switches, without the leading `--`, added after the built-in ones.
    pub extra_args: Vec<String>,
}

impl Chrome {
    /// Launches Chrome as `settings` say and returns the session over it. It must be awaited
    /// inside a Tokio runtime with timers.
    ///
    /// # Errors
    ///
    /// Outside a Tokio runtime it fails with [`volt_browser::Error::NoRuntime`]. A browser that
    /// cannot be configured, or that has not started within [`Settings::timeout`], fails with
    /// [`volt_browser::Error::Engine`].
    pub async fn launch(
        self,
        settings: Settings,
    ) -> Result<volt_browser::Session, volt_browser::Error> {
        let runtime = Handle::try_current().map_err(|_| volt_browser::Error::NoRuntime)?;
        self.launch_in(TempProfile::create(), runtime, settings)
            .await
    }

    /// Launches Chrome in `profile` as `settings` say and returns the session over it, which
    /// removes the profile once the browser has quit.
    async fn launch_in(
        self,
        profile: TempProfile,
        runtime: Handle,
        settings: Settings,
    ) -> Result<volt_browser::Session, volt_browser::Error> {
        let options = self.options(&settings);
        let session = Session::launch(&options, profile.path()).await?;
        let launched = Launched {
            session: Mutex::new(Some(session)),
            profile: Some(profile),
            headed: settings.visible,
            runtime,
        };
        volt_browser::Session::new(launched, settings)
    }

    /// Returns the launch options `settings` ask for.
    fn options(self, settings: &Settings) -> Options {
        let mut extra_args = self.extra_args;
        if let Some(user_agent) = &settings.user_agent {
            extra_args.push(format!("user-agent={user_agent}"));
        }
        Options {
            path: self.path,
            timeout: settings.timeout,
            headed: settings.visible,
            extra_args,
        }
    }
}

/// A launched Chrome with the temporary profile it runs in, which is removed only once the
/// browser's process has ended, so that nothing writes to it any more.
#[derive(Debug)]
struct Launched {
    /// The running browser, until it is closed.
    session: Mutex<Option<Session>>,
    /// The browser's user data directory, until it is removed.
    profile: Option<TempProfile>,
    /// Whether the browser has a window.
    headed: bool,
    /// The runtime that quits a browser dropped without being closed.
    runtime: Handle,
}

#[async_trait]
impl Browser for Launched {
    async fn open(&self) -> Result<Arc<dyn Engine>, volt_browser::Error> {
        let session = self.session.lock().await;
        let session = session.as_ref().ok_or(volt_browser::Error::Closed)?;
        let page = session.new_page().await?;
        Ok(Arc::new(Tab::new(page, self.headed)))
    }

    async fn close(&self) {
        let session = self.session.lock().await.take();
        if let Some(session) = session {
            session.quit().await;
        }
    }
}

impl Drop for Launched {
    fn drop(&mut self) {
        let profile = self.profile.take();
        if let Some(session) = self.session.get_mut().take() {
            self.runtime.spawn(async move {
                session.quit().await;
                drop(profile);
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::Duration;

    use serde_json::Value;
    use tokio::runtime::Handle;
    use volt_browser::Settings;
    use volt_browser_checks::Checks;

    use super::{Chrome, TempProfile};

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "launches Chrome"]
    async fn a_closed_session_leaves_no_profile_behind() {
        let profile = TempProfile::create();
        let directory = profile.path().to_owned();
        let session = Chrome::default()
            .launch_in(profile, Handle::current(), Settings::default())
            .await
            .unwrap();
        let page = session.new_page().await.unwrap();
        page.goto("data:text/html,<title>volt</title>")
            .await
            .unwrap();
        session.close().await;
        assert!(!directory.exists(), "{directory:?}");
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "launches Chrome"]
    async fn chrome_passes_every_shared_check() {
        let session = Chrome::default().launch(Settings::default()).await.unwrap();
        let failures = Checks::run(&session).await.unwrap();
        session.close().await;
        assert!(failures.is_empty(), "{failures:#?}");
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "launches Chrome with a window"]
    async fn chrome_passes_every_shared_check_in_a_shown_window() {
        let settings = Settings {
            visible: true,
            ..Settings::default()
        };
        let session = Chrome::default().launch(settings).await.unwrap();
        let failures = Checks::run(&session).await.unwrap();
        session.close().await;
        assert!(failures.is_empty(), "{failures:#?}");
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "launches Chrome"]
    async fn closing_the_session_closes_its_pages() {
        let session = Chrome::default().launch(Settings::default()).await.unwrap();
        let page = session.new_page().await.unwrap();
        session.close().await;
        let outcome = page.evaluate::<Value>("1").await;
        assert!(
            matches!(outcome, Err(volt_browser::Error::Closed)),
            "{outcome:?}"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "launches Chrome"]
    async fn a_script_that_does_not_parse_fails_as_a_script() {
        let session = Chrome::default().launch(Settings::default()).await.unwrap();
        let page = session.new_page().await.unwrap();
        let outcome = page.evaluate::<Value>("1 +").await;
        session.close().await;
        assert!(
            matches!(outcome, Err(volt_browser::Error::Script(_))),
            "{outcome:?}"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "launches Chrome"]
    async fn a_launch_ends_at_the_timeout() {
        let settings = Settings {
            timeout: Duration::from_millis(1),
            ..Settings::default()
        };
        let outcome = Chrome::default().launch(settings).await;
        assert!(outcome.is_err(), "{outcome:?}");
    }

    #[test]
    fn the_settings_become_the_launch_options() {
        let chrome = Chrome {
            path: Some(PathBuf::from("/opt/chrome")),
            extra_args: vec!["lang=de".to_owned()],
        };
        let settings = Settings {
            visible: true,
            user_agent: Some("Agent/1.0".to_owned()),
            ..Settings::default()
        };
        let options = chrome.options(&settings);
        assert_eq!(options.path, Some(PathBuf::from("/opt/chrome")));
        assert_eq!(options.timeout, settings.timeout);
        assert!(options.headed);
        assert_eq!(options.extra_args, ["lang=de", "user-agent=Agent/1.0"]);
    }

    #[test]
    fn the_default_runs_the_found_chrome_without_a_window() {
        let options = Chrome::default().options(&Settings::default());
        assert_eq!(options.path, None);
        assert!(!options.headed);
        assert_eq!(options.extra_args, [""; 0]);
    }
}
