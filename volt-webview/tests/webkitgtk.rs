//! What only the `WebKitGTK` backend can get wrong, checked for real on Linux and the BSDs. The
//! tests that open webview pages, all hidden, run only when asked for with `--ignored`, as the
//! window tests do; they need a display, a virtual one included. Tests whose name starts with
//! `child_` do nothing unless another test runs them in a process of their own: GTK starts only
//! once per process, and only a process with one page has one web content process to end.

#![cfg(any(
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd"
))]

use std::env;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{self, Command, ExitStatus};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use gtk::glib::{self, MainContext, MainLoop};
use gtk::prelude::{ObjectExt, WidgetExt};
use gtk::{OffscreenWindow, Widget, Window};
use serde_json::Value;
use tokio::runtime::Builder;
use volt_browser::{Page, Session, Settings};
use volt_webview::Webview;

/// A page titled `volt`, as a data URL.
const TITLED_PAGE: &str = "data:text/html,%3Ctitle%3Evolt%3C%2Ftitle%3E";
/// A display no server answers on.
const GONE_DISPLAY: &str = ":999";

/// What a call that failed for the webview's own reason says.
struct Failure;

impl Failure {
    /// Whether `outcome` failed with the engine's own kind and a message starting with `reason`.
    fn says<T>(outcome: &Result<T, volt_browser::Error>, reason: &str) -> bool {
        matches!(
            outcome,
            Err(volt_browser::Error::Engine(cause)) if cause.to_string().starts_with(reason)
        )
    }
}

/// Sessions with one hidden page whose commands wait a few seconds at most.
struct Open;

impl Open {
    /// The most a command waits.
    const TIMEOUT: Duration = Duration::from_secs(5);

    /// Returns the settings of every session here.
    fn settings() -> Settings {
        Settings {
            timeout: Self::TIMEOUT,
            ..Settings::default()
        }
    }

    /// Launches a session and opens its first page.
    async fn page() -> (Session, Page) {
        let session = Webview.launch(Self::settings()).await.unwrap();
        let page = session.new_page().await.unwrap();
        (session, page)
    }

    /// Launches a session from a thread of its own runtime, loads the titled page and reads its
    /// title.
    fn title() -> Result<String, volt_browser::Error> {
        let runtime = Builder::new_multi_thread().enable_all().build().unwrap();
        runtime.block_on(async {
            Webview
                .launch(Self::settings())
                .await?
                .run(|page| async move {
                    page.goto(TITLED_PAGE).await?;
                    page.title().await
                })
                .await
        })
    }
}

/// A web server on 127.0.0.1 that answers every request by its path.
struct Site;

