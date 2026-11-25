use std::fmt::Debug;

use async_trait::async_trait;
use serde_json::Value;
use url::Url;

use crate::{Cookie, Error};

/// One open page of a browser, reduced to the few things only the browser itself can do. Every
/// command of [`Page`](crate::Page) and [`Element`](crate::Element) is built from these, so an
/// engine that implements them gets the whole command set.
#[async_trait]
pub trait Engine: Debug + Send + Sync {
    /// Loads `url` and returns once that load has ended.
    async fn goto(&self, url: &Url) -> Result<(), Error>;

    /// Evaluates the JavaScript expression `script` in the page's top document and returns its
    /// value as JSON, without waiting for a promise; [`Value::Null`] where it has none. A failure
    /// that a navigation under way explains comes back as [`Error::Engine`].
    async fn evaluate(&self, script: &str) -> Result<Value, Error>;

    /// The cookies the page's store sends with a request to `url`, `HttpOnly` ones included.
    async fn cookies(&self, url: &Url) -> Result<Vec<Cookie>, Error>;

    /// Adds `cookie` to the page's store, in place of one with its name, domain and path.
    async fn set_cookie(&self, cookie: &Cookie) -> Result<(), Error>;

    /// Removes the cookie with `cookie`'s name, domain and path from the page's store.
    async fn delete_cookie(&self, cookie: &Cookie) -> Result<(), Error>;

    /// Shows the page's window in front, or hides it.
    async fn set_visible(&self, visible: bool) -> Result<(), Error>;

    /// Closes the page.
    async fn close(&self) -> Result<(), Error>;
}
