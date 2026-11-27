//! The window tests: every test opens webview windows in this process, so they run only when
//! asked for with `--ignored` or `--include-ignored`, as libtest runs ignored tests, and plain
//! `cargo test` lists them as ignored without creating anything. On macOS a tao event loop runs
//! on the main thread while the tests run on another thread; elsewhere the tests run on the main
//! thread, since volt-webview brings its own window thread there. A name given on the
//! command line runs only the tests whose names contain it, and `--skip NAME` leaves out those
//! whose names contain `NAME`.

use std::env;
use std::future::Future;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::pin::Pin;
use std::process::{self, ExitCode};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use serde_json::Value;
use tokio::runtime::{Builder, Runtime};
use volt_browser::{Error, Page, Session, Settings};
use volt_browser_checks::Checks;
use volt_http::middleware::BrowserBuilder;
use volt_http::{Client, StatusCode};
use volt_webview::Webview;
use wreq::cookie::Jar;

/// A page titled `volt`, as a data URL.
const TITLED_PAGE: &str = "data:text/html,%3Ctitle%3Evolt%3C%2Ftitle%3E";
/// An address on a port no webview uses, so that a load there fails.
const REFUSED_URL: &str = "http://127.0.0.1:1/";

/// A test's body, as a future the runner awaits.
type Body = fn() -> Pin<Box<dyn Future<Output = ()> + Send>>;

/// Every window test, by name, in the order they run: the first two settle whether anything else
/// can work on a platform.
const CASES: &[(&str, Body)] = &[
    ("a_new_page_finishes_loading_about_blank", || {
        Box::pin(Cases::a_new_page_finishes_loading_about_blank())
    }),
    ("cookies_include_http_only_ones", || {
        Box::pin(Cases::cookies_include_http_only_ones())
    }),
    ("a_loaded_page_evaluates_expressions", || {
        Box::pin(Cases::a_loaded_page_evaluates_expressions())
    }),
    ("the_shared_checks_pass_on_hidden_pages", || {
        Box::pin(Cases::the_shared_checks_pass_on_hidden_pages())
    }),
    (
        "a_page_that_replaces_its_own_json_cannot_break_a_call",
        || Box::pin(Cases::a_page_that_replaces_its_own_json_cannot_break_a_call()),
    ),
    (
        "the_user_agent_is_the_webviews_own_unless_overridden",
        || Box::pin(Cases::the_user_agent_is_the_webviews_own_unless_overridden()),
    ),
    ("a_window_can_be_opened_shown_and_hidden", || {
        Box::pin(Cases::a_window_can_be_opened_shown_and_hidden())
    }),
    ("the_shared_checks_pass_on_shown_pages", || {
        Box::pin(Cases::the_shared_checks_pass_on_shown_pages())
    }),
    ("a_hidden_page_has_a_real_viewport", || {
        Box::pin(Cases::a_hidden_page_has_a_real_viewport())
    }),
    (
        "a_job_that_outlasts_the_timeout_fails_at_its_timeout",
        || Box::pin(Cases::a_job_that_outlasts_the_timeout_fails_at_its_timeout()),
    ),
    ("closing_the_session_closes_its_pages", || {
        Box::pin(Cases::closing_the_session_closes_its_pages())
    }),
    ("a_new_goto_replaces_one_still_waiting", || {
        Box::pin(Cases::a_new_goto_replaces_one_still_waiting())
    }),
    ("a_failed_load_ends_as_the_platform_reports_it", || {
        Box::pin(Cases::a_failed_load_ends_as_the_platform_reports_it())
    }),
    ("a_url_that_does_not_parse_fails_without_a_panic", || {
        Box::pin(Cases::a_url_that_does_not_parse_fails_without_a_panic())
    }),
    ("dialogs_and_downloads_never_block_a_page", || {
        Box::pin(Cases::dialogs_and_downloads_never_block_a_page())
    }),
    ("two_sessions_run_side_by_side", || {
        Box::pin(Cases::two_sessions_run_side_by_side())
    }),
    (
        "a_refused_request_is_solved_in_the_webview_and_sent_again",
        || Box::pin(Cases::a_refused_request_is_solved_in_the_webview_and_sent_again()),
    ),
];

/// The window tests every platform runs.
struct Cases;

