# volt-chrome

[![CI](https://github.com/efbx/volt/actions/workflows/ci.yml/badge.svg)](https://github.com/efbx/volt/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](../LICENSE)

A Chrome session over [chromiumoxide](https://crates.io/crates/chromiumoxide): launch a browser that does not announce itself as automated, open pages, run one job against a page, and shut the browser down cleanly.

Written for one task, getting past browser challenges that plain HTTP clients cannot solve, and kept small enough to serve any other scripted-browser need.

## Features

- **`Options`**: the Chrome binary path, headed or headless, the time limit for a `Session::run` job with `DEFAULT_TIMEOUT` of 60 seconds as the baseline, and extra command-line arguments.
- **Quiet launch flags**: the session replaces chromiumoxide's defaults with a set that omits `--enable-automation` and disables the automation-controlled blink feature, so challenge pages see an ordinary browser.
- **`Session`**: `launch` starts the browser and drives its event loop on the tokio runtime; `new_page` opens a blank tab; `browser()` exposes the chromiumoxide handle; `close` stops the event loop and the browser.
- **`Session::run`**: launch, hand one page to a closure, return its result and close, for one-shot jobs.
- **`Error`**: one error type over launch, navigation and timeout failures.
- **Re-exports**: chromiumoxide's `Browser`, `Page` and `Cookie`, so users do not depend on chromiumoxide directly.

## Installation

```toml
[dependencies]
volt-chrome = { git = "https://github.com/efbx/volt", tag = "v0.5.0" }
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
```

Requires Rust 1.95 or newer and a Chrome or Chromium binary on the machine.

## Example

```rust
use volt_chrome::{Error, Options, Session};

async fn cookie_count(url: &str) -> Result<usize, Error> {
    let session = Session::launch(Options::default()).await?;
    let page = session.new_page().await?;
    page.goto(url).await?;
    let cookies = page.get_cookies().await?;
    session.close().await;
    Ok(cookies.len())
}
```

## License

[MIT](../LICENSE)
