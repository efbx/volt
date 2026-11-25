use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use bytes::Bytes;
use http::{HeaderName, HeaderValue, Response, StatusCode};
use serde::{Deserialize, Serialize};

use crate::page_script::Op;
use crate::{Error, Page};

/// One cache, and one URL in it for a command about a single answer.
#[derive(Serialize)]
struct Entry<'a> {
    /// The cache's name.
    cache: &'a str,
    /// The URL the answer is kept for; empty for a command about the whole cache.
    url: &'a str,
}

/// A cached answer as the page hands it over.
#[derive(Deserialize)]
struct Stored {
    /// The HTTP status.
    status: u16,
    /// The headers, in order.
    headers: Vec<(String, String)>,
    /// The body in base64.
    body: String,
}

impl Stored {
    /// Returns the answer as an HTTP response; headers HTTP cannot carry are left out.
    fn response(self) -> Result<Response<Bytes>, Error> {
        let status = StatusCode::from_u16(self.status).map_err(|_| {
            Error::Unsupported("reading a cached answer the page may not look into")
        })?;
        let body = STANDARD
            .decode(self.body)
            .map_err(|error| Error::Script(error.to_string()))?;
        let mut response = Response::new(Bytes::from(body));
        *response.status_mut() = status;
        for (name, value) in self.headers {
            if let (Ok(name), Ok(value)) =
                (HeaderName::try_from(name), HeaderValue::try_from(value))
            {
                response.headers_mut().append(name, value);
            }
        }
        Ok(response)
    }
}

/// The caches a page's site keeps through the browser's Cache Storage, where service workers
/// and pages put answers to requests. Browsers offer it on HTTPS pages and on this machine's own
/// addresses only; elsewhere its commands fail with [`Error::Script`].
#[derive(Clone, Debug)]
pub struct Caches {
    /// The page whose site the caches belong to.
    page: Page,
}

impl Caches {
    /// Returns the caches of `page`'s site.
    pub(crate) fn new(page: Page) -> Self {
        Self { page }
    }

    /// The names of the site's caches.
    pub async fn names(&self) -> Result<Vec<String>, Error> {
        self.page.ask(Op::CacheNames, &()).await
    }

    /// The URLs the cache called `cache` holds answers for; none when there is no such cache.
    pub async fn urls(&self, cache: &str) -> Result<Vec<String>, Error> {
        self.page
            .ask(Op::CacheUrls, &Entry { cache, url: "" })
            .await
    }

    /// The answer the cache called `cache` holds for `url`, with its status, headers and body.
    pub async fn get(&self, cache: &str, url: &str) -> Result<Option<Response<Bytes>>, Error> {
        let stored: Option<Stored> = self.page.ask(Op::CacheGet, &Entry { cache, url }).await?;
        stored.map(Stored::response).transpose()
    }

    /// Removes the cache called `cache`; `false` when there was none.
    pub async fn delete(&self, cache: &str) -> Result<bool, Error> {
        self.page
            .ask(Op::CacheDelete, &Entry { cache, url: "" })
            .await
    }

    /// Removes the answer the cache called `cache` holds for `url`; `false` when there was none.
    pub async fn remove(&self, cache: &str, url: &str) -> Result<bool, Error> {
        self.page.ask(Op::CacheRemove, &Entry { cache, url }).await
    }
}

#[cfg(test)]
mod tests {
    use http::StatusCode;
    use serde_json::json;

    use crate::Error;
    use crate::fake::Fake;

    #[tokio::test]
    async fn a_cached_answer_comes_back_as_an_http_response() {
        let stored = json!({
            "status": 200,
            "headers": [["content-type", "text/plain"], ["bad name", "x"], ["x-two", "a"], ["x-two", "b"]],
            "body": "aGVsbG8="
        });
        let fake = Fake::answering([
            json!({"status": "pending"}),
            json!({"status": "ok", "json": stored.to_string()}),
        ]);
        let response = fake
            .page()
            .caches()
            .get("pages", "https://example.com/a")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["content-type"], "text/plain");
        assert_eq!(response.headers().get_all("x-two").iter().count(), 2);
        assert_eq!(response.headers().len(), 3);
        assert_eq!(response.body().as_ref(), b"hello");
        assert_eq!(
            fake.commands()[0]["arg"],
            json!({"cache": "pages", "url": "https://example.com/a"})
        );
    }

    #[tokio::test]
    async fn nothing_cached_is_none_and_an_unreadable_answer_fails() {
        let fake = Fake::answering([
            json!({"status": "ok", "json": "null"}),
            json!({"status": "ok", "json": "{\"status\":0,\"headers\":[],\"body\":\"\"}"}),
            json!({"status": "ok", "json": "{\"status\":200,\"headers\":[],\"body\":\"***\"}"}),
        ]);
        let caches = fake.page().caches();
        assert!(
            caches
                .get("pages", "https://example.com/a")
                .await
                .unwrap()
                .is_none()
        );
        assert!(matches!(
            caches.get("pages", "https://example.com/b").await,
            Err(Error::Unsupported(_))
        ));
        assert!(matches!(
            caches.get("pages", "https://example.com/c").await,
            Err(Error::Script(_))
        ));
    }

    #[tokio::test]
    async fn listing_and_removing_name_the_cache_and_the_url() {
        let fake = Fake::answering([
            json!({"status": "ok", "json": "[\"pages\",\"images\"]"}),
            json!({"status": "ok", "json": "[\"https://example.com/a\"]"}),
            json!({"status": "ok", "json": "true"}),
            json!({"status": "ok", "json": "false"}),
        ]);
        let caches = fake.page().caches();
        assert_eq!(caches.names().await.unwrap(), ["pages", "images"]);
        assert_eq!(
            caches.urls("pages").await.unwrap(),
            ["https://example.com/a"]
        );
        assert!(
            caches
                .remove("pages", "https://example.com/a")
                .await
                .unwrap()
        );
        assert!(!caches.delete("gone").await.unwrap());
        let commands = fake.commands();
        let asked: Vec<_> = commands.iter().map(|command| &command["op"]).collect();
        assert_eq!(
            asked,
            ["cache_names", "cache_urls", "cache_remove", "cache_delete"]
        );
        assert_eq!(commands[3]["arg"], json!({"cache": "gone", "url": ""}));
    }
}
