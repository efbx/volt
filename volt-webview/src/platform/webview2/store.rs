use std::cell::RefCell;
use std::env;
use std::ffi::OsString;
use std::fmt::Write as _;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};

use wry::WebContext;

/// The start of the data folder's name: this crate and its version.
const DATA_FOLDER: &str = concat!(env!("CARGO_PKG_NAME"), "-", env!("CARGO_PKG_VERSION"));

/// A session's webview store on Windows: the `WebView2` data folder its pages use, in the temp
/// directory rather than next to the program, which may sit in a folder it cannot write to.
pub(crate) struct Store {
    /// The data folder, lent to wry while a page's webview is built.
    pub(super) context: RefCell<WebContext>,
}

impl Store {
    /// Returns the store of the running program's data folder.
    pub(crate) fn new() -> Self {
        Self {
            context: RefCell::new(WebContext::new(Some(Self::data_folder()))),
        }
    }

    /// Returns the data folder: in the temp directory, named after this crate, its version, and
    /// the running program's file name and full path, so that no two programs share one.
    fn data_folder() -> PathBuf {
        let mut name = OsString::from(DATA_FOLDER);
        if let Ok(program) = env::current_exe() {
            Self::name_after(&mut name, &program);
        }
        let mut folder = env::temp_dir();
        folder.push(name);
        folder
    }

    /// Adds `program`'s file name and a hash of its full path to `name`.
    fn name_after(name: &mut OsString, program: &Path) {
        let mut hasher = DefaultHasher::new();
        program.hash(&mut hasher);
        name.push("-");
        name.push(program.file_stem().unwrap_or_default());
        let _ = write!(name, "-{:016x}", hasher.finish());
    }
}