impl Site {
    /// Starts a server that answers each request with what `answer` gives for its path, and
    /// returns its address without a trailing slash, under the host name `host`.
    fn serve(host: &str, answer: fn(&str) -> String) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        thread::spawn(move || {
            for stream in listener.incoming().map_while(Result::ok) {
                thread::spawn(move || Self::answer(&stream, answer));
            }
        });
        format!("http://{host}:{port}")
    }

    /// Returns the address of a port on 127.0.0.1 that refuses connections.
    fn refusing() -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        format!("http://{}/", listener.local_addr().unwrap())
    }

    /// Reads one request from `stream` and writes what `answer` gives for its path.
    fn answer(stream: &TcpStream, answer: fn(&str) -> String) {
        let mut lines = BufReader::new(stream).lines().map_while(Result::ok);
        let Some(request) = lines.next() else {
            return;
        };
        let path = request.split(' ').nth(1).unwrap_or("/");
        let response = answer(path);
        lines.take_while(|line| !line.is_empty()).for_each(drop);
        let mut writer = stream;
        let _ = writer.write_all(response.as_bytes());
    }

    /// Returns a response that carries `body` as an HTML page, after the header lines `extra`.
    fn html(extra: &str, body: &str) -> String {
        Self::page("200 OK", extra, body)
    }

    /// Returns a response with the status `status` that carries `body` as an HTML page, after
    /// the header lines `extra`.
    fn page(status: &str, extra: &str, body: &str) -> String {
        format!(
            "HTTP/1.1 {status}\r\n{extra}Content-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }
}

/// The processes this test process started.
struct Children;

impl Children {
    /// Returns the ids of this process's children whose command line contains `name`.
    fn named(name: &str) -> Vec<u32> {
        let parent = process::id();
        fs::read_dir("/proc")
            .unwrap()
            .filter_map(Result::ok)
            .filter_map(|entry| entry.file_name().to_str()?.parse().ok())
            .filter(|id| Self::is(*id, parent, name))
            .collect()
    }

    /// Whether process `id` is a child of `parent` whose command line contains `name`.
    fn is(id: u32, parent: u32, name: &str) -> bool {
        let stat = fs::read_to_string(format!("/proc/{id}/stat")).unwrap_or_default();
        let command = fs::read_to_string(format!("/proc/{id}/cmdline")).unwrap_or_default();
        let after_name = stat.rsplit(')').next().unwrap_or_default();
        let parent_of = after_name.split_whitespace().nth(1);
        parent_of.and_then(|field| field.parse().ok()) == Some(parent) && command.contains(name)
    }

    /// Ends process `id` at once, as a crash would.
    fn kill(id: u32) {
        let status = Command::new("kill")
            .args(["-9", &id.to_string()])
            .status()
            .unwrap();
        assert!(status.success());
    }
}

/// The windows GTK has on the display.
struct Windows;

impl Windows {
    /// Counts the windows shown on the display: every visible window but the offscreen ones
    /// that hold hidden pages.
    fn shown() -> usize {
        Self::count(|window| window.is_visible() && !window.is::<OffscreenWindow>())
    }

    /// Counts every window GTK still has, shown or not.
    fn open() -> usize {
        Self::count(|_| true)
    }

    /// Counts, on the thread that runs GTK, the windows that `counts` picks.
    fn count(counts: fn(&Widget) -> bool) -> usize {
        let (count, counted) = mpsc::channel();
        MainContext::default().invoke(move || {
            let picked = Window::list_toplevels()
                .iter()
                .filter(|window| counts(window))
                .count();
            let _ = count.send(picked);
        });
        counted.recv().unwrap()
    }

    /// Waits, two seconds at most, until GTK has no window left, and returns whether it came to
    /// that.
    async fn all_closed() -> bool {
        for _ in 0..20 {
            if Self::open() == 0 {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        false
    }
}

/// A test run in a process of its own.
struct Child;

impl Child {
    /// The variable that names the test a child process runs.
    const CASE: &str = "VOLT_WEBVIEW_CHILD_CASE";

    /// Whether this process is the child started for test `case`.
    fn runs(case: &str) -> bool {
        env::var_os(Self::CASE).is_some_and(|named| named == case)
    }

    /// Runs test `case` of this binary in a process of its own, set up by `configure`, and
    /// returns how it ended.
    fn run(case: &str, configure: impl FnOnce(&mut Command)) -> ExitStatus {
        let mut command = Command::new(env::current_exe().unwrap());
        command
            .args(["--ignored", "--exact", case, "--nocapture"])
            .env(Self::CASE, case);
        configure(&mut command);
        command.status().unwrap()
    }
}

#[test]
fn a_launch_after_a_failed_gtk_start_fails_at_once() {
    let ended = Child::run("child_two_launches_where_the_display_is_gone", |command| {
        command
            .env("DISPLAY", GONE_DISPLAY)
            .env_remove("WAYLAND_DISPLAY");
    });
    assert!(ended.success(), "{ended}");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "runs in the process its parent test starts"]
async fn child_two_launches_where_the_display_is_gone() {
    if !Child::runs("child_two_launches_where_the_display_is_gone") {
        return;
    }
    for _ in 0..2 {
        let start = Instant::now();
        let launched = Webview.launch(Open::settings()).await;
        assert!(
            Failure::says(&launched, "the system webview cannot run here"),
            "{launched:?}"
        );
        assert!(
            start.elapsed() < Duration::from_secs(2),
            "{:?}",
            start.elapsed()
        );
    }
}

#[test]
#[ignore = "opens webview windows"]
fn a_program_that_runs_gtk_serves_the_session() {
    let ended = Child::run("child_a_session_in_a_program_that_runs_gtk", |_| {});
    assert!(ended.success(), "{ended}");
}

#[test]
#[ignore = "runs in the process its parent test starts"]
fn child_a_session_in_a_program_that_runs_gtk() {
    if !Child::runs("child_a_session_in_a_program_that_runs_gtk") {
        return;
    }
    gtk::init().unwrap();
    let worker = thread::spawn(|| {
        let title = Open::title();
        MainContext::default().invoke(gtk::main_quit);
        title
    });
    gtk::main();
    assert_eq!(worker.join().unwrap().unwrap(), "volt");
}

#[test]
#[ignore = "opens webview windows"]
fn a_program_with_a_plain_glib_loop_serves_the_session() {
    let ended = Child::run(
        "child_a_session_in_a_program_with_a_plain_glib_loop",
        |_| {},
    );
    assert!(ended.success(), "{ended}");
}

#[test]
#[ignore = "runs in the process its parent test starts"]
fn child_a_session_in_a_program_with_a_plain_glib_loop() {
    if !Child::runs("child_a_session_in_a_program_with_a_plain_glib_loop") {
        return;
    }
    let main_loop = MainLoop::new(None, false);
    let (running, ran) = mpsc::channel();
    let _ = glib::idle_add_once(move || {
        let _ = running.send(());
    });
    let quitting = main_loop.clone();
    let worker = thread::spawn(move || {
        ran.recv().unwrap();
        let title = Open::title();
        quitting.quit();
        title
    });
    main_loop.run();
    assert_eq!(worker.join().unwrap().unwrap(), "volt");
    assert!(gtk::is_initialized_main_thread());
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "opens webview windows"]
async fn a_page_that_asks_before_unload_can_be_left() {
    let site = Site::serve("127.0.0.1", |path| match path {
        "/stay" => Site::html(
            "",
            "<title>stay</title><script>addEventListener('beforeunload', function (event) { event.preventDefault(); event.returnValue = 'stay'; return 'stay'; });</script>",
        ),
        _ => Site::html("", "<title>next</title>"),
    });
    let (session, page) = Open::page().await;
    page.goto(&format!("{site}/stay")).await.unwrap();
    assert_eq!(page.title().await.unwrap(), "stay");

    let left = page.goto(&format!("{site}/next")).await;
    assert!(left.is_ok(), "{left:?}");
    assert_eq!(page.title().await.unwrap(), "next");
    session.close().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "opens webview windows"]
async fn a_goto_within_the_same_document_ends() {
    let site = Site::serve("127.0.0.1", |_| {
        Site::html(
            "",
            "<title>anchors</title><p id=top>top</p><p id=low>low</p>",
        )
    });
    let (session, page) = Open::page().await;
    page.goto(&format!("{site}/page")).await.unwrap();

    for fragment in ["low", "top"] {
        let start = Instant::now();
        let moved = page.goto(&format!("{site}/page#{fragment}")).await;
        assert!(moved.is_ok(), "{moved:?}");
        assert!(
            start.elapsed() < Duration::from_secs(2),
            "{:?}",
            start.elapsed()
        );
        assert_eq!(page.url().await.unwrap().fragment(), Some(fragment));
    }
    page.goto(&format!("{site}/page#top")).await.unwrap();
    page.goto(&format!("{site}/other")).await.unwrap();
    assert_eq!(page.url().await.unwrap().path(), "/other");
    session.close().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "opens webview windows"]
async fn a_page_behind_a_password_fails_its_goto() {
    let site = Site::serve("127.0.0.1", |path| match path {
        "/locked" => Site::page(
            "401 Unauthorized",
            "WWW-Authenticate: Basic realm=\"volt\"\r\n",
            "<title>locked</title>",
        ),
        _ => Site::html("", "<title>open</title>"),
    });
    let (session, page) = Open::page().await;
    page.goto(&format!("{site}/open")).await.unwrap();

    let locked = page.goto(&format!("{site}/locked")).await;
    assert!(Failure::says(&locked, "the load failed"), "{locked:?}");
    assert_eq!(page.title().await.unwrap(), "open");
    page.goto(TITLED_PAGE).await.unwrap();
    assert_eq!(page.title().await.unwrap(), "volt");
    session.close().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "opens webview windows"]
async fn a_goto_right_after_a_failed_one_waits_for_its_own_load() {
    let site = Site::serve("127.0.0.1", |_| {
        thread::sleep(Duration::from_millis(400));
        Site::html("", "<title>slow</title>")
    });
    let refusing = Site::refusing();
    let (session, page) = Open::page().await;
    page.goto(TITLED_PAGE).await.unwrap();

    for _ in 0..5 {
        let failed = page.goto(&refusing).await;
        assert!(Failure::says(&failed, "the load failed"), "{failed:?}");
        page.goto(&format!("{site}/slow")).await.unwrap();
        assert_eq!(page.title().await.unwrap(), "slow");
        page.goto(TITLED_PAGE).await.unwrap();
    }
    session.close().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "opens webview windows"]
async fn a_failed_load_leaves_the_page_where_it_was() {
    let refusing = Site::refusing();
    let (session, page) = Open::page().await;
    page.goto(TITLED_PAGE).await.unwrap();

    assert!(page.goto(&refusing).await.is_err());
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(page.title().await.unwrap(), "volt");
    assert_eq!(page.url().await.unwrap().as_str(), TITLED_PAGE);
    session.close().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "opens webview windows"]
async fn clearing_cookies_removes_a_sites_domain_cookies() {
    let site = Site::serve("app.localhost", |_| {
        Site::html(
            "Set-Cookie: narrow=1; Path=/\r\nSet-Cookie: wide=2; Domain=app.localhost; Path=/\r\n",
            "<title>jar</title>",
        )
    });
    let (session, page) = Open::page().await;
    page.goto(&format!("{site}/")).await.unwrap();
    let stored = page.cookies().await.unwrap();
    assert_eq!(stored.len(), 2, "{stored:?}");
    for name in ["narrow", "wide"] {
        assert!(
            stored.iter().any(|cookie| cookie.name == name),
            "{stored:?}"
        );
    }

    page.delete_cookie("wide").await.unwrap();
    let left = page.cookies().await.unwrap();
    assert!(left.iter().all(|cookie| cookie.name != "wide"), "{left:?}");
    page.goto(&format!("{site}/")).await.unwrap();
    page.clear_cookies().await.unwrap();
    let left = page.cookies().await.unwrap();
    assert!(left.is_empty(), "{left:?}");
    session.close().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "opens webview windows"]
async fn a_file_or_colour_input_opens_no_panel() {
    let site = Site::serve("127.0.0.1", |_| {
        Site::html(
            "",
            "<title>pick</title><input id=file type=file><input id=colour type=color><script>var cancelled = 0; document.getElementById('file').addEventListener('cancel', function () { cancelled += 1; });</script>",
        )
    });
    let (session, page) = Open::page().await;
    page.goto(&format!("{site}/")).await.unwrap();

    for id in ["file", "colour"] {
        page.evaluate::<Value>(&format!("void document.getElementById('{id}').click()"))
            .await
            .unwrap();
    }
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(Windows::shown(), 0);
    assert_eq!(page.evaluate::<u8>("cancelled").await.unwrap(), 1);
    session.close().await;
}

#[test]
#[ignore = "opens webview windows"]
fn hidden_pages_show_no_window_and_leave_none_behind() {
    let ended = Child::run("child_windows_of_hidden_pages", |_| {});
    assert!(ended.success(), "{ended}");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "runs in the process its parent test starts"]
async fn child_windows_of_hidden_pages() {
    if !Child::runs("child_windows_of_hidden_pages") {
        return;
    }
    let session = Webview.launch(Open::settings()).await.unwrap();
    let first = session.new_page().await.unwrap();
    let second = session.new_page().await.unwrap();
    first.goto(TITLED_PAGE).await.unwrap();
    assert_eq!(Windows::open(), 4);
    assert_eq!(Windows::shown(), 0);

    second.close().await.unwrap();
    session.close().await;
    assert!(Windows::all_closed().await, "{} left", Windows::open());
}

#[test]
#[ignore = "opens webview windows"]
fn a_web_process_that_dies_fails_the_goto_and_the_page_recovers() {
    let ended = Child::run("child_a_goto_whose_web_process_dies", |_| {});
    assert!(ended.success(), "{ended}");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "runs in the process its parent test starts"]
async fn child_a_goto_whose_web_process_dies() {
    if !Child::runs("child_a_goto_whose_web_process_dies") {
        return;
    }
    let silent = TcpListener::bind("127.0.0.1:0").unwrap();
    let stuck_url = format!("http://{}/", silent.local_addr().unwrap());
    let (session, page) = Open::page().await;
    page.goto(TITLED_PAGE).await.unwrap();
    let stuck = page.clone();
    let waiting = tokio::spawn(async move { stuck.goto(&stuck_url).await });
    tokio::time::sleep(Duration::from_millis(500)).await;

    let processes = Children::named("WebKitWebProcess");
    assert_eq!(processes.len(), 1, "{processes:?}");
    Children::kill(processes[0]);
    let crashed = waiting.await.unwrap();
    assert!(
        Failure::says(&crashed, "the page's web content process ended"),
        "{crashed:?}"
    );
    page.goto(TITLED_PAGE).await.unwrap();
    assert_eq!(page.title().await.unwrap(), "volt");
    session.close().await;
}
