use std::io::ErrorKind;
use std::path::{Path, PathBuf};

/// A fresh user data directory under the system temp directory, removed on drop.
#[derive(Debug)]
pub(super) struct TempProfile(PathBuf);

impl TempProfile {
    /// Creates a directory under the first numbered name of this process that no directory has
    /// yet; a failure to create it is ignored and left for the browser to report.
    pub(super) fn create() -> Self {
        let temp = std::env::temp_dir();
        let process = std::process::id();
        let mut unique = 0_u64;
        loop {
            let path = temp.join(format!("volt-chrome-{process}-{unique}"));
            if Self::claim(&path) {
                return Self(path);
            }
            unique += 1;
        }
    }

    /// Returns the directory's path.
    pub(super) fn path(&self) -> &Path {
        &self.0
    }

    /// Claims the name `path` by creating its directory; `false` where a directory has that name
    /// already.
    fn claim(path: &Path) -> bool {
        !std::fs::create_dir(path).is_err_and(|error| error.kind() == ErrorKind::AlreadyExists)
    }
}

impl Drop for TempProfile {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::TempProfile;

    #[test]
    fn each_profile_gets_a_directory_of_its_own_until_dropped() {
        let first = TempProfile::create();
        let second = TempProfile::create();
        assert_ne!(first.path(), second.path());
        assert!(first.path().is_dir() && second.path().is_dir());
        let gone = first.path().to_owned();
        drop(first);
        assert!(!gone.exists());
    }
}
