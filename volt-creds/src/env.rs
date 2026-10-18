use std::env::VarError;
use std::sync::Mutex;

use super::{CredError, Identity, IdentityPayload, IdentityProvider};
use volt_stringx::StringExt;

/// A provider that keeps each identity in two environment variables,
/// `<LABEL>_USERNAME` and `<LABEL>_PASSWORD`.
///
/// Writes and removals change the environment of the whole process. One instance serialises
/// its own calls; separate instances and other code touching the same variables do not wait
/// for each other.
#[derive(Debug, Default)]
pub struct EnvProvider {
    mu: Mutex<()>,
}

impl EnvProvider {
    /// The variable suffix that holds the user.
    const USER_KEY: &'static str = "username";
    /// The variable suffix that holds the secret.
    const SECRET_KEY: &'static str = "password";

    /// Creates a provider over the current process environment.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the variable name for `label` and `key`: both joined by `_`, uppercased one
    /// character at a time, so `straße` keeps its `ß`.
    pub(super) fn env_var_name(label: &str, key: &str) -> String {
        let mut name = format!("{label}_{key}");
        name.simple_uppercase_in_place();
        name
    }

    /// Returns the value of the variable for `label` and `key`, failing with
    /// [`CredError::EnvVarNotFound`] when it is unset and [`CredError::Other`] when it is not
    /// UTF-8.
    fn read(label: &str, key: &str) -> Result<String, CredError> {
        let name = Self::env_var_name(label, key);
        match std::env::var(&name) {
            Ok(value) => Ok(value),
            Err(VarError::NotPresent) => Err(CredError::EnvVarNotFound(name)),
            Err(VarError::NotUnicode(_)) => Err(CredError::Other(format!(
                "environment variable {name:?} is not valid UTF-8"
            ))),
        }
    }

    /// Sets the variable for `label` and `key` to `value`, failing when the name is invalid or
    /// `value` contains a NUL byte.
    fn write(label: &str, key: &str, value: &str) -> Result<(), CredError> {
        let name = Self::validated_name(label, key)?;
        if value.contains('\0') {
            return Err(Self::invalid_variable(&name));
        }
        // SAFETY: serialized by self.mu — no concurrent access on this instance.
        unsafe { std::env::set_var(name, value) };
        Ok(())
    }

    /// Removes the variable for `label` and `key`; an unset variable is not an error.
    fn unset(label: &str, key: &str) -> Result<(), CredError> {
        let name = Self::validated_name(label, key)?;
        // SAFETY: same as write above.
        unsafe { std::env::remove_var(name) };
        Ok(())
    }

    /// Returns the variable name for `label` and `key`, failing when it is empty or contains `=`
    /// or a NUL byte.
    fn validated_name(label: &str, key: &str) -> Result<String, CredError> {
        let name = Self::env_var_name(label, key);
        if name.is_empty() || name.contains('=') || name.contains('\0') {
            return Err(Self::invalid_variable(&name));
        }
        Ok(name)
    }

    /// Returns the error for an environment variable `name` that cannot be written.
    fn invalid_variable(name: &str) -> CredError {
        CredError::Other(format!("invalid environment variable: {name:?}"))
    }
}

impl IdentityProvider for EnvProvider {
    fn identity(&self, label: &str) -> Result<Identity, CredError> {
        let _g = self.mu.lock().unwrap();
        Ok(Identity {
            user: Self::read(label, Self::USER_KEY)?,
            secret: Self::read(label, Self::SECRET_KEY)?,
        })
    }

    fn user(&self, label: &str) -> Result<String, CredError> {
        let _g = self.mu.lock().unwrap();
        Self::read(label, Self::USER_KEY)
    }

    fn secret(&self, label: &str) -> Result<String, CredError> {
        let _g = self.mu.lock().unwrap();
        Self::read(label, Self::SECRET_KEY)
    }

    fn present(&self, label: &str) -> bool {
        let _g = self.mu.lock().unwrap();
        Self::read(label, Self::USER_KEY).is_ok() && Self::read(label, Self::SECRET_KEY).is_ok()
    }

