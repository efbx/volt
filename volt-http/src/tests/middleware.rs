use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::time::Instant;

use super::mock_server::{MockServer, Reply};
use crate::middleware::{Middleware, MiddlewareResult};
use crate::retry::RetryPolicy;
use crate::{Client, Error, Request, Response, StatusCode};

type Log = Arc<Mutex<Vec<String>>>;

struct Recorder {
    name: &'static str,
    log: Log,
}

#[async_trait::async_trait]
impl Middleware for Recorder {
    async fn before(&self, _request: &mut Request) -> MiddlewareResult<Option<Response>> {
        self.log
            .lock()
            .unwrap()
            .push(format!("before {}", self.name));
        Ok(None)
    }

    async fn after(&self, result: MiddlewareResult<Response>) -> MiddlewareResult<Response> {
        self.log
            .lock()
            .unwrap()
            .push(format!("after {}", self.name));
        result
    }
}

struct ShortCircuit;

#[async_trait::async_trait]
impl Middleware for ShortCircuit {
    async fn before(&self, _request: &mut Request) -> MiddlewareResult<Option<Response>> {
        let response = http::Response::builder()
            .status(StatusCode::OK)
            .body("short")
            .unwrap();
        Ok(Some(response.into()))
    }
}

struct Refuse;

#[async_trait::async_trait]
impl Middleware for Refuse {
    async fn before(&self, _request: &mut Request) -> MiddlewareResult<Option<Response>> {
        Err("refused".into())
    }
}

struct Veto;

#[async_trait::async_trait]
impl Middleware for Veto {
    async fn after(&self, result: MiddlewareResult<Response>) -> MiddlewareResult<Response> {
        result.and_then(|_| Err("vetoed".into()))
    }
}

fn recorder(name: &'static str, log: &Log) -> Recorder {
    Recorder {
        name,
        log: log.clone(),
    }
}

#[tokio::test]
async fn before_runs_in_order_and_after_in_reverse() {
    let server = MockServer::start(|_, _| Reply::status(200)).await;
    let log: Log = Arc::default();
    let client = Client::builder()
        .middleware(recorder("a", &log))
        .middleware(recorder("b", &log))
        .build()
        .unwrap();

    client.get(server.url()).send().await.unwrap();

    assert_eq!(
        *log.lock().unwrap(),
        ["before a", "before b", "after b", "after a"]
    );
    assert_eq!(server.hits(), 1);
}

#[tokio::test]
async fn before_can_short_circuit_the_transport() {
    let server = MockServer::start(|_, _| Reply::status(200).body("remote")).await;
    let client = Client::builder().middleware(ShortCircuit).build().unwrap();

    assert_eq!(client.get(server.url()).text().await.unwrap(), "short");
    assert_eq!(server.hits(), 0);
}

#[tokio::test]
async fn before_error_becomes_a_middleware_error() {
    let server = MockServer::start(|_, _| Reply::status(200)).await;
    let client = Client::builder().middleware(Refuse).build().unwrap();

    let error = client.get(server.url()).send().await.unwrap_err();
    assert!(matches!(error, Error::Middleware(_)), "got: {error}");
    assert_eq!(error.to_string(), "middleware: refused");
    assert_eq!(server.hits(), 0);
}

#[tokio::test]
async fn after_can_replace_the_result() {
    let server = MockServer::start(|_, _| Reply::status(200)).await;
    let client = Client::builder().middleware(Veto).build().unwrap();

    let error = client.get(server.url()).send().await.unwrap_err();
    assert_eq!(error.to_string(), "middleware: vetoed");
    assert_eq!(server.hits(), 1);
}

#[tokio::test]
async fn hooks_run_on_every_attempt() {
    let server = MockServer::start(|hit, _| match hit {
        1 => Reply::status(503),
        _ => Reply::status(200),
    })
    .await;
    let log: Log = Arc::default();
    let retry = RetryPolicy::new()
        .initial_wait(Duration::from_millis(1))
        .max_wait(Duration::from_millis(2));
    let client = Client::builder()
        .middleware(recorder("a", &log))
        .retry(retry)
        .build()
        .unwrap();

    client.get(server.url()).send().await.unwrap();

    assert_eq!(
        *log.lock().unwrap(),
        ["before a", "after a", "before a", "after a"]
    );
    assert_eq!(server.hits(), 2);
}

