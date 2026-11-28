//! Calls awaited on the main thread of a program that runs no event loop there, as in a terminal
//! program's `#[tokio::main]`: on macOS such a call runs the main thread's event loop while it
//! waits, which serves the call and the page's window. Each test opens a webview window, so they
//! run only when asked for with `--ignored` or `--include-ignored`; a name given on the command
//! line runs only the tests whose names contain it, and `--skip NAME` leaves out those whose
//! names contain `NAME`. Other platforms bring their own window thread and have nothing to test
//! here.
//!
//! The test with `shown` in its name covers the screen with another program's window, shows its
//! page, which has to come in front of that window, and clicks in it through the system's event
//! stream. That takes over the pointer of whoever sits at the machine, so the test runs only on a
//! GitHub runner.

use std::env;
use std::process::ExitCode;
use std::time::Duration;

use tokio::runtime::{Builder, Runtime};
use volt_browser::{Error, Settings};
use volt_webview::Webview;

/// The test that needs a hidden page only.
const SERVED: &str = "a_call_awaited_on_an_idle_main_thread_is_served";
/// The test that puts a window on the screen and clicks in it.
const CLICKED: &str = "a_shown_window_without_an_event_loop_takes_a_click";
/// A page titled `volt`, as a data URL.
const TITLED_PAGE: &str = "data:text/html,%3Ctitle%3Evolt%3C%2Ftitle%3E";
/// A page that fills its window and titles itself `clicked` when clicked, as a data URL.
const CLICKABLE_PAGE: &str = "data:text/html,%3Cbody%20style=%22margin:0;height:100vh%22%20onclick=%22document.title=%27clicked%27%22%3E";
/// The function that tells whether the clickable page was clicked.
const RETITLED: &str = "function () { return document.title === 'clicked'; }";
/// The most the clicked page's job takes.
const CLICK_TIMEOUT: Duration = Duration::from_secs(30);

/// The tests of this binary, each run from the main thread, inside a runtime's `block_on` and
/// outside any event loop.
struct IdleMainThread;

impl IdleMainThread {
    /// Opens a session, loads the titled page in one of its pages and reads the title.
    async fn title() -> Result<String, Error> {
        let session = Webview.launch(Settings::default()).await?;
        session
            .run(|page| async move {
                page.goto(TITLED_PAGE).await?;
                page.title().await
            })
            .await
    }

    /// Opens the clickable page in a shown window, clicks in that window through the system
    /// once it is the frontmost one there, and waits for the page's new title; gives whether
    /// the click could be made at all.
    async fn click() -> Result<bool, Error> {
        let settings = Settings {
            visible: true,
            timeout: CLICK_TIMEOUT,
            ..Settings::default()
        };
        let session = Webview.launch(settings).await?;
        session
            .run(|page| async move {
                page.goto(CLICKABLE_PAGE).await?;
                #[cfg(target_os = "macos")]
                if !desktop::Desktop::click(&page).await? {
                    return Ok(false);
                }
                page.wait_until(RETITLED, &()).await?;
                Ok(true)
            })
            .await
    }

    /// Whether a job read the page's title.
    fn serves_a_call(runtime: &Runtime) -> bool {
        let title = runtime.block_on(Self::title());
        matches!(&title, Ok(title) if title == "volt")
    }

    /// Whether a page shown while another program's window covers the screen came in front of
    /// that window and took a click; a click that did not reach the page is followed by what
    /// the system says of the program and its windows.
    fn takes_a_click(runtime: &Runtime) -> bool {
        #[cfg(target_os = "macos")]
        let Some(_cover) = desktop::Cover::raise() else {
            eprintln!("no other program's window came to the front of the screen");
            return false;
        };
        let clicked = runtime.block_on(Self::click());
        if !matches!(clicked, Ok(true)) {
            eprintln!("the click: {clicked:?}");
            #[cfg(target_os = "macos")]
            desktop::Desktop::describe();
        }
        matches!(clicked, Ok(true))
    }
}

