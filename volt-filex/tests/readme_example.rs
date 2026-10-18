use std::path::Path;

use volt_filex::{FileNameExt, PathExt};

#[test]
fn readme_example_holds() {
    let name = "Odessa Vale: Route/Book?".to_file_name();
    assert!(!name.contains('/') && !name.contains('?') && !name.contains(':'));
    let target = Path::new("exports").join(format!("{name}.png"));
    let free = target.next_available(Some(".png"));
    assert!(!free.exists_as_file());
}
