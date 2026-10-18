# volt-coerce-macros

[![CI](https://github.com/efbx/volt/actions/workflows/ci.yml/badge.svg)](https://github.com/efbx/volt/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](../LICENSE)

The `#[coerce]` attribute macro for [volt-coerce](../volt-coerce).

Use it through `volt-coerce`, which re-exports it. This crate is not meant to be a direct dependency, and the code it generates refers to `::volt_coerce`, so that crate must be in scope under its own name. See the `volt-coerce` README for the rules and the attribute options.

## License

[MIT](../LICENSE)
