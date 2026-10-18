use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;

use super::mock_server::{MockServer, Reply};
use crate::Client;
use crate::middleware::{Anonymous, Bearer, MiddlewareResult, Token, TokenSource};

struct Counting {
    fetches: Arc<AtomicU32>,
    delay: Duration,
    lifetime: Option<Duration>,
}

impl Counting {
    fn new(lifetime: Option<Duration>) -> Self {
        Self {
            fetches: Arc::default(),
            delay: Duration::ZERO,
            lifetime,
        }
    }
}

#[async_trait::async_trait]
impl TokenSource for Counting {
    async fn fetch(&self) -> MiddlewareResult<Token> {
        tokio::time::sleep(self.delay).await;
        let number = self.fetches.fetch_add(1, Ordering::SeqCst) + 1;
        Ok(Token {
            value: format!("tok{number}"),
            expires_at: self.lifetime.map(|lifetime| SystemTime::now() + lifetime),
        })
    }
}

type Seen = Arc<Mutex<Vec<String>>>;

async fn recording_server(
    status: impl Fn(u32) -> u16 + Send + Sync + 'static,
) -> (MockServer, Seen) {
    let seen: Seen = Arc::default();
    let capture = seen.clone();
    let server = MockServer::start(move |hit, request| {
        capture.lock().unwrap().push(authorization_of(request));
        Reply::status(status(hit)).body("ok")
    })
    .await;
    (server, seen)
}

fn authorization_of(raw: &str) -> String {
    raw.lines()
        .find(|line| line.to_lowercase().starts_with("authorization:"))
        .and_then(|line| line.split_once(':'))
        .map(|(_, value)| value.trim().to_owned())
        .unwrap_or_default()
}

fn jwt_expiring_in(seconds: u64) -> String {
    let exp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + seconds;
    let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"HS256"}"#);
    let payload = URL_SAFE_NO_PAD.encode(format!(r#"{{"exp":{exp}}}"#));
    format!("{header}.{payload}.sig")
}

#[tokio::test]
async fn applies_the_bearer_token() {
    let (server, seen) = recording_server(|_| 200).await;
    let client = Client::builder()
        .middleware(Bearer::new(Counting::new(None)))
        .build()
        .unwrap();

    client.get(server.url()).text().await.unwrap();

    assert_eq!(seen.lock().unwrap()[0], "Bearer tok1");
}

#[tokio::test]
async fn refreshes_on_401_and_replays() {
    let (server, seen) = recording_server(|hit| if hit == 1 { 401 } else { 200 }).await;
    let source = Counting::new(None);
    let fetches = source.fetches.clone();
    let client = Client::builder()
        .middleware(Bearer::new(source))
        .build()
        .unwrap();

    assert_eq!(client.get(server.url()).text().await.unwrap(), "ok");

    assert_eq!(server.hits(), 2);
    assert_eq!(
        fetches.load(Ordering::SeqCst),
        2,
        "the first token, then the refresh"
    );
    assert_eq!(*seen.lock().unwrap(), ["Bearer tok1", "Bearer tok2"]);
}

#[tokio::test]
async fn caches_a_valid_token() {
    let (server, seen) = recording_server(|_| 200).await;
    let source = Counting::new(Some(Duration::from_secs(3600)));
    let fetches = source.fetches.clone();
    let client = Client::builder()
        .middleware(Bearer::new(source))
        .build()
        .unwrap();

    client.get(server.url()).text().await.unwrap();
    client.get(server.url()).text().await.unwrap();

    assert_eq!(
        fetches.load(Ordering::SeqCst),
        1,
        "a valid token is not re-fetched"
    );
    assert_eq!(*seen.lock().unwrap(), ["Bearer tok1", "Bearer tok1"]);
}

#[tokio::test]
async fn concurrent_requests_fetch_once() {
    let server = MockServer::start(|_, _| Reply::status(200).body("ok")).await;
    let mut source = Counting::new(Some(Duration::from_secs(3600)));
    source.delay = Duration::from_millis(40);
    let fetches = source.fetches.clone();
    let client = Client::builder()
        .middleware(Bearer::new(source))
        .build()
        .unwrap();

    let requests: Vec<_> = (0..6)
        .map(|_| {
            let client = client.clone();
            let url = server.url().to_owned();
            tokio::spawn(async move { client.get(url).text().await })
        })
        .collect();
    for request in requests {
        request.await.unwrap().unwrap();
    }

    assert_eq!(
        fetches.load(Ordering::SeqCst),
        1,
        "a herd triggers one fetch"
    );
    assert_eq!(server.hits(), 6);
}

#[tokio::test]
async fn manual_login_lifecycle() {
    let bearer = Bearer::new(Counting::new(Some(Duration::from_secs(3600))));
    assert!(!bearer.session_valid(), "no session before the first login");

    bearer.login().await.unwrap();
    assert!(bearer.session_valid(), "login captured a session");

    let expiring = Bearer::new(Counting::new(Some(Duration::from_secs(10))));
    expiring.login().await.unwrap();
    assert!(
        !expiring.session_valid(),
        "a token inside the refresh buffer counts as expired"
    );

    let opaque = Bearer::new(Counting::new(None));
    opaque.login().await.unwrap();
    assert!(opaque.session_valid(), "a token without expiry stays valid");
}

#[tokio::test]
async fn anonymous_requests_skip_the_token_and_the_refresh() {
    let (server, seen) = recording_server(|_| 401).await;
    let source = Counting::new(None);
    let fetches = source.fetches.clone();
    let client = Client::builder()
        .middleware(Bearer::new(source))
        .build()
        .unwrap();

    let response = client
        .get(server.url())
        .extension(Anonymous)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 401);
    assert_eq!(seen.lock().unwrap()[0], "", "no token on the wire");
    assert_eq!(fetches.load(Ordering::SeqCst), 0, "no fetch, no refresh");
    assert_eq!(server.hits(), 1, "no replay for an anonymous 401");
}

#[test]
fn jwt_expiry_is_read_from_the_claims() {
    let token = Token::jwt(jwt_expiring_in(3600));
    let expires_at = token.expires_at.expect("exp claim parsed");
    let remaining = expires_at.duration_since(SystemTime::now()).unwrap();
    assert!(remaining > Duration::from_secs(3590) && remaining <= Duration::from_secs(3600));

    assert!(Token::jwt("not.a.jwt".into()).expires_at.is_none());
}
