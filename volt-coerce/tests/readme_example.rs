use volt_coerce::coerce;

#[coerce]
#[derive(Debug, Default)]
#[serde(default)]
struct Card {
    name: String,
    tags: Vec<String>,
    enabled: bool,
    #[serde(rename = "createdAt")]
    created_at: Option<i64>,
}

#[test]
fn readme_example_holds() {
    let card: Card = serde_json::from_str(
        r#"{"name": 42, "tags": "solo", "enabled": "true", "createdAt": "junk"}"#,
    )
    .unwrap();

    assert_eq!(card.name, "42");
    assert_eq!(card.tags, vec!["solo"]);
    assert!(card.enabled);
    assert_eq!(card.created_at, None);
}
