use std::fmt::{self, Debug};
use std::sync::Arc;

use url::Url;
use wreq::cookie::Jar;
use wreq::header::{
    AUTHORIZATION, CONTENT_ENCODING, CONTENT_LENGTH, CONTENT_TYPE, COOKIE, HeaderMap, LOCATION,
    PROXY_AUTHORIZATION, TRANSFER_ENCODING, WWW_AUTHENTICATE,
};

use super::middleware::{Middleware, MiddlewareResult};
use super::outcome::Outcome;
use super::redirect::{RedirectContext, RedirectPolicy};
use super::retry::{RetryContext, RetryPolicy};
use super::{
    ClientBuilder, ClientDerive, Error, IntoUri, Method, Request, RequestBuilder, Response, Result,
    StatusCode, Uri,
};

/// An HTTP client that sends every request through its middleware chain, redirect policy and retry
/// policy. Clones share the connection pool, cookie jar and middleware instances.
#[derive(Clone)]
pub struct Client {
    /// The wreq client, with its own retries, redirects and `Referer` turned off.
    pub(super) inner: wreq::Client,
    /// The cookie jar set through [`ClientBuilder::cookie_store`] or [`ClientBuilder::cookie_jar`].
    pub(super) jar: Option<Arc<Jar>>,
    /// The middleware chain, in the order it was added.
    pub(super) middleware: Arc<[Arc<dyn Middleware>]>,
    /// The retry policy for requests that carry none of their own.
    pub(super) retry_policy: RetryPolicy,
    /// The redirect policy for requests that carry none of their own.
    pub(super) redirect_policy: RedirectPolicy,
    /// The most middleware recoveries one request may replay.
    pub(super) max_recoveries: u32,
}

impl Debug for Client {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Client")
            .field("cookies", &self.jar.is_some())
            .field("middleware", &self.middleware.len())
            .field("retry_policy", &self.retry_policy)
            .field("redirect_policy", &self.redirect_policy)
            .field("max_recoveries", &self.max_recoveries)
            .finish_non_exhaustive()
    }
}

impl Client {
    /// Returns a [`ClientBuilder`] with the default policies and no middleware.
    #[must_use]
    pub fn builder() -> ClientBuilder {
        ClientBuilder::new()
    }

    /// Assembles a client from the parts a builder collected.
    pub(super) fn from_parts(
        inner: wreq::Client,
        jar: Option<Arc<Jar>>,
        middleware: Vec<Arc<dyn Middleware>>,
        retry_policy: RetryPolicy,
        redirect_policy: RedirectPolicy,
        max_recoveries: u32,
    ) -> Self {
        Self {
            inner,
            jar,
            middleware: middleware.into(),
            retry_policy,
            redirect_policy,
            max_recoveries,
        }
    }

    /// Returns a [`ClientDerive`] that starts from this client's transport, cookie jar, middleware
    /// and policies, for building a child client.
    #[must_use]
    pub fn derive(&self) -> ClientDerive {
        ClientDerive::from_client(self)
    }

    /// Returns the cookie jar, or `None` when the client was built without
    /// [`ClientBuilder::cookie_store`] or [`ClientBuilder::cookie_jar`].
    #[must_use]
    pub fn cookie_jar(&self) -> Option<&Arc<Jar>> {
        self.jar.as_ref()
    }

    /// Returns a [`RequestBuilder`] for `method` on `uri`; an invalid `uri` fails the request when
    /// it is built.
    pub fn request<U: IntoUri>(&self, method: Method, uri: U) -> RequestBuilder {
        RequestBuilder::new(self.clone(), self.inner.request(method, uri))
    }

    /// Returns a [`RequestBuilder`] for a `GET` to `uri`.
    pub fn get<U: IntoUri>(&self, uri: U) -> RequestBuilder {
        self.request(Method::GET, uri)
    }

    /// Returns a [`RequestBuilder`] for a `POST` to `uri`.
    pub fn post<U: IntoUri>(&self, uri: U) -> RequestBuilder {
        self.request(Method::POST, uri)
    }

    /// Returns a [`RequestBuilder`] for a `PUT` to `uri`.
    pub fn put<U: IntoUri>(&self, uri: U) -> RequestBuilder {
        self.request(Method::PUT, uri)
    }

    /// Returns a [`RequestBuilder`] for a `PATCH` to `uri`.
    pub fn patch<U: IntoUri>(&self, uri: U) -> RequestBuilder {
        self.request(Method::PATCH, uri)
    }

