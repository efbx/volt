/// The failures of reading and writing credentials.
#[derive(Debug, thiserror::Error)]
pub enum CredError {
    /// The named environment variable is not set.
    #[error("environment variable not set: {0}")]
    EnvVarNotFound(String),
    /// The keyring holds no complete identity for the named label.
    #[error("no identity stored for {0}")]
    NoIdentity(String),
    /// The OS keyring reported an error, carried as its message.
    #[error("keyring error: {0}")]
    KeyRing(String),
    /// Any other failure, such as an invalid variable name or value, a non UTF-8 variable or a
    /// corrupted keyring entry.
    #[error("{0}")]
    Other(String),
}

impl From<keyring_core::Error> for CredError {
    fn from(error: keyring_core::Error) -> Self {
        Self::KeyRing(error.to_string())
    }
}
