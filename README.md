# volt

[![CI](https://github.com/efbx/volt/actions/workflows/ci.yml/badge.svg)](https://github.com/efbx/volt/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Small, standalone Rust utility crates: an HTTP client with middleware, lenient JSON decoding, string, slice, file and time helpers, text templates, credential storage, and browser commands written once for two engines, Chrome and the system webview. One repository, one toolchain, one version, and every crate usable on its own as a git dependency.

## Crates

Every crate is its own package with its own README, and depends on a sibling only where the table says so.

| Crate | What it does | Uses |
| --- | --- | --- |
| [volt-browser](volt-browser) | One set of browser commands for every engine: selectors, clicks and typing, waiting, navigation, cookies, storage and caches | |
| [volt-browser-checks](volt-browser-checks) | The checks every engine of volt-browser has to pass, run against a real browser from the engine's own tests | volt-browser |
| [volt-chrome](volt-chrome) | Chrome over chromiumoxide that launches without automation flags, as an engine of volt-browser | volt-browser |
| [volt-coerce](volt-coerce) | Lenient JSON decoding on top of serde: junk falls to the field default instead of failing the document | volt-coerce-macros |
| [volt-coerce-macros](volt-coerce-macros) | The `#[coerce]` attribute, re-exported by volt-coerce | |
| [volt-creds](volt-creds) | Credential identities read from the environment or the OS keyring behind one reader and writer interface | volt-stringx |
| [volt-filex](volt-filex) | Path sanitising, existence checks, safe copies and next-available file names | volt-stringx |
| [volt-http](volt-http) | An HTTP client on wreq with middleware for bearer tokens, rate gates, retries, redirects and browser cookie solving | volt-browser (feature `browser`) |
| [volt-slicex](volt-slicex) | Stable in-place deduplication for vectors | |
| [volt-stringx](volt-stringx) | Blank checks, typographic symbol normalisation, in-place string edits and non-blank joins for `str` and `String` | |
| [volt-templater](volt-templater) | Compiled text templates with typed tokens, matched in one Aho-Corasick pass | |
| [volt-timestamp](volt-timestamp) | Nanosecond, microsecond, millisecond and second timestamps with lenient parsing and conversions | volt-coerce |
| [volt-webview](volt-webview) | The webview the system already has, WKWebView, WebView2 or WebKitGTK, as an engine of volt-browser inside the calling program | volt-browser |

## Installation

Each crate is a workspace member, so Cargo finds it by package name in this repository:

```toml
[dependencies]
volt-http = { git = "https://github.com/efbx/volt", tag = "v0.6.1" }
```

Every crate requires Rust 1.95 or newer.

## Documentation

Each crate's README covers what it does, how to install it and one example that its test suite pins. The rustdoc covers every type and function:

```sh
cargo doc --workspace --all-features --no-deps --open
```

## Development

The nightly toolchain is pinned by `rust-toolchain.toml` and installs itself through rustup on first use.

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps
```

CI runs the same four on every push and pull request, tests on Linux, macOS and Windows, and runs the tests once more on Rust 1.95 to keep the minimum version honest. Pushing a `v*` tag reruns CI and publishes a GitHub release. The volt-creds keyring tests sit behind its `keyring-tests` feature because they need a desktop keyring, so they run only on a developer machine. The tests that start a real browser are ignored by default: `cargo test -p volt-chrome -- --ignored` runs them on Chrome and `cargo test -p volt-webview -- --ignored` on the system webview, `--skip shown` leaves out the ones that put a window on the screen, and CI runs all of them on the three systems. Building on Linux needs the WebKitGTK and GTK development packages, `libwebkit2gtk-4.1-dev` and `libgtk-3-dev`.

## License

[MIT](LICENSE), copyright efbx.