impl Cases {
    /// Opens a page, which only returns once `about:blank` has finished loading.
    async fn a_new_page_finishes_loading_about_blank() {
        let session = Webview.launch(Settings::default()).await.unwrap();
        let page = session.new_page().await.unwrap();
        assert_eq!(page.url().await.unwrap().as_str(), "about:blank");
        session.close().await;
    }

    /// Runs the checks every engine of volt-browser has to pass, on hidden pages.
    async fn the_shared_checks_pass_on_hidden_pages() {
        let session = Webview.launch(Settings::default()).await.unwrap();
        let failures = Checks::run(&session).await.unwrap();
        session.close().await;
        assert!(failures.is_empty(), "{failures:#?}");
    }

    /// Reads an `HttpOnly` cookie set by a local server.
    async fn cookies_include_http_only_ones() {
        let (url, server) = Servers::cookie();
        let session = Webview.launch(Settings::default()).await.unwrap();
        let page = session.new_page().await.unwrap();
        page.goto(&url).await.unwrap();
        let cookies = page.cookies().await.unwrap();
        session.close().await;
        assert!(
            cookies
                .iter()
                .any(|cookie| cookie.name == "token" && cookie.value == "abc" && cookie.http_only),
            "{cookies:?}"
        );
        server.join().unwrap();
    }

    /// Loads a titled page and evaluates expressions whose values JSON holds or cannot hold.
    async fn a_loaded_page_evaluates_expressions() {
        let (session, page) = Open::page(Settings::default()).await;
        page.goto(TITLED_PAGE).await.unwrap();
        assert_eq!(
            page.evaluate::<String>("document.title").await.unwrap(),
            "volt"
        );
        assert_eq!(
            page.evaluate::<String>("document.title;").await.unwrap(),
            "volt"
        );
        assert_eq!(
            page.evaluate::<Value>("undefined").await.unwrap(),
            Value::Null
        );
        assert_eq!(page.evaluate::<u8>("1+1").await.unwrap(), 2);
        assert_eq!(page.evaluate::<Value>("0/0").await.unwrap(), Value::Null);
        assert!(
            page.evaluate::<Value>("new Date(0)")
                .await
                .unwrap()
                .is_string()
        );
        let thrown = page.evaluate::<Value>("missing.name").await;
        assert!(matches!(thrown, Err(Error::Script(_))), "{thrown:?}");
        assert!(
            page.evaluate::<bool>("(function () { return document.title === 'volt'; })()")
                .await
                .unwrap()
        );
        session.close().await;
    }

    /// Replaces the page's `navigator.userAgent` and `JSON.stringify`, then evaluates: the calls
    /// answer or fail, and none of them panics.
    async fn a_page_that_replaces_its_own_json_cannot_break_a_call() {
        let (session, page) = Open::page(Settings::default()).await;
        page.goto(TITLED_PAGE).await.unwrap();
        page.evaluate::<Value>(
            "void Object.defineProperty(navigator,'userAgent',{get(){return new Date()}})",
        )
        .await
        .unwrap();
        assert_ne!(page.user_agent().await.unwrap(), "");
        let replacing = page
            .evaluate::<Value>("void (JSON.stringify=function(){return new Date()})")
            .await;
        assert!(
            matches!(replacing, Ok(Value::Null) | Err(Error::Detached)),
            "{replacing:?}"
        );
        let broken = page.evaluate::<Value>("1").await;
        assert!(
            matches!(broken, Ok(Value::Null) | Err(Error::Detached)),
            "{broken:?}"
        );
        session.close().await;
    }

    /// Reads the webview's own user agent, then an override.
    async fn the_user_agent_is_the_webviews_own_unless_overridden() {
        let (session, page) = Open::page(Settings::default()).await;
        assert_ne!(page.user_agent().await.unwrap(), "");
        session.close().await;
        let options = Settings {
            user_agent: Some("volt-webview-test/1.0".to_owned()),
            ..Settings::default()
        };
        let (session, page) = Open::page(options).await;
        assert_eq!(page.user_agent().await.unwrap(), "volt-webview-test/1.0");
        session.close().await;
    }

