#![allow(unsafe_code)]

use std::error;
use std::io;
use std::num::NonZeroIsize;
use std::os::windows::io::AsRawHandle;
use std::thread::JoinHandle;

use windows::Win32::Foundation::{
    ERROR_CLASS_ALREADY_EXISTS, HANDLE, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    CW_USEDEFAULT, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
    MSG, PM_NOREMOVE, PeekMessageW, PostThreadMessageW, RegisterClassW, SW_HIDE, SW_SHOWNOACTIVATE,
    SW_SHOWNORMAL, SetForegroundWindow, ShowWindow, TranslateMessage, WINDOW_EX_STYLE, WM_APP,
    WM_CLOSE, WM_QUIT, WM_USER, WNDCLASSW, WS_OVERLAPPEDWINDOW,
};
use windows::core::{PCWSTR, w};
use wry::raw_window_handle::{
    HandleError, HasWindowHandle, RawWindowHandle, Win32WindowHandle, WindowHandle,
};
use wry::{WebView, WebViewBuilder, WebViewExtWindows};

use super::store::Store;
use crate::Error;

/// The width a page's window opens with, in pixels.
const WIDTH: i32 = 1280;
/// The height a page's window opens with, in pixels.
const HEIGHT: i32 = 800;
/// The window class name, unique to this crate and its version, so that two versions in one
/// program each keep their own window procedure.
const CLASS_TEXT: &str = concat!(env!("CARGO_PKG_NAME"), " ", env!("CARGO_PKG_VERSION"));
/// [`CLASS_TEXT`] as the NUL-terminated UTF-16 text Win32 takes.
const CLASS_NAME: &[u16; CLASS_TEXT.len() + 1] = &PageWindow::wide(CLASS_TEXT);

/// A page's top-level window, hidden until shown and destroyed when dropped; it never leaves the
/// thread that opened it. A person's close only hides it.
#[derive(Debug)]
pub(crate) struct PageWindow {
    /// The window.
    handle: HWND,
    /// The module that owns the window's class.
    instance: HINSTANCE,
}

impl PageWindow {
    /// Registers the window class when no session has yet, and opens a hidden window of it.
    pub(crate) fn new() -> Result<Self, Error> {
        let instance = Self::register().map_err(Self::failure)?;
        let handle = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                PCWSTR(CLASS_NAME.as_ptr()),
                w!("volt-webview"),
                WS_OVERLAPPEDWINDOW,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                WIDTH,
                HEIGHT,
                None,
                None,
                Some(instance),
                None,
            )
        }
        .map_err(Self::failure)?;
        Ok(Self { handle, instance })
    }

    /// Builds the webview `configure` describes into the window, with the session's data folder,
    /// and turns off its own script dialogs.
    pub(crate) fn attach(
        &self,
        store: &Store,
        configure: impl FnOnce(WebViewBuilder<'_>) -> WebViewBuilder<'_>,
    ) -> Result<WebView, Error> {
        let mut context = store.context.borrow_mut();
        let webview = configure(WebViewBuilder::new_with_web_context(&mut context)).build(self)?;
        Self::silence(&webview)?;
        Ok(webview)
    }

    /// Turns off `WebView2`'s own dialogs for `alert`, `confirm`, `prompt` and `beforeunload`,
    /// which wry leaves on and which would stop a hidden page's scripts until a person answers
    /// them; with no handler for them, `WebView2` answers each at once.
    fn silence(webview: &WebView) -> Result<(), Error> {
        let core = webview.webview();
        unsafe {
            let settings = core.Settings().map_err(Self::failure)?;
            settings
                .SetAreDefaultScriptDialogsEnabled(false)
                .map_err(Self::failure)
        }
    }

    /// Leaves `builder` as it is: wry reports no end of `WebView2`'s web content process, so
    /// `_crashed` is never called and a crashed page's calls time out.
    pub(crate) fn watch(
        builder: WebViewBuilder<'_>,
        _crashed: impl Fn() + 'static,
    ) -> WebViewBuilder<'_> {
        builder
    }

    /// Whether the webview can load `url`; `WebView2` reports a URL it cannot parse as an error
    /// of its own.
    pub(crate) fn accepts(_url: &str) -> bool {
        true
    }

    /// Shows the window without activating it, so that the keyboard focus stays where it is.
    pub(crate) fn reveal(&self) {
        let _ = unsafe { ShowWindow(self.handle, SW_SHOWNOACTIVATE) };
    }

    /// Shows the window, restored when minimized, and brings it to the foreground, which Windows
    /// grants only while the program is in the foreground; otherwise its taskbar button flashes.
    pub(crate) fn show(&self) {
        unsafe {
            let _ = ShowWindow(self.handle, SW_SHOWNORMAL);
            let _ = SetForegroundWindow(self.handle);
        }
    }

    /// Hides the window.
    pub(crate) fn hide(&self) {
        let _ = unsafe { ShowWindow(self.handle, SW_HIDE) };
    }

    /// Registers the window class, or finds it registered by an earlier session, and returns the
    /// program's module, which owns it.
    fn register() -> io::Result<HINSTANCE> {
        let instance = HINSTANCE::from(unsafe { GetModuleHandleW(PCWSTR::null()) }?);
        let class = WNDCLASSW {
            lpfnWndProc: Some(Self::procedure),
            hInstance: instance,
            lpszClassName: PCWSTR(CLASS_NAME.as_ptr()),
            ..WNDCLASSW::default()
        };
        if unsafe { RegisterClassW(&raw const class) } == 0 {
            Self::already_registered(io::Error::last_os_error())?;
        }
        Ok(instance)
    }

    /// Accepts the failure to register when it only says that the class exists already.
    fn already_registered(error: io::Error) -> io::Result<()> {
        if error.raw_os_error() == Some(ERROR_CLASS_ALREADY_EXISTS.0.cast_signed()) {
            Ok(())
        } else {
            Err(error)
        }
    }

    /// Returns the failure for a window Win32 could not register or create.
    fn failure(error: impl error::Error + Send + Sync + 'static) -> Error {
        Error::Webview(Box::new(error))
    }

    /// Answers a page window's messages: a close request hides the window instead of destroying
    /// it, and every other message gets the default handling.
    ///
    /// # Safety
    ///
    /// Only the system calls it, for a live window of the calling thread, with the arguments of
    /// that message.
    unsafe extern "system" fn procedure(
        window: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if message == WM_CLOSE {
            let _ = unsafe { ShowWindow(window, SW_HIDE) };
            return LRESULT(0);
        }
        unsafe { DefWindowProcW(window, message, wparam, lparam) }
    }

    /// Returns ASCII `text` as NUL-terminated UTF-16 of `N` units, `N` being its length plus one.
    const fn wide<const N: usize>(text: &str) -> [u16; N] {
        let bytes = text.as_bytes();
        assert!(text.is_ascii() && bytes.len() + 1 == N);
        let mut wide = [0; N];
        let mut index = 0;
        while index < bytes.len() {
            wide[index] = u16::from_le_bytes([bytes[index], 0]);
            index += 1;
        }
        wide
    }
}

