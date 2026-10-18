use std::borrow::Cow;
use std::fmt::{self, Debug};
use std::sync::Arc;

use super::{
    CredError, Identity, IdentityPayload, IdentityProvider, IdentityReader, IdentityWriter,
};

/// An identity reader and writer bound to one label of an [`IdentityProvider`].
///
/// Several managers may share one provider; each reads and writes only its own label.
pub struct Manager {
    label: String,
    provider: Arc<dyn IdentityProvider>,
}

impl Debug for Manager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Manager")
            .field("label", &self.label)
            .finish_non_exhaustive()
    }
}

impl Manager {
    /// Creates a manager for `label` over `provider`.
    pub fn new(provider: Arc<dyn IdentityProvider>, label: &str) -> Self {
        Self {
            label: label.to_string(),
            provider,
        }
    }
}

impl IdentityReader for Manager {
    fn cred_label(&self) -> &str {
        &self.label
    }

    fn get(&self) -> Result<Identity, CredError> {
        self.provider.identity(&self.label)
    }

    fn get_user(&self) -> Result<String, CredError> {
        self.provider.user(&self.label)
    }

    fn get_secret(&self) -> Result<String, CredError> {
        self.provider.secret(&self.label)
    }

    fn present(&self) -> bool {
        self.provider.present(&self.label)
    }
}

impl IdentityWriter for Manager {
    fn cred_label(&self) -> &str {
        &self.label
    }

    fn set_all(&self, identity: &Identity) -> Result<(), CredError> {
        self.provider.apply(
            &self.label,
            &IdentityPayload {
                user: Some(Cow::Borrowed(&identity.user)),
                secret: Some(Cow::Borrowed(&identity.secret)),
            },
        )
    }

    fn set_payload(&self, payload: &IdentityPayload<'_>) -> Result<(), CredError> {
        self.provider.apply(&self.label, payload)
    }

    fn set_user(&self, user: &str) -> Result<(), CredError> {
        self.provider.apply(
            &self.label,
            &IdentityPayload {
                user: Some(Cow::Borrowed(user)),
                secret: None,
            },
        )
    }

    fn set_secret(&self, secret: &str) -> Result<(), CredError> {
        self.provider.apply(
            &self.label,
            &IdentityPayload {
                user: None,
                secret: Some(Cow::Borrowed(secret)),
            },
        )
    }

    fn delete(&self) -> Result<(), CredError> {
        self.provider.remove(&self.label)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::*;
    use std::sync::Arc;

    struct ManagerSuite<'a> {
        m: &'a Manager,
        not_found: fn(&CredError) -> bool,
    }

    impl ManagerSuite<'_> {
        fn lifecycle(&self) {
            assert!((self.not_found)(&self.m.get().unwrap_err()));

            let identity = Identity {
                user: "test-lifecycle-user".into(),
                secret: "s3cr3t-l1fecycl3-p@ssw0rd!".into(),
            };
            self.m.set_all(&identity).unwrap();

            let got = self.m.get().unwrap();
            assert_eq!(got.user, identity.user);
            assert_eq!(got.secret, identity.secret);

            let updated = Identity {
                user: "updated-user".into(),
                secret: "updated-secret".into(),
            };
            self.m.set_all(&updated).unwrap();
            let got = self.m.get().unwrap();
            assert_eq!(got.user, updated.user);
            assert_eq!(got.secret, updated.secret);

            self.m.delete().unwrap();
            assert!((self.not_found)(&self.m.get().unwrap_err()));

            self.m.delete().unwrap();
        }

        fn partial_payloads(&self) {
            self.m
                .set_payload(&IdentityPayload {
                    user: Some("partial-user".into()),
                    secret: None,
                })
                .unwrap();
            assert_eq!(self.m.get_user().unwrap(), "partial-user");
            assert!((self.not_found)(&self.m.get_secret().unwrap_err()));

            self.m
                .set_payload(&IdentityPayload {
                    user: None,
                    secret: Some("p@rtial-s3cr3t".into()),
                })
                .unwrap();
            assert_eq!(self.m.get_secret().unwrap(), "p@rtial-s3cr3t");
            assert_eq!(self.m.get_user().unwrap(), "partial-user");

            let id = self.m.get().unwrap();
            assert_eq!(id.user, "partial-user");
            assert_eq!(id.secret, "p@rtial-s3cr3t");

            self.m
                .set_payload(&IdentityPayload {
                    user: None,
                    secret: None,
                })
                .unwrap();
            assert_eq!(self.m.get_user().unwrap(), "partial-user");
            assert_eq!(self.m.get_secret().unwrap(), "p@rtial-s3cr3t");

            self.m.delete().unwrap();
        }

