use std::collections::BTreeMap;
use std::sync::{Mutex, Once};

use keyring_core::Entry;
use serde::{Deserialize, Serialize};

use super::{CredError, Identity, IdentityPayload, IdentityProvider};

/// The stored halves of one label inside the keyring entry.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Slot {
    user: Option<String>,
    secret: Option<String>,
}

impl Slot {
    /// Whether both the user and the secret are stored.
    fn is_complete(&self) -> bool {
        self.user.is_some() && self.secret.is_some()
    }

    /// Overwrites each half that `payload` sets and keeps the others.
    fn merge(&mut self, payload: &IdentityPayload<'_>) {
        if let Some(user) = payload.user.as_deref() {
            self.user = Some(user.to_owned());
        }
        if let Some(secret) = payload.secret.as_deref() {
            self.secret = Some(secret.to_owned());
        }
    }

    /// Returns the complete identity, or [`CredError::NoIdentity`] for `label` when a half is
    /// missing.
    fn into_identity(self, label: &str) -> Result<Identity, CredError> {
        match (self.user, self.secret) {
            (Some(user), Some(secret)) => Ok(Identity { user, secret }),
            _ => Err(CredError::NoIdentity(label.to_string())),
        }
    }
}

/// A provider that keeps every label of one application in a single OS keyring entry.
///
/// The entry has the application name as its service and `identities` as its user, and holds
/// a JSON map of label to user and secret. It is created on the first write and deleted when
/// its last label is removed. Every call reads the keyring again, so nothing is cached.
#[derive(Debug)]
pub struct KeyProvider {
    app: String,
    mu: Mutex<()>,
}

impl KeyProvider {
    /// The keyring user name of the shared entry.
    const ACCOUNT: &'static str = "identities";

    /// Creates a provider for the application `app`, initialising the native keyring store on
    /// first use. Panics when the native store cannot be initialised.
    #[must_use]
    pub fn new(app: &str) -> Self {
        Self::init_native_store();
        Self {
            app: app.to_string(),
            mu: Mutex::new(()),
        }
    }

    /// Initialises the `keyring` crate's native store once per process.
    fn init_native_store() {
        static INIT: Once = Once::new();
        INIT.call_once(|| {
            keyring::Entry::store_status()
                .as_ref()
                .expect("failed to initialise native keyring store");
        });
    }

    /// Returns the keyring entry for this application.
    fn entry(&self) -> Result<Entry, CredError> {
        Ok(Entry::new(&self.app, Self::ACCOUNT)?)
    }

    /// Returns the map of label to slot, empty when the entry does not exist, failing with
    /// [`CredError::Other`] when its content is not valid JSON.
    fn load(&self) -> Result<BTreeMap<String, Slot>, CredError> {
        match self.entry()?.get_password() {
            Ok(blob) => serde_json::from_str(&blob)
                .map_err(|e| CredError::Other(format!("credential store is corrupted: {e}"))),
            Err(keyring_core::Error::NoEntry) => Ok(BTreeMap::new()),
            Err(e) => Err(e.into()),
        }
    }

    /// Writes `map` to the entry, deleting the entry instead when `map` is empty.
    fn store(&self, map: &BTreeMap<String, Slot>) -> Result<(), CredError> {
        if map.is_empty() {
            return self.clear();
        }
        let blob = serde_json::to_string(map).map_err(|e| CredError::Other(e.to_string()))?;
        Ok(self.entry()?.set_password(&blob)?)
    }

