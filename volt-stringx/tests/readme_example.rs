use volt_stringx::{StrExt, StringExt, StrsExt};

#[test]
fn readme_example_holds() {
    let title = "  “Quoted” title  ";
    assert!(title.is_not_blank());
    assert_eq!(title.normalize_symbols(), "\"Quoted\" title");
    assert_eq!("Slice-of-Life 2".alphanumeric_lower(), "sliceoflife2");
    assert_eq!("a+b%20c".query_unescape().as_deref(), Some("a b c"));

    let mut name = String::from("  Ada  ");
    name.trim_in_place();
    name.set_if_not_blank(String::from("   "));
    assert_eq!(name, "Ada");

    let parts = ["Tagline", "", "   ", "Notes"];
    assert_eq!(parts.join_non_blank("\n\n"), "Tagline\n\nNotes");
    assert_eq!(parts.join_where(", ", |part| part.len() > 5), "Tagline");
}