    /// Opens a shown window, then hides and shows it.
    async fn a_window_can_be_opened_shown_and_hidden() {
        let options = Settings {
            visible: true,
            ..Settings::default()
        };
        let (session, page) = Open::page(options).await;
        page.hide().await.unwrap();
        page.show().await.unwrap();
        session.close().await;
    }

    /// Runs the checks every engine of volt-browser has to pass, on shown pages.
    async fn the_shared_checks_pass_on_shown_pages() {
        let options = Settings {
            visible: true,
            ..Settings::default()
        };
        let session = Webview.launch(options).await.unwrap();
        let failures = Checks::run(&session).await.unwrap();
        session.close().await;
        assert!(failures.is_empty(), "{failures:#?}");
    }

    /// Reads the width a hidden page reports, which anti-bot scripts look at.
    async fn a_hidden_page_has_a_real_viewport() {
        let (session, page) = Open::page(Settings::default()).await;
        page.goto(TITLED_PAGE).await.unwrap();
        let width: u64 = page.evaluate("window.innerWidth").await.unwrap();
        assert!(width > 0, "{width}");
        session.close().await;
    }

    /// Runs a job past its timeout, which fails at the timeout.
    async fn a_job_that_outlasts_the_timeout_fails_at_its_timeout() {
        let options = Settings {
            timeout: Duration::from_secs(1),
            ..Settings::default()
        };
        let start = Instant::now();
        let result = Open::run(options, |_page| async {
            tokio::time::sleep(Duration::from_secs(5)).await;
            Ok(())
        })
        .await;
        assert!(matches!(result, Err(Error::Timeout(_))), "{result:?}");
        assert!(start.elapsed() < Duration::from_secs(3));
    }

    /// Closes the session and uses its page.
    async fn closing_the_session_closes_its_pages() {
        let (session, page) = Open::page(Settings::default()).await;
        session.close().await;
        assert!(matches!(
            page.evaluate::<Value>("1").await,
            Err(Error::Closed)
        ));
    }

    /// Starts a goto to a server that never answers, then replaces it.
    async fn a_new_goto_replaces_one_still_waiting() {
        let (url, _silent) = Servers::silent();
        let (session, page) = Open::page(Settings::default()).await;
        let stuck = page.clone();
        let (first, second) = tokio::join!(stuck.goto(&url), async {
            tokio::time::sleep(Duration::from_millis(500)).await;
            page.goto(TITLED_PAGE).await
        });
        assert!(
            matches!(
                &first,
                Err(Error::Engine(cause))
                    if cause.to_string() == "a later goto on the same page replaced this one"
            ),
            "{first:?}"
        );
        second.unwrap();
        session.close().await;
    }

    /// Loads from a port nothing listens on, then a page that loads.
    async fn a_failed_load_ends_as_the_platform_reports_it() {
        let options = Settings {
            timeout: Duration::from_secs(5),
            ..Settings::default()
        };
        let (session, page) = Open::page(options).await;
        let failed = page.goto(REFUSED_URL).await;
        assert!(Platform::reports_a_failed_load_as(&failed), "{failed:?}");
        page.goto(TITLED_PAGE).await.unwrap();
        session.close().await;
    }

    /// Hands the webview URLs that do not parse, and one that only `NSURL` may reject.
    async fn a_url_that_does_not_parse_fails_without_a_panic() {
        let options = Settings {
            timeout: Duration::from_secs(5),
            ..Settings::default()
        };
        let (session, page) = Open::page(options).await;
        assert!(matches!(
            page.goto("not a url").await,
            Err(Error::InvalidUrl)
        ));
        assert!(matches!(page.goto("").await, Err(Error::InvalidUrl)));
        let opaque = page.goto("volt:{opaque}").await;
        assert!(!matches!(opaque, Err(Error::Closed)), "{opaque:?}");
        assert_eq!(page.evaluate::<u8>("1").await.unwrap(), 1);
        session.close().await;
    }

    /// Opens an alert in a hidden page and follows a link to a download.
    async fn dialogs_and_downloads_never_block_a_page() {
        let (url, server) = Servers::download();
        let options = Settings {
            timeout: Duration::from_secs(5),
            ..Settings::default()
        };
        let (session, page) = Open::page(options).await;
        page.goto(TITLED_PAGE).await.unwrap();
        page.evaluate::<Value>("setTimeout(function(){alert('volt')},0)")
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(page.evaluate::<u8>("1+1").await.unwrap(), 2);
        let _ = page.goto(&url).await;
        assert_eq!(page.evaluate::<u8>("1+1").await.unwrap(), 2);
        session.close().await;
        server.join().unwrap();
    }

