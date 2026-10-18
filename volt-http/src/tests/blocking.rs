use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use serde_json::Value;
use tokio::runtime::{Builder, Runtime};

use super::mock_server::{MockServer, Reply};
use crate::Client;
use crate::middleware::{Bearer, MiddlewareResult, Token, TokenSource};

fn side_runtime() -> &'static Runtime {
    static SIDE: OnceLock<Runtime> = OnceLock::new();
    SIDE.get_or_init(|| {
        Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap()
    })
}

#[tokio::test]
async fn sync_calls_work_from_a_blocking_pool_thread_of_a_current_thread_runtime() {
    let server = MockServer::start(|_, _| Reply::status(200).body("hello")).await;
    let client = Client::builder().build().unwrap();
    let url = server.url().to_owned();

    let body = tokio::task::spawn_blocking(move || client.get(url).text_sync())
        .await
        .unwrap()
        .unwrap();

    assert_eq!(body, "hello");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sync_calls_work_from_inside_an_async_task() {
    let server = MockServer::start(|_, _| Reply::status(200).body(r#"{"n":7}"#)).await;
    let client = Client::builder().build().unwrap();

    let response = client.get(server.url()).send_sync().unwrap();
    assert_eq!(response.status(), 200);

    let text = client.get(server.url()).text_sync().unwrap();
    assert_eq!(text, r#"{"n":7}"#);

    let bytes = client.get(server.url()).bytes_sync().unwrap();
    assert_eq!(&bytes[..], br#"{"n":7}"#);

    let json: Value = client.get(server.url()).json_sync().unwrap();
    assert_eq!(json["n"], 7);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn parallel_sync_calls_are_not_funnelled() {
    let server = MockServer::start(|_, _| {
        Reply::status(200)
            .body("ok")
            .delay(Duration::from_millis(200))
    })
    .await;
    let client = Client::builder().build().unwrap();

    let start = Instant::now();
    let workers: Vec<_> = (0..8)
        .map(|_| {
            let client = client.clone();
            let url = server.url().to_owned();
            std::thread::spawn(move || client.get(url).text_sync().unwrap())
        })
        .collect();
    for worker in workers {
        assert_eq!(worker.join().unwrap(), "ok");
    }

    assert_eq!(server.hits(), 8);
    assert!(
        start.elapsed() < Duration::from_millis(600),
        "eight 200 ms requests ran side by side, got {:?}",
        start.elapsed()
    );
}

#[test]
fn sync_calls_from_worker_tasks_hand_the_worker_off() {
    let server = side_runtime().block_on(MockServer::start(|_, _| {
        Reply::status(200)
            .body("ok")
            .delay(Duration::from_millis(250))
    }));
    let app = Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();

    let start = Instant::now();
    app.block_on(async {
        let client = Client::builder().build().unwrap();
        let tasks: Vec<_> = (0..4)
            .map(|_| {
                let client = client.clone();
                let url = server.url().to_owned();
                tokio::spawn(async move { client.get(url).text_sync().unwrap() })
            })
            .collect();
        for task in tasks {
            assert_eq!(task.await.unwrap(), "ok");
        }
    });

    assert_eq!(server.hits(), 4);
    assert!(
        start.elapsed() < Duration::from_millis(450),
        "four 250 ms requests on two workers ran side by side, got {:?}",
        start.elapsed()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sync_calls_work_from_a_blocking_pool_thread_of_a_multi_thread_runtime() {
    let server = MockServer::start(|_, _| Reply::status(200).body("hello")).await;
    let client = Client::builder().build().unwrap();
    let url = server.url().to_owned();

    let body = tokio::task::spawn_blocking(move || client.get(url).text_sync())
        .await
        .unwrap()
        .unwrap();

    assert_eq!(body, "hello");
}

#[test]
fn sync_calls_work_from_a_thread_that_entered_the_runtime() {
    let app = Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let server = app.block_on(MockServer::start(|_, _| Reply::status(200).body("hello")));
    let client = Client::builder().build().unwrap();
    let _guard = app.enter();

    let body = client.get(server.url()).text_sync().unwrap();

    assert_eq!(body, "hello");
}

struct Slow(Arc<AtomicU32>);

#[async_trait::async_trait]
impl TokenSource for Slow {
    async fn fetch(&self) -> MiddlewareResult<Token> {
        tokio::time::sleep(Duration::from_millis(20)).await;
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(Token {
            value: "tok".into(),
            expires_at: None,
        })
    }
}

#[test]
fn login_sync_works_from_a_plain_thread() {
    let fetches = Arc::new(AtomicU32::new(0));
    let bearer = Bearer::new(Slow(fetches.clone()));

    bearer.login_sync().unwrap();

    assert!(bearer.session_valid());
    assert_eq!(fetches.load(Ordering::SeqCst), 1);
}