/// Which tests run, and how their outcomes are reported, as libtest would.
struct Runner {
    /// Whether the command line asks for ignored tests.
    asked: bool,
    /// The text a test's name must contain to run, when given.
    filter: Option<String>,
    /// The texts whose tests are left out.
    skipped: Vec<String>,
}

impl Runner {
    /// Reads the command line the way libtest does for the flags this runner knows.
    fn from_args() -> Self {
        let mut runner = Self {
            asked: false,
            filter: None,
            skipped: Vec::new(),
        };
        let mut args = env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--ignored" | "--include-ignored" => runner.asked = true,
                "--skip" => runner.skipped.extend(args.next()),
                flag if flag.starts_with('-') => {}
                _ => runner.filter = Some(arg),
            }
        }
        runner
    }

    /// Whether test `name` runs.
    fn selects(&self, name: &str) -> bool {
        self.filter
            .as_deref()
            .is_none_or(|filter| name.contains(filter))
            && !self
                .skipped
                .iter()
                .any(|skipped| name.contains(skipped.as_str()))
    }

    /// Whether the program runs on a GitHub runner, where nobody's pointer is taken over.
    fn on_a_github_runner() -> bool {
        env::var_os("GITHUB_ACTIONS").is_some()
    }

    /// Lists both tests as ignored, as libtest does for ignored tests nobody asked for.
    fn report_ignored() -> ExitCode {
        println!("\nrunning 2 tests");
        println!("test {SERVED} ... ignored, opens a webview window on macOS");
        println!("test {CLICKED} ... ignored, shows a webview window on macOS and clicks in it");
        println!("\ntest result: ok. 0 passed; 0 failed; 2 ignored\n");
        ExitCode::SUCCESS
    }

    /// Prints the outcome of test `name` and returns 1 when it failed.
    fn report(name: &str, passed: bool) -> u8 {
        println!("test {name} ... {}", if passed { "ok" } else { "FAILED" });
        u8::from(!passed)
    }

    /// Runs the selected tests on the main thread and returns how many failed; the test that
    /// clicks is left out anywhere but on a GitHub runner.
    fn run(&self) -> u8 {
        let runtime = Builder::new_multi_thread().enable_all().build().unwrap();
        let mut failed = 0;
        if self.selects(SERVED) {
            failed += Self::report(SERVED, IdleMainThread::serves_a_call(&runtime));
        }
        if self.selects(CLICKED) && Self::on_a_github_runner() {
            failed += Self::report(CLICKED, IdleMainThread::takes_a_click(&runtime));
        } else if self.selects(CLICKED) {
            println!("test {CLICKED} ... ignored, clicks through the system: GitHub runners only");
        }
        failed
    }
}

#[cfg(target_os = "macos")]
mod desktop {
    use std::env;
    use std::process::{self, Child, Command};
    use std::thread;
    use std::time::Duration;

    use objc2::rc::Retained;
    use objc2::{MainThreadMarker, MainThreadOnly};
    use objc2_app_kit::{
        NSApplication, NSApplicationActivationPolicy, NSScreen, NSView, NSViewController, NSWindow,
        NSWorkspace,
    };
    use objc2_core_foundation::{CFRunLoop, CFString};
    use objc2_core_graphics::{CGEvent, CGEventTapLocation, CGEventType, CGMouseButton};
    use objc2_foundation::{NSPoint, ns_string};
    use volt_browser::{Error, Page};

    /// How often the screen is looked at before a test gives up on what it waits to see there.
    const LOOKS: usize = 100;
    /// The pause between two looks.
    const PAUSE: Duration = Duration::from_millis(100);

    /// The machine's screen, as its window server has it.
    pub(super) struct Desktop;

