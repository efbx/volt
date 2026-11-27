# volt-webview

[![CI](https://github.com/efbx/volt/actions/workflows/ci.yml/badge.svg)](https://github.com/efbx/volt/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](../LICENSE)

The webview the machine already has, WKWebView on macOS, WebView2 on Windows and WebKitGTK on Linux and the BSDs, as a browser for the commands of [volt-browser](../volt-browser): open a page inside the calling program, find elements, click, type and wait, let a page pass a browser challenge, then read its cookies, `HttpOnly` ones such as `cf_clearance` included.

There is no helper process and nothing to call in `main`. Windows and webviews may only be touched from one thread of a program, so the crate hands its window work to that thread and the library works from any thread or tokio task.

## Features

- **`Webview::launch`**: reaches the thread that may touch webviews and returns a `volt_browser::Session`. Its pages take every selector, action, script and storage command of volt-browser.
- **Pages are windows**: each page is a window of its own, hidden unless `Settings::visible` is set; `show` and `hide` act on it, and a person who closes a shown window only hides it.
- **Incognito stores**: on macOS, Linux and the BSDs no cookie outlives its session; on Windows every session of the program shares one store for as long as WebView2's browser process runs, and WebView2's data folder stays in `%TEMP%`. Closing the session closes every page and waits a few seconds at most for their windows to go; dropping it closes them without waiting.
- **Cookies both ways**: the store is read with `HttpOnly` cookies included, and cookies can be set and removed.
- **One error type**: every failure is a `volt_browser::Error`. What only a webview can fail with, a webview thread that cannot be reached, a webview that fails, a failed or replaced load, a page whose process ended, is its `Engine` kind, with the reason as its message.

## Installation

```toml
[dependencies]
volt-browser = { git = "https://github.com/efbx/volt", tag = "v0.6.1" }
volt-webview = { git = "https://github.com/efbx/volt", tag = "v0.6.1" }
tokio = { version = "1", features = ["rt-multi-thread", "macros", "time"] }
```

Requires Rust 1.95 or newer. `Webview::launch` must be awaited inside a Tokio runtime with timers; the session's pages then work from any executor while that runtime runs.

## Platform requirements

- **macOS**: the main thread must run an event loop, as every program with a window does, or await the call itself, as a terminal program does in the body of `#[tokio::main]`. Anywhere else `Webview::launch` fails within a second, with the `Engine` kind.
- **Windows**: Windows 10 or later with the WebView2 Runtime, which Windows 11 includes. Each session runs a window thread of its own. Programs built for a `*-windows-gnu` target need `WebView2Loader.dll` next to the executable.
- **Linux and the BSDs**: WebKitGTK 4.1, version 2.40 or newer, GTK 3 and a display, with `libwebkit2gtk-4.1-dev` and `libgtk-3-dev` to build. A program that runs GTK itself must start it before the first session; otherwise the crate starts a GTK thread of its own.

On every other target `Webview::launch` fails with the `Engine` kind, since there is no system webview to run.

## Example

```rust
use std::time::Duration;

use volt_browser::{By, Error, Settings};
use volt_webview::Webview;

async fn sign_in(user: &str, password: &str) -> Result<Vec<String>, Error> {
    let settings = Settings {
        timeout: Duration::from_secs(30),
        ..Settings::default()
    };
    let session = Webview.launch(settings).await?;
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

## Feature flags

- `webkitgtk` (default): the part for Linux and the BSDs, with its GTK dependencies. Cargo allows a program one version of the GTK bindings and checks that on every system, so a program whose other dependencies bring an older one sets `default-features = false`. macOS and Windows keep the webview; on Linux and the BSDs `Webview::launch` then fails with the `Engine` kind.

## Tests

The tests that open real webview windows are ignored by default: `cargo test -p volt-webview -- --ignored` runs them, with a display on Linux and the BSDs, and `--skip shown` keeps every window hidden.

## License

[MIT](../LICENSE)