    fn apply(&self, label: &str, payload: &IdentityPayload<'_>) -> Result<(), CredError> {
        let _g = self.mu.lock().unwrap();
        payload
            .user
            .as_deref()
            .map(|u| Self::write(label, Self::USER_KEY, u))
            .transpose()?;
        payload
            .secret
            .as_deref()
            .map(|s| Self::write(label, Self::SECRET_KEY, s))
            .transpose()?;
        Ok(())
    }

    fn remove(&self, label: &str) -> Result<(), CredError> {
        let _g = self.mu.lock().unwrap();
        Self::unset(label, Self::USER_KEY)?;
        Self::unset(label, Self::SECRET_KEY)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn test_label(name: &str) -> String {
        format!("ccard-cred-test-{name}-{}", std::process::id())
    }

    #[test]
    fn test_env_provider_lifecycle() {
        let label = test_label("envprov");
        let p = EnvProvider::new();

        let err = p.user(&label).unwrap_err();
        assert!(
            matches!(err, CredError::EnvVarNotFound(_)),
            "expected EnvVarNotFound, got {err:?}"
        );

        p.apply(
            &label,
            &IdentityPayload {
                user: Some("value".into()),
                secret: None,
            },
        )
        .unwrap();
        assert_eq!(p.user(&label).unwrap(), "value");

        p.remove(&label).unwrap();

        let err = p.user(&label).unwrap_err();
        assert!(matches!(err, CredError::EnvVarNotFound(_)));

        p.remove(&label).unwrap();
    }

    #[test]
    fn test_env_provider_error_message_contains_not_set() {
        let label = test_label("envprov-errmsg");
        let p = EnvProvider::new();
        let err = p.user(&label).unwrap_err();
        assert!(
            err.to_string().contains("not set"),
            "error message should contain 'not set': {err}"
        );
    }

    #[test]
    fn test_env_var_name_unicode_uppercase() {
        for (label, key, expected) in [
            ("Pygmalion", "username", "PYGMALION_USERNAME"),
            ("café", "username", "CAFÉ_USERNAME"),
            ("straße", "key", "STRAßE_KEY"),
        ] {
            assert_eq!(
                EnvProvider::env_var_name(label, key),
                expected,
                "env_var_name({label:?}, {key:?})"
            );
        }
    }

    #[test]
    fn test_env_provider_set_nul_value_errors() {
        let label = test_label("nul-value");
        let p = EnvProvider::new();
        let result = p.apply(
            &label,
            &IdentityPayload {
                user: Some("bad\0value".into()),
                secret: None,
            },
        );
        assert!(
            matches!(result, Err(CredError::Other(_))),
            "NUL in value must return an error, not panic"
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_env_provider_get_non_utf8_is_distinct_error() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;
        let label = test_label("non-utf8");
        let p = EnvProvider::new();
        let name = EnvProvider::env_var_name(&label, "username");
        unsafe { std::env::set_var(&name, OsStr::from_bytes(&[0xFF, 0xFE])) };
        let err = p.user(&label).unwrap_err();
        unsafe { std::env::remove_var(&name) };
        assert!(
            matches!(err, CredError::Other(_)),
            "present-but-non-UTF8 must be a distinct error, not EnvVarNotFound: {err:?}"
        );
    }

    #[test]
    fn test_env_provider_concurrency() {
        let label = test_label("envprov-concurrent");
        let p = Arc::new(EnvProvider::new());
        p.apply(
            &label,
            &IdentityPayload {
                user: Some("initial-value".into()),
                secret: None,
            },
        )
        .unwrap();

        let handles: Vec<_> = (0..10)
            .map(|i| {
                let p = Arc::clone(&p);
                let label = label.clone();
                std::thread::spawn(move || {
                    let _ = p.user(&label);
                    let _ = p.apply(
                        &label,
                        &IdentityPayload {
                            user: Some(format!("key-from-{i}").into()),
                            secret: None,
                        },
                    );
                    let _ = p.user(&label);
                    let _ = p.remove(&label);
                    let _ = p.user(&label);
                    let _ = p.apply(
                        &label,
                        &IdentityPayload {
                            user: Some(format!("final-key-from-{i}").into()),
                            secret: None,
                        },
                    );
                })
            })
            .collect();

        for h in handles {
            h.join().expect("thread panicked");
        }

        let _ = p.remove(&label);
    }
}
