use std::time::Duration;

/// The timeout [`Settings::default`] uses: 60 seconds.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_mins(1);

/// How often [`Settings::default`] lets a waiting command look again: every 100 milliseconds.
const DEFAULT_POLL: Duration = Duration::from_millis(100);

/// What every engine is told when it launches and every command follows; the default keeps
/// windows hidden and the engine's own user agent, and uses [`DEFAULT_TIMEOUT`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    /// The most any command waits, and the limit for [`Session::run`](crate::Session::run)'s
    /// job.
    pub timeout: Duration,
    /// How often a waiting command looks at the page again.
    pub poll: Duration,
    /// Whether the browser shows its windows; default `false`.
    pub visible: bool,
    /// A full user agent override; `None` keeps the engine's own.
    pub user_agent: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            timeout: DEFAULT_TIMEOUT,
            poll: DEFAULT_POLL,
            visible: false,
            user_agent: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{DEFAULT_TIMEOUT, Settings};

    #[test]
    fn default_settings_hide_the_window_and_keep_the_user_agent() {
        let settings = Settings::default();
        assert_eq!(settings.timeout, DEFAULT_TIMEOUT);
        assert_eq!(DEFAULT_TIMEOUT.as_secs(), 60);
        assert_eq!(settings.poll, Duration::from_millis(100));
        assert!(!settings.visible);
        assert_eq!(settings.user_agent, None);
    }
}