    /// Deletes the entry; a missing entry is not an error.
    fn clear(&self) -> Result<(), CredError> {
        match self.entry()?.delete_credential() {
            Ok(()) | Err(keyring_core::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    /// Returns the slot of `label`, or [`CredError::NoIdentity`] when the entry has none.
    fn slot(&self, label: &str) -> Result<Slot, CredError> {
        self.load()?
            .remove(label)
            .ok_or_else(|| CredError::NoIdentity(label.to_string()))
    }
}

impl IdentityProvider for KeyProvider {
    fn identity(&self, label: &str) -> Result<Identity, CredError> {
        let _g = self.mu.lock().unwrap();
        self.slot(label)?.into_identity(label)
    }

    fn user(&self, label: &str) -> Result<String, CredError> {
        let _g = self.mu.lock().unwrap();
        self.slot(label)?
            .user
            .ok_or_else(|| CredError::NoIdentity(label.to_string()))
    }

    fn secret(&self, label: &str) -> Result<String, CredError> {
        let _g = self.mu.lock().unwrap();
        self.slot(label)?
            .secret
            .ok_or_else(|| CredError::NoIdentity(label.to_string()))
    }

    fn present(&self, label: &str) -> bool {
        let _g = self.mu.lock().unwrap();
        self.load()
            .is_ok_and(|map| map.get(label).is_some_and(Slot::is_complete))
    }

    fn apply(&self, label: &str, payload: &IdentityPayload<'_>) -> Result<(), CredError> {
        if payload.user.is_none() && payload.secret.is_none() {
            return Ok(());
        }
        let _g = self.mu.lock().unwrap();
        let mut map = self.load()?;
        map.entry(label.to_string()).or_default().merge(payload);
        self.store(&map)
    }

    fn remove(&self, label: &str) -> Result<(), CredError> {
        let _g = self.mu.lock().unwrap();
        let mut map = self.load()?;
        if map.remove(label).is_none() {
            return Ok(());
        }
        self.store(&map)
    }
}

#[cfg(all(test, feature = "keyring-tests"))]
mod keyring_tests {
    use super::*;
    use crate::*;
    use std::collections::HashMap;
    use std::sync::Arc;

    fn test_app(suffix: &str) -> String {
        format!("volt_test_kr_{suffix}")
    }

    #[test]
    fn test_key_provider_lifecycle() {
        let p = KeyProvider::new(&test_app("kp-lifecycle"));

        assert!(p.user("alice-slot").is_err());

        p.apply(
            "alice-slot",
            &IdentityPayload {
                user: Some("alice".into()),
                secret: None,
            },
        )
        .unwrap();
        assert_eq!(p.user("alice-slot").unwrap(), "alice");

        p.apply(
            "alice-slot",
            &IdentityPayload {
                user: Some("bob".into()),
                secret: None,
            },
        )
        .unwrap();
        assert_eq!(p.user("alice-slot").unwrap(), "bob");

        p.remove("alice-slot").unwrap();
        assert!(p.user("alice-slot").is_err());

        p.remove("alice-slot").unwrap();
    }

    #[test]
    fn test_key_provider_single_item_for_many_labels() {
        let app = test_app("kp-single-item");
        let provider: Arc<dyn IdentityProvider> = Arc::new(KeyProvider::new(&app));
        let alpha = Manager::new(provider.clone(), "alpha");
        let beta = Manager::new(provider.clone(), "beta");

        alpha
            .set_all(&Identity {
                user: "alpha-user".into(),
                secret: "alpha-secret".into(),
            })
            .unwrap();
        beta.set_all(&Identity {
            user: "beta-user".into(),
            secret: "beta-secret".into(),
        })
        .unwrap();

        let spec = HashMap::from([("service", app.as_str())]);
        let entries = Entry::search(&spec).unwrap();
        assert_eq!(entries.len(), 1, "two logins live inside ONE keychain item");

        alpha.delete().unwrap();
        assert!(!alpha.present());
        assert_eq!(
            beta.get().unwrap().secret,
            "beta-secret",
            "removing one label leaves the others"
        );

        beta.delete().unwrap();
        let entries = Entry::search(&spec).unwrap();
        assert!(
            entries.is_empty(),
            "the item itself dies with its last identity"
        );
    }

    #[test]
    fn test_key_provider_absent_item_is_silently_absent() {
        let p = KeyProvider::new(&test_app("kp-silent"));
        assert!(
            !p.present("anything"),
            "no stored item answers false from the existence probe alone"
        );
    }

    #[test]
    fn test_key_provider_holds_no_state_between_calls() {
        let app = test_app("kp-stateless");
        let writer = KeyProvider::new(&app);
        writer
            .apply(
                "acct",
                &IdentityPayload {
                    user: Some("first-user".into()),
                    secret: Some("first-secret".into()),
                },
            )
            .unwrap();

        let reader = KeyProvider::new(&app);
        assert!(reader.present("acct"));

        writer
            .apply(
                "acct",
                &IdentityPayload {
                    user: Some("rotated-user".into()),
                    secret: None,
                },
            )
            .unwrap();
        assert_eq!(
            reader.user("acct").unwrap(),
            "rotated-user",
            "every read goes straight to the keychain"
        );

        writer.remove("acct").unwrap();
        assert!(
            !reader.present("acct"),
            "every presence check goes straight to the keychain too"
        );
    }
}
