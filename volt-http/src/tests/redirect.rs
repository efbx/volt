use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::mock_server::{MockServer, Reply};
use crate::redirect::{Classifier, RedirectPolicy};
use crate::retry::RetryPolicy;
use crate::{Client, Error, StatusCode};

type Seen = Arc<Mutex<Vec<String>>>;

async fn redirecting_server(status: u16, location: &str) -> (MockServer, Seen) {
    let seen: Seen = Arc::default();
    let capture = seen.clone();
    let location = location.to_owned();
    let server = MockServer::start(move |hit, request| {
        capture.lock().unwrap().push(request.to_owned());
        match hit {
            1 => Reply::status(status).header("Location", &location),
            _ => Reply::status(200).body("landed"),
        }
    })
    .await;
    (server, seen)
}

#[tokio::test]
async fn follows_302_as_get_without_body() {
    let (server, seen) = redirecting_server(302, "/next").await;
    let client = Client::builder().build().unwrap();

    let body = client
        .post(server.url())
        .body("payload")
        .text()
        .await
        .unwrap();

    assert_eq!(body, "landed");
    let second = seen.lock().unwrap()[1].clone();
    assert!(second.starts_with("GET /next "), "got: {second}");
    assert!(!second.contains("payload"), "body dropped");
}

#[tokio::test]
async fn follows_307_with_method_and_body() {
    let (server, seen) = redirecting_server(307, "/next").await;
    let client = Client::builder().build().unwrap();

    client
        .post(server.url())
        .body("payload")
        .send_for_status()
        .await
        .unwrap();

    let second = seen.lock().unwrap()[1].clone();
    assert!(second.starts_with("POST /next "), "got: {second}");
    assert!(second.ends_with("payload"), "body replayed");
}

#[tokio::test]
async fn redirect_none_returns_the_3xx() {
    let (server, _) = redirecting_server(302, "/next").await;
    let client = Client::builder()
        .redirect(RedirectPolicy::none())
        .build()
        .unwrap();

    let response = client.get(server.url()).send().await.unwrap();
    assert_eq!(response.status(), StatusCode::FOUND);
    assert_eq!(response.headers()["location"], "/next");
    assert_eq!(server.hits(), 1);
}

#[tokio::test]
async fn too_many_redirects_errors() {
    let server = MockServer::start(|_, _| Reply::status(302).header("Location", "/loop")).await;
    let client = Client::builder()
        .redirect(RedirectPolicy::new().max_redirects(3))
        .build()
        .unwrap();

    let error = client.get(server.url()).send().await.unwrap_err();
    assert!(matches!(error, Error::Redirect(_)), "got: {error}");
    assert_eq!(server.hits(), 4, "the original plus three followed hops");
}

#[tokio::test]
async fn same_host_stops_a_cross_host_hop() {
    let other = MockServer::start(|_, _| Reply::status(200).body("elsewhere")).await;
    let other_host = other.url().replace("127.0.0.1", "localhost");
    let (server, _) = redirecting_server(302, &other_host).await;
    let client = Client::builder()
        .redirect(RedirectPolicy::new().classifier(Classifier::same_host))
        .build()
        .unwrap();

    let response = client.get(server.url()).send().await.unwrap();
    assert_eq!(response.status(), StatusCode::FOUND);
    assert_eq!(other.hits(), 0);
}

#[tokio::test]
async fn credentials_survive_a_same_origin_hop() {
    let (server, seen) = redirecting_server(302, "/next").await;
    let client = Client::builder().build().unwrap();

    client
        .get(server.url())
        .header("Authorization", "Bearer secret")
        .send_for_status()
        .await
        .unwrap();

    let second = seen.lock().unwrap()[1].to_lowercase();
    assert!(second.contains("authorization: bearer secret"));
}

#[tokio::test]
async fn credentials_are_stripped_on_an_origin_change() {
    let other_seen: Seen = Arc::default();
    let capture = other_seen.clone();
    let other = MockServer::start(move |_, request| {
        capture.lock().unwrap().push(request.to_owned());
        Reply::status(200).body("elsewhere")
    })
    .await;
    let (server, _) = redirecting_server(302, other.url()).await;
    let client = Client::builder().build().unwrap();

    let body = client
        .get(server.url())
        .header("Authorization", "Bearer secret")
        .text()
        .await
        .unwrap();

    assert_eq!(body, "elsewhere");
    let landed = other_seen.lock().unwrap()[0].to_lowercase();
    assert!(!landed.contains("authorization"), "got: {landed}");
}

#[tokio::test]
async fn a_hop_does_not_consume_the_retry_budget() {
    let server = MockServer::start(|hit, _| match hit {
        1 => Reply::status(302).header("Location", "/next"),
        2 => Reply::status(503),
        _ => Reply::status(200).body("landed"),
    })
    .await;
    let retry = RetryPolicy::new()
        .max_retries(1)
        .initial_wait(Duration::from_millis(1))
        .max_wait(Duration::from_millis(2));
    let client = Client::builder().retry(retry).build().unwrap();

    assert_eq!(client.get(server.url()).text().await.unwrap(), "landed");
    assert_eq!(server.hits(), 3, "hop, then the one retry, then success");
}

#[tokio::test]
async fn head_stays_head_on_303() {
    let (server, seen) = redirecting_server(303, "/next").await;
    let client = Client::builder().build().unwrap();

    client.head(server.url()).send_for_status().await.unwrap();

    let second = seen.lock().unwrap()[1].clone();
    assert!(second.starts_with("HEAD /next "), "got: {second}");
}
