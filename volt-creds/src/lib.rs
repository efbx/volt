//! Credential identities read from the environment or the OS keyring behind one reader and
//! writer interface.
//!
//! A [`Manager`] binds one label to an [`IdentityProvider`] and implements [`IdentityReader`],
//! [`IdentityWriter`] and [`IdentityManager`] over it. [`EnvProvider`] keeps each [`Identity`] in
//! environment variables, [`KeyProvider`] keeps every label of an application in one OS keyring
//! entry, [`Mode`] picks between the two, [`Cached`] wraps either so each label is read from it
//! once, and every failure is a [`CredError`].

#![warn(missing_docs)]

mod access;
mod cached;
mod env;
mod error;
mod identity;
mod key;
mod manager;
mod provider;

pub use access::{IdentityManager, IdentityReader, IdentityWriter};
pub use cached::Cached;
pub use env::EnvProvider;
pub use error::CredError;
pub use identity::{Identity, IdentityPayload};
pub use key::KeyProvider;
pub use manager::Manager;
pub use provider::{IdentityProvider, Mode};
