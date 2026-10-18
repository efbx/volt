use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt::{self, Debug};
use std::sync::{Arc, Mutex};

use super::{CredError, Identity, IdentityPayload, IdentityProvider};

/// A provider that remembers what another provider holds, so each label is read from it once.
///
/// The first read of a label reads both halves from the inner provider and keeps them, a half
/// it does not hold included; every later read of that label is served from memory. Writes and
/// removals go to the inner provider first and update the kept halves only when they succeed.
/// A read that fails for another reason than a missing half is returned and not kept. Changes
/// made to the inner store from outside this provider are not seen.
pub struct Cached {
    inner: Arc<dyn IdentityProvider>,
    halves: Mutex<HashMap<String, IdentityPayload<'static>>>,
}

impl Debug for Cached {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Cached").finish_non_exhaustive()
    }
}

impl Cached {
    /// Returns a provider that reads each label of `inner` once.
    pub fn new(inner: Arc<dyn IdentityProvider>) -> Self {
        Self {
            inner,
            halves: Mutex::default(),
        }
    }

    /// Returns what `look` makes of the halves of `label`, `None` for a half the inner provider
    /// does not hold, reading them from it the first time.
    fn read<R>(
        &self,
        label: &str,
        look: impl FnOnce(&IdentityPayload<'static>) -> R,
    ) -> Result<R, CredError> {
        let mut kept = self.halves.lock().expect("credential cache poisoned");
        if let Some(halves) = kept.get(label) {
            return Ok(look(halves));
        }
        let halves = IdentityPayload {
            user: Self::held(self.inner.user(label))?.map(Cow::Owned),
            secret: Self::held(self.inner.secret(label))?.map(Cow::Owned),
        };
        Ok(look(kept.entry(label.to_owned()).or_insert(halves)))
    }

    /// Returns the half `read` returned, `None` when the inner provider holds no such half, and
    /// any other failure as it is.
    fn held(read: Result<String, CredError>) -> Result<Option<String>, CredError> {
        match read {
            Ok(value) => Ok(Some(value)),
            Err(CredError::NoIdentity(_) | CredError::EnvVarNotFound(_)) => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// Returns a copy of `half`, failing with [`CredError::NoIdentity`] for `label` when it is
    /// missing.
    fn required(half: Option<&str>, label: &str) -> Result<String, CredError> {
        half.map(str::to_owned)
            .ok_or_else(|| CredError::NoIdentity(label.to_owned()))
    }
}

impl IdentityProvider for Cached {
    fn identity(&self, label: &str) -> Result<Identity, CredError> {
        self.read(label, |halves| {
            Ok(Identity {
                user: Self::required(halves.user.as_deref(), label)?,
                secret: Self::required(halves.secret.as_deref(), label)?,
            })
        })?
    }

    fn user(&self, label: &str) -> Result<String, CredError> {
        self.read(label, |halves| {
            Self::required(halves.user.as_deref(), label)
        })?
    }

    fn secret(&self, label: &str) -> Result<String, CredError> {
        self.read(label, |halves| {
            Self::required(halves.secret.as_deref(), label)
        })?
    }

    fn present(&self, label: &str) -> bool {
        self.read(label, |halves| {
            halves.user.is_some() && halves.secret.is_some()
        })
        .unwrap_or_default()
    }

    fn apply(&self, label: &str, payload: &IdentityPayload<'_>) -> Result<(), CredError> {
        let mut kept = self.halves.lock().expect("credential cache poisoned");
        self.inner.apply(label, payload)?;
        if let Some(halves) = kept.get_mut(label) {
            if let Some(user) = payload.user.as_deref() {
                halves.user = Some(Cow::Owned(user.to_owned()));
            }
            if let Some(secret) = payload.secret.as_deref() {
                halves.secret = Some(Cow::Owned(secret.to_owned()));
            }
        }
        Ok(())
    }

    fn remove(&self, label: &str) -> Result<(), CredError> {
        let mut kept = self.halves.lock().expect("credential cache poisoned");
        self.inner.remove(label)?;
        kept.insert(label.to_owned(), IdentityPayload::default());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    use crate::EnvProvider;

    struct Counting {
        inner: EnvProvider,
        reads: AtomicUsize,
        fail_next: AtomicBool,
    }

    impl Counting {
        fn new(fail_first: bool) -> Arc<Self> {
            Arc::new(Self {
                inner: EnvProvider::new(),
                reads: AtomicUsize::new(0),
                fail_next: AtomicBool::new(fail_first),
            })
        }

        fn read(
            &self,
            read: impl FnOnce() -> Result<String, CredError>,
        ) -> Result<String, CredError> {
            self.reads.fetch_add(1, Ordering::SeqCst);
            if self.fail_next.swap(false, Ordering::SeqCst) {
                return Err(CredError::KeyRing("locked".into()));
            }
            read()
        }

        fn reads(&self) -> usize {
            self.reads.load(Ordering::SeqCst)
        }
    }

    impl IdentityProvider for Counting {
        fn identity(&self, label: &str) -> Result<Identity, CredError> {
            self.inner.identity(label)
        }

        fn user(&self, label: &str) -> Result<String, CredError> {
            self.read(|| self.inner.user(label))
        }

        fn secret(&self, label: &str) -> Result<String, CredError> {
            self.read(|| self.inner.secret(label))
        }

        fn present(&self, label: &str) -> bool {
            self.inner.present(label)
        }

        fn apply(&self, label: &str, payload: &IdentityPayload<'_>) -> Result<(), CredError> {
            self.inner.apply(label, payload)
        }

        fn remove(&self, label: &str) -> Result<(), CredError> {
            self.inner.remove(label)
        }
    }

    fn label(name: &str) -> String {
        format!("volt-cached-test-{name}-{}", std::process::id())
    }

    fn login<'a>(user: &'a str, secret: &'a str) -> IdentityPayload<'a> {
        IdentityPayload {
            user: Some(user.into()),
            secret: Some(secret.into()),
        }
    }

    #[test]
    fn a_label_is_read_once_even_when_nothing_is_stored() {
        let label = label("once");
        let counting = Counting::new(false);
        let cached = Cached::new(counting.clone());

        for _ in 0..3 {
            assert!(!cached.present(&label));
            assert!(matches!(cached.user(&label), Err(CredError::NoIdentity(_))));
        }
        assert_eq!(counting.reads(), 2, "one read per half, then memory");
    }

    #[test]
    fn writes_go_through_and_update_what_is_kept() {
        let label = label("writes");
        let counting = Counting::new(false);
        let cached = Cached::new(counting.clone());
        assert!(!cached.present(&label));

        cached.apply(&label, &login("aria", "hunter2")).unwrap();
        assert!(cached.present(&label));
        assert_eq!(cached.identity(&label).unwrap().secret, "hunter2");
        assert_eq!(
            counting.inner.user(&label).unwrap(),
            "aria",
            "written through"
        );

        cached
            .apply(
                &label,
                &IdentityPayload {
                    user: None,
                    secret: Some("rotated".into()),
                },
            )
            .unwrap();
        assert_eq!(
            cached.identity(&label).unwrap().user,
            "aria",
            "a None half is kept"
        );
        assert_eq!(cached.secret(&label).unwrap(), "rotated");

        cached.remove(&label).unwrap();
        assert!(!cached.present(&label));
        assert!(!counting.inner.present(&label), "removed through");
        assert_eq!(
            counting.reads(),
            2,
            "only the first read reached the inner provider"
        );
    }

    #[test]
    fn a_label_first_written_is_read_on_its_next_read() {
        let label = label("first-write");
        let counting = Counting::new(false);
        let cached = Cached::new(counting.clone());

        cached.apply(&label, &login("aria", "hunter2")).unwrap();
        assert!(cached.present(&label));
        assert_eq!(counting.reads(), 2);
        cached.remove(&label).unwrap();
    }

    #[test]
    fn other_failures_are_returned_and_not_kept() {
        let label = label("failures");
        let counting = Counting::new(true);
        counting
            .inner
            .apply(&label, &login("aria", "hunter2"))
            .unwrap();
        let cached = Cached::new(counting.clone());

        assert!(matches!(cached.user(&label), Err(CredError::KeyRing(_))));
        assert_eq!(
            cached.user(&label).unwrap(),
            "aria",
            "the failure was not kept"
        );
        assert_eq!(counting.reads(), 3, "the failed read, then both halves");
        counting.inner.remove(&label).unwrap();
    }
}
