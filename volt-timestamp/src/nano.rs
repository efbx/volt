use super::{Micro, Milli, Seconds, Timestamp};
use chrono::Utc;
use serde::de::Error;
use serde::{Deserialize, Serialize};
use std::fmt;
use volt_coerce::Coerce;

/// A count of nanoseconds since the Unix epoch, serialised as the plain integer.
///
/// Converting from a finer unit truncates toward zero; converting from a coarser unit
/// saturates at the `i64` bounds instead of overflowing.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct Nano(pub i64);

impl Nano {
    /// Whether the timestamp is zero.
    #[must_use]
    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    /// Returns the current time, or zero when it falls outside the range `i64`
    /// nanoseconds can hold.
    #[must_use]
    pub fn now() -> Self {
        Self(Utc::now().timestamp_nanos_opt().unwrap_or_default())
    }
}

impl Timestamp for Nano {}

impl From<Micro> for Nano {
    fn from(value: Micro) -> Self {
        Self(value.0.saturating_mul(1_000))
    }
}

impl From<Milli> for Nano {
    fn from(value: Milli) -> Self {
        Self(value.0.saturating_mul(1_000_000))
    }
}

impl From<Seconds> for Nano {
    fn from(value: Seconds) -> Self {
        Self(value.0.saturating_mul(1_000_000_000))
    }
}

impl fmt::Display for Nano {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl From<i64> for Nano {
    fn from(value: i64) -> Self {
        Self(value)
    }
}

impl From<Nano> for i64 {
    fn from(value: Nano) -> Self {
        value.0
    }
}

impl Coerce<'_> for Nano {
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
