use std::borrow::Cow;
use std::fmt::{self, Debug};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use wreq::cookie::{IntoCookieStore, Jar};
use wreq::dns::IntoResolve;
use wreq::header::{HeaderMap, HeaderValue, OrigHeaderMap};
use wreq::http1::Http1Options;
use wreq::http2::Http2Options;
use wreq::tls::keylog::KeyLog;
use wreq::tls::session::IntoTlsSessionCache;
use wreq::tls::trust::{CertStore, Identity};
use wreq::tls::{TlsOptions, TlsVersion};
use wreq::{redirect, retry};

use super::middleware::Middleware;
use super::redirect::RedirectPolicy;
use super::retry::RetryPolicy;
use super::{Client, IntoEmulation, Proxy, Result};

/// A builder for [`Client`] that configures the wreq transport, the middleware chain and the retry
/// and redirect policies.
pub struct ClientBuilder {
    inner: wreq::ClientBuilder,
    jar: Option<Arc<Jar>>,
    middleware: Vec<Arc<dyn Middleware>>,
    retry_policy: RetryPolicy,
    redirect_policy: RedirectPolicy,
    max_recoveries: u32,
}

impl Debug for ClientBuilder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClientBuilder")
            .field("cookies", &self.jar.is_some())
            .field("middleware", &self.middleware.len())
            .field("retry_policy", &self.retry_policy)
            .field("redirect_policy", &self.redirect_policy)
            .field("max_recoveries", &self.max_recoveries)
            .finish_non_exhaustive()
    }
}

