use std::sync::Arc;

use super::{CredError, Identity, IdentityPayload, IdentityProvider, Manager};

/// Read access to the identity stored under one label, for async callers.
#[async_trait::async_trait]
pub trait AsyncIdentityReader: Send + Sync {
    /// Returns the complete identity, failing when either the user or the secret is missing.
    async fn get(&self) -> Result<Identity, CredError>;
    /// Returns the stored user alone.
    async fn get_user(&self) -> Result<String, CredError>;
    /// Whether both the user and the secret are stored.
    async fn present(&self) -> bool;
}

/// Write access to the identity stored under one label, for async callers.
#[async_trait::async_trait]
pub trait AsyncIdentityWriter: Send + Sync {
    /// Stores the halves of `payload` that are `Some` and leaves the others untouched.
    async fn set_payload(&self, payload: IdentityPayload<'static>) -> Result<(), CredError>;
    /// Removes the user and the secret. Removing a label that holds nothing succeeds.
    async fn delete(&self) -> Result<(), CredError>;
}

impl Manager {
    /// Runs `work` on this manager's provider and label on the blocking pool of the current
    /// tokio runtime, resuming its panic on the caller if it panics.
    async fn off_thread<T: Send + 'static>(
        &self,
        work: impl FnOnce(&dyn IdentityProvider, &str) -> T + Send + 'static,
    ) -> T {
        let provider = Arc::clone(&self.provider);
        let label = Arc::clone(&self.label);
        match tokio::task::spawn_blocking(move || work(&*provider, &label)).await {
            Ok(value) => value,
            Err(error) => std::panic::resume_unwind(error.into_panic()),
        }
    }
}

#[async_trait::async_trait]
impl AsyncIdentityReader for Manager {
    async fn get(&self) -> Result<Identity, CredError> {
        self.off_thread(|provider, label| provider.identity(label))
            .await
    }

    async fn get_user(&self) -> Result<String, CredError> {
        self.off_thread(|provider, label| provider.user(label))
            .await
    }

    async fn present(&self) -> bool {
        self.off_thread(|provider, label| provider.present(label))
            .await
    }
}

#[async_trait::async_trait]
impl AsyncIdentityWriter for Manager {
    async fn set_payload(&self, payload: IdentityPayload<'static>) -> Result<(), CredError> {
        self.off_thread(move |provider, label| provider.apply(label, &payload))
            .await
    }

    async fn delete(&self) -> Result<(), CredError> {
        self.off_thread(|provider, label| provider.remove(label))
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EnvProvider, IdentityReader};

    struct PanickingProvider;

    impl IdentityProvider for PanickingProvider {
        fn identity(&self, _: &str) -> Result<Identity, CredError> {
            panic!("provider panicked")
        }
        fn user(&self, _: &str) -> Result<String, CredError> {
            panic!("provider panicked")
        }
        fn secret(&self, _: &str) -> Result<String, CredError> {
            panic!("provider panicked")
        }
        fn present(&self, _: &str) -> bool {
            panic!("provider panicked")
        }
        fn apply(&self, _: &str, _: &IdentityPayload<'_>) -> Result<(), CredError> {
            panic!("provider panicked")
        }
        fn remove(&self, _: &str) -> Result<(), CredError> {
            panic!("provider panicked")
        }
    }

    fn env_manager(name: &str) -> Manager {
        let label = format!("volt-creds-async-{name}-{}", std::process::id());
        Manager::new(Arc::new(EnvProvider::new()), &label)
    }

    fn payload(user: &'static str, secret: &'static str) -> IdentityPayload<'static> {
        IdentityPayload {
            user: Some(user.into()),
            secret: Some(secret.into()),
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn async_calls_store_read_and_remove_an_identity() {
        let manager = env_manager("lifecycle");
        assert!(!AsyncIdentityReader::present(&manager).await);

        AsyncIdentityWriter::set_payload(&manager, payload("async-user", "async-secret"))
            .await
            .unwrap();
        assert!(AsyncIdentityReader::present(&manager).await);
        assert_eq!(
            AsyncIdentityReader::get_user(&manager).await.unwrap(),
            "async-user"
        );
        let identity = AsyncIdentityReader::get(&manager).await.unwrap();
        assert_eq!(identity.user, "async-user");
        assert_eq!(identity.secret, "async-secret");

        AsyncIdentityWriter::delete(&manager).await.unwrap();
        assert!(!AsyncIdentityReader::present(&manager).await);
        assert!(matches!(
            AsyncIdentityReader::get(&manager).await.unwrap_err(),
            CredError::EnvVarNotFound(_)
        ));
    }

    #[tokio::test]
    async fn async_calls_run_on_a_single_thread_runtime() {
        let manager = env_manager("single-thread");
        AsyncIdentityWriter::set_payload(&manager, payload("single-user", "single-secret"))
            .await
            .unwrap();
        assert_eq!(
            AsyncIdentityReader::get(&manager).await.unwrap().user,
            "single-user"
        );
        AsyncIdentityWriter::delete(&manager).await.unwrap();
    }

    #[tokio::test]
    async fn async_writes_are_seen_by_sync_reads() {
        let manager = env_manager("shared");
        AsyncIdentityWriter::set_payload(&manager, payload("shared-user", "shared-secret"))
            .await
            .unwrap();
        assert_eq!(IdentityReader::get(&manager).unwrap().user, "shared-user");
        AsyncIdentityWriter::delete(&manager).await.unwrap();
    }

    #[tokio::test]
    #[should_panic(expected = "provider panicked")]
    async fn a_provider_panic_reaches_the_caller() {
        let manager = Manager::new(Arc::new(PanickingProvider), "panicking");
        AsyncIdentityReader::present(&manager).await;
    }
}
