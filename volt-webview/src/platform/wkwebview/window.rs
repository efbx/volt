use std::borrow::Cow;

use objc2::rc::Retained;
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSView, NSViewController, NSWindow, NSWindowStyleMask, NSWindowTabbingMode,
};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString, NSURL, ns_string};
use wry::{WebView, WebViewBuilder, WebViewBuilderExtDarwin};

use super::main_thread::NO_EVENT_LOOP;
use super::program::Program;
use crate::Error;

/// The size a page's window opens with, in points.
const SIZE: NSSize = NSSize::new(1280.0, 800.0);

/// A session's webview store on macOS, which holds nothing: every page opens in an incognito
/// store of its own.
pub(crate) struct Store;

impl Store {
    /// Returns the empty store.
    pub(crate) fn new() -> Self {
        Self
    }
}

/// A page's window: titled, closable and resizable, kept out of the Window menu, the Dock and
/// tab groups, never restored, hidden until shown, and closed when dropped. A person's click on
/// its close button only hides it, since the window is never released on close.
pub(crate) struct PageWindow {
    /// The `AppKit` window.
    window: Retained<NSWindow>,
    /// The view wry builds the webview into, kept for the window handle wry asks for.
    view: Retained<NSView>,
    /// The controller the window was made from, kept with its window.
    _controller: Retained<NSViewController>,
    /// The id wry keys the webview's state by, unique in the process and never a number, so that
    /// it meets no id of the program's own webviews.
    id: String,
}

impl PageWindow {
    /// Creates the hidden window around a new view. It comes from
    /// `windowWithContentViewController`, which leaves `releasedWhenClosed` off, so that closing
    /// it never frees it under its owner.
    pub(crate) fn new() -> Result<Self, Error> {
        let mtm =
            MainThreadMarker::new().ok_or(Error::Unavailable(Cow::Borrowed(NO_EVENT_LOOP)))?;
        let _ = NSApplication::sharedApplication(mtm);
        let view = NSView::initWithFrame(NSView::alloc(mtm), NSRect::new(NSPoint::ZERO, SIZE));
        let controller = NSViewController::new(mtm);
        controller.setView(&view);
        controller.setTitle(Some(ns_string!("volt-webview")));
        let window = NSWindow::windowWithContentViewController(&controller);
        window.setStyleMask(
            NSWindowStyleMask::Titled | NSWindowStyleMask::Closable | NSWindowStyleMask::Resizable,
        );
        window.setContentSize(SIZE);
        window.setExcludedFromWindowsMenu(true);
        window.setTabbingMode(NSWindowTabbingMode::Disallowed);
        window.setRestorable(false);
        window.center();
        let id = format!("volt-webview-{:p}", Retained::as_ptr(&window));
        Ok(Self {
            window,
            view,
            _controller: controller,
            id,
        })
    }

    /// Builds the webview `configure` describes into the window, under the window's own id.
    pub(crate) fn attach(
        &self,
        _store: &Store,
        configure: impl FnOnce(WebViewBuilder<'_>) -> WebViewBuilder<'_>,
    ) -> Result<WebView, Error> {
        Ok(configure(WebViewBuilder::new().with_id(&self.id)).build(self)?)
    }

    /// Has `crashed` called when the webview's web content process ends.
    pub(crate) fn watch(
        builder: WebViewBuilder<'_>,
        crashed: impl Fn() + 'static,
    ) -> WebViewBuilder<'_> {
        builder.with_on_web_content_process_terminate_handler(crashed)
    }

    /// Whether the webview can load `url`: wry panics on a URL that `NSURL` rejects.
    pub(crate) fn accepts(url: &str) -> bool {
        NSURL::URLWithString(&NSString::from_str(url)).is_some()
    }

    /// Puts the window in front of the program's other windows without making it key, so that
    /// the keyboard focus stays where it is. In a program without an event loop of its own the
    /// window also comes in front of the other programs' windows.
    pub(crate) fn reveal(&self) {
        let program = Program::new(self.window.mtm());
        program.admit();
        self.window.orderFront(None);
        program.overtake(&self.window);
    }

    /// Puts the window in front of the program's other windows and makes it key, without
    /// activating the program. In a program without an event loop of its own the window also
    /// comes in front of the other programs' windows.
    pub(crate) fn show(&self) {
        let program = Program::new(self.window.mtm());
        program.admit();
        self.window.makeKeyAndOrderFront(None);
        program.overtake(&self.window);
    }

    /// Takes the window off the screen.
    pub(crate) fn hide(&self) {
        self.window.orderOut(None);
    }

    /// The view wry builds the webview into.
    pub(super) fn view(&self) -> &NSView {
        &self.view
    }
}

impl Drop for PageWindow {
    fn drop(&mut self) {
        self.window.close();
    }
}
