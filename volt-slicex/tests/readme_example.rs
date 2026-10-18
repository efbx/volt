use volt_slicex::DedupStable;

#[test]
fn readme_example_holds() {
    let mut greetings = vec!["Hi", "Hello", "Hi", "Hey", "Hello"];
    greetings.unique_stable();
    assert_eq!(greetings, ["Hi", "Hello", "Hey"]);

    let merged = vec!["Hi", "Hello"].dedup_stable(vec!["Hey", "Hi", "Yo"]);
    assert_eq!(merged, ["Hi", "Hello", "Hey", "Yo"]);
}
