use chromiumoxide::error::CdpError;

/// A failure to configure, launch or drive the browser, which a caller gets as the
/// [`volt_browser::Error`] it turns into.
#[derive(Debug, thiserror::Error)]
pub(super) enum Error {
    /// The launch configuration could not be built, for example when no Chrome binary
    /// is found.
    #[error("browser config: {0}")]
    Config(String),
    /// Launching the browser or a `DevTools` protocol call failed.
    #[error("{0}")]
    Cdp(#[from] CdpError),
}

impl From<Error> for volt_browser::Error {
    fn from(error: Error) -> Self {
        match error {
            Error::Cdp(CdpError::ChannelSendError(_)) => Self::Closed,
            Error::Cdp(CdpError::JavascriptException(details)) => Self::Script(
                details
                    .exception
                    .and_then(|exception| exception.description)
                    .unwrap_or(details.text),
            ),
            error @ (Error::Config(_) | Error::Cdp(_)) => Self::Engine(Box::new(error)),
        }
    }
}

#[cfg(test)]
mod tests {
    use chromiumoxide::error::CdpError;
    use futures::channel::oneshot::Canceled;

    use super::Error;

    #[test]
    fn a_page_whose_channel_is_gone_is_closed() {
        assert!(matches!(
            volt_browser::Error::from(Error::Cdp(CdpError::from(Canceled))),
            volt_browser::Error::Closed
        ));
    }

    #[test]
    fn a_configuration_that_cannot_be_built_is_the_engines_failure() {
        let config = volt_browser::Error::from(Error::Config("no binary".to_owned()));
        assert!(matches!(
            config,
            volt_browser::Error::Engine(cause) if cause.to_string() == "browser config: no binary"
        ));
    }
}