    /// Runs two sessions at once.
    async fn two_sessions_run_side_by_side() {
        let (first, second) = tokio::join!(
            Open::run(Settings::default(), |page| async move {
                page.goto(TITLED_PAGE).await?;
                page.title().await
            }),
            Open::run(Settings::default(), |page| async move {
                page.goto(TITLED_PAGE).await?;
                page.title().await
            }),
        );
        assert_eq!(first.unwrap(), "volt");
        assert_eq!(second.unwrap(), "volt");
    }

    /// Sends a request the gated site refuses until volt-http has opened the site in a hidden
    /// page of the webview, and reads the request it then sent again.
    async fn a_refused_request_is_solved_in_the_webview_and_sent_again() {
        let (url, site) = Gated::start();
        let jar = Arc::new(Jar::default());
        let browser = BrowserBuilder::new(jar.clone(), |settings| Webview.launch(settings))
            .recovery_url(format!("{url}solve"))
            .headed(false)
            .solve_timeout(Duration::from_secs(60))
            .build();
        let client = Client::builder()
            .cookie_jar(jar)
            .middleware(browser)
            .build()
            .unwrap();

        let response = client.get(format!("{url}data")).send().await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let sent_again = site.join().unwrap();
        assert!(
            sent_again.contains("user-agent: mozilla/5.0"),
            "{sent_again}"
        );
    }
}

/// Sessions with one page.
struct Open;

impl Open {
    /// Launches a session from `settings` and opens its first page.
    async fn page(settings: Settings) -> (Session, Page) {
        let session = Webview.launch(settings).await.unwrap();
        let page = session.new_page().await.unwrap();
        (session, page)
    }

    /// Launches a session from `settings`, hands one page to `job` and closes the session.
    async fn run<T, F, Fut>(settings: Settings, job: F) -> Result<T, Error>
    where
        F: FnOnce(Page) -> Fut,
        Fut: Future<Output = Result<T, Error>>,
    {
        Webview.launch(settings).await?.run(job).await
    }
}

/// How this platform's webview ends a load that failed.
struct Platform;

impl Platform {
    /// Whether `failed` is how this platform ends a load from a port its webview does not use:
    /// `WKWebView` reports no end, `WebKitGTK` reports the failure, and `WebView2` shows its
    /// error page, whose error code the load then fails for. Newer `WebKit`, seen with Safari
    /// 27.2 and `WebKitGTK` 2.52.6, opens a blank page instead, which fails the load on both.
    fn reports_a_failed_load_as(failed: &Result<(), Error>) -> bool {
        if cfg!(target_os = "macos") {
            matches!(failed, Err(Error::Timeout(_))) || Self::says(failed, "the load failed")
        } else if cfg!(target_os = "windows") {
            Self::says(failed, "the load failed: ERR_")
        } else {
            Self::says(failed, "the load failed")
        }
    }

    /// Whether `failed` is the engine's own failure, with a message that starts with `message`.
    fn says(failed: &Result<(), Error>, message: &str) -> bool {
        matches!(
            failed,
            Err(Error::Engine(cause)) if cause.to_string().starts_with(message)
        )
    }
}

/// Web servers on 127.0.0.1, each on a free port.
struct Servers;

impl Servers {
    /// Starts a server that answers one request with `Set-Cookie: token=abc; HttpOnly; Path=/`.
    fn cookie() -> (String, JoinHandle<()>) {
        Self::answering(
            "HTTP/1.1 200 OK\r\nSet-Cookie: token=abc; HttpOnly; Path=/\r\nContent-Type: text/html\r\nContent-Length: 24\r\nConnection: close\r\n\r\n<title>cookie</title>set",
        )
    }

    /// Starts a server that answers one request with a file to download.
    fn download() -> (String, JoinHandle<()>) {
        Self::answering(
            "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Disposition: attachment; filename=volt.bin\r\nContent-Length: 4\r\nConnection: close\r\n\r\nvolt",
        )
    }

