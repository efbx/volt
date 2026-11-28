use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy, NSEventMask, NSWindow};
use objc2_foundation::ns_string;

use super::main_thread::DEFAULT_MODE;

/// The program as the application `AppKit` takes it for. A program that runs no event loop of
/// its own, as a terminal program does not, has nothing to do an application's part for its
/// windows, so this crate does it: the right to show windows, the place in front of the other
/// programs' windows, and the events the windows are sent. A program with an event loop of its
/// own is left as it is.
pub(super) struct Program {
    /// The program's application object.
    app: Retained<NSApplication>,
}

impl Program {
    /// Returns the program's application, which `AppKit` creates on first use.
    pub(super) fn new(mtm: MainThreadMarker) -> Self {
        Self {
            app: NSApplication::sharedApplication(mtm),
        }
    }

    /// Gives a program without an event loop of its own the right to show windows, as an
    /// accessory without a Dock icon or a menu bar, when it has none: a program outside an
    /// application bundle starts without it, and its windows then never reach the screen.
    pub(super) fn admit(&self) {
        if !self.hosts_itself()
            && self.app.activationPolicy() == NSApplicationActivationPolicy::Prohibited
        {
            let _ = self
                .app
                .setActivationPolicy(NSApplicationActivationPolicy::Accessory);
        }
    }

    /// Puts `window` in front of the other programs' windows in a program without an event
    /// loop of its own: such a program is never the active one, so its window would stay behind
    /// theirs. A click in the window then makes the program the active one.
    pub(super) fn overtake(&self, window: &NSWindow) {
        if !self.hosts_itself() {
            window.orderFrontRegardless();
        }
    }

    /// Hands the program's windows the events the system has queued for them, the mouse and
    /// the keyboard included, as the event loop of the program would.
    pub(super) fn deliver_events(&self) {
        while let Some(event) = self.app.nextEventMatchingMask_untilDate_inMode_dequeue(
            NSEventMask::Any,
            None,
            ns_string!(DEFAULT_MODE),
            true,
        ) {
            self.app.sendEvent(&event);
        }
    }

    /// Whether the program runs an event loop of its own, which then serves its windows.
    fn hosts_itself(&self) -> bool {
        self.app.isRunning()
    }
}
