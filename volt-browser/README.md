# volt-browser

[![CI](https://github.com/efbx/volt/actions/workflows/ci.yml/badge.svg)](https://github.com/efbx/volt/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](../LICENSE)

One set of browser commands for every engine: find elements, click, type, wait, navigate, and read cookies, storage and caches, all as plain Rust. The commands are written once here; [volt-chrome](../volt-chrome) and [volt-webview](../volt-webview) are the two browsers underneath, and code written against a `Page` runs unchanged on either.

The vocabulary follows the W3C WebDriver specification, the one behind Selenium. Elements that are looked up afresh by every command, waiting, and the checks before each action follow Playwright.

## Features

- **`Session`**: a running browser of any engine. `new_page` opens a page, `run` hands one page to a closure and closes the browser, `close` shuts it down.
- **`Page`**: `goto`, `reload`, `back`, `forward`, `stop`, `url`, `title`, `html`, `set_html`, `wait_for_load`, `wait_for_url`, `wait_until`, `show`, `hide`, `close`.
- **`By`**: fifteen ways to find elements: `css`, `xpath`, `id`, `name`, `tag`, `class`, `text`, `text_containing`, `link`, `link_containing`, `label`, `placeholder`, `alt`, `title`, `test_id`.
- **`Element`**: a description of an element, not a handle, so it never goes stale. Narrow it with `find`, `nth`, `first`, `last`, `with_text`, `without_text`, `having`, `not_having`, `visible`, `or`, `parent`, `closest`, `frame`, `shadow` and `timeout`, none of which costs a trip to the page.
- **Reading**: `exists`, `count`, `all`, `text`, `texts`, `text_content`, `html`, `outer_html`, `attribute`, `property`, `value`, `style`, `rect`, `tag`, and `is_visible`, `is_enabled`, `is_editable`, `is_checked`, `is_selected`, `is_focused`.
- **Acting**: `click`, `click_with`, `double_click`, `right_click`, `hover`, `focus`, `blur`, `fill`, `clear`, `type_text`, `press`, `check`, `uncheck`, `set_checked`, `select`, `select_all`, `submit`, `scroll_into_view`, `scroll_to`, `scroll_by`, `select_text`, `dispatch`. An action waits until its element is visible, holds still, is enabled and is not covered, as the action needs.
- **Waiting**: `wait`, `wait_for` a `State`, `wait_for_text`. Waiting happens between short calls, so a navigation in the middle does not break it.
- **Scripts**: `evaluate` an expression or `call` a function with an argument, on the page or on an element, with typed results. A promise is waited for, a thrown error is an `Error::Script`, and arguments travel as JSON, never as script text.
- **Cookies**: `cookies`, `cookies_for` a URL, `cookie`, `set_cookie`, `delete_cookie`, `clear_cookies`, `HttpOnly` ones included. A `Cookie` writes itself as a `Set-Cookie` header for an HTTP client's jar.
- **Storage**: `storage(Area::Local)` and `storage(Area::Session)` with `get`, `set`, `remove`, `clear`, `entries`, and `get_json` and `set_json` for the JSON sites keep there.
- **Caches**: `caches()` lists the site's Cache Storage and reads an answer as an `http::Response`.
- **`Snapshot`**: cookies and both stores taken in one call, serializable, and put back with `restore`: a signed-in state carried from one browser to another.
- **`Browser` and `Engine`**: the two traits a crate implements to give its browser all of the above. The checks every engine has to pass, run for real against pages of a small local site, are in [volt-browser-checks](../volt-browser-checks).

## What it does not do

Clicks and typing are carried out by a script inside the page, the same on every engine. Ordinary pages cannot tell, but the events are not marked as coming from a person, CSS `:hover` styles do not switch on, and a "prove you are human" box stays a person's job in a shown window. Frames from another site cannot be entered. Intercepting network requests is not offered, since the system webviews cannot do it.

## Installation

```toml
[dependencies]
volt-browser = { git = "https://github.com/efbx/volt", tag = "v0.6.1" }
volt-webview = { git = "https://github.com/efbx/volt", tag = "v0.6.1" }
tokio = { version = "1", features = ["rt-multi-thread", "macros", "time"] }
```

Requires Rust 1.95 or newer and a Tokio runtime with timers.

## Example

```rust
use volt_browser::{Area, By, Error, NamedKey, Session, Snapshot};

async fn sign_in(session: &Session, user: &str, password: &str) -> Result<Snapshot, Error> {
    let page = session.new_page().await?;
    page.goto("https://example.com/login").await?;
    page.find(By::label("Email")).fill(user).await?;
    let secret = page.find(By::label("Password"));
    secret.fill(password).await?;
    secret.press(NamedKey::Enter).await?;
    page.find(By::link("Sign out")).wait().await?;
    let theme = page.storage(Area::Local).get("theme").await?;
    println!("signed in, theme {theme:?}");
    page.snapshot().await
}
```

## License

[MIT](../LICENSE)
