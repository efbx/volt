use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::time::Instant;

use super::mock_server::{MockServer, Reply};
use crate::Client;
use crate::retry::RetryPolicy;

#[tokio::test]
async fn plain_get_returns_body() {
    let server = MockServer::start(|_, _| Reply::status(200).body("hello")).await;
    let client = Client::builder().build().unwrap();
    assert_eq!(client.get(server.url()).text().await.unwrap(), "hello");
}

#[tokio::test]
async fn header_appends_to_previous_value() {
    let seen = Arc::new(Mutex::new(String::new()));
    let capture = seen.clone();
    let server = MockServer::start(move |_, request| {
        *capture.lock().unwrap() = request.to_owned();
        Reply::status(200)
    })
    .await;

    let client = Client::builder().build().unwrap();
    client
        .get(server.url())
        .header("Accept", "text/html")
        .header("Accept", "image/png")
        .send()
        .await
        .unwrap();

    let raw = seen.lock().unwrap().clone();
    let accepts: Vec<&str> = raw
        .lines()
        .filter(|line| line.to_lowercase().starts_with("accept:"))
        .collect();
    assert_eq!(accepts.len(), 2, "both accept headers, got: {accepts:?}");
    assert!(accepts[0].contains("text/html"), "first set comes first");
    assert!(accepts[1].contains("image/png"), "second set comes second");
}

#[tokio::test]
async fn error_on_status_4xx() {
    let server = MockServer::start(|_, _| Reply::status(404).body("missing")).await;
    let client = Client::builder()
        .retry(RetryPolicy::none())
        .build()
        .unwrap();
    assert!(client.get(server.url()).text().await.is_err());
    assert_eq!(server.hits(), 1);
}

#[tokio::test]
async fn timeout_produces_error() {
    let server = MockServer::silent().await;
    let client = Client::builder()
        .retry(RetryPolicy::none())
        .timeout(Duration::from_millis(150))
        .build()
        .unwrap();
    let start = Instant::now();
    assert!(client.get(server.url()).send().await.is_err());
    assert!(start.elapsed() < Duration::from_secs(2));
}

#[tokio::test]
async fn derive_overrides_the_policy_without_touching_the_base() {
    let server = MockServer::start(|_, _| Reply::status(503)).await;
    let retry = RetryPolicy::new()
        .max_retries(1)
        .initial_wait(Duration::from_millis(1))
        .max_wait(Duration::from_millis(2));
    let base = Client::builder().retry(retry).build().unwrap();
    let derived = base.derive().retry(RetryPolicy::none()).build();

    derived.get(server.url()).send().await.unwrap();
    assert_eq!(server.hits(), 1, "the derived client does not retry");

    base.get(server.url()).send().await.unwrap();
    assert_eq!(server.hits(), 3, "the base client still retries once");
}
