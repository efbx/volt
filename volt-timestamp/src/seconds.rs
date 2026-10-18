use super::{Micro, Milli, Nano, Timestamp};
use chrono::Utc;
use serde::de::Error;
use serde::{Deserialize, Serialize};
use std::fmt;
use volt_coerce::Coerce;

/// A count of seconds since the Unix epoch, serialised as the plain integer.
///
/// Converting from a finer unit truncates toward zero; converting from a coarser unit
/// saturates at the `i64` bounds instead of overflowing.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct Seconds(pub i64);

impl Seconds {
    /// Whether the timestamp is zero, the value a missing or junk field reads as.
    #[must_use]
    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    /// Returns the current time.
    #[must_use]
    pub fn now() -> Self {
        Self(Utc::now().timestamp())
    }
}

impl Timestamp for Seconds {}

impl From<Nano> for Seconds {
    fn from(value: Nano) -> Self {
        Self(value.0 / 1_000_000_000)
    }
}

impl From<Micro> for Seconds {
    fn from(value: Micro) -> Self {
        Self(value.0 / 1_000_000)
    }
}

impl From<Milli> for Seconds {
    fn from(value: Milli) -> Self {
        Self(value.0 / 1_000)
    }
}

impl fmt::Display for Seconds {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl From<i64> for Seconds {
    fn from(value: i64) -> Self {
        Self(value)
    }
}

impl From<Seconds> for i64 {
    fn from(value: Seconds) -> Self {
        value.0
    }
}

impl Coerce<'_> for Seconds {
    fn read_i64<E: Error>(value: i64) -> Result<Option<Self>, E> {
        Ok(Some(Self(value)))
    }

    fn read_u64<E: Error>(value: u64) -> Result<Option<Self>, E> {
        Ok(i64::read_u64(value)?.map(Self))
    }

    fn read_f64<E: Error>(value: f64) -> Result<Option<Self>, E> {
        Ok(i64::read_f64(value)?.map(Self))
    }

    fn read_str<E: Error>(value: &str) -> Result<Option<Self>, E> {
        Ok(i64::read_str(value)?.map(Self))
    }
}
