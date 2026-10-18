use std::borrow::Cow;
use std::fmt::{self, Debug};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::time::Duration;

use bytes::Bytes;
use http::Extensions;
use serde::Serialize;
use serde::de::DeserializeOwned;
use wreq::cookie::IntoCookieStore;
use wreq::header::{HeaderMap, HeaderValue, OrigHeaderMap};
use wreq::multipart::Form;

use super::blocking;
use super::redirect::RedirectPolicy;
use super::retry::RetryPolicy;
use super::{Body, Client, Group, IntoEmulation, Proxy, Request, Response, Result, Version};

/// A request being built for a [`Client`], sent through its middleware, redirect and retry
/// policies. The `_sync` senders block the calling thread and work with or without a runtime.
pub struct RequestBuilder {
    client: Client,
    inner: wreq::RequestBuilder,
    extensions: Extensions,
}

impl Debug for RequestBuilder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RequestBuilder")
            .field("client", &self.client)
            .finish_non_exhaustive()
    }
}

impl RequestBuilder {
    /// Wraps the wreq builder `inner` for `client`, with no extensions.
    pub(crate) fn new(client: Client, inner: wreq::RequestBuilder) -> Self {
        Self {
            client,
            inner,
            extensions: Extensions::new(),
        }
    }

    /// Adds header `name` with `value`, keeping earlier values of the same name; an invalid name or
    /// value fails the request when it is built.
    #[must_use]
    pub fn header(mut self, name: impl AsRef<str>, value: impl AsRef<[u8]>) -> Self {
        self.inner = self.inner.header(name.as_ref(), value.as_ref());
        self
    }

    /// Sets every header in `headers`, replacing earlier values of the same names.
    #[must_use]
    pub fn headers(mut self, headers: HeaderMap) -> Self {
        self.inner = self.inner.headers(headers);
        self
    }

    /// Sets the original casing and order of header names for this request.
    #[must_use]
    pub fn orig_headers(mut self, headers: OrigHeaderMap) -> Self {
        self.inner = self.inner.orig_headers(headers);
        self
    }

    /// Whether the client's default headers are sent with this request; on by default.
    #[must_use]
    pub fn default_headers(mut self, enabled: bool) -> Self {
        self.inner = self.inner.default_headers(enabled);
        self
    }

    /// Sets the `Authorization` header to `token` as given, marked sensitive.
    #[must_use]
    pub fn auth<V>(mut self, token: V) -> Self
    where
        HeaderValue: TryFrom<V>,
        <HeaderValue as TryFrom<V>>::Error: Into<http::Error>,
    {
        self.inner = self.inner.auth(token);
        self
    }

    /// Sets the `Authorization` header to HTTP Basic credentials for `username` and `password`.
    #[must_use]
    pub fn basic_auth<U, P>(mut self, username: U, password: Option<P>) -> Self
    where
        U: fmt::Display,
        P: fmt::Display,
    {
        self.inner = self.inner.basic_auth(username, password);
        self
    }

    /// Sets the `Authorization` header to `Bearer` followed by `token`.
    #[must_use]
    pub fn bearer_auth(mut self, token: impl fmt::Display) -> Self {
        self.inner = self.inner.bearer_auth(token);
        self
    }

    /// Sets the request body. A body that cannot be cloned, such as a stream, is sent once with no
    /// recovery, redirect or retry.
    #[must_use]
    pub fn body(mut self, body: impl Into<Body>) -> Self {
        self.inner = self.inner.body(body);
        self
    }

    /// Serialises `value` as the JSON body, with `Content-Type: application/json` unless one is
    /// already set; a serialisation failure fails the request when it is built.
    #[must_use]
    pub fn json_body<T: Serialize + ?Sized>(mut self, value: &T) -> Self {
        self.inner = self.inner.json(value);
        self
    }

    /// Serialises `value` as a URL-encoded form body, with the matching `Content-Type` unless one
    /// is already set; a serialisation failure fails the request when it is built.
    #[must_use]
    pub fn form<T: Serialize + ?Sized>(mut self, value: &T) -> Self {
        self.inner = self.inner.form(value);
        self
    }