#[tokio::test]
async fn derive_inherits_hooks_until_cleared() {
    let server = MockServer::start(|_, _| Reply::status(200)).await;
    let log: Log = Arc::default();
    let base = Client::builder()
        .middleware(recorder("a", &log))
        .build()
        .unwrap();

    base.derive()
        .build()
        .get(server.url())
        .send()
        .await
        .unwrap();
    assert_eq!(log.lock().unwrap().len(), 2, "inherited by default");

    base.derive()
        .clear_middleware()
        .build()
        .get(server.url())
        .send()
        .await
        .unwrap();
    assert_eq!(
        log.lock().unwrap().len(),
        2,
        "cleared on the derived client"
    );

    base.get(server.url()).send().await.unwrap();
    assert_eq!(log.lock().unwrap().len(), 4, "base untouched");
}

struct Recoverer {
    name: &'static str,
    yes_until: u32,
    log: Log,
}

#[async_trait::async_trait]
impl Middleware for Recoverer {
    async fn recover(&self, _request: &Request, response: &Response) -> MiddlewareResult<bool> {
        let mut log = self.log.lock().unwrap();
        log.push(format!("recover {} {}", self.name, response.status()));
        let times_asked = log.iter().filter(|line| line.contains(self.name)).count();
        Ok(u32::try_from(times_asked).unwrap() <= self.yes_until)
    }
}

struct FailingRecoverer;

#[async_trait::async_trait]
impl Middleware for FailingRecoverer {
    async fn recover(&self, _request: &Request, _response: &Response) -> MiddlewareResult<bool> {
        Err("recovery broke".into())
    }
}

#[tokio::test]
async fn recover_replays_the_request_without_a_sleep() {
    let server = MockServer::start(|hit, _| match hit {
        1 => Reply::status(403),
        _ => Reply::status(200).body("cleared"),
    })
    .await;
    let log: Log = Arc::default();
    let client = Client::builder()
        .middleware(Recoverer {
            name: "solver",
            yes_until: 1,
            log: log.clone(),
        })
        .retry(RetryPolicy::none())
        .build()
        .unwrap();

    let start = Instant::now();
    assert_eq!(client.get(server.url()).text().await.unwrap(), "cleared");
    assert_eq!(server.hits(), 2);
    assert!(start.elapsed() < Duration::from_millis(200));
    assert_eq!(
        *log.lock().unwrap(),
        ["recover solver 403 Forbidden", "recover solver 200 OK"]
    );
}

#[tokio::test]
async fn recover_budget_is_separate_from_retries() {
    let server = MockServer::start(|_, _| Reply::status(403)).await;
    let log: Log = Arc::default();
    let client = Client::builder()
        .middleware(Recoverer {
            name: "solver",
            yes_until: u32::MAX,
            log: log.clone(),
        })
        .retry(RetryPolicy::none())
        .max_recoveries(2)
        .build()
        .unwrap();

    let response = client.get(server.url()).send().await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "the real response"
    );
    assert_eq!(server.hits(), 3, "the original plus two recoveries");
    assert_eq!(
        log.lock().unwrap().len(),
        2,
        "not asked once the budget is spent"
    );
}

#[tokio::test]
async fn every_middleware_gets_to_recover_before_one_replay() {
    let server = MockServer::start(|hit, _| match hit {
        1 => Reply::status(403),
        _ => Reply::status(200),
    })
    .await;
    let log: Log = Arc::default();
    let client = Client::builder()
        .middleware(Recoverer {
            name: "first",
            yes_until: 1,
            log: log.clone(),
        })
        .middleware(Recoverer {
            name: "second",
            yes_until: 1,
            log: log.clone(),
        })
        .build()
        .unwrap();

    client.get(server.url()).send_for_status().await.unwrap();

    assert_eq!(server.hits(), 2, "both acted, one replay");
    assert_eq!(
        *log.lock().unwrap(),
        [
            "recover first 403 Forbidden",
            "recover second 403 Forbidden",
            "recover first 200 OK",
            "recover second 200 OK",
        ]
    );
}

#[tokio::test]
async fn recover_error_becomes_a_middleware_error() {
    let server = MockServer::start(|_, _| Reply::status(403)).await;
    let client = Client::builder()
        .middleware(FailingRecoverer)
        .build()
        .unwrap();

    let error = client.get(server.url()).send().await.unwrap_err();
    assert_eq!(error.to_string(), "middleware: recovery broke");
    assert_eq!(server.hits(), 1);
}
