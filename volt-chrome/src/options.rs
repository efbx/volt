use std::path::{Path, PathBuf};
use std::time::Duration;

use chromiumoxide::browser::BrowserConfig;

use super::Error;

/// The timeout [`Options::default`] uses: 60 seconds.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);

/// How to launch the browser; the default is headless, with the binary chromiumoxide
/// finds and [`DEFAULT_TIMEOUT`].
#[derive(Debug)]
pub struct Options {
    /// The Chrome or Chromium binary to run, or `None` to let chromiumoxide find one.
    pub path: Option<PathBuf>,
    /// How long [`Session::run`](crate::Session::run) lets its job take, opening the page
    /// included.
    pub timeout: Duration,
    /// Whether to show the browser window instead of running headless.
    pub headed: bool,
    /// Further command-line switches, without the leading `--`, added after the built-in
    /// ones.
    pub extra_args: Vec<String>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            path: None,
            timeout: DEFAULT_TIMEOUT,
            headed: false,
            extra_args: Vec::new(),
        }
    }
}

// Replaces chromiumoxide's DEFAULT_ARGS: must omit --enable-automation, else CloudFlare detects it.
/// The switches used in place of chromiumoxide's defaults, which leave out
/// `--enable-automation` and disable the automation-controlled blink feature.
const ANTI_DETECT_ARGS: &[&str] = &[
    "no-first-run",
    "no-default-browser-check",
    "disable-blink-features=AutomationControlled",
    "disable-infobars",
    "disable-dev-shm-usage",
    "disable-background-networking",
    "disable-sync",
    "metrics-recording-only",
    "mute-audio",
    "enable-features=NetworkService,NetworkServiceInProcess",
    "disable-background-timer-throttling",
    "disable-backgrounding-occluded-windows",
    "disable-breakpad",
    "disable-client-side-phishing-detection",
    "disable-component-extensions-with-background-pages",
    "disable-default-apps",
    "disable-features=TranslateUI",
    "disable-hang-monitor",
    "disable-ipc-flooding-protection",
    "disable-popup-blocking",
    "disable-prompt-on-repost",
    "disable-renderer-backgrounding",
    "force-color-profile=srgb",
    "password-store=basic",
    "use-mock-keychain",
    "enable-blink-features=IdleDetection",
    "lang=en_US",
];

impl Options {
    /// Builds the chromiumoxide configuration: a 1920 by 1080 window without viewport
    /// emulation or sandbox, the quiet launch switches, then `extra_args`, with `profile`
    /// as the user data directory when given. Fails with [`Error::Config`] when
    /// chromiumoxide rejects the configuration.
    pub(super) fn build_config(&self, profile: Option<&Path>) -> Result<BrowserConfig, Error> {
        let mut builder = BrowserConfig::builder()
            .disable_default_args()
            .no_sandbox()
            .window_size(1920, 1080)
            .viewport(None);

        if let Some(dir) = profile {
            builder = builder.user_data_dir(dir);
        }
        if let Some(ref path) = self.path {
            builder = builder.chrome_executable(path);
        }
        if self.headed {
            builder = builder.with_head();
        }
        for arg in ANTI_DETECT_ARGS {
            builder = builder.arg(*arg);
        }
        for arg in &self.extra_args {
            builder = builder.arg(arg.as_str());
        }

        builder.build().map_err(Error::Config)
    }
}