    /// Starts a server that accepts connections and never answers; it stops with the listener.
    fn silent() -> (String, TcpListener) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        (url, listener)
    }

    /// How long a connection may take to bring its request.
    const PATIENCE: Duration = Duration::from_millis(500);

    /// Starts a server that answers its first request with `response`. A webview may open a
    /// connection ahead of need and send its request on another, so a connection that brings no
    /// request is passed over.
    fn answering(response: &'static str) -> (String, JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            let served = listener
                .incoming()
                .map_while(Result::ok)
                .any(|stream| Self::answered(&stream, response));
            assert!(served, "the webview sent a request");
        });
        (url, server)
    }

    /// Reads one request's headers from `stream` and writes `response`, or does nothing for a
    /// connection that brings no request within [`Servers::PATIENCE`]; returns whether it wrote.
    fn answered(stream: &TcpStream, response: &str) -> bool {
        stream.set_read_timeout(Some(Self::PATIENCE)).unwrap();
        let headers = BufReader::new(stream)
            .lines()
            .map_while(Result::ok)
            .take_while(|line| !line.is_empty())
            .count();
        if headers == 0 {
            return false;
        }
        let mut writer = stream;
        writer.write_all(response.as_bytes()).unwrap();
        true
    }
}

/// A site on 127.0.0.1 that refuses every request without its cookie, and gives the cookie,
/// hidden from scripts, to whoever opens `/solve`.
struct Gated;

impl Gated {
    /// The answer to whoever opens `/solve`: a page, with the cookie.
    const SOLVED: &str = "HTTP/1.1 200 OK\r\nSet-Cookie: pass=1; HttpOnly; Path=/\r\nContent-Type: text/html\r\nContent-Length: 21\r\nConnection: close\r\n\r\n<title>Solved</title>";
    /// The answer to a request that carries the cookie.
    const THROUGH: &str =
        "HTTP/1.1 200 OK\r\nContent-Length: 7\r\nConnection: close\r\n\r\nthrough";
    /// The answer to every other request.
    const REFUSED: &str =
        "HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";

    /// Starts the site on a free port. It ends once it has let a request for `/data` through,
    /// and its thread returns that request's headers, in lower case.
    fn start() -> (String, JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        let site = thread::spawn(move || {
            listener
                .incoming()
                .map_while(Result::ok)
                .find_map(|stream| Self::let_through(&stream))
                .expect("a request for /data came with the cookie")
        });
        (url, site)
    }

    /// Answers the request `stream` brings, and returns its headers when it was a request for
    /// `/data` that the site let through.
    fn let_through(stream: &TcpStream) -> Option<String> {
        let head = Self::head(stream);
        let passed = head.starts_with("get /data") && head.contains("pass=1");
        if !head.is_empty() {
            let mut writer = stream;
            let _ = writer.write_all(Self::answer(&head, passed).as_bytes());
        }
        passed.then_some(head)
    }