    /// Returns a [`RequestBuilder`] for a `DELETE` to `uri`.
    pub fn delete<U: IntoUri>(&self, uri: U) -> RequestBuilder {
        self.request(Method::DELETE, uri)
    }

    /// Returns a [`RequestBuilder`] for a `HEAD` to `uri`.
    pub fn head<U: IntoUri>(&self, uri: U) -> RequestBuilder {
        self.request(Method::HEAD, uri)
    }

    /// Returns a [`RequestBuilder`] for an `OPTIONS` to `uri`.
    pub fn options<U: IntoUri>(&self, uri: U) -> RequestBuilder {
        self.request(Method::OPTIONS, uri)
    }

    /// Sends `request` through the middleware chain, replaying after middleware recoveries,
    /// following redirects and retrying until the policies return a response or an error.
    ///
    /// A policy attached to the request wins over the client's. A followed redirect does not use up
    /// a retry. A request whose body cannot be cloned, such as a stream, is sent exactly once with
    /// no recovery, redirect or retry. A 4xx or 5xx status is returned as a response, not an error.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Redirect`] past the redirect limit, the transport or middleware error of
    /// the last send when it is not retried, and the error of a middleware that fails while
    /// recovering.
    pub async fn execute(&self, mut request: Request) -> Result<Response> {
        let mut history = Vec::new();
        let mut attempt = 1;
        let mut recoveries = 0;

        loop {
            let Some(replay) = request.try_clone() else {
                return self.send_once(request).await.map_err(Into::into);
            };
            let result = self.send_once(replay).await;
            let visited = Self::visited(&history, &request);
            let outcome = self
                .next(&request, &result, visited, attempt, recoveries)
                .await?;

            match outcome {
                Outcome::Replay => recoveries += 1,
                Outcome::Redirect {
                    status,
                    destination,
                } => {
                    if history.is_empty() {
                        history.push(request.uri().clone());
                    }
                    Self::prepare_next_redirect(&mut request, status, destination);
                    history.push(request.uri().clone());
                }
                Outcome::Retry(delay) => {
                    drop(result);
                    tokio::time::sleep(delay).await;
                    attempt += 1;
                }
                Outcome::Return => return result.map_err(Into::into),
            }
        }
    }

    /// Decides what follows one send: a replay when a middleware recovered, a hop for a redirect
    /// status with a usable `Location` the redirect policy accepts, otherwise a retry when the
    /// retry policy gives a delay, or returning the result.
    async fn next(
        &self,
        request: &Request,
        result: &MiddlewareResult<Response>,
        history: &[Uri],
        attempt: u32,
        recoveries: u32,
    ) -> Result<Outcome> {
        if let Ok(response) = result {
            if recoveries < self.max_recoveries && self.recover(request, response).await? {
                return Ok(Outcome::Replay);
            }

            let status = response.status();
            if Self::is_redirect(status)
                && let Some(destination) = Self::location(response, history)
            {
                let context = RedirectContext {
                    status,
                    destination: &destination,
                    history,
                    hop: history.len(),
                };
                let redirect_policy = request
                    .extensions()
                    .get::<RedirectPolicy>()
                    .copied()
                    .unwrap_or(self.redirect_policy);
                return match redirect_policy.follow(&context) {
                    Ok(true) => Ok(Outcome::Redirect {
                        status,
                        destination,
                    }),
                    Ok(false) => Ok(Outcome::Return),
                    Err(error) => Err(Error::Redirect(error)),
                };
            }
        }

        let retry_policy = request
            .extensions()
            .get::<RetryPolicy>()
            .copied()
            .unwrap_or(self.retry_policy);
        let context = RetryContext {
            attempt,
            request,
            response: result.as_ref().ok(),
            error: result.as_ref().err().map(AsRef::as_ref),
        };
        Ok(match retry_policy.delay(&context) {
            Some(delay) => Outcome::Retry(delay),
            None => Outcome::Return,
        })
    }

    /// Rewrites `request` for the hop to `destination`. Credentials are dropped when the origin
    /// changes; for 301, 302 and 303 the body and its headers are dropped and the method becomes
    /// `GET` unless it is `HEAD`.
    fn prepare_next_redirect(request: &mut Request, status: StatusCode, destination: Uri) {
        if !Self::same_origin(request.uri(), &destination) {
            Self::remove_credentials(request.headers_mut());
        }
        if !Self::keeps_body(status) {
            if request.method() != Method::HEAD {
                *request.method_mut() = Method::GET;
            }
            *request.body_mut() = None;
            Self::remove_payload_headers(request.headers_mut());
        }
        *request.uri_mut() = destination;
    }

