# volt-filex

[![CI](https://github.com/efbx/volt/actions/workflows/ci.yml/badge.svg)](https://github.com/efbx/volt/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](../LICENSE)

Small file-system helpers for programs that write user-named files: sanitise a name, find the next free path, copy a file, check what exists.

## Features

- **`FileNameExt::to_file_name`** on `str`: turns arbitrary text into a safe single path component: slashes become underscores, characters file systems reject are dropped, and runs of whitespace become single hyphens.
- **`PathExt::next_available`** on `Path`: returns the path itself when free, otherwise the same name numbered one above the highest already taken in that directory, so `img.png` next to `img5.png` becomes `img6.png`. Pass the extension with its dot to keep the number in front of it.
- **`PathExt::copy_to`**: copies a file and returns the bytes written.
- **`PathExt::exists_as_file`**: whether something other than a directory exists at the path.
- **Types**: `Entry` and `FileType` with `as_str`, plus `FILE_PERMISSION` and `DIRECTORY_PERMISSION` for the private-directory and written-file Unix modes.

## Installation

```toml
[dependencies]
volt-filex = { git = "https://github.com/efbx/volt", tag = "v0.5.0" }
```

Requires Rust 1.95 or newer.

## Example

```rust
use std::path::Path;

use volt_filex::{FileNameExt, PathExt};

let name = "Odessa Vale: Route/Book?".to_file_name();
let target = Path::new("exports").join(format!("{name}.png"));
let free = target.next_available(Some(".png"));
assert!(!free.exists_as_file());
```

## License

[MIT](../LICENSE)
