//! The webview the system already has, `WKWebView` on macOS, `WebView2` on Windows and
//! `WebKitGTK` on Linux and the BSDs, as a browser for the commands of [`volt_browser`], opened
//! inside the calling program and driven from any thread or tokio task.
//!
//! [`Webview::launch`] reaches the one thread of the program that may touch windows and returns
//! a [`volt_browser::Session`]. Its pages are windows in the system webview, hidden unless asked
//! otherwise, and take every selector, action and storage command of that crate, cookies with
//! `HttpOnly` ones included. Every failure is a [`volt_browser::Error`]; what only a webview can
//! fail with, as a display that is missing or a page whose process ended, is its
//! [`Engine`](volt_browser::Error::Engine) kind.
//!
//! The `webkitgtk` feature, on by default, holds the part for Linux and the BSDs with its GTK
//! dependencies. A program whose other dependencies bring an older version of those turns it
//! off: macOS and Windows keep the webview, and a launch on Linux and the BSDs fails with the
//! `Engine` kind.

#![cfg_attr(doctest, doc = include_str!("../README.md"))]
#![cfg_attr(
    not(any(
        test,
        target_os = "macos",
        target_os = "windows",
        all(
            feature = "webkitgtk",
            any(
                target_os = "linux",
                target_os = "dragonfly",
                target_os = "freebsd",
                target_os = "netbsd",
                target_os = "openbsd"
            )
        )
    )),
    expect(dead_code)
)]
#![warn(missing_docs)]

#[cfg(any(
    target_os = "macos",
    target_os = "windows",
    all(
        feature = "webkitgtk",
        any(
            target_os = "linux",
            target_os = "dragonfly",
            target_os = "freebsd",
            target_os = "netbsd",
            target_os = "openbsd"
        )
    )
))]
mod cookie;
mod error;
#[cfg(any(test, target_os = "windows"))]
mod error_page;
mod link;
mod pages;
mod platform;
mod script;
mod tab;
mod webview;

pub(crate) use error::Error;
pub use webview::Webview;
