# volt-stringx

[![CI](https://github.com/efbx/volt/actions/workflows/ci.yml/badge.svg)](https://github.com/efbx/volt/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](../LICENSE)

Three small extension traits for `str`, `String` and string slices: blank checks, typographic symbol normalisation, in-place edits that avoid reallocation, and joins that skip blank items.

The traits are implemented on the standard types, so importing them is the whole setup.

## Features

- **Blank checks**: `is_blank` and `is_not_blank` treat whitespace-only text as blank.
- **Symbol normalisation**: `normalize_symbols` trims the text and replaces typographic quotes and commas (`“ ” « » 「 」 ‘ ’ ‚` and their relatives) with their ASCII forms. One Aho-Corasick pass, built once and shared.
- **Simple uppercase**: `simple_uppercase` upper-cases only characters whose uppercase form is a single character, so `ß` stays `ß` instead of becoming `SS`, and ASCII text takes the fast path.
- **Alphanumeric key**: `alphanumeric_lower` keeps ASCII letters and digits, lower-cased, and drops everything else, for loose comparisons such as matching `LOREBOOK_ENTRY_AFTER_CHAR` against `afterchar`.
- **Query unescape**: `query_unescape` decodes `+` and `%XX` form encoding, returning `None` on a malformed escape.
- **In-place edits** on `String`: `trim_in_place`, `normalize_symbols_in_place` and `simple_uppercase_in_place` only reallocate when the text changes, and `set_if_not_blank` replaces the value only when the new one has content.
- **Joins** on slices of strings: `join_non_blank` and `join_where` join with a separator while skipping the items that do not pass.

## Installation

```toml
[dependencies]
volt-stringx = { git = "https://github.com/efbx/volt", tag = "v0.5.0" }
```

Requires Rust 1.95 or newer.

## Example

```rust
use volt_stringx::{StrExt, StringExt, StrsExt};

let title = "  “Quoted” title  ";
assert!(title.is_not_blank());
assert_eq!(title.normalize_symbols(), "\"Quoted\" title");
assert_eq!("Slice-of-Life 2".alphanumeric_lower(), "sliceoflife2");
assert_eq!("a+b%20c".query_unescape().as_deref(), Some("a b c"));

let mut name = String::from("  Ada  ");
name.trim_in_place();
name.set_if_not_blank(String::from("   "));
assert_eq!(name, "Ada");

let parts = ["Tagline", "", "   ", "Notes"];
assert_eq!(parts.join_non_blank("\n\n"), "Tagline\n\nNotes");
assert_eq!(parts.join_where(", ", |part| part.len() > 5), "Tagline");
```

## License

[MIT](../LICENSE)