        fn set_user_get_user(&self) {
            assert!((self.not_found)(&self.m.get_user().unwrap_err()));
            self.m.set_user("standalone-user").unwrap();
            assert_eq!(self.m.get_user().unwrap(), "standalone-user");
            self.m.delete().unwrap();
        }

        fn set_secret_get_secret(&self) {
            assert!((self.not_found)(&self.m.get_secret().unwrap_err()));
            self.m.set_secret("st@ndal0ne-s3cr3t").unwrap();
            assert_eq!(self.m.get_secret().unwrap(), "st@ndal0ne-s3cr3t");
            self.m.delete().unwrap();
        }

        fn get_partial_failure(&self) {
            self.m.set_user("only-user-exists").unwrap();
            assert!((self.not_found)(&self.m.get().unwrap_err()));

            self.m.delete().unwrap();
            self.m.set_secret("only-secret-exists").unwrap();
            assert!((self.not_found)(&self.m.get().unwrap_err()));

            self.m.delete().unwrap();
        }

        fn presence(&self) {
            assert!(!self.m.present());

            self.m.set_user("presence-user").unwrap();
            assert!(!self.m.present(), "a lone user is not a login");

            self.m.set_secret("presence-secret").unwrap();
            assert!(self.m.present());

            self.m.delete().unwrap();
            assert!(!self.m.present());
        }

