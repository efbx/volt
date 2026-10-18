//! File-system helpers for programs that write user-named files.
//!
//! [`FileNameExt::to_file_name`] turns arbitrary text into a safe single path component,
//! and [`PathExt`] adds [`next_available`](PathExt::next_available) to find a numbered
//! path that does not clash with what exists, [`copy_to`](PathExt::copy_to) to copy a
//! file and [`exists_as_file`](PathExt::exists_as_file) to tell files from directories.
//! [`Entry`], [`FileType`], [`FILE_PERMISSION`] and [`DIRECTORY_PERMISSION`] name the
//! entry kinds, file kinds and Unix modes.

#![warn(missing_docs)]

mod next_available;
mod path_ext;
mod sanitize;
#[cfg(test)]
mod test_dir;
mod types;

pub use path_ext::PathExt;
pub use sanitize::FileNameExt;
pub use types::{DIRECTORY_PERMISSION, Entry, FILE_PERMISSION, FileType};
