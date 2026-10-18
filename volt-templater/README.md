# volt-templater

[![CI](https://github.com/efbx/volt/actions/workflows/ci.yml/badge.svg)](https://github.com/efbx/volt/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](../LICENSE)

Text templates with typed tokens. Each token names a key and a function that extracts its value from the templated type; a template is compiled once and executed per value in a single Aho-Corasick pass.

Meant for user-editable patterns such as file names and export paths, where the set of tokens is fixed by the program and the arrangement is up to the user.

## Features

- **Typed tokens**: `Token::new(key, extractor)` pairs a key with `fn(&T) -> Cow<str>`, borrowing text fields instead of copying them, with `with_description` for help text.
- **Compile once**: `Templater::compile` scans the template for every token in one pass and keeps the pieces; `CompiledTemplate::execute` fills them for each input.
- **One-shot**: `Templater::execute` compiles and executes in one call when the template changes every time.
- **Unknown text stays**: anything that is not a token is copied through unchanged.

## Installation

```toml
[dependencies]
volt-templater = { git = "https://github.com/efbx/volt", tag = "v0.5.0" }
```

Requires Rust 1.95 or newer.

## Example

```rust
use volt_templater::{Templater, Token};

struct Card {
    name: String,
    creator: String,
}

let templater = Templater::new(vec![
    Token::new("{name}", |card: &Card| card.name.as_str().into()),
    Token::new("{creator}", |card: &Card| card.creator.as_str().into()),
]);

let compiled = templater.compile("{creator} - {name}.png");
let card = Card { name: "Odessa Vale".into(), creator: "ahuro1980".into() };
assert_eq!(compiled.execute(&card), "ahuro1980 - Odessa Vale.png");
```

## License

[MIT](../LICENSE)
