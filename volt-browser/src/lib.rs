//! One set of browser commands for every engine: selectors, clicks and typing, waiting,
//! navigation, cookies, storage and caches, written once.
//!
//! A [`Session`] is a running browser. Its [`Page`]s load URLs and [`find`](Page::find)
//! [`Element`]s [`By`] a selector; an element is a description that every command looks up
//! afresh, and commands wait until the page is ready for them, so callers write no loops and no
//! JavaScript. Cookies, `localStorage`, `sessionStorage` and Cache Storage come out as plain
//! Rust values, and a [`Snapshot`] carries a signed-in state from one browser to another.
//! Failures come back as [`Error`].
//!
//! A crate gives its browser all of this by implementing two traits: [`Browser`] for the
//! running program and [`Engine`] for one of its pages. The commands are built from the few
//! things an engine does, so they behave the same on each.
//!
//! The vocabulary follows the W3C `WebDriver` specification, the one behind Selenium; lazy
//! elements, waiting and the checks before each action follow Playwright.

#![cfg_attr(doctest, doc = include_str!("../README.md"))]
#![warn(missing_docs)]

mod browser;
mod by;
mod cookie;
mod element;
mod engine;
mod error;
#[cfg(test)]
mod fake;
mod geometry;
mod input;
mod page;
mod page_script;
mod session;
mod settings;
mod storage;

pub use browser::Browser;
pub use by::By;
pub use cookie::{Cookie, SameSite, SetCookie};
pub use element::{Element, State};
pub use engine::Engine;
pub use error::{Blocker, Cause, Error};
pub use geometry::{Point, Rect};
pub use input::{Button, Choice, Click, Keystroke};
pub use keyboard_types::{Key, Modifiers, NamedKey};
pub use page::{Load, Match, Page};
pub use session::Session;
pub use settings::{DEFAULT_TIMEOUT, Settings};
pub use storage::{Area, Caches, Snapshot, Storage};
