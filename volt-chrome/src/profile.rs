use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// A fresh user data directory under the system temp directory, removed on drop.
pub(super) struct TempProfile(PathBuf);

impl TempProfile {
    /// Creates a directory unique to this process and call; a failure to create it is
    /// ignored and left for the browser to report.
    pub(super) fn create() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("volt-chrome-{}-{unique}", std::process::id()));
        let _ = std::fs::create_dir_all(&path);
        Self(path)
    }

    /// Returns the directory's path.
    pub(super) fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempProfile {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
