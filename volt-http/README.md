# volt-http

[![CI](https://github.com/efbx/volt/actions/workflows/ci.yml/badge.svg)](https://github.com/efbx/volt/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](../LICENSE)

An HTTP client for scraping and API work, built on [wreq](https://crates.io/crates/wreq) with browser emulation, and a middleware layer for the things every site integration ends up needing: bearer logins, rate gates, retries, redirects and browser cookie solving.

## Features

- **`Client` and `ClientBuilder`**: wreq underneath, with `emulation` to look like a real browser, a cookie store, timeouts, proxies and TLS options. `derive()` makes a child client that shares the connection pool and cookies while changing middleware.
- **`RequestBuilder`**: the async request API plus `send_sync`, `text_sync`, `json_sync` and `bytes_sync` for code that lives outside a runtime, `json_body`, `form`, `query`, `multipart`, and `extension` to attach per-request markers.
- **Middleware**: the `Middleware` trait sees every request and response. Built in:
  - `Bearer`: a `TokenSource` logs in once, the `Token` is attached to every request and refreshed before it expires; `Anonymous` on a request skips it. `session_valid` and `login_sync` expose the state.
  - `RateGate`: a minimum gap between requests with jitter, pacing that widens after a `429` and calms down after quiet, and `trip` to back off on demand.
  - `Browser` (feature `chrome`): detects a challenge page, solves it in Chrome through [volt-chrome](../volt-chrome), copies the cookies and user agent into the client and replays the request. Chrome opens a visible window unless `headed(false)` is set.
- **Retries**: a `RetryPolicy` with max attempts, initial and maximum waits, and a `Classifier` deciding what is worth retrying: transient errors, transport failures, rate limits, server errors. `retry_after` reads the header in both forms.
- **Redirects**: a `RedirectPolicy` with a limit and a `Classifier` to follow, stop or stay on the same host.
- **Errors**: one `Error` with the transport and middleware cases, and the HTTP status where there is one.
- **Re-exports**: `Method`, `StatusCode`, `Uri`, `Version`, `Body`, `Request`, `Response`, `Proxy`, `multipart` and `Emulation`, so users do not depend on wreq directly.

## Installation

```toml
[dependencies]
volt-http = { git = "https://github.com/efbx/volt", tag = "v0.5.0" }
tokio = { version = "1", features = ["rt-multi-thread"] }
```

Add `features = ["chrome"]` for the browser middleware. Requires Rust 1.95 or newer.

## Example

```rust
use std::sync::Arc;

use volt_http::middleware::{Anonymous, Bearer, MiddlewareResult, Token, TokenSource};
use volt_http::{Client, Emulation};

struct Login;

#[async_trait::async_trait]
impl TokenSource for Login {
    async fn fetch(&self) -> MiddlewareResult<Token> {
        Ok(Token::jwt("eyJ…".to_owned()))
    }
}

let base = Client::builder().emulation(Emulation::Chrome131).cookie_store(true).build()?;
let api = base.derive().middleware(Bearer::new(Login)).build();

let private = api.get("https://example.com/api/me").text_sync()?;
let public = api
    .get("https://example.com/api/cards/1")
    .extension(Anonymous)
    .json_sync::<serde_json::Value>()?;
```

## Feature flags

- `chrome`: the `Browser` middleware and its `BrowserBuilder`, which pull in `volt-chrome` and a Chrome binary at runtime.

## License

[MIT](../LICENSE)
