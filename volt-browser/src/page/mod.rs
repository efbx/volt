mod cookies;
mod navigation;

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde::Serialize;
use serde::de::DeserializeOwned;
use tokio::runtime::Handle;
use tokio::time::{self, Instant, Sleep};

use crate::element::{Path, Step};
use crate::page_script::{Answer, Code, Obstacle, Op, Request, Script};
use crate::{By, Element, Engine, Error, Rect, Settings};
pub use navigation::{Load, Match};

/// The pause between the two looks that show an element holds still.
const SETTLE: Duration = Duration::from_millis(20);

/// One page of a browser, the same whichever engine drives it. Clones drive the same page.
#[derive(Clone, Debug)]
pub struct Page {
    /// What the clones share.
    inner: Arc<Inner>,
}

/// What the clones of a page share.
#[derive(Debug)]
struct Inner {
    /// The engine's side of the page.
    engine: Arc<dyn Engine>,
    /// What the session's commands follow.
    settings: Arc<Settings>,
    /// The runtime whose timers waiting commands use, so that a page works from any executor
    /// while that runtime runs.
    runtime: Handle,
    /// The number the next command keeps a pending promise under.
    tokens: AtomicU64,
}

impl Page {
    /// Returns the page `engine` drives, whose commands follow `settings` and wait on
    /// `runtime`'s timers.
    pub(crate) fn new(engine: Arc<dyn Engine>, settings: Arc<Settings>, runtime: Handle) -> Self {
        Self {
            inner: Arc::new(Inner {
                engine,
                settings,
                runtime,
                tokens: AtomicU64::new(0),
            }),
        }
    }

    /// Describes the elements `by` finds in the page. Nothing is looked up until a command runs.
    #[must_use]
    pub fn find(&self, by: By) -> Element {
        Element::new(self.clone(), Step::Find(by))
    }

    /// Describes the element that has the keyboard focus.
    #[must_use]
    pub fn focused(&self) -> Element {
        Element::new(self.clone(), Step::Focused)
    }

    /// Evaluates the JavaScript `expression` in the page and reads its value as `T`, waiting
    /// for a promise. A script that throws fails with [`Error::Script`], and so does one that
    /// does not parse on Chrome; on the web view that one gives no answer, [`Error::Detached`].
    pub async fn evaluate<T: DeserializeOwned>(&self, expression: &str) -> Result<T, Error> {
        let request = Request {
            op: Op::Evaluate,
            path: None,
            arg: &(),
            code: Code::Expression(expression),
        };
        self.perform(&request, self.settings().timeout, Patience::Now)
            .await
    }

    /// Calls the JavaScript `function` with `arg` and reads what it returns as `T`, waiting for
    /// a promise. The argument reaches the page as JSON, never as script text. A function that
    /// throws or does not parse fails as [`evaluate`](Self::evaluate) says.
    pub async fn call<T, A>(&self, function: &str, arg: &A) -> Result<T, Error>
    where
        T: DeserializeOwned,
        A: Serialize + ?Sized,
    {
        let request = Request {
            op: Op::Call,
            path: None,
            arg,
            code: Code::Function(function),
        };
        self.perform(&request, self.settings().timeout, Patience::Now)
            .await
    }

    /// Waits until the JavaScript function `predicate`, called with `arg`, returns something
    /// true. It has to answer at once: a promise counts as true.
    pub async fn wait_until<A: Serialize + ?Sized>(
        &self,
        predicate: &str,
        arg: &A,
    ) -> Result<(), Error> {
        let request = Request {
            op: Op::Until,
            path: None,
            arg,
            code: Code::Function(predicate),
        };
        self.perform(&request, self.settings().timeout, Patience::Wait)
            .await
    }

    /// The user agent the page reports.
    pub async fn user_agent(&self) -> Result<String, Error> {
        self.ask(Op::UserAgent, &()).await
    }

    /// Shows the page's window in front. Chrome cannot show a browser launched hidden, and fails
    /// with [`Error::Unsupported`].
    pub async fn show(&self) -> Result<(), Error> {
        self.engine().set_visible(true).await
    }

    /// Hides the page's window. Chrome cannot hide a browser launched shown, and fails with
    /// [`Error::Unsupported`].
    pub async fn hide(&self) -> Result<(), Error> {
        self.engine().set_visible(false).await
    }

