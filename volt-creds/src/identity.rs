use std::borrow::Cow;

/// A user and the secret that authenticates it.
#[derive(Debug, Clone, Default)]
pub struct Identity {
    /// The user name or login.
    pub user: String,
    /// The password or other secret for [`Identity::user`].
    pub secret: String,
}

/// The halves of an identity to write; a `None` half is left as stored. Each half borrows its
/// text or owns it, so a payload is written without copies and kept as
/// `IdentityPayload<'static>` when it owns everything.
#[derive(Debug, Clone, Default)]
pub struct IdentityPayload<'a> {
    /// The user to store, or `None` to keep the current one.
    pub user: Option<Cow<'a, str>>,
    /// The secret to store, or `None` to keep the current one.
    pub secret: Option<Cow<'a, str>>,
}
