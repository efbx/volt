use std::sync::Arc;

use volt_creds::{EnvProvider, IdentityReader, Manager};

#[test]
fn readme_example_holds() {
    let reader: Arc<dyn IdentityReader> = Arc::new(Manager::new(
        Arc::new(EnvProvider::new()),
        "VoltReadmeExampleNeverSet",
    ));

    assert!(!reader.present());
    assert!(reader.get().is_err());
}
