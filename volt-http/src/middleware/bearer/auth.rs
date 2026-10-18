use std::fmt::{self, Debug};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, PoisonError, RwLock};
use std::time::{Duration, SystemTime};

use tokio::sync::Mutex;
use wreq::header::{AUTHORIZATION, HeaderValue};

use super::{Anonymous, TokenSource};
use crate::blocking;
use crate::middleware::{Middleware, MiddlewareResult};
use crate::{Request, Response, StatusCode};

/// How long before its expiry a token is replaced, unless [`Bearer::refresh_buffer`] changes it.
const DEFAULT_REFRESH_BUFFER: Duration = Duration::from_secs(120);

/// Middleware that sets `Authorization: Bearer <token>` on every request. The token comes from
/// a [`TokenSource`] on the first request, again once it is within the refresh buffer of its
/// expiry, and again when the server answers `401`, after which the request is replayed.
/// Clones share one session, and a request carrying [`Anonymous`] is left untouched.
#[derive(Clone)]
pub struct Bearer {
    refresh_buffer: Duration,
    state: Arc<State>,
}

impl Debug for Bearer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Bearer")
            .field("refresh_buffer", &self.refresh_buffer)
            .finish_non_exhaustive()
    }
}

/// The token source, the cached session, the lock that lets one refresh run at a time and a
/// count of completed refreshes, shared by every clone of a [`Bearer`].
struct State {
    source: Box<dyn TokenSource>,
    session: RwLock<Option<Session>>,
    refresh_lock: Mutex<()>,
    generation: AtomicU64,
}

/// The cached `Authorization` header value and the expiry of the token it carries.
#[derive(Clone)]
struct Session {
    header: HeaderValue,
    expires_at: Option<SystemTime>,
}

impl Bearer {
    /// The `Authorization` scheme put before every token.
    const SCHEME: &'static str = "Bearer ";

    /// Returns a bearer that fetches its tokens from `source`, with no session until the first
    /// request or [`login`](Self::login).
    pub fn new(source: impl TokenSource) -> Self {
        Self {
            refresh_buffer: DEFAULT_REFRESH_BUFFER,
            state: Arc::new(State {
                source: Box::new(source),
                session: RwLock::new(None),
                refresh_lock: Mutex::new(()),
                generation: AtomicU64::new(0),
            }),
        }
    }

    /// Sets how long before its expiry a token is replaced. Defaults to two minutes.
    #[must_use]
    pub fn refresh_buffer(mut self, buffer: Duration) -> Self {
        self.refresh_buffer = buffer;
        self
    }

    /// Whether a token is cached and not within the refresh buffer of its expiry.
    pub fn session_valid(&self) -> bool {
        self.state
            .session
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .is_some_and(|session| self.usable(session))
    }

    /// Fetches and caches a new token now, even when the cached one is still valid.
    pub async fn login(&self) -> MiddlewareResult<()> {
        let _refreshing = self.state.refresh_lock.lock().await;
        self.refresh().await.map(drop)
    }

    /// Blocks on [`login`](Self::login), inside or outside a Tokio runtime.
    pub fn login_sync(&self) -> MiddlewareResult<()> {
        blocking::block_on(self.login())
    }

    /// Returns the cached header while its token is not within the refresh buffer of its expiry.
    fn cached(&self) -> Option<HeaderValue> {
        let session = self
            .state
            .session
            .read()
            .unwrap_or_else(PoisonError::into_inner);
        let session = session.as_ref()?;
        self.usable(session).then(|| session.header.clone())
    }

    /// Whether `session`'s token is not within the refresh buffer of its expiry.
    fn usable(&self, session: &Session) -> bool {
        match session.expires_at {
            Some(expires_at) => SystemTime::now() + self.refresh_buffer < expires_at,
            None => true,
        }
    }

    /// Returns the cached header, or fetches a token when there is no usable one; concurrent
    /// callers wait for a single fetch.
    async fn header(&self) -> MiddlewareResult<HeaderValue> {
        if let Some(header) = self.cached() {
            return Ok(header);
        }
        let _refreshing = self.state.refresh_lock.lock().await;
        if let Some(header) = self.cached() {
            return Ok(header);
        }
        self.refresh().await
    }

    /// Fetches a token, caches it as a sensitive header value and counts the refresh. Fails when
    /// the source fails or the token is not a valid header value.
    async fn refresh(&self) -> MiddlewareResult<HeaderValue> {
        let token = self.state.source.fetch().await?;
        let mut bearer = String::with_capacity(Self::SCHEME.len() + token.value.len());
        bearer.push_str(Self::SCHEME);
        bearer.push_str(&token.value);
        let mut header = HeaderValue::try_from(bearer)?;
        header.set_sensitive(true);
        *self
            .state
            .session
            .write()
            .unwrap_or_else(PoisonError::into_inner) = Some(Session {
            header: header.clone(),
            expires_at: token.expires_at,
        });
        self.state.generation.fetch_add(1, Ordering::Release);
        Ok(header)
    }
}

#[async_trait::async_trait]
impl Middleware for Bearer {
    async fn before(&self, request: &mut Request) -> MiddlewareResult<Option<Response>> {
        if request.extensions().get::<Anonymous>().is_some() {
            return Ok(None);
        }
        let header = self.header().await?;
        request.headers_mut().insert(AUTHORIZATION, header);
        Ok(None)
    }

    async fn recover(&self, request: &Request, response: &Response) -> MiddlewareResult<bool> {
        if response.status() != StatusCode::UNAUTHORIZED
            || request.extensions().get::<Anonymous>().is_some()
        {
            return Ok(false);
        }
        let observed = self.state.generation.load(Ordering::Acquire);
        let _refreshing = self.state.refresh_lock.lock().await;
        if self.state.generation.load(Ordering::Acquire) == observed {
            self.refresh().await?;
        }
        Ok(true)
    }
}
