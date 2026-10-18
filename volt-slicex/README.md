# volt-slicex

[![CI](https://github.com/efbx/volt/actions/workflows/ci.yml/badge.svg)](https://github.com/efbx/volt/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](../LICENSE)

One extension trait for `Vec<T>`: remove repeats while keeping the first occurrence of each item in its original position.

`Vec::dedup` only removes consecutive repeats and sorting first would lose the order. `DedupStable` keeps order and removes every later repeat, in place, without allocating a second buffer. Items only need `PartialEq`.

## Features

- **`unique_stable`**: removes every repeated item from a `Vec<T>` in place, keeping the first occurrence in its original position.
- **`dedup_stable`**: appends a second vector and then deduplicates the whole, so an existing list can absorb new items without picking up repeats.
- **No allocation**: a scan over the vector with swaps and one truncate.

## Installation

```toml
[dependencies]
volt-slicex = { git = "https://github.com/efbx/volt", tag = "v0.5.0" }
```

Requires Rust 1.95 or newer.

## Example

```rust
use volt_slicex::DedupStable;

let mut greetings = vec!["Hi", "Hello", "Hi", "Hey", "Hello"];
greetings.unique_stable();
assert_eq!(greetings, ["Hi", "Hello", "Hey"]);

let merged = vec!["Hi", "Hello"].dedup_stable(vec!["Hey", "Hi", "Yo"]);
assert_eq!(merged, ["Hi", "Hello", "Hey", "Yo"]);
```

## License

[MIT](../LICENSE)