    impl Desktop {
        /// Clicks in the middle of the page's window through the system's event stream once
        /// that window is the frontmost one there, which `page` is asked about itself meanwhile
        /// so that the main thread keeps serving it; gives whether it got to click.
        pub(super) async fn click(page: &Page) -> Result<bool, Error> {
            for _ in 0..LOOKS {
                if let Some(middle) = Self::middle_of_the_frontmost_page() {
                    Self::press(middle);
                    return Ok(true);
                }
                tokio::time::sleep(PAUSE).await;
                page.evaluate::<u8>("1").await?;
            }
            Ok(false)
        }

        /// Prints what the system says of the program and its windows, for a test whose click
        /// did not reach the page.
        pub(super) fn describe() {
            let Some(mtm) = MainThreadMarker::new() else {
                return eprintln!("the test does not run on the main thread");
            };
            let app = NSApplication::sharedApplication(mtm);
            eprintln!(
                "the program: policy {:?}, active {}, window {:?} in the middle of the screen",
                app.activationPolicy(),
                app.isActive(),
                Self::middle(mtm).map(|middle| Self::window_at(middle, mtm))
            );
            for window in &app.windows() {
                eprintln!(
                    "a window: number {}, title {}, visible {}, key {}, occlusion {:?}, frame {:?}",
                    window.windowNumber(),
                    window.title(),
                    window.isVisible(),
                    window.isKeyWindow(),
                    window.occlusionState(),
                    window.frame()
                );
            }
        }

        /// Returns the middle of the page's window, in the screen's coordinates, when a click
        /// there would hit that window and no other.
        fn middle_of_the_frontmost_page() -> Option<NSPoint> {
            let mtm = MainThreadMarker::new()?;
            let window = Self::page_window(mtm)?;
            let frame = window.frame();
            let middle = NSPoint::new(
                frame.origin.x + frame.size.width / 2.0,
                frame.origin.y + frame.size.height / 2.0,
            );
            (Self::window_at(middle, mtm) == window.windowNumber()).then_some(middle)
        }

        /// Returns the program's window that shows a page, if it has one on the screen.
        fn page_window(mtm: MainThreadMarker) -> Option<Retained<NSWindow>> {
            NSApplication::sharedApplication(mtm)
                .windows()
                .iter()
                .find(|window| {
                    window.isVisible() && window.title().isEqualToString(ns_string!("volt-webview"))
                })
        }

        /// Returns the middle of the screen, in the screen's coordinates.
        fn middle(mtm: MainThreadMarker) -> Option<NSPoint> {
            let frame = NSScreen::screens(mtm).firstObject()?.frame();
            Some(NSPoint::new(
                frame.size.width / 2.0,
                frame.size.height / 2.0,
            ))
        }

        /// Returns the number of the window, of any program, that a click at `point` would hit.
        fn window_at(point: NSPoint, mtm: MainThreadMarker) -> isize {
            NSWindow::windowNumberAtPoint_belowWindowWithWindowNumber(point, 0, mtm)
        }

        /// Moves the pointer to `point`, in the screen's coordinates, and presses and lets go of
        /// the left button there, as a person's hand would through the system.
        fn press(point: NSPoint) {
            let Some(mtm) = MainThreadMarker::new() else {
                return;
            };
            let Some(screen) = NSScreen::screens(mtm).firstObject() else {
                return;
            };
            let at = NSPoint::new(point.x, screen.frame().size.height - point.y);
            for kind in [
                CGEventType::MouseMoved,
                CGEventType::LeftMouseDown,
                CGEventType::LeftMouseUp,
            ] {
                let event = CGEvent::new_mouse_event(None, kind, at, CGMouseButton::Left);
                CGEvent::post(CGEventTapLocation::HIDEventTap, event.as_deref());
            }
        }
    }

    /// Another program on the screen: a process of this binary whose window covers the screen
    /// and whose program is the frontmost one, for the page's window to come in front of. It
    /// ends with this value.
    pub(super) struct Cover {
        /// The process that plays the other program.
        process: Child,
    }