    /// Closes the page; commands through its other clones then fail with [`Error::Closed`].
    pub async fn close(self) -> Result<(), Error> {
        self.engine().close().await
    }

    /// What the session's commands follow.
    pub(crate) fn settings(&self) -> &Settings {
        &self.inner.settings
    }

    /// The engine's side of the page.
    pub(crate) fn engine(&self) -> &dyn Engine {
        self.inner.engine.as_ref()
    }

    /// Runs the page command `op` once and reads its answer.
    pub(crate) async fn ask<T, A>(&self, op: Op, arg: &A) -> Result<T, Error>
    where
        T: DeserializeOwned,
        A: Serialize + ?Sized,
    {
        let request = Request {
            op,
            path: None,
            arg,
            code: Code::None,
        };
        self.perform(&request, self.settings().timeout, Patience::Now)
            .await
    }

    /// Runs the page command `op` again and again until it says its condition holds.
    pub(crate) async fn until<A: Serialize + ?Sized>(&self, op: Op, arg: &A) -> Result<(), Error> {
        let request = Request {
            op,
            path: None,
            arg,
            code: Code::None,
        };
        self.perform(&request, self.settings().timeout, Patience::Wait)
            .await
    }

    /// Runs `request` in the page and reads its answer as `T`. With [`Patience::Wait`] a page
    /// that is not ready is asked again every [`Settings::poll`] until `timeout`; a call a
    /// navigation cut off counts as not ready. A promise the command leaves pending is awaited
    /// either way.
    pub(crate) async fn perform<T, A>(
        &self,
        request: &Request<'_, A>,
        timeout: Duration,
        patience: Patience,
    ) -> Result<T, Error>
    where
        T: DeserializeOwned,
        A: Serialize + ?Sized,
    {
        let deadline = Instant::now() + timeout;
        let token = self.inner.tokens.fetch_add(1, Ordering::Relaxed);
        let mut script = Script::build(request, None, token)?;
        loop {
            let stall = match self.attempt(&script).await {
                Ok(Some(Answer::Ok { json })) => return Script::value(json.as_deref()),
                Ok(Some(Answer::Pending)) => return self.collect(token, deadline, timeout).await,
                Ok(Some(Answer::Error {
                    kind,
                    message,
                    count,
                })) => return Err(kind.error(request.path, message, count)),
                Ok(Some(Answer::Wait { blocker, rect })) => Stall::Blocked(blocker, rect),
                Ok(None) => Stall::Silent,
                Err(error) if error.is_final() => return Err(error),
                Err(error) => Stall::Failed(error),
            };
            if let Some(rect) = stall.held() {
                script = Script::build(request, Some(rect), token)?;
            }
            if patience == Patience::Now || Instant::now() >= deadline {
                return Err(stall.error(request.path, timeout));
            }
            self.pause(stall.pause(self.settings())).await;
        }
    }

    /// Waits for the promise kept under `token` to settle and reads its value as `T`.
    async fn collect<T: DeserializeOwned>(
        &self,
        token: u64,
        deadline: Instant,
        timeout: Duration,
    ) -> Result<T, Error> {
        let request = Request {
            op: Op::Take,
            path: None,
            arg: &(),
            code: Code::None,
        };
        let script = Script::build(&request, None, token)?;
        loop {
            match self.attempt(&script).await {
                Ok(Some(Answer::Ok { json })) => return Script::value(json.as_deref()),
                Ok(Some(Answer::Error {
                    kind,
                    message,
                    count,
                })) => return Err(kind.error(None, message, count)),
                Err(error) if error.is_final() => return Err(error),
                Ok(_) | Err(_) => {}
            }
            if Instant::now() >= deadline {
                return Err(Error::Timeout(timeout));
            }
            self.pause(self.settings().poll).await;
        }
    }

    /// Evaluates `script` in the page once and reads the helper's answer, `None` where the page
    /// gave none.
    async fn attempt(&self, script: &str) -> Result<Option<Answer>, Error> {
        Answer::read(self.engine().evaluate(script).await?)
    }

    /// Returns a sleep of `duration` on the session's runtime, whichever executor polls it.
    fn pause(&self, duration: Duration) -> Sleep {
        let _entered = self.inner.runtime.enter();
        time::sleep(duration)
    }
}

