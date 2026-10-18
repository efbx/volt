# volt-creds

[![CI](https://github.com/efbx/volt/actions/workflows/ci.yml/badge.svg)](https://github.com/efbx/volt/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](../LICENSE)

Site credentials behind one small interface. A program asks a `Manager` for the identity of a label such as `"Pygmalion"`; where that identity is stored, the environment or the operating system keyring, is the provider's business.

## Features

- **`Identity`**: a user and a secret. **`IdentityPayload`**: the optional pair used when writing one half; each half borrows its text or owns it, so writes copy nothing and an owned payload can be kept as `IdentityPayload<'static>`.
- **`IdentityReader`, `IdentityWriter`, `IdentityManager`**: the traits a consumer depends on. `present()` tells whether a label has credentials without reading them.
- **`Manager`**: implements all three for one label over any `IdentityProvider`, so a fetcher receives an `Arc<dyn IdentityReader>` and never sees the storage.
- **`EnvProvider`**: reads `<LABEL>_USERNAME` and `<LABEL>_PASSWORD`, upper-cased, from the environment. Writing sets or removes the variables for the process.
- **`KeyProvider`**: keeps every label of one application in a single entry of the OS keyring, as a map of label to user and secret, through the `keyring` crate's native store.
- **`Cached`**: wraps any provider so each label is read from it once and served from memory afterwards. Writes and removals go to the wrapped provider first and update memory only when they succeed.
- **`Mode`**: picks a provider by name and builds it for an application.
- **`CredError`**: one error type over missing variables, keyring failures and malformed entries.

## Installation

```toml
[dependencies]
volt-creds = { git = "https://github.com/efbx/volt", tag = "v0.5.0" }
```

Requires Rust 1.95 or newer. The keyring tests touch the real OS keyring and only run with the `keyring-tests` feature. This crate is the one member of the workspace that allows `unsafe` code, for the two standard-library calls that set and remove environment variables.

## Example

```rust
use std::sync::Arc;

use volt_creds::{EnvProvider, IdentityReader, Manager};

let reader: Arc<dyn IdentityReader> = Arc::new(Manager::new(Arc::new(EnvProvider::new()), "Pygmalion"));

if reader.present() {
    let identity = reader.get().unwrap();
    println!("logging in as {}", identity.user);
}
```

## License

[MIT](../LICENSE)