    impl Cover {
        /// The variable that tells a process of this binary to play the other program.
        const ROLE: &str = "VOLT_WEBVIEW_COVER";
        /// How long the other program stays when nobody ends it.
        const STAY: Duration = Duration::from_secs(120);

        /// Whether this process was started to play the other program.
        pub(super) fn is_this_process() -> bool {
            env::var_os(Self::ROLE).is_some()
        }

        /// Starts the other program and returns once its window covers the middle of the screen
        /// and a click in it has made its program the frontmost one; gives nothing when the
        /// screen does not get there in time.
        pub(super) fn raise() -> Option<Self> {
            let mtm = MainThreadMarker::new()?;
            let _connected = NSApplication::sharedApplication(mtm);
            let middle = Desktop::middle(mtm)?;
            let behind = Desktop::window_at(middle, mtm);
            let process = Command::new(env::current_exe().ok()?)
                .env(Self::ROLE, "1")
                .spawn()
                .ok()?;
            let cover = Self { process };
            Self::seen(|| Desktop::window_at(middle, mtm) != behind)?;
            Self::seen(|| cover.comes_to_the_front(middle))?;
            Some(cover)
        }

        /// Plays the other program in this process: an application whose window covers the
        /// screen, until the process is ended or [`Cover::STAY`] has passed.
        pub(super) fn run() -> ! {
            thread::spawn(|| {
                thread::sleep(Self::STAY);
                process::exit(0);
            });
            let mtm = MainThreadMarker::new().expect("a process starts on its main thread");
            let app = NSApplication::sharedApplication(mtm);
            let _ = app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
            let frame = NSScreen::screens(mtm)
                .firstObject()
                .expect("the machine has a screen")
                .frame();
            let view = NSView::initWithFrame(NSView::alloc(mtm), frame);
            let controller = NSViewController::new(mtm);
            controller.setView(&view);
            let window = NSWindow::windowWithContentViewController(&controller);
            window.setFrame_display(frame, true);
            window.makeKeyAndOrderFront(None);
            app.run();
            process::exit(0)
        }

        /// Looks until `there` is true, and gives nothing when it never is. The main thread's run
        /// loop turns once between two looks, which is when the system brings up to date what it
        /// tells this program of the others.
        fn seen(mut there: impl FnMut() -> bool) -> Option<()> {
            let mode = CFString::from_static_str("kCFRunLoopDefaultMode");
            for _ in 0..LOOKS {
                if there() {
                    return Some(());
                }
                thread::sleep(PAUSE);
                let _ = CFRunLoop::run_in_mode(Some(&mode), 0.0, false);
            }
            None
        }

        /// Whether the other program is the frontmost one; while it is not, a click in the
        /// `middle` of its window asks the system to make it so.
        fn comes_to_the_front(&self, middle: NSPoint) -> bool {
            let frontmost = self.is_frontmost();
            if !frontmost {
                Desktop::press(middle);
            }
            frontmost
        }

        /// Whether the other program is the frontmost one.
        fn is_frontmost(&self) -> bool {
            NSWorkspace::sharedWorkspace()
                .frontmostApplication()
                .and_then(|program| u32::try_from(program.processIdentifier()).ok())
                .is_some_and(|program| program == self.process.id())
        }
    }

    impl Drop for Cover {
        fn drop(&mut self) {
            let _ = self.process.kill();
            let _ = self.process.wait();
        }
    }
}

fn main() -> ExitCode {
    #[cfg(target_os = "macos")]
    if desktop::Cover::is_this_process() {
        desktop::Cover::run();
    }
    let runner = Runner::from_args();
    if !runner.asked || cfg!(not(target_os = "macos")) {
        return Runner::report_ignored();
    }
    let failed = runner.run();
    println!(
        "\ntest result: {}. {failed} failed\n",
        if failed == 0 { "ok" } else { "FAILED" }
    );
    ExitCode::from(failed)
}