    /// Sends `request` once: `before` hooks in order, the transport unless a hook answered with a
    /// response, then every `after` hook in reverse order.
    async fn send_once(&self, mut request: Request) -> MiddlewareResult<Response> {
        let result = match self.before(&mut request).await {
            Ok(Some(response)) => Ok(response),
            Ok(None) => self.inner.execute(request).await.map_err(Into::into),
            Err(error) => Err(error),
        };
        self.after(result).await
    }

    /// Runs each `before` hook in order, stopping at the first that answers with a response.
    async fn before(&self, request: &mut Request) -> MiddlewareResult<Option<Response>> {
        for middleware in self.middleware.iter() {
            if let Some(response) = middleware.before(request).await? {
                return Ok(Some(response));
            }
        }
        Ok(None)
    }

    /// Runs every `after` hook on `result`, last added first.
    async fn after(&self, mut result: MiddlewareResult<Response>) -> MiddlewareResult<Response> {
        for middleware in self.middleware.iter().rev() {
            result = middleware.after(result).await;
        }
        result
    }

    /// Asks every middleware to recover from `response`; true when at least one did.
    async fn recover(&self, request: &Request, response: &Response) -> Result<bool> {
        let mut recovered = false;
        for middleware in self.middleware.iter() {
            recovered |= middleware.recover(request, response).await?;
        }
        Ok(recovered)
    }

    /// Whether `status` is 301, 302, 303, 307 or 308.
    fn is_redirect(status: StatusCode) -> bool {
        matches!(
            status,
            StatusCode::MOVED_PERMANENTLY
                | StatusCode::FOUND
                | StatusCode::SEE_OTHER
                | StatusCode::TEMPORARY_REDIRECT
                | StatusCode::PERMANENT_REDIRECT
        )
    }

    /// Whether a redirect with `status` keeps the method and body, which holds for 307 and 308.
    fn keeps_body(status: StatusCode) -> bool {
        matches!(
            status,
            StatusCode::TEMPORARY_REDIRECT | StatusCode::PERMANENT_REDIRECT
        )
    }

    /// Whether `left` and `right` share scheme, port and host, the host compared ignoring ASCII
    /// case.
    fn same_origin(left: &Uri, right: &Uri) -> bool {
        let same_host = match (left.host(), right.host()) {
            (Some(left), Some(right)) => left.eq_ignore_ascii_case(right),
            (None, None) => true,
            _ => false,
        };
        same_host && left.scheme() == right.scheme() && left.port_u16() == right.port_u16()
    }

    /// Returns the URIs `request` has visited: `history` once a redirect was followed, otherwise
    /// its own URI alone, so a request that never redirects keeps no history.
    fn visited<'a>(history: &'a [Uri], request: &'a Request) -> &'a [Uri] {
        if history.is_empty() {
            std::slice::from_ref(request.uri())
        } else {
            history
        }
    }

    /// Returns the response's `Location` resolved against the last URI in `history`, or `None` when
    /// it is missing or invalid.
    fn location(response: &Response, history: &[Uri]) -> Option<Uri> {
        let location = response.headers().get(LOCATION)?.to_str().ok()?;
        Url::parse(&history.last()?.to_string())
            .ok()?
            .join(location)
            .ok()?
            .as_str()
            .parse()
            .ok()
    }

    /// Removes the `Authorization`, `Cookie`, `Proxy-Authorization` and `WWW-Authenticate` headers.
    fn remove_credentials(headers: &mut HeaderMap) {
        headers.remove(AUTHORIZATION);
        headers.remove(COOKIE);
        headers.remove(PROXY_AUTHORIZATION);
        headers.remove(WWW_AUTHENTICATE);
    }

    /// Removes the `Content-Type`, `Content-Length`, `Content-Encoding` and `Transfer-Encoding`
    /// headers.
    fn remove_payload_headers(headers: &mut HeaderMap) {
        headers.remove(CONTENT_TYPE);
        headers.remove(CONTENT_LENGTH);
        headers.remove(CONTENT_ENCODING);
        headers.remove(TRANSFER_ENCODING);
    }
}
