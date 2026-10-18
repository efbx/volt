use super::{CredError, Identity, IdentityPayload};

/// Read access to the identity stored under one label.
pub trait IdentityReader: Send + Sync {
    /// Returns the label this reader is bound to.
    fn cred_label(&self) -> &str;
    /// Returns the complete identity, failing when either the user or the secret is missing.
    fn get(&self) -> Result<Identity, CredError>;
    /// Returns the stored user alone.
    fn get_user(&self) -> Result<String, CredError>;
    /// Returns the stored secret alone.
    fn get_secret(&self) -> Result<String, CredError>;
    /// Whether both the user and the secret are stored. Defaults to [`IdentityReader::get`]
    /// succeeding.
    fn present(&self) -> bool {
        self.get().is_ok()
    }
}

/// Write access to the identity stored under one label.
pub trait IdentityWriter: Send + Sync {
    /// Returns the label this writer is bound to.
    fn cred_label(&self) -> &str;
    /// Stores both the user and the secret of `identity`, replacing what was there.
    fn set_all(&self, identity: &Identity) -> Result<(), CredError>;
    /// Stores the halves of `payload` that are `Some` and leaves the others untouched.
    fn set_payload(&self, payload: &IdentityPayload<'_>) -> Result<(), CredError>;
    /// Stores `user` and leaves the secret untouched.
    fn set_user(&self, user: &str) -> Result<(), CredError>;
    /// Stores `secret` and leaves the user untouched.
    fn set_secret(&self, secret: &str) -> Result<(), CredError>;
    /// Removes the user and the secret. Removing a label that holds nothing succeeds.
    fn delete(&self) -> Result<(), CredError>;
}

/// Read and write access to one label, implemented for every type that is both an
/// [`IdentityReader`] and an [`IdentityWriter`].
pub trait IdentityManager: IdentityReader + IdentityWriter {}

impl<T: IdentityReader + IdentityWriter> IdentityManager for T {}