        fn concurrency(m: &Arc<Manager>) {
            m.set_all(&Identity {
                user: "init-user".into(),
                secret: "init-secret".into(),
            })
            .unwrap();

            let handles: Vec<_> = (0..10)
                .map(|i| {
                    let m = Arc::clone(m);
                    std::thread::spawn(move || {
                        let _ = m.get();
                        let _ = m.set_user(&format!("user-{i}"));
                        let _ = m.get_user();
                        let _ = m.set_secret(&format!("secret-{i}"));
                        let _ = m.get_secret();
                        let _ = m.delete();
                        let _ = m.get();
                        let _ = m.set_all(&Identity {
                            user: format!("final-user-{i}"),
                            secret: format!("final-secret-{i}"),
                        });
                    })
                })
                .collect();

            for h in handles {
                h.join().expect("thread panicked");
            }

            let _ = m.delete();
        }
    }

    const NOT_FOUND: fn(&CredError) -> bool = |e| matches!(e, CredError::EnvVarNotFound(_));

    fn test_label(name: &str) -> String {
        format!("ccard-cred-test-{name}-{}", std::process::id())
    }

    fn env_manager(label: &str) -> Manager {
        Manager::new(Arc::new(EnvProvider::new()), label)
    }

    #[test]
    fn test_manager_env_lifecycle() {
        let m = env_manager(&test_label("mgr-lifecycle"));
        ManagerSuite {
            m: &m,
            not_found: NOT_FOUND,
        }
        .lifecycle();
    }

    #[test]
    fn test_manager_env_partial_payloads() {
        let m = env_manager(&test_label("mgr-partial"));
        ManagerSuite {
            m: &m,
            not_found: NOT_FOUND,
        }
        .partial_payloads();
    }

    #[test]
    fn test_manager_env_presence() {
        let m = env_manager(&test_label("mgr-presence"));
        ManagerSuite {
            m: &m,
            not_found: NOT_FOUND,
        }
        .presence();
    }

    #[test]
    fn test_manager_env_set_user_get_user() {
        let m = env_manager(&test_label("mgr-user"));
        ManagerSuite {
            m: &m,
            not_found: NOT_FOUND,
        }
        .set_user_get_user();
    }

    #[test]
    fn test_manager_env_set_secret_get_secret() {
        let m = env_manager(&test_label("mgr-secret"));
        ManagerSuite {
            m: &m,
            not_found: NOT_FOUND,
        }
        .set_secret_get_secret();
    }

    #[test]
    fn test_manager_env_get_partial_failure() {
        let m = env_manager(&test_label("mgr-partial-fail"));
        ManagerSuite {
            m: &m,
            not_found: NOT_FOUND,
        }
        .get_partial_failure();
    }

    #[test]
    fn test_manager_label() {
        let m = env_manager("test-label");
        assert_eq!(IdentityReader::cred_label(&m), "test-label");
        assert_eq!(IdentityWriter::cred_label(&m), "test-label");
    }

    #[test]
    fn test_identity_manager_supertrait() {
        fn accepts_manager(_m: &dyn IdentityManager) {}
        let m = env_manager("supertrait-test");
        accepts_manager(&m);
    }

    #[test]
    fn test_managers_share_one_env_provider() {
        let provider: Arc<dyn IdentityProvider> = Arc::new(EnvProvider::new());
        let alpha_label = test_label("shared-alpha");
        let beta_label = test_label("shared-beta");
        let alpha = Manager::new(provider.clone(), &alpha_label);
        let beta = Manager::new(provider.clone(), &beta_label);

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

        assert_eq!(alpha.get().unwrap().user, "alpha-user");
        assert_eq!(beta.get().unwrap().user, "beta-user");

        alpha.delete().unwrap();
        assert!(!alpha.present());
        assert_eq!(
            beta.get().unwrap().secret,
            "beta-secret",
            "removing one label leaves the others"
        );

        beta.delete().unwrap();
    }

    #[test]
    fn test_manager_env_concurrency() {
        let m = Arc::new(env_manager(&test_label("mgr-concurrent")));
        ManagerSuite::concurrency(&m);
    }

    #[cfg(feature = "keyring-tests")]
    mod keyring_tests {
        use super::*;
        use std::sync::Arc;

        const NOT_FOUND: fn(&CredError) -> bool = |e| matches!(e, CredError::NoIdentity(_));

        fn test_app(suffix: &str) -> String {
            format!("volt_test_kr_{suffix}")
        }

        fn keyring_manager(app: &str) -> Manager {
            Manager::new(Arc::new(KeyProvider::new(app)), "account")
        }

        #[test]
        fn test_manager_keyring_lifecycle() {
            let m = keyring_manager(&test_app("mgr-lifecycle"));
            ManagerSuite {
                m: &m,
                not_found: NOT_FOUND,
            }
            .lifecycle();
        }

        #[test]
        fn test_manager_keyring_partial_payloads() {
            let m = keyring_manager(&test_app("mgr-partial"));
            ManagerSuite {
                m: &m,
                not_found: NOT_FOUND,
            }
            .partial_payloads();
        }

        #[test]
        fn test_manager_keyring_presence() {
            let m = keyring_manager(&test_app("mgr-presence"));
            ManagerSuite {
                m: &m,
                not_found: NOT_FOUND,
            }
            .presence();
        }

        #[test]
        fn test_manager_keyring_set_user_get_user() {
            let m = keyring_manager(&test_app("mgr-user"));
            ManagerSuite {
                m: &m,
                not_found: NOT_FOUND,
            }
            .set_user_get_user();
        }

        #[test]
        fn test_manager_keyring_set_secret_get_secret() {
            let m = keyring_manager(&test_app("mgr-secret"));
            ManagerSuite {
                m: &m,
                not_found: NOT_FOUND,
            }
            .set_secret_get_secret();
        }

        #[test]
        fn test_manager_keyring_get_partial_failure() {
            let m = keyring_manager(&test_app("mgr-partial-fail"));
            ManagerSuite {
                m: &m,
                not_found: NOT_FOUND,
            }
            .get_partial_failure();
        }

        #[test]
        fn test_manager_keyring_concurrency() {
            let m = Arc::new(keyring_manager(&test_app("mgr-concurrent")));
            ManagerSuite::concurrency(&m);
        }
    }
}