impl ClientBuilder {
    /// Returns a builder with no cookie store, no middleware, [`RetryPolicy::new`],
    /// [`RedirectPolicy::new`] and up to 4 middleware recoveries per request.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: wreq::Client::builder(),
            jar: None,
            middleware: Vec::new(),
            retry_policy: RetryPolicy::new(),
            redirect_policy: RedirectPolicy::new(),
            max_recoveries: 4,
        }
    }

    /// Sets the browser to emulate: default headers, HTTP/1 and HTTP/2 settings and TLS
    /// fingerprint. It overwrites those settings, so call it before fine-tuning them.
    #[must_use]
    pub fn emulation(mut self, emulation: impl IntoEmulation) -> Self {
        self.inner = self.inner.emulation(emulation);
        self
    }

    /// Sets the `User-Agent` header sent with every request.
    #[must_use]
    pub fn user_agent<V>(mut self, value: V) -> Self
    where
        HeaderValue: TryFrom<V>,
        <HeaderValue as TryFrom<V>>::Error: Into<http::Error>,
    {
        self.inner = self.inner.user_agent(value);
        self
    }

    /// Sets the headers sent with every request.
    #[must_use]
    pub fn default_headers(mut self, headers: HeaderMap) -> Self {
        self.inner = self.inner.default_headers(headers);
        self
    }

    /// Sets the original casing and order of header names for every request.
    #[must_use]
    pub fn orig_headers(mut self, headers: OrigHeaderMap) -> Self {
        self.inner = self.inner.orig_headers(headers);
        self
    }

    /// Gives the client a fresh cookie jar when `enabled`, readable later through
    /// [`Client::cookie_jar`]; `false` leaves the builder unchanged.
    #[must_use]
    pub fn cookie_store(self, enabled: bool) -> Self {
        if enabled {
            self.cookie_jar(Arc::new(Jar::default()))
        } else {
            self
        }
    }

    /// Uses `jar` as the cookie store and keeps it readable through [`Client::cookie_jar`].
    #[must_use]
    pub fn cookie_jar(mut self, jar: Arc<Jar>) -> Self {
        self.inner = self.inner.cookie_provider(jar.clone());
        self.jar = Some(jar);
        self
    }

    /// Uses `cookie_store` as the cookie store. Unlike [`ClientBuilder::cookie_jar`], it is not
    /// exposed through [`Client::cookie_jar`].
    #[must_use]
    pub fn cookie_provider<C>(mut self, cookie_store: C) -> Self
    where
        C: IntoCookieStore,
    {
        self.inner = self.inner.cookie_provider(cookie_store);
        self
    }

    /// Whether to ask for gzip responses and decompress them.
    #[must_use]
    pub fn gzip(mut self, enabled: bool) -> Self {
        self.inner = self.inner.gzip(enabled);
        self
    }

    /// Whether to ask for Brotli responses and decompress them.
    #[must_use]
    pub fn brotli(mut self, enabled: bool) -> Self {
        self.inner = self.inner.brotli(enabled);
        self
    }

    /// Whether to ask for zstd responses and decompress them.
    #[must_use]
    pub fn zstd(mut self, enabled: bool) -> Self {
        self.inner = self.inner.zstd(enabled);
        self
    }

    /// Whether to ask for deflate responses and decompress them.
    #[must_use]
    pub fn deflate(mut self, enabled: bool) -> Self {
        self.inner = self.inner.deflate(enabled);
        self
    }

    /// Sets the timeout for one send, from connecting until the body is read; none by default. Each
    /// retry and redirect hop gets the full timeout again.
    #[must_use]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.inner = self.inner.timeout(timeout);
        self
    }

    /// Sets the timeout for the read phase of each send; none by default.
    #[must_use]
    pub fn read_timeout(mut self, timeout: Duration) -> Self {
        self.inner = self.inner.read_timeout(timeout);
        self
    }

    /// Sets the timeout for connecting, covering the whole race when a host resolves to several
    /// addresses; none by default.
    #[must_use]
    pub fn connect_timeout(mut self, timeout: Duration) -> Self {
        self.inner = self.inner.connect_timeout(timeout);
        self
    }

    /// Adds `proxy` to the proxies the client uses, which turns off the system proxy.
    #[must_use]
    pub fn proxy(mut self, proxy: Proxy) -> Self {
        self.inner = self.inner.proxy(proxy);
        self
    }

    /// Clears every proxy, including the system proxy.
    #[must_use]
    pub fn no_proxy(mut self) -> Self {
        self.inner = self.inner.no_proxy();
        self
    }

    /// Whether connections log their reads and writes at `TRACE` level.
    #[must_use]
    pub fn connection_verbose(mut self, verbose: bool) -> Self {
        self.inner = self.inner.connection_verbose(verbose);
        self
    }

    /// Sets how long an idle connection stays in the pool; `None` keeps it with no limit, and the
    /// default is 90 seconds.
    #[must_use]
    pub fn pool_idle_timeout<D>(mut self, timeout: D) -> Self
    where
        D: Into<Option<Duration>>,
    {
        self.inner = self.inner.pool_idle_timeout(timeout);
        self
    }

    /// Sets the most idle connections the pool keeps per host.
    #[must_use]
    pub fn pool_max_idle_per_host(mut self, max: usize) -> Self {
        self.inner = self.inner.pool_max_idle_per_host(max);
        self
    }

    /// Sets the most connections the pool holds.
    #[must_use]
    pub fn pool_max_size(mut self, max: usize) -> Self {
        self.inner = self.inner.pool_max_size(max);
        self
    }

    /// Whether to refuse every request that is not HTTPS; off by default.
    #[must_use]
    pub fn https_only(mut self, enabled: bool) -> Self {
        self.inner = self.inner.https_only(enabled);
        self
    }

    /// Restricts the client to HTTP/1.
    #[must_use]
    pub fn http1_only(mut self) -> Self {
        self.inner = self.inner.http1_only();
        self
    }

    /// Restricts the client to HTTP/2.
    #[must_use]
    pub fn http2_only(mut self) -> Self {
        self.inner = self.inner.http2_only();
        self
    }

    /// Sets the HTTP/1 connection options.
    #[must_use]
    pub fn http1_options<T>(mut self, options: T) -> Self
    where
        T: Into<Option<Http1Options>>,
    {
        self.inner = self.inner.http1_options(options);
        self
    }

    /// Sets the HTTP/2 connection options.
    #[must_use]
    pub fn http2_options<T>(mut self, options: T) -> Self
    where
        T: Into<Option<Http2Options>>,
    {
        self.inner = self.inner.http2_options(options);
        self
    }

    /// Whether sockets set `TCP_NODELAY`; on by default.
    #[must_use]
    pub fn tcp_nodelay(mut self, enabled: bool) -> Self {
        self.inner = self.inner.tcp_nodelay(enabled);
        self
    }

    /// Sets the TCP keepalive idle time on every socket; `None` leaves the option unset, and the
    /// default is 15 seconds.
    #[must_use]
    pub fn tcp_keepalive<D>(mut self, value: D) -> Self
    where
        D: Into<Option<Duration>>,
    {
        self.inner = self.inner.tcp_keepalive(value);
        self
    }

    /// Sets the interval between TCP keepalive probes; `None` leaves the option unset, and the
    /// default is 15 seconds.
    #[must_use]
    pub fn tcp_keepalive_interval<D>(mut self, value: D) -> Self
    where
        D: Into<Option<Duration>>,
    {
        self.inner = self.inner.tcp_keepalive_interval(value);
        self
    }

    /// Sets how many unanswered TCP keepalive probes drop a connection; `None` leaves the option
    /// unset, and the default is 3.
    #[must_use]
    pub fn tcp_keepalive_retries<C>(mut self, retries: C) -> Self
    where
        C: Into<Option<u32>>,
    {
        self.inner = self.inner.tcp_keepalive_retries(retries);
        self
    }

    /// Sets `TCP_USER_TIMEOUT` on every socket; `None` leaves the option unset.
    #[cfg(any(target_os = "android", target_os = "fuchsia", target_os = "linux"))]
    #[must_use]
    pub fn tcp_user_timeout<D>(mut self, value: D) -> Self
    where
        D: Into<Option<Duration>>,
    {
        self.inner = self.inner.tcp_user_timeout(value);
        self
    }

    /// Whether sockets set `SO_REUSEADDR`.
    #[must_use]
    pub fn tcp_reuse_address(mut self, enabled: bool) -> Self {
        self.inner = self.inner.tcp_reuse_address(enabled);
        self
    }

    /// Sets `SO_LINGER` on every socket; zero resets the connection on close instead of closing it
    /// cleanly, and `None` keeps the system default.
    #[must_use]
    pub fn tcp_linger<D>(mut self, value: D) -> Self
    where
        D: Into<Option<Duration>>,
    {
        self.inner = self.inner.tcp_linger(value);
        self
    }

    /// Sets the socket send buffer size, `SO_SNDBUF` on most systems.
    #[must_use]
    pub fn tcp_send_buffer_size<S>(mut self, size: S) -> Self
    where
        S: Into<Option<usize>>,
    {
        self.inner = self.inner.tcp_send_buffer_size(size);
        self
    }

    /// Sets the socket receive buffer size, `SO_RCVBUF` on most systems.
    #[must_use]
    pub fn tcp_recv_buffer_size<S>(mut self, size: S) -> Self
    where
        S: Into<Option<usize>>,
    {
        self.inner = self.inner.tcp_recv_buffer_size(size);
        self
    }

    /// Sets the delay before the next address of a host is tried in parallel (Happy Eyeballs);
    /// `None` turns parallel attempts off, and the default is 300 milliseconds.
    #[must_use]
    pub fn tcp_happy_eyeballs_timeout<D>(mut self, value: D) -> Self
    where
        D: Into<Option<Duration>>,
    {
        self.inner = self.inner.tcp_happy_eyeballs_timeout(value);
        self
    }

    /// Binds outgoing connections to the local `address`.
    #[must_use]
    pub fn local_address<T>(mut self, address: T) -> Self
    where
        T: Into<Option<IpAddr>>,
    {
        self.inner = self.inner.local_address(address);
        self
    }

    /// Binds outgoing connections to the local `ipv4` or `ipv6` address.
    #[must_use]
    pub fn local_addresses<V4, V6>(mut self, ipv4: V4, ipv6: V6) -> Self
    where
        V4: Into<Option<Ipv4Addr>>,
        V6: Into<Option<Ipv6Addr>>,
    {
        self.inner = self.inner.local_addresses(ipv4, ipv6);
        self
    }

    /// Binds outgoing connections to the network interface named `interface`.
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
    pub fn interface<T>(mut self, interface: T) -> Self
    where
        T: Into<Cow<'static, str>>,
    {
        self.inner = self.inner.interface(interface);
        self
    }

    /// Sets the client certificate presented for TLS client authentication.
    #[must_use]
    pub fn tls_identity(mut self, identity: Identity) -> Self {
        self.inner = self.inner.tls_identity(identity);
        self
    }

    /// Sets the certificate store that verifies servers, in place of the system store.
    #[must_use]
    pub fn tls_cert_store(mut self, store: CertStore) -> Self {
        self.inner = self.inner.tls_cert_store(store);
        self
    }

    /// Whether to verify server certificates; on by default, and off trusts any certificate.
    #[must_use]
    pub fn tls_cert_verification(mut self, enabled: bool) -> Self {
        self.inner = self.inner.tls_cert_verification(enabled);
        self
    }

    /// Whether to check that the server certificate matches the host name; on by default.
    #[must_use]
    pub fn tls_verify_hostname(mut self, enabled: bool) -> Self {
        self.inner = self.inner.tls_verify_hostname(enabled);
        self
    }

    /// Whether to send Server Name Indication; on by default.
    #[must_use]
    pub fn tls_sni(mut self, enabled: bool) -> Self {
        self.inner = self.inner.tls_sni(enabled);
        self
    }

    /// Sets where TLS session keys are logged, for decrypting captured traffic.
    #[must_use]
    pub fn tls_keylog(mut self, keylog: KeyLog) -> Self {
        self.inner = self.inner.tls_keylog(keylog);
        self
    }

    /// Sets the lowest TLS version the client accepts; the TLS backend decides by default.
    #[must_use]
    pub fn tls_min_version(mut self, version: TlsVersion) -> Self {
        self.inner = self.inner.tls_min_version(version);
        self
    }

    /// Sets the highest TLS version the client offers; no maximum by default.
    #[must_use]
    pub fn tls_max_version(mut self, version: TlsVersion) -> Self {
        self.inner = self.inner.tls_max_version(version);
        self
    }

    /// Whether responses carry the TLS connection details as an extension.
    #[must_use]
    pub fn tls_info(mut self, enabled: bool) -> Self {
        self.inner = self.inner.tls_info(enabled);
        self
    }

    /// Sets the cache that keeps TLS sessions for resumption, in place of the in-memory default.
    #[must_use]
    pub fn tls_session_cache<S>(mut self, store: S) -> Self
    where
        S: IntoTlsSessionCache,
    {
        self.inner = self.inner.tls_session_cache(store);
        self
    }

    /// Sets the TLS options for every connection.
    #[must_use]
    pub fn tls_options<T>(mut self, options: T) -> Self
    where
        T: Into<Option<TlsOptions>>,
    {
        self.inner = self.inner.tls_options(options);
        self
    }

    /// Resolves `domain` to `address` without asking DNS. The port of `address` is ignored: the
    /// port comes from the URI.
    #[must_use]
    pub fn resolve<D>(mut self, domain: D, address: SocketAddr) -> Self
    where
        D: Into<Cow<'static, str>>,
    {
        self.inner = self.inner.resolve(domain, address);
        self
    }

    /// Resolves `domain` to `addresses` without asking DNS. Their ports are ignored: the port comes
    /// from the URI.
    #[must_use]
    pub fn resolve_to_addrs<D, A>(mut self, domain: D, addresses: A) -> Self
    where
        D: Into<Cow<'static, str>>,
        A: IntoIterator<Item = SocketAddr>,
    {
        self.inner = self.inner.resolve_to_addrs(domain, addresses);
        self
    }

    /// Sets the DNS resolver; [`ClientBuilder::resolve`] and [`ClientBuilder::resolve_to_addrs`]
    /// still apply on top of it.
    #[must_use]
    pub fn dns_resolver<R>(mut self, resolver: R) -> Self
    where
        R: IntoResolve,
    {
        self.inner = self.inner.dns_resolver(resolver);
        self
    }

    /// Appends `middleware` to the chain: `before` hooks run in the order added and `after` hooks
    /// in reverse.
    #[must_use]
    pub fn middleware(mut self, middleware: impl Middleware) -> Self {
        self.middleware.push(Arc::new(middleware));
        self
    }

    /// Sets the retry policy for requests that carry none of their own; [`RetryPolicy::new`] by
    /// default.
    #[must_use]
    pub fn retry(mut self, retry_policy: RetryPolicy) -> Self {
        self.retry_policy = retry_policy;
        self
    }

    /// Sets the redirect policy for requests that carry none of their own; [`RedirectPolicy::new`]
    /// by default.
    #[must_use]
    pub fn redirect(mut self, redirect_policy: RedirectPolicy) -> Self {
        self.redirect_policy = redirect_policy;
        self
    }

    /// Sets how many times middleware recoveries may replay one request; 4 by default, 0 turns
    /// recovery off.
    #[must_use]
    pub fn max_recoveries(mut self, max_recoveries: u32) -> Self {
        self.max_recoveries = max_recoveries;
        self
    }

    /// Builds the [`Client`], with wreq's own retries, redirects and `Referer` header turned off so
    /// the client's policies decide. Fails when wreq rejects the configuration.
    pub fn build(self) -> Result<Client> {
        let inner = self
            .inner
            .retry(retry::Policy::never())
            .redirect(redirect::Policy::none())
            .referer(false)
            .build()?;

        Ok(Client::from_parts(
            inner,
            self.jar,
            self.middleware,
            self.retry_policy,
            self.redirect_policy,
            self.max_recoveries,
        ))
    }
}

impl Default for ClientBuilder {
    fn default() -> Self {
        Self::new()
    }
}
