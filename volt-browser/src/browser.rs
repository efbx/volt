use std::fmt::Debug;
use std::sync::Arc;

use async_trait::async_trait;

use crate::{Engine, Error};

/// A running browser: the one thing a crate implements, with [`Engine`] for its pages, to give
/// its browser the whole command set. A [`Session`](crate::Session) drives it.
#[async_trait]
pub trait Browser: Debug + Send + Sync {
    /// Opens a new page on `about:blank`.
    async fn open(&self) -> Result<Arc<dyn Engine>, Error>;

    /// Closes every page and the browser.
    async fn close(&self);
}