impl HasWindowHandle for PageWindow {
    /// Returns a Win32 handle to the window. Borrowing it is sound: the window lives until this
    /// value is dropped, which `&self` prevents for as long as the handle lives, and this value
    /// never leaves the thread that created the window, as the handle requires. wry keeps the
    /// window's handle after building into it; every webview is dropped before its window, and
    /// only this value destroys the window, since its procedure turns a close into a hide.
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let hwnd = NonZeroIsize::new(self.handle.0.addr().cast_signed())
            .ok_or(HandleError::Unavailable)?;
        let mut raw = Win32WindowHandle::new(hwnd);
        raw.hinstance = NonZeroIsize::new(self.instance.0.addr().cast_signed());
        Ok(unsafe { WindowHandle::borrow_raw(RawWindowHandle::Win32(raw)) })
    }
}

impl Drop for PageWindow {
    fn drop(&mut self) {
        let _ = unsafe { DestroyWindow(self.handle) };
    }
}

/// A window thread's message queue, read one message at a time.
#[derive(Debug)]
pub(super) struct Queue {
    /// The message read last.
    message: MSG,
}

impl Queue {
    /// Creates the calling thread's message queue, so that a doorbell rung from now on reaches
    /// it.
    pub(super) fn create() -> Self {
        let mut message = MSG::default();
        let _ = unsafe { PeekMessageW(&raw mut message, None, WM_USER, WM_USER, PM_NOREMOVE) };
        Self { message }
    }

    /// Waits for the next message; `false` once a quit message arrives.
    pub(super) fn wait(&mut self) -> bool {
        unsafe { GetMessageW(&raw mut self.message, None, 0, 0) }.0 > 0
    }

    /// Hands the message to its window, with the characters its keys type.
    pub(super) fn dispatch(&self) {
        unsafe {
            let _ = TranslateMessage(&raw const self.message);
            DispatchMessageW(&raw const self.message);
        }
    }
}

/// Wakes a window thread from any thread by posting to its message queue. It holds the thread's
/// handle, which keeps the thread's id from going to another thread, even after the thread ends.
#[derive(Debug)]
pub(super) struct Doorbell {
    /// The window thread.
    thread: JoinHandle<()>,
}

impl Doorbell {
    /// Returns the doorbell of `thread`.
    pub(super) fn new(thread: JoinHandle<()>) -> Self {
        Self { thread }
    }

    /// Wakes the thread so that it reads its channel.
    pub(super) fn ring(&self) {
        self.post(WM_APP);
    }

    /// Ends whichever message loop the thread is in, its own or one inside the webview, unless the
    /// thread has ended already.
    pub(super) fn quit(&self) {
        if !self.thread.is_finished() {
            self.post(WM_QUIT);
        }
    }

    /// Posts `message` to the thread's queue; a thread that has ended gets nothing.
    fn post(&self, message: u32) {
        unsafe {
            let thread = GetThreadId(HANDLE(self.thread.as_raw_handle()));
            let _ = PostThreadMessageW(thread, message, WPARAM(0), LPARAM(0));
        }
    }
}
