# volt-coerce

[![CI](https://github.com/efbx/volt/actions/workflows/ci.yml/badge.svg)](https://github.com/efbx/volt/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](../LICENSE)

Lenient JSON decoding on top of serde. A field that arrives as junk, the wrong type or `null` takes its default instead of failing the whole document.

Web APIs send `"tags": 5`, `"count": "12"`, `"enabled": "true"` and `"entries": {}` where a list was promised. With plain serde any one of those rejects the response. With `#[coerce]` the struct still decodes, the bad field becomes its default, and the fields that are fine keep their values. Every rule is a trait implementation, so custom types can take part.

## Features

- **`#[coerce]`** on a struct: derives `Deserialize` with every field read through the `Coerce` trait. Works with `#[serde(default)]` and the usual `rename`, `flatten`, `deserialize_with` and `default = "…"` attributes.
- **Scalar rules**: numbers read from numeric strings and booleans, booleans from `"true"`/`"false"`/`0`/`1`, strings from numbers and booleans, floats from integers. The numeric types are `i32`, `i64`, `u32`, `u64`, `f32` and `f64`; an integer out of range saturates at the type's bounds.
- **Lists and maps**: a lone value becomes a one-item list, junk items are dropped instead of failing the list, `null` is empty. `HashMap`, `BTreeMap` and `serde_json::Map` keep the entries that read and drop the rest.
- **`Option<T>`**: missing, `null` and junk become `None`, so a field that must tell "absent" from "default" stays honest.
- **`Json<T>`**: a JSON document stored inside a string, decoded on the fly.
- **`#[strict]`**: the field decodes with plain serde, with no leniency at all. **`#[reject]`**: the field still reads leniently, but junk fails the document instead of falling to the default.
- **`#[coerce(custom)]`** on a type with a hand-written `impl Coerce`: `read_str`, `read_i64`, `read_bool` and friends decide what each JSON shape means, for enums with several spellings.
- **`#[coerce(after = Self::hook)]`**: a method that runs after decoding to fix up or renumber the value; it returns anything implementing `AfterOutcome`.

## Installation

```toml
[dependencies]
volt-coerce = { git = "https://github.com/efbx/volt", tag = "v0.5.0" }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

Requires Rust 1.95 or newer. The attribute macro lives in [volt-coerce-macros](../volt-coerce-macros), which this crate re-exports; depend on this crate only.

## Example

```rust
use volt_coerce::coerce;

#[coerce]
#[derive(Debug, Default)]
#[serde(default)]
struct Card {
    name: String,
    tags: Vec<String>,
    enabled: bool,
    #[serde(rename = "createdAt")]
    created_at: Option<i64>,
}

let card: Card = serde_json::from_str(
    r#"{"name": 42, "tags": "solo", "enabled": "true", "createdAt": "junk"}"#,
).unwrap();

assert_eq!(card.name, "42");
assert_eq!(card.tags, vec!["solo"]);
assert!(card.enabled);
assert_eq!(card.created_at, None);
```

## License

[MIT](../LICENSE)
