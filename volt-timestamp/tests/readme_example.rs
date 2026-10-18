use volt_timestamp::{Nano, Seconds, Timestamp};

#[test]
fn readme_example_holds() {
    let created = Nano::from_rfc3339("2026-01-28T17:50:00.000Z").unwrap();
    assert_eq!(created, Nano(1_769_622_600_000_000_000));

    let seconds: Seconds = created.into();
    assert_eq!(seconds, Seconds(1_769_622_600));
    assert_eq!(seconds.format_utc("%Y-%m-%d"), "2026-01-28");

    let from_api: Seconds = serde_json::from_str("1706328712").unwrap();
    assert_eq!(Nano::from(from_api), Nano(1_706_328_712_000_000_000));
}
