use std::sync::Arc;

use super::{CredError, EnvProvider, Identity, IdentityPayload, KeyProvider};

/// A credential store that holds identities keyed by label.
pub trait IdentityProvider: Send + Sync {
    /// Returns the complete identity of `label`, failing when either half is missing.
    fn identity(&self, label: &str) -> Result<Identity, CredError>;
    /// Returns the user of `label`.
    fn user(&self, label: &str) -> Result<String, CredError>;
    /// Returns the secret of `label`.
    fn secret(&self, label: &str) -> Result<String, CredError>;
    /// Whether `label` has both a user and a secret, without returning them.
    fn present(&self, label: &str) -> bool;
    /// Writes the `Some` halves of `payload` for `label` and leaves the others as stored.
    fn apply(&self, label: &str, payload: &IdentityPayload<'_>) -> Result<(), CredError>;
    /// Removes both halves of `label`. Removing a label that holds nothing succeeds.
    fn remove(&self, label: &str) -> Result<(), CredError>;
}

/// The kinds of credential store, chosen by name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// The OS keyring, through [`KeyProvider`].
    KeyRing,
    /// The process environment, through [`EnvProvider`].
    Env,
}

impl Mode {
    /// Builds the provider for this mode. `app` names the keyring entry and is ignored by
    /// [`Mode::Env`].
    #[must_use]
    pub fn provider(self, app: &str) -> Arc<dyn IdentityProvider> {
        match self {
            Self::KeyRing => Arc::new(KeyProvider::new(app)),
            Self::Env => Arc::new(EnvProvider::new()),
        }
    }
}
