use std::fmt::{self, Debug};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde_json::Value;

use crate::middleware::MiddlewareResult;

/// A bearer token from a [`TokenSource`], with the time it stops being valid.
pub struct Token {
    /// The token sent after `Bearer ` in the `Authorization` header.
    pub value: String,
    /// When the token expires; [`None`] keeps it until the server answers `401`.
    pub expires_at: Option<SystemTime>,
}

impl Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Token")
            .field("expires_at", &self.expires_at)
            .finish_non_exhaustive()
    }
}

impl Token {
    /// Returns a token whose expiry is the `exp` claim of the JWT `value`, in seconds since the
    /// Unix epoch. A value that is not a JWT, or has no numeric `exp`, gets no expiry.
    #[must_use]
    pub fn jwt(value: String) -> Self {
        let expires_at = Self::jwt_expiry(&value);
        Self { value, expires_at }
    }

    /// Reads the `exp` claim from the unpadded base64url payload of `value`, dropping any
    /// fraction of a second.
    fn jwt_expiry(value: &str) -> Option<SystemTime> {
        let payload = URL_SAFE_NO_PAD.decode(value.split('.').nth(1)?).ok()?;
        let claims: Value = serde_json::from_slice(&payload).ok()?;
        let exp = claims.get("exp")?;
        let seconds = exp
            .as_u64()
            .or_else(|| exp.as_f64().map(|seconds| seconds as u64))?;
        Some(UNIX_EPOCH + Duration::from_secs(seconds))
    }
}

/// Where [`Bearer`](crate::middleware::Bearer) gets its tokens, usually a login call.
#[async_trait::async_trait]
pub trait TokenSource: Send + Sync + 'static {
    /// Fetches a fresh token. An error fails the request that needed the token.
    async fn fetch(&self) -> MiddlewareResult<Token>;
}