/// Whether a command waits for the page to be ready for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Patience {
    /// It asks again until the timeout.
    Wait,
    /// It asks once.
    Now,
}

/// Why a command has no answer yet.
#[derive(Debug)]
enum Stall {
    /// The page said what is in the way, and the element's box when it has to hold still.
    Blocked(Obstacle, Option<Rect>),
    /// The page gave no answer, as during a navigation.
    Silent,
    /// The engine failed, as when a navigation cuts a call off.
    Failed(Error),
}

impl Stall {
    /// The box a moving element has to keep until the command is asked again, where the page
    /// gave one.
    fn held(&self) -> Option<Rect> {
        match self {
            Self::Blocked(Obstacle::Moving, rect) => *rect,
            Self::Blocked(..) | Self::Silent | Self::Failed(_) => None,
        }
    }

    /// How long to wait before asking again: a moment for an element that has to hold still,
    /// otherwise the session's poll.
    fn pause(&self, settings: &Settings) -> Duration {
        match self {
            Self::Blocked(Obstacle::Moving, _) => SETTLE.min(settings.poll),
            Self::Blocked(..) | Self::Silent | Self::Failed(_) => settings.poll,
        }
    }

    /// Returns the error for a command that still had no answer when it stopped asking.
    fn error(self, path: Option<&Path>, timeout: Duration) -> Error {
        match self {
            Self::Blocked(Obstacle::Unmet, _) => Error::Timeout(timeout),
            Self::Blocked(obstacle, _) => match obstacle.blocker() {
                Some(blocker) => Error::NotActionable(Path::label(path), blocker),
                None => Error::NotFound(Path::label(path)),
            },
            Self::Silent if path.is_some() => Error::NotFound(Path::label(path)),
            Self::Silent => Error::Detached,
            Self::Failed(error) => error,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::time::Duration;

    use futures::executor;
    use serde_json::{Value, json};
    use tokio::runtime::Builder;

    use crate::fake::Fake;
    use crate::{By, Error};

    #[tokio::test]
    async fn an_expression_travels_as_code_and_its_value_comes_back_typed() {
        let fake = Fake::answering([json!({"status": "ok", "json": "42"})]);
        let answer: u32 = fake.page().evaluate("20 + 22").await.unwrap();
        assert_eq!(answer, 42);
        assert_eq!(fake.commands()[0]["op"], "evaluate");
        assert_eq!(fake.tail(0), ",function(){return (\n20 + 22\n)})");
    }

    #[tokio::test]
    async fn a_function_gets_its_argument_as_json() {
        let fake = Fake::answering([json!({"status": "ok", "json": "\"tea, milk\""})]);
        let answer: String = fake
            .page()
            .call(
                "function (words) { return words.join(', '); }",
                &["tea", "milk"],
            )
            .await
            .unwrap();
        assert_eq!(answer, "tea, milk");
        assert_eq!(fake.commands()[0]["arg"], json!(["tea", "milk"]));
    }

    #[tokio::test]
    async fn a_thrown_script_is_an_error_and_not_null() {
        let fake = Fake::answering([
            json!({"status": "error", "kind": "script", "message": "nope is not defined", "count": 0}),
        ]);
        let outcome = fake.page().evaluate::<Value>("nope").await;
        assert!(matches!(outcome, Err(Error::Script(message)) if message == "nope is not defined"));
    }

    #[tokio::test]
    async fn a_pending_promise_is_asked_for_until_it_settles() {
        let fake = Fake::answering([
            json!({"status": "pending"}),
            json!({"status": "pending"}),
            json!({"status": "ok", "json": "\"late\""}),
        ]);
        let answer: String = fake.page().evaluate("later()").await.unwrap();
        assert_eq!(answer, "late");
        let commands = fake.commands();
        assert_eq!(commands.len(), 3);
        assert_eq!(
            (&commands[1]["op"], &commands[2]["op"]),
            (&json!("take"), &json!("take"))
        );
        assert_eq!(commands[0]["token"], commands[2]["token"]);
    }

    #[tokio::test]
    async fn each_command_keeps_its_promise_under_its_own_number() {
        let fake = Fake::answering([
            json!({"status": "ok", "json": "1"}),
            json!({"status": "ok", "json": "2"}),
        ]);
        let page = fake.page();
        page.evaluate::<u8>("1").await.unwrap();
        page.clone().evaluate::<u8>("2").await.unwrap();
        let commands = fake.commands();
        assert_ne!(commands[0]["token"], commands[1]["token"]);
    }

    #[tokio::test]
    async fn a_rejected_promise_and_a_lost_document_fail_the_wait() {
        let rejected = Fake::answering([
            json!({"status": "pending"}),
            json!({"status": "error", "kind": "script", "message": "refused", "count": 0}),
        ]);
        assert!(matches!(
            rejected.page().evaluate::<Value>("later()").await,
            Err(Error::Script(message)) if message == "refused"
        ));
        let lost = Fake::answering([
            json!({"status": "pending"}),
            json!({"status": "error", "kind": "detached", "message": "", "count": 0}),
        ]);
        assert!(matches!(
            lost.page().evaluate::<Value>("later()").await,
            Err(Error::Detached)
        ));
    }

    #[tokio::test]
    async fn a_promise_that_never_settles_times_out() {
        let fake = Fake::repeating(json!({"status": "pending"}));
        let outcome = fake
            .page()
            .evaluate::<Value>("new Promise(function () {})")
            .await;
        assert!(matches!(outcome, Err(Error::Timeout(timeout)) if timeout == Fake::TIMEOUT));
    }

    #[tokio::test]
    async fn a_call_a_navigation_cut_off_counts_as_not_ready() {
        let fake = Fake::failing_then(
            [Error::Engine(Box::new(io::Error::other(
                "context destroyed",
            )))],
            [Value::Null, json!({"status": "ok", "json": "\"Welcome\""})],
        );
        let text = fake.page().find(By::tag("h1")).text().await.unwrap();
        assert_eq!(text, "Welcome");
        assert_eq!(fake.commands().len(), 3);
    }

    #[tokio::test]
    async fn an_engine_that_keeps_failing_gives_its_error_at_the_timeout() {
        let fake = Fake::breaking(|| Error::Engine(Box::new(io::Error::other("no connection"))));
        let outcome = fake.page().find(By::tag("h1")).text().await;
        assert!(
            matches!(outcome, Err(Error::Engine(cause)) if cause.to_string() == "no connection")
        );
    }

    #[tokio::test]
    async fn a_closed_page_fails_without_waiting() {
        let fake = Fake::breaking(|| Error::Closed);
        let outcome = fake.page().find(By::tag("h1")).text().await;
        assert!(matches!(outcome, Err(Error::Closed)));
        assert_eq!(fake.commands().len(), 1);
    }

    #[tokio::test]
    async fn a_page_command_with_no_answer_reports_the_lost_document() {
        let fake = Fake::answering([Value::Null]);
        assert!(matches!(
            fake.page().evaluate::<Value>("1").await,
            Err(Error::Detached)
        ));
    }

    #[tokio::test]
    async fn waiting_for_a_condition_asks_until_it_holds() {
        let fake = Fake::answering([
            json!({"status": "wait", "blocker": "unmet", "rect": null}),
            json!({"status": "ok", "json": null}),
        ]);
        fake.page()
            .wait_until("function (n) { return window.ready >= n; }", &3)
            .await
            .unwrap();
        let commands = fake.commands();
        assert_eq!(commands.len(), 2);
        assert_eq!(
            (&commands[0]["op"], &commands[0]["arg"]),
            (&json!("until"), &json!(3))
        );
    }

    #[tokio::test]
    async fn showing_hiding_and_closing_reach_the_engine() {
        let fake = Fake::answering([]);
        let page = fake.page();
        page.show().await.unwrap();
        assert!(fake.is_visible());
        page.hide().await.unwrap();
        assert!(!fake.is_visible());
        page.close().await.unwrap();
        assert!(fake.is_closed());
    }

    #[test]
    fn a_page_works_from_another_executor_while_its_runtime_runs() {
        let runtime = Builder::new_multi_thread()
            .worker_threads(1)
            .enable_time()
            .build()
            .unwrap();
        let fake = Fake::answering([
            json!({"status": "wait", "blocker": "missing", "rect": null}),
            json!({"status": "ok", "json": "\"here\""}),
        ]);
        let page = runtime.block_on(async { fake.page() });
        let text = executor::block_on(page.find(By::id("late")).text()).unwrap();
        assert_eq!(text, "here");
        runtime.shutdown_timeout(Duration::from_secs(1));
    }
}
