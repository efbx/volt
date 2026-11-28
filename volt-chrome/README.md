# volt-chrome

[![CI](https://github.com/efbx/volt/actions/workflows/ci.yml/badge.svg)](https://github.com/efbx/volt/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](../LICENSE)

Chrome, over [chromiumoxide](https://crates.io/crates/chromiumoxide), as a browser for the commands of [volt-browser](../volt-browser): launch a browser that does not announce itself as automated, find elements, click, type and wait, let a page pass a browser challenge, then read its cookies, `HttpOnly` ones included.

The commands are the same ones [volt-webview](../volt-webview) takes, so code written for one engine runs on the other.

## Features

- **`Chrome`**: the binary to run, or the one chromiumoxide finds, and extra command-line switches. `Chrome::default().launch(settings)` returns a `volt_browser::Session`, whose pages take every selector, action, script and storage command of volt-browser.
- **Quiet launch flags**: chromiumoxide's defaults are replaced with a set that omits `--enable-automation` and disables the automation-controlled blink feature, so challenge pages see an ordinary browser.
- **A profile that goes away**: each session runs in a temporary profile, removed when the browser has quit.
- **Window or none**: `Settings::visible` decides at launch. `Page::show` fails with `Error::Unsupported` for a browser launched without a window, and `Page::hide` for one launched with a window.
- **Timeouts**: `Settings::timeout` is the time the launch gets, and chromiumoxide gives up on a protocol call after one to two times as long; a load keeps chromiumoxide's own 30 seconds.
- **One error type**: every failure is a `volt_browser::Error`. What only Chrome can fail with, a launch that finds no binary or runs out of time, a protocol call chromiumoxide gives up on, is its `Engine` kind, with the reason as its message.

## Installation

```toml
[dependencies]
volt-browser = { git = "https://github.com/efbx/volt", tag = "v0.6.2" }
volt-chrome = { git = "https://github.com/efbx/volt", tag = "v0.6.2" }
tokio = { version = "1", features = ["rt-multi-thread", "macros", "time"] }
```

Requires Rust 1.95 or newer and a Chrome or Chromium binary on the machine. `Chrome::launch` must be awaited inside a Tokio runtime with timers.

## Example

```rust
use std::time::Duration;

use volt_browser::{By, Error, Settings};
use volt_chrome::Chrome;

async fn sign_in(user: &str, password: &str) -> Result<Vec<String>, Error> {
    let settings = Settings {
        timeout: Duration::from_secs(30),
        ..Settings::default()
    };
    let session = Chrome::default().launch(settings).await?;
    session
        .run(|page| async move {
            page.goto("https://example.com/login").await?;
            page.find(By::label("Email")).fill(user).await?;
            page.find(By::label("Password")).fill(password).await?;
            page.find(By::text("Sign in")).click().await?;
            page.find(By::link("Sign out")).wait().await?;
            let cookies = page.cookies().await?;
            Ok(cookies
                .iter()
                .map(|cookie| cookie.header().to_string())
                .collect())
        })
        .await
}
```

## Tests

The tests that drive a real Chrome are ignored by default: `cargo test -p volt-chrome -- --ignored` runs them on the Chrome or Chromium that chromiumoxide finds, and `--skip shown` leaves out the one that opens a window.

## License

[MIT](../LICENSE)