    /// Returns the site's answer to the request with the headers `head`, which carried the
    /// cookie on its way to `/data` when `passed` says so.
    fn answer(head: &str, passed: bool) -> &'static str {
        if head.starts_with("get /solve") {
            Self::SOLVED
        } else if passed {
            Self::THROUGH
        } else {
            Self::REFUSED
        }
    }

    /// Reads one request's headers from `stream`, in lower case; a connection that brings no
    /// request within [`Servers::PATIENCE`] gives none.
    fn head(stream: &TcpStream) -> String {
        stream.set_read_timeout(Some(Servers::PATIENCE)).unwrap();
        let mut head = BufReader::new(stream)
            .lines()
            .map_while(Result::ok)
            .take_while(|line| !line.is_empty())
            .fold(String::new(), |mut head, line| {
                head.push_str(&line);
                head.push('\n');
                head
            });
        head.make_ascii_lowercase();
        head
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

    /// Lists every test in `names` as ignored, as libtest does for ignored tests nobody asked
    /// for.
    fn report_ignored(names: impl Iterator<Item = &'static str> + Clone) -> ExitCode {
        let count = names.clone().count();
        println!("\nrunning {count} tests");
        for name in names {
            println!("test {name} ... ignored, opens webview windows");
        }
        println!("\ntest result: ok. 0 passed; 0 failed; {count} ignored\n");
        ExitCode::SUCCESS
    }

    /// Runs every selected test of [`CASES`] on `runtime`, each in a task of its own, and
    /// returns how many failed.
    fn run_cases(self, runtime: &Runtime) -> usize {
        runtime.block_on(async {
            let mut failed = 0;
            for (name, body) in CASES.iter().filter(|(name, _)| self.selects(name)) {
                failed += Self::report(name, tokio::spawn(body()).await.is_ok());
            }
            failed
        })
    }

    /// Prints the outcome of test `name` and returns 1 when it failed.
    fn report(name: &str, passed: bool) -> usize {
        println!("test {name} ... {}", if passed { "ok" } else { "FAILED" });
        usize::from(!passed)
    }

    /// Ends the process with the outcome of the run.
    fn finish(failed: usize) -> ! {
        println!(
            "\ntest result: {}. {failed} failed\n",
            if failed == 0 { "ok" } else { "FAILED" }
        );
        process::exit(i32::from(failed > 0));
    }

    /// Returns the one tokio runtime of this binary.
    fn runtime() -> Runtime {
        Builder::new_multi_thread().enable_all().build().unwrap()
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::{Arc, Mutex, PoisonError};
    use std::task::{Context, Wake, Waker};
    use std::thread;
    use std::time::Duration;

    use futures::executor;
    use tao::event::Event;
    use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
    use tao::platform::macos::{ActivationPolicy, EventLoopExtMacOS};
    use tokio::runtime::Runtime;
    use tokio::sync::oneshot::{self, Sender};
    use volt_browser::{Error, Session, Settings};
    use volt_webview::Webview;

    use super::{CASES, Open, Runner, TITLED_PAGE};

    /// A future the main thread polls from inside its event loop.
    type MainTask = Pin<Box<dyn Future<Output = ()> + Send>>;

    /// The tests that only macOS has, where the program owns the main thread.
    const MAIN_THREAD_CASES: [&str; 2] = [
        "a_task_polled_on_the_main_thread_completes",
        "blocking_the_main_thread_fails_with_main_thread_blocked",
    ];

    /// What the test thread asks of the main thread.
    pub(super) enum Signal {
        /// Poll this task on the main thread until it is done.
        Spawn(MainTask),
        /// Poll the task again: its waker fired.
        Poll,
        /// Block the main thread on a launch, from inside the event loop, and report its outcome.
        Block(Sender<Result<Session, Error>>),
    }

    /// The test host on macOS: a tao event loop on the main thread.
    pub(super) struct Host;

    impl Host {
        /// Every test name, macOS ones included.
        pub(super) fn names() -> impl Iterator<Item = &'static str> + Clone {
            CASES.iter().map(|(name, _)| *name).chain(MAIN_THREAD_CASES)
        }

        /// Runs the tests on another thread while a tao event loop, which never activates the
        /// program, runs on the main thread; the test thread ends the process.
        pub(super) fn run(runner: Runner) -> ! {
            let runtime = Runner::runtime();
            let _context = runtime.enter();
            let mut event_loop = EventLoopBuilder::<Signal>::with_user_event().build();
            event_loop.set_activation_policy(ActivationPolicy::Accessory);
            event_loop.set_activate_ignoring_other_apps(false);
            let mut main_thread = MainThread::new(event_loop.create_proxy());
            let proxy = event_loop.create_proxy();
            let selected = MAIN_THREAD_CASES.map(|name| runner.selects(name));
            thread::spawn(move || {
                let failed = runner.run_cases(&runtime)
                    + Self::run_main_thread_cases(selected, &runtime, &proxy);
                Runner::finish(failed);
            });
            event_loop.run(move |event, _, control_flow| {
                *control_flow = ControlFlow::Wait;
                if let Event::UserEvent(signal) = event {
                    main_thread.handle(signal);
                }
            })
        }

        /// Runs the macOS-only tests that `selected` picks and returns how many failed.
        fn run_main_thread_cases(
            selected: [bool; 2],
            runtime: &Runtime,
            proxy: &EventLoopProxy<Signal>,
        ) -> usize {
            let [polled, blocked] = MAIN_THREAD_CASES;
            let mut failed = 0;
            if selected[0] {
                let passed = runtime.block_on(Self::polled_on_the_main_thread(proxy));
                failed += Runner::report(polled, passed);
            }
            if selected[1] {
                let passed = runtime.block_on(Self::blocked_main_thread(proxy));
                failed += Runner::report(blocked, passed);
            }
            failed
        }

        /// Has the main thread poll a job's task from inside its event loop, and whether it
        /// completed.
        async fn polled_on_the_main_thread(proxy: &EventLoopProxy<Signal>) -> bool {
            let (done, finished) = oneshot::channel();
            let task = Box::pin(async move {
                let title = Open::run(Settings::default(), |page| async move {
                    page.goto(TITLED_PAGE).await?;
                    page.title().await
                })
                .await;
                let _ = done.send(title);
            });
            proxy.send_event(Signal::Spawn(task)).is_ok()
                && matches!(finished.await, Ok(Ok(title)) if title == "volt")
        }

        /// Has the main thread block on a launch from inside the event loop, and whether that
        /// failed for the blocked main thread.
        async fn blocked_main_thread(proxy: &EventLoopProxy<Signal>) -> bool {
            let (outcome, reported) = oneshot::channel();
            proxy.send_event(Signal::Block(outcome)).is_ok()
                && matches!(
                    reported.await,
                    Ok(Err(Error::Engine(cause)))
                        if cause.to_string().contains("the main thread is blocked")
                )
        }
    }

    /// The task the main thread polls, with the waker that asks for its next poll.
    struct MainThread {
        /// The task being polled, until it is done.
        task: Option<MainTask>,
        /// The waker that sends [`Signal::Poll`] to the event loop.
        waker: Waker,
    }

    impl MainThread {
        /// Returns a main thread with no task, whose waker reaches the loop behind `proxy`.
        fn new(proxy: EventLoopProxy<Signal>) -> Self {
            Self {
                task: None,
                waker: Waker::from(Arc::new(LoopWaker(Mutex::new(proxy)))),
            }
        }

        /// Takes up `signal`: a new task to poll, another poll of the current one, or a launch
        /// to block on.
        fn handle(&mut self, signal: Signal) {
            match signal {
                Signal::Spawn(task) => {
                    self.task = Some(task);
                    self.poll();
                }
                Signal::Poll => self.poll(),
                Signal::Block(outcome) => {
                    let _ = outcome.send(Self::block_on_a_launch());
                }
            }
        }

        /// Polls the current task once, dropping it when it is done.
        fn poll(&mut self) {
            let Some(task) = self.task.as_mut() else {
                return;
            };
            if task
                .as_mut()
                .poll(&mut Context::from_waker(&self.waker))
                .is_ready()
            {
                self.task = None;
            }
        }

        /// Blocks the main thread on a launch with a short timeout, from inside the event loop.
        fn block_on_a_launch() -> Result<Session, Error> {
            let options = Settings {
                timeout: Duration::from_secs(2),
                ..Settings::default()
            };
            executor::block_on(Webview.launch(options))
        }
    }

    /// Wakes the main thread's task by sending [`Signal::Poll`] to the event loop.
    struct LoopWaker(Mutex<EventLoopProxy<Signal>>);

    impl Wake for LoopWaker {
        fn wake(self: Arc<Self>) {
            let _ = self
                .0
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .send_event(Signal::Poll);
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod elsewhere {
    use super::{CASES, Runner};

    /// The test host on Windows, Linux and the BSDs: the main thread runs the tests, and
    /// volt-webview runs its own window thread.
    pub(super) struct Host;

    impl Host {
        /// Every test name.
        pub(super) fn names() -> impl Iterator<Item = &'static str> + Clone {
            CASES.iter().map(|(name, _)| *name)
        }

        /// Runs the tests on the main thread and ends the process with their outcome.
        pub(super) fn run(runner: Runner) -> ! {
            let runtime = Runner::runtime();
            Runner::finish(runner.run_cases(&runtime))
        }
    }
}

#[cfg(target_os = "macos")]
use macos::Host;

#[cfg(not(target_os = "macos"))]
use elsewhere::Host;

fn main() -> ExitCode {
    let runner = Runner::from_args();
    if !runner.asked {
        return Runner::report_ignored(Host::names());
    }
    Host::run(runner)
}
