# volt-timestamp

[![CI](https://github.com/efbx/volt/actions/workflows/ci.yml/badge.svg)](https://github.com/efbx/volt/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](../LICENSE)

Four integer timestamp types, one per unit, that convert into each other, parse the common wire formats and read leniently from JSON.

APIs disagree on units and on types: one sends nanoseconds as a number, the next sends seconds as a string. Each newtype names its unit, so a value never changes meaning on the way through, and the conversions saturate instead of overflowing.

## Features

- **Units as types**: `Nano`, `Micro`, `Milli` and `Seconds`, each a `Copy` newtype over `i64` with `From` conversions between all of them.
- **`Timestamp` trait** on every unit: `to_time` to a UTC `DateTime`, `format_utc` and `format_local` with a chrono format string, `from_rfc3339` for ISO 8601 text and `parse` with any chrono format.
- **Serde**: serialises and deserialises as the plain integer.
- **Lenient JSON** inside a `#[coerce]` struct from [volt-coerce](../volt-coerce): there a numeric string, a float or a boolean read as well, and junk reads as zero.
- **`DISPLAY_FORMAT`**: the day-first `dd/mm/yyyy, HH:MM:SS` display format.

## Installation

```toml
[dependencies]
volt-timestamp = { git = "https://github.com/efbx/volt", tag = "v0.5.0" }
```

Requires Rust 1.95 or newer.

## Example

```rust
use volt_timestamp::{Nano, Seconds, Timestamp};

let created = Nano::from_rfc3339("2026-01-28T17:50:00.000Z").unwrap();
assert_eq!(created, Nano(1_769_622_600_000_000_000));

let seconds: Seconds = created.into();
assert_eq!(seconds, Seconds(1_769_622_600));
assert_eq!(seconds.format_utc("%Y-%m-%d"), "2026-01-28");

let from_api: Seconds = serde_json::from_str("1706328712").unwrap();
assert_eq!(Nano::from(from_api), Nano(1_706_328_712_000_000_000));
```

## License

[MIT](../LICENSE)
