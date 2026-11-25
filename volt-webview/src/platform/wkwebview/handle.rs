#![allow(unsafe_code)]

use std::ptr::NonNull;

use wry::raw_window_handle::{
    AppKitWindowHandle, HandleError, HasWindowHandle, RawWindowHandle, WindowHandle,
};

use super::window::PageWindow;

impl HasWindowHandle for PageWindow {
    /// Returns an `AppKit` handle to the window's view. Borrowing it is sound: the handle points
    /// to the `NSView` that the window keeps in a `Retained` for as long as `&self` lives, and a
    /// window only exists on the main thread, the one thread where an `AppKit` handle may be used.
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let view = AppKitWindowHandle::new(NonNull::from(self.view()).cast());
        Ok(unsafe { WindowHandle::borrow_raw(RawWindowHandle::AppKit(view)) })
    }
}
