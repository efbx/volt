use std::time::Duration;

use tokio::time::Instant;

use super::mock_server::{MockServer, Reply};
use crate::Client;
use crate::middleware::RateGate;

fn quiet_gate() -> RateGate {
    RateGate::new()
        .wake_jitter(Duration::ZERO)
        .pacing(Duration::ZERO, Duration::ZERO)
}

#[tokio::test]
async fn pauses_every_request_after_a_429() {
    let server = MockServer::start(|hit, _| match hit {
        1 => Reply::status(429).header("Retry-After", "1"),
        _ => Reply::status(200).body("ok"),
    })
    .await;
    let client = Client::builder()
        .middleware(RateGate::new().wake_jitter(Duration::ZERO))
        .build()
        .unwrap();

    let start = Instant::now();
    assert_eq!(client.get(server.url()).text().await.unwrap(), "ok");
    assert!(
        start.elapsed() >= Duration::from_secs(1),
        "the tripped gate holds the replay until the window opens"
    );
    assert_eq!(server.hits(), 2);

    let quick = Instant::now();
    assert_eq!(client.get(server.url()).text().await.unwrap(), "ok");
    assert!(
        quick.elapsed() < Duration::from_millis(500),
        "an open gate costs nothing"
    );
}

#[tokio::test]
async fn paces_after_a_trip() {
    let gate = RateGate::new()
        .wake_jitter(Duration::ZERO)
        .pacing(Duration::from_millis(200), Duration::from_millis(800))
        .calm_after(Duration::from_secs(30));

    assert_eq!(
        gate.spacing(),
        Duration::ZERO,
        "an untripped gate never paces"
    );

    gate.trip(Duration::ZERO);
    assert_eq!(
        gate.spacing(),
        Duration::from_millis(200),
        "first trip opens a gap"
    );
    gate.trip(Duration::ZERO);
    assert_eq!(
        gate.spacing(),
        Duration::from_millis(400),
        "repeat trips widen it"
    );
    gate.trip(Duration::ZERO);
    gate.trip(Duration::ZERO);
    assert_eq!(
        gate.spacing(),
        Duration::from_millis(800),
        "widening stops at the cap"
    );

    let start = Instant::now();
    gate.pass().await;
    gate.pass().await;
    gate.pass().await;
    assert!(
        start.elapsed() >= Duration::from_millis(1500),
        "callers are spaced by the gap instead of released together, got {:?}",
        start.elapsed()
    );
}

#[tokio::test]
async fn holds_every_concurrent_waiter_until_the_window_opens() {
    let gate = quiet_gate();
    gate.trip(Duration::from_millis(600));

    let start = Instant::now();
    let waiters: Vec<_> = (0..8)
        .map(|_| {
            let gate = gate.clone();
            tokio::spawn(async move {
                gate.pass().await;
                start.elapsed()
            })
        })
        .collect();

    for waiter in waiters {
        let waited = waiter.await.unwrap();
        assert!(
            waited >= Duration::from_millis(550),
            "every concurrent caller serves the whole window, got {waited:?}"
        );
    }
}

#[tokio::test]
async fn waiters_honor_a_deadline_that_moves_while_they_sleep() {
    let gate = quiet_gate();
    gate.trip(Duration::from_millis(300));

    let start = Instant::now();
    let waiters: Vec<_> = (0..4)
        .map(|_| {
            let gate = gate.clone();
            tokio::spawn(async move {
                gate.pass().await;
                start.elapsed()
            })
        })
        .collect();

    tokio::time::sleep(Duration::from_millis(150)).await;
    gate.trip(Duration::from_millis(450));

    for waiter in waiters {
        let waited = waiter.await.unwrap();
        assert!(
            waited >= Duration::from_millis(550),
            "a sleeping waiter re-reads the extended deadline, got {waited:?}"
        );
    }
}

#[tokio::test]
async fn replays_after_a_closure_leave_one_gap_apart() {
    let gate = RateGate::new()
        .wake_jitter(Duration::ZERO)
        .pacing(Duration::from_millis(100), Duration::from_millis(100));
    gate.trip(Duration::from_millis(200));

    let start = Instant::now();
    let waiters: Vec<_> = (0..4)
        .map(|_| {
            let gate = gate.clone();
            tokio::spawn(async move {
                gate.pass().await;
                start.elapsed()
            })
        })
        .collect();

    let mut releases = Vec::new();
    for waiter in waiters {
        releases.push(waiter.await.unwrap());
    }
    releases.sort();

    assert!(
        releases[0] >= Duration::from_millis(200),
        "nobody leaves before the window opens"
    );
    for pair in releases.windows(2) {
        assert!(
            pair[1].checked_sub(pair[0]).unwrap() >= Duration::from_millis(60),
            "replays are spaced, not released together: {releases:?}"
        );
    }
}

#[tokio::test]
async fn relaxes_after_a_calm_spell() {
    let gate = RateGate::new()
        .wake_jitter(Duration::ZERO)
        .pacing(Duration::from_millis(50), Duration::from_millis(50))
        .calm_after(Duration::from_millis(200));

    gate.trip(Duration::ZERO);
    assert_eq!(gate.spacing(), Duration::from_millis(50));

    tokio::time::sleep(Duration::from_millis(260)).await;
    let start = Instant::now();
    gate.pass().await;
    gate.pass().await;
    assert_eq!(
        gate.spacing(),
        Duration::ZERO,
        "quiet source returns to full speed"
    );
    assert!(
        start.elapsed() < Duration::from_millis(50),
        "and stops costing anything"
    );
}

#[tokio::test]
async fn fixed_gap_spaces_requests() {
    let server = MockServer::start(|_, _| Reply::status(200).body("ok")).await;
    let client = Client::builder()
        .middleware(quiet_gate().gap(Duration::from_millis(120)))
        .build()
        .unwrap();

    let start = Instant::now();
    client.get(server.url()).text().await.unwrap();
    client.get(server.url()).text().await.unwrap();
    assert!(
        start.elapsed() >= Duration::from_millis(100),
        "the second request waits out the gap, got {:?}",
        start.elapsed()
    );
}
