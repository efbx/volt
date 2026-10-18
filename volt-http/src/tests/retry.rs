use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, SystemTime};

use tokio::fs::File;
use tokio::time::Instant;

use super::mock_server::{MockServer, Reply};
use crate::middleware::{Middleware, MiddlewareResult};
use crate::retry::{RetryContext, RetryPolicy};
use crate::{Client, Method, Request, Response, StatusCode};

#[tokio::test]
async fn retry_transient_5xx_then_succeeds() {
    let server = MockServer::start(|hit, _| match hit {
        1 => Reply::status(500),
        _ => Reply::status(200).body("ok"),
    })
    .await;
    let retry = RetryPolicy::new()
        .initial_wait(Duration::from_millis(1))
        .max_wait(Duration::from_millis(5));
    let client = Client::builder().retry(retry).build().unwrap();
    assert_eq!(client.get(server.url()).text().await.unwrap(), "ok");
    assert_eq!(server.hits(), 2);
}

#[tokio::test]
async fn retry_none_returns_the_5xx_as_a_response() {
    let server = MockServer::start(|_, _| Reply::status(503)).await;
    let client = Client::builder()
        .retry(RetryPolicy::none())
        .build()
        .unwrap();
    let response = client.get(server.url()).send().await.unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(server.hits(), 1);
}

#[tokio::test]
async fn retry_after_header_is_honored() {
    let server = MockServer::start(|hit, _| match hit {
        1 => Reply::status(429).header("Retry-After", "1"),
        _ => Reply::status(200).body("ok"),
    })
    .await;
    let client = Client::builder().build().unwrap();
    let start = Instant::now();
    assert_eq!(client.get(server.url()).text().await.unwrap(), "ok");
    assert_eq!(server.hits(), 2);
    assert!(
        start.elapsed() >= Duration::from_secs(1),
        "client must wait the Retry-After duration"
    );
}

#[tokio::test]
async fn exhausted_429_is_returned_as_a_response() {
    let server = MockServer::start(|_, _| Reply::status(429).header("Retry-After", "1")).await;
    let client = Client::builder()
        .retry(RetryPolicy::new().max_retries(1))
        .build()
        .unwrap();

    let response = client.get(server.url()).send().await.unwrap();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(
        response.headers()["retry-after"],
        "1",
        "the server's wait travels in the response"
    );
    assert_eq!(server.hits(), 2, "one retry, then the 429 is handed back");
}

#[tokio::test]
async fn retry_after_beyond_max_wait_fails_fast() {
    let server = MockServer::start(|_, _| Reply::status(429).header("Retry-After", "1")).await;
    let retry = RetryPolicy::new()
        .max_retries(3)
        .initial_wait(Duration::from_millis(10))
        .max_wait(Duration::from_millis(20));
    let client = Client::builder().retry(retry).build().unwrap();

    let start = Instant::now();
    let response = client.get(server.url()).send().await.unwrap();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(
        server.hits(),
        1,
        "a wait beyond max_wait is not retried into"
    );
    assert!(
        start.elapsed() < Duration::from_millis(500),
        "no sleep before giving up"
    );
}

#[tokio::test]
async fn request_level_policy_overrides_the_client() {
    let server = MockServer::start(|_, _| Reply::status(503)).await;
    let retry = RetryPolicy::new()
        .max_retries(2)
        .initial_wait(Duration::from_millis(1))
        .max_wait(Duration::from_millis(2));
    let client = Client::builder().retry(retry).build().unwrap();

    client
        .get(server.url())
        .retry(RetryPolicy::none())
        .send()
        .await
        .unwrap();
    assert_eq!(server.hits(), 1, "the request said never");

    client.get(server.url()).send().await.unwrap();
    assert_eq!(server.hits(), 4, "the client policy still retries twice");
}

fn always(_context: &RetryContext<'_>) -> bool {
    true
}

#[test]
fn backoff_doubles_and_jitters_between_half_and_full() {
    let policy = RetryPolicy::new()
        .initial_wait(Duration::from_millis(100))
        .max_wait(Duration::from_secs(1))
        .classifier(always);
    let request = Request::new(Method::GET, "http://example/".parse().unwrap());
    let context = |attempt| RetryContext {
        attempt,
        request: &request,
        response: None,
        error: None,
    };

    for (attempt, full) in [(1, 100), (2, 200), (3, 400), (4, 800), (5, 1000)] {
        let full = Duration::from_millis(full);
        for _ in 0..50 {
            let delay = policy.delay(&context(attempt)).unwrap();
            assert!(
                full / 2 <= delay && delay <= full,
                "attempt {attempt}: {delay:?} outside {:?}..={full:?}",
                full / 2
            );
        }
    }
    assert!(policy.delay(&context(6)).is_none(), "budget exhausted");
}

#[tokio::test]
async fn retry_after_http_date_is_honored() {
    let server = MockServer::start(|hit, _| match hit {
        1 => {
            let deadline = SystemTime::now() + Duration::from_secs(2);
            Reply::status(429).header("Retry-After", &httpdate::fmt_http_date(deadline))
        }
        _ => Reply::status(200).body("ok"),
    })
    .await;
    let client = Client::builder().build().unwrap();

    let start = Instant::now();
    assert_eq!(client.get(server.url()).text().await.unwrap(), "ok");
    assert_eq!(server.hits(), 2);
    assert!(
        start.elapsed() >= Duration::from_millis(900),
        "waited until the date, got {:?}",
        start.elapsed()
    );
}

#[tokio::test]
async fn retry_after_date_in_the_past_retries_immediately() {
    let server = MockServer::start(|hit, _| match hit {
        1 => Reply::status(429).header("Retry-After", "Thu, 01 Jan 2015 00:00:00 GMT"),
        _ => Reply::status(200).body("ok"),
    })
    .await;
    let client = Client::builder().build().unwrap();

    let start = Instant::now();
    assert_eq!(client.get(server.url()).text().await.unwrap(), "ok");
    assert_eq!(server.hits(), 2);
    assert!(start.elapsed() < Duration::from_millis(500));
}

struct Fixed {
    status: StatusCode,
    calls: Arc<AtomicU32>,
}

#[async_trait::async_trait]
impl Middleware for Fixed {
    async fn before(&self, _request: &mut Request) -> MiddlewareResult<Option<Response>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let response = http::Response::builder()
            .status(self.status)
            .body("")
            .unwrap();
        Ok(Some(response.into()))
    }
}

#[tokio::test]
async fn a_streaming_body_is_sent_once_and_never_replayed() {
    let calls = Arc::new(AtomicU32::new(0));
    let retry = RetryPolicy::new()
        .max_retries(2)
        .initial_wait(Duration::from_millis(1))
        .max_wait(Duration::from_millis(2));
    let client = Client::builder()
        .middleware(Fixed {
            status: StatusCode::SERVICE_UNAVAILABLE,
            calls: calls.clone(),
        })
        .retry(retry)
        .build()
        .unwrap();
    let manifest = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");

    let stream = File::open(manifest).await.unwrap();
    let response = client
        .post("http://unused/")
        .body(stream)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(calls.load(Ordering::SeqCst), 1, "no clone, no replay");

    client
        .post("http://unused/")
        .body("bytes")
        .send()
        .await
        .unwrap();
    assert_eq!(
        calls.load(Ordering::SeqCst),
        4,
        "a cloneable body is retried"
    );
}
