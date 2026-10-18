use super::Nano;
use chrono::{DateTime, Local, NaiveDateTime, Utc};

/// The day-first `dd/mm/yyyy, HH:MM:SS` chrono display format.
pub const DISPLAY_FORMAT: &str = "%d/%m/%Y, %H:%M:%S";

/// Conversion to chrono time, formatting and parsing, shared by every unit through
/// [`Nano`].
pub trait Timestamp: Copy + From<Nano> + Into<Nano> {
    /// Returns the timestamp as a UTC `DateTime`.
    fn to_time(self) -> DateTime<Utc> {
        let nanos: Nano = self.into();
        DateTime::from_timestamp_nanos(nanos.0)
    }

    /// Formats the timestamp in UTC with the chrono `format` string.
    fn format_utc(self, format: &str) -> String {
        self.to_time().format(format).to_string()
    }

    /// Formats the timestamp in the local time zone with the chrono `format` string.
    fn format_local(self, format: &str) -> String {
        self.to_time()
            .with_timezone(&Local)
            .format(format)
            .to_string()
    }

    /// Parses RFC 3339 text with any offset. Returns `None` when `s` does not parse or
    /// falls outside the range `i64` nanoseconds can hold.
    fn from_rfc3339(s: &str) -> Option<Self> {
        let parsed = DateTime::parse_from_rfc3339(s).ok()?;
        parsed.timestamp_nanos_opt().map(Nano).map(Self::from)
    }

    /// Parses `s` with the chrono `format`, honouring an offset when the format has one
    /// and reading it as UTC otherwise. Returns `None` when `s` does not match or falls
    /// outside the range `i64` nanoseconds can hold.
    fn parse(format: &str, s: &str) -> Option<Self> {
        let parsed = DateTime::parse_from_str(s, format)
            .map(|dt| dt.to_utc())
            .or_else(|_| NaiveDateTime::parse_from_str(s, format).map(|dt| dt.and_utc()))
            .ok()?;
        parsed.timestamp_nanos_opt().map(Nano).map(Self::from)
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Micro, Milli, Seconds};
    use super::*;

    const NANO: Nano = Nano(1_753_935_627_123_456_789);

    const MICRO: Micro = Micro(1_753_935_627_123_456);

    const MILLI: Milli = Milli(1_753_935_627_123);

    const SEC: Seconds = Seconds(1_753_935_627);

    #[test]
    fn converts_between_units() {
        assert_eq!(Nano::from(SEC), Nano(1_753_935_627_000_000_000));
        assert_eq!(Nano::from(MILLI), Nano(1_753_935_627_123_000_000));
        assert_eq!(Nano::from(MICRO), Nano(1_753_935_627_123_456_000));
        assert_eq!(Micro::from(SEC), Micro(1_753_935_627_000_000));
        assert_eq!(Micro::from(MILLI), Micro(1_753_935_627_123_000));
        assert_eq!(Milli::from(SEC), Milli(1_753_935_627_000));
        assert_eq!(Micro::from(NANO), MICRO);
        assert_eq!(Milli::from(NANO), MILLI);
        assert_eq!(Milli::from(MICRO), MILLI);
        assert_eq!(Seconds::from(NANO), SEC);
        assert_eq!(Seconds::from(MICRO), SEC);
        assert_eq!(Seconds::from(MILLI), SEC);
        assert_eq!(Seconds::from(Milli::from(Micro::from(NANO))), SEC);
        assert_eq!(Seconds::from(Nano(-1_000_000_000)), Seconds(-1));
        assert_eq!(Nano::from(Seconds(i64::MAX)), Nano(i64::MAX));
        assert_eq!(
            Milli::from(Seconds(10_000_000_000)),
            Milli(10_000_000_000_000)
        );
    }

    #[test]
    fn converts_to_chrono() {
        assert_eq!(SEC.to_time().timestamp(), SEC.0);
        assert_eq!(MILLI.to_time().timestamp_millis(), MILLI.0);
        assert_eq!(MICRO.to_time().timestamp_micros(), MICRO.0);
        assert_eq!(NANO.to_time().timestamp_nanos_opt(), Some(NANO.0));
    }

    #[test]
    fn now_agrees_across_units() {
        let reference = Seconds::now();
        assert!(reference > Seconds(1_700_000_000));
        for now in [
            Seconds::from(Nano::now()),
            Seconds::from(Micro::now()),
            Seconds::from(Milli::now()),
            Seconds::now(),
        ] {
            assert!(
                now >= reference && now.0 - reference.0 <= 1,
                "{now} vs {reference}"
            );
        }
    }

    #[test]
    fn parses_rfc3339_with_any_offset() {
        for input in [
            "2025-07-31T04:20:27.123456789Z",
            "2025-07-31T04:20:27.123456789+00:00",
            "2025-07-31T07:20:27.123456789+03:00",
        ] {
            assert_eq!(Nano::from_rfc3339(input), Some(NANO), "{input}");
        }
        assert_eq!(Seconds::from_rfc3339("2025-07-31T04:20:27Z"), Some(SEC));
        assert_eq!(Nano::from_rfc3339("not a date"), None);
    }

    #[test]
    fn parses_custom_formats() {
        let expected = Some(Nano::from(SEC));
        assert_eq!(
            Nano::parse("%Y-%m-%dT%H:%M:%S", "2025-07-31T04:20:27"),
            expected
        );
        assert_eq!(
            Nano::parse(DISPLAY_FORMAT, "31/07/2025, 04:20:27"),
            expected
        );
        assert_eq!(
            Nano::parse("%Y-%m-%dT%H:%M:%S%z", "2025-07-31T07:20:27+03:00"),
            expected
        );
        assert_eq!(Nano::parse("%Y-%m-%d", "not a date"), None);
    }

    #[test]
    fn formats_at_each_resolution() {
        assert_eq!(NANO.format_utc(DISPLAY_FORMAT), "31/07/2025, 04:20:27");
        assert_eq!(
            NANO.format_utc("%Y-%m-%dT%H:%M:%S%.f"),
            "2025-07-31T04:20:27.123456789"
        );
        assert_eq!(
            MILLI.format_utc("%Y-%m-%dT%H:%M:%S%.f"),
            "2025-07-31T04:20:27.123"
        );
        assert_eq!(
            SEC.format_utc("%Y-%m-%dT%H:%M:%S%.f"),
            "2025-07-31T04:20:27"
        );
        assert_eq!(
            NANO.format_local(DISPLAY_FORMAT).len(),
            NANO.format_utc(DISPLAY_FORMAT).len()
        );
    }

    #[test]
    fn serializes_as_bare_i64() {
        assert_eq!(serde_json::to_string(&SEC).unwrap(), "1753935627");
        assert_eq!(serde_json::to_string(&NANO).unwrap(), "1753935627123456789");
        assert_eq!(
            serde_json::from_str::<Milli>("1753935627123").unwrap(),
            MILLI
        );
        assert_eq!(
            serde_json::from_str::<Micro>("1753935627123456").unwrap(),
            MICRO
        );
    }

    #[test]
    fn converts_to_and_from_i64() {
        assert_eq!(NANO.to_string(), "1753935627123456789");
        assert_eq!(i64::from(SEC), 1_753_935_627);
        assert_eq!(Nano::from(42), Nano(42));
        assert!(Nano::default().is_zero());
        assert!(!SEC.is_zero());
        assert!(Nano(1) < Nano(2));
    }
}