    /// Sets `form` as a multipart body.
    #[must_use]
    pub fn multipart(mut self, form: Form) -> Self {
        self.inner = self.inner.multipart(form);
        self
    }

    /// Sets the URL's query string to `value` serialised as a URL-encoded form; a serialisation
    /// failure fails the request when it is built.
    #[must_use]
    pub fn query<T: Serialize + ?Sized>(mut self, value: &T) -> Self {
        self.inner = self.inner.query(value);
        self
    }

    /// Sets the timeout for one send of this request, overriding the client's; each retry and
    /// redirect hop gets the full timeout again.
    #[must_use]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.inner = self.inner.timeout(timeout);
        self
    }

    /// Sets the timeout for reading the response body of this request, overriding the client's.
    #[must_use]
    pub fn read_timeout(mut self, timeout: Duration) -> Self {
        self.inner = self.inner.read_timeout(timeout);
        self
    }

    /// Sets the HTTP version of this request.
    #[must_use]
    pub fn version(mut self, version: Version) -> Self {
        self.inner = self.inner.version(version);
        self
    }

    /// Uses `cookie_store` as the cookie store for this request instead of the client's.
    #[must_use]
    pub fn cookie_provider<C>(mut self, cookie_store: C) -> Self
    where
        C: IntoCookieStore,
    {
        self.inner = self.inner.cookie_provider(cookie_store);
        self
    }

    /// Whether this request asks for gzip responses and decompresses them, overriding the client.
    #[must_use]
    pub fn gzip(mut self, enabled: bool) -> Self {
        self.inner = self.inner.gzip(enabled);
        self
    }

    /// Whether this request asks for Brotli responses and decompresses them, overriding the client.
    #[must_use]
    pub fn brotli(mut self, enabled: bool) -> Self {
        self.inner = self.inner.brotli(enabled);
        self
    }

    /// Whether this request asks for zstd responses and decompresses them, overriding the client.
    #[must_use]
    pub fn zstd(mut self, enabled: bool) -> Self {
        self.inner = self.inner.zstd(enabled);
        self
    }

    /// Whether this request asks for deflate responses and decompresses them, overriding the
    /// client.
    #[must_use]
    pub fn deflate(mut self, enabled: bool) -> Self {
        self.inner = self.inner.deflate(enabled);
        self
    }

    /// Sends this request through `proxy` instead of the client's proxies.
    #[must_use]
    pub fn proxy(mut self, proxy: Proxy) -> Self {
        self.inner = self.inner.proxy(proxy);
        self
    }

    /// Binds this request's connection to the local `address`.
    #[must_use]
    pub fn local_address<A>(mut self, address: A) -> Self
    where
        A: Into<Option<IpAddr>>,
    {
        self.inner = self.inner.local_address(address);
        self
    }

    /// Binds this request's connection to the local `ipv4` or `ipv6` address.
    #[must_use]
    pub fn local_addresses<A4, A6>(mut self, ipv4: A4, ipv6: A6) -> Self
    where
        A4: Into<Option<Ipv4Addr>>,
        A6: Into<Option<Ipv6Addr>>,
    {
        self.inner = self.inner.local_addresses(ipv4, ipv6);
        self
    }

    /// Binds this request's connection to the network interface named `interface`.
    #[cfg(any(
        target_os = "android",
        target_os = "fuchsia",
        target_os = "illumos",
        target_os = "ios",
        target_os = "linux",
        target_os = "macos",
        target_os = "solaris",
        target_os = "tvos",
        target_os = "visionos",
        target_os = "watchos",
    ))]
    #[must_use]
    pub fn interface<I>(mut self, interface: I) -> Self
    where
        I: Into<Cow<'static, str>>,
    {
        self.inner = self.inner.interface(interface);
        self
    }

    /// Sets the browser to emulate for this request, overriding the client's.
    #[must_use]
    pub fn emulation<E>(mut self, emulation: E) -> Self
    where
        E: IntoEmulation,
    {
        self.inner = self.inner.emulation(emulation);
        self
    }

    /// Assigns this request to a wreq `Group`, which keeps its connections and state apart from
    /// other groups.
    #[must_use]
    pub fn group(mut self, group: Group) -> Self {
        self.inner = self.inner.group(group);
        self
    }

    /// Sets the retry policy for this request, overriding the client's.
    #[must_use]
    pub fn retry(self, retry: RetryPolicy) -> Self {
        self.extension(retry)
    }

    /// Sets the redirect policy for this request, overriding the client's.
    #[must_use]
    pub fn redirect(self, redirect: RedirectPolicy) -> Self {
        self.extension(redirect)
    }

    /// Attaches `value` to the request for middleware to read, replacing an earlier value of the
    /// same type; [`Anonymous`](crate::middleware::Anonymous) skips the bearer login this way.
    #[must_use]
    pub fn extension<T: Clone + Send + Sync + 'static>(mut self, value: T) -> Self {
        self.extensions.insert(value);
        self
    }

    /// Returns a copy of this builder, or `None` when the body cannot be cloned.
    pub fn try_clone(&self) -> Option<Self> {
        Some(Self {
            client: self.client.clone(),
            inner: self.inner.try_clone()?,
            extensions: self.extensions.clone(),
        })
    }

    /// Builds the [`Request`] with its extensions attached, failing when a header, URI or body set
    /// earlier was invalid.
    pub fn build(self) -> Result<Request> {
        self.into_parts().1
    }

    /// Sends the request and returns the final response, whatever its status; see
    /// [`Client::execute`] for the errors.
    pub async fn send(self) -> Result<Response> {
        let (client, request) = self.into_parts();
        client.execute(request?).await
    }

    /// Splits the builder into its client and the request [`RequestBuilder::build`] returns.
    fn into_parts(self) -> (Client, Result<Request>) {
        let Self {
            client,
            inner,
            extensions,
        } = self;
        let request = inner.build().map_err(Into::into).map(|mut request| {
            request.extensions_mut().extend(extensions);
            request
        });
        (client, request)
    }

    /// Sends the request and fails with [`Error::Transport`](crate::Error::Transport) when the
    /// final status is 4xx or 5xx.
    pub async fn send_for_status(self) -> Result<Response> {
        self.send().await?.error_for_status().map_err(Into::into)
    }

    /// Sends the request and returns the body as text, failing on a 4xx or 5xx status.
    pub async fn text(self) -> Result<String> {
        self.send_for_status()
            .await?
            .text()
            .await
            .map_err(Into::into)
    }

    /// Sends the request and returns the raw body, failing on a 4xx or 5xx status.
    pub async fn bytes(self) -> Result<Bytes> {
        self.send_for_status()
            .await?
            .bytes()
            .await
            .map_err(Into::into)
    }

    /// Sends the request and deserialises the JSON body into `T`, failing on a 4xx or 5xx status or
    /// a body that does not fit `T`.
    pub async fn json<T: DeserializeOwned>(self) -> Result<T> {
        self.send_for_status()
            .await?
            .json::<T>()
            .await
            .map_err(Into::into)
    }

    /// Blocks the calling thread on [`RequestBuilder::send`].
    pub fn send_sync(self) -> Result<Response> {
        blocking::block_on(self.send())
    }

    /// Blocks the calling thread on [`RequestBuilder::text`].
    pub fn text_sync(self) -> Result<String> {
        blocking::block_on(self.text())
    }

    /// Blocks the calling thread on [`RequestBuilder::bytes`].
    pub fn bytes_sync(self) -> Result<Bytes> {
        blocking::block_on(self.bytes())
    }

    /// Blocks the calling thread on [`RequestBuilder::json`].
    pub fn json_sync<T: DeserializeOwned + Send>(self) -> Result<T> {
        blocking::block_on(self.json::<T>())
    }
}
