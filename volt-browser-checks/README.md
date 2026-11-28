# volt-browser-checks

[![CI](https://github.com/efbx/volt/actions/workflows/ci.yml/badge.svg)](https://github.com/efbx/volt/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](../LICENSE)

The checks every engine of [volt-browser](../volt-browser) has to pass. An engine's crate runs them from its own tests against its real browser, so that every engine is held to the same behaviour: selectors, clicks and typing, waiting, frames, scripts, navigation, cookies, storage and caches.

The pages the checks open come from a small site the crate serves on this machine.

## Installation

```toml
[dev-dependencies]
volt-browser-checks = { git = "https://github.com/efbx/volt", tag = "v0.6.2" }
```

Requires Rust 1.95 or newer. `Checks::run` must be awaited inside a Tokio runtime.

## Example

```rust
use volt_browser::{Error, Session};
use volt_browser_checks::{Checks, Failure};

async fn failures_of(session: Session) -> Result<Vec<Failure>, Error> {
    let failures = Checks::run(&session).await?;
    session.close().await;
    Ok(failures)
}
```

An engine passes when the list is empty. Each `Failure` names its check and says what went wrong.

## License

[MIT](../LICENSE)
