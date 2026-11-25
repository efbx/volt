use std::sync::{Arc, Mutex};
use std::time::Duration;

use volt_chrome::Chrome;
use wreq::cookie::Jar;

use super::mock_server::{MockServer, Reply};
use crate::middleware::BrowserBuilder;
use crate::{Client, StatusCode};

/// The page the browser is sent to, which hands out the cookie that lets a request through.
const SOLVED: &str = "<title>Solved</title>";

/// A site that refuses every request without its cookie, and gives the cookie, hidden from
/// scripts, to whoever opens `/solve`.
struct Gated;

impl Gated {
    /// Returns the answer to `request`, which is also added to `seen`.
    fn answer(seen: &Mutex<Vec<String>>, request: &str) -> Reply {
        seen.lock().unwrap().push(request.to_owned());
        if request.starts_with("GET /solve") {
            Reply::status(200)
                .header("Content-Type", "text/html")
                .header("Set-Cookie", "pass=1; HttpOnly; Path=/")
                .body(SOLVED)
        } else if request.contains("pass=1") {
            Reply::status(200).body("through")
        } else {
            Reply::status(403)
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "launches Chrome"]
async fn a_refused_request_is_solved_in_the_browser_and_sent_again() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    let server = MockServer::start(move |_, request| Gated::answer(&log, request)).await;
    let jar = Arc::new(Jar::default());
    let browser = BrowserBuilder::new(jar.clone(), |settings| Chrome::default().launch(settings))
        .recovery_url(format!("{}/solve", server.url()))
        .headed(false)
        .solve_timeout(Duration::from_secs(60))
        .build();
    let client = Client::builder()
        .cookie_jar(jar)
        .middleware(browser)
        .build()
        .unwrap();

    let response = client
        .get(format!("{}/data", server.url()))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let seen = seen.lock().unwrap();
    let last = seen.last().unwrap().to_ascii_lowercase();
    assert!(last.starts_with("get /data"), "{last}");
    assert!(last.contains("cookie: pass=1"), "{last}");
    assert!(last.contains("user-agent: mozilla/5.0"), "{last}");
}
