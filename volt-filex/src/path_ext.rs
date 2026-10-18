use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};

use super::next_available::Numbering;

/// File helpers on [`Path`] beyond what the standard library offers.
pub trait PathExt {
    /// Whether something other than a directory exists at the path; `false` when it cannot
    /// be read.
    fn exists_as_file(&self) -> bool;

    /// Copies the file at the path to `dst`, creating or truncating `dst`, and returns the
    /// number of bytes written.
    ///
    /// # Errors
    ///
    /// Fails when the source cannot be opened, `dst` cannot be created (its directory must
    /// already exist) or the copy itself fails.
    fn copy_to(&self, dst: impl AsRef<Path>) -> io::Result<u64>;

    /// Returns the path when nothing exists there, otherwise the same name with a number
    /// one above the highest already taken in its directory, so `img.png` with `img5.png`
    /// present becomes `img6.png`; gaps are not filled.
    ///
    /// The number goes before `ext` when the name ends with it and `ext` is not blank, and
    /// at the very end otherwise, so `ext` should include its dot.
    #[must_use]
    fn next_available(&self, ext: Option<&str>) -> PathBuf;
}

impl PathExt for Path {
    fn exists_as_file(&self) -> bool {
        fs::metadata(self).is_ok_and(|m| !m.is_dir())
    }

    fn copy_to(&self, dst: impl AsRef<Path>) -> io::Result<u64> {
        let mut src_file = File::open(self)?;
        let mut dst_file = File::create(dst)?;
        io::copy(&mut src_file, &mut dst_file)
    }

    fn next_available(&self, ext: Option<&str>) -> PathBuf {
        Numbering::next(self, ext)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_dir::TestDir;

    #[test]
    fn test_exists_as_file() {
        let t = TestDir::new();
        let file = t.0.join("f.txt");
        fs::write(&file, "").unwrap();
        let dir = t.0.join("subdir");
        fs::create_dir_all(&dir).unwrap();
        let nope = t.0.join("nope");
        for (path, is_file, ctx) in [
            (&file, true, "file"),
            (&dir, false, "dir"),
            (&nope, false, "nonexistent"),
        ] {
            assert_eq!(path.exists_as_file(), is_file, "exists_as_file({ctx})");
        }
    }

    #[test]
    fn test_copy_to() {
        let t = TestDir::new();
        let src = t.0.join("src.txt");
        let content = "hello copy_to";
        fs::write(&src, content).unwrap();

        let dst = t.0.join("dst.txt");
        assert_eq!(src.copy_to(&dst).unwrap(), content.len() as u64);
        assert_eq!(fs::read_to_string(&dst).unwrap(), content);

        let missing_src = t.0.join("nope.txt");
        let bad_dst = t.0.join("nonexistent_subdir/dst.txt");
        for (s, d, ctx) in [
            (&missing_src, &dst, "nonexistent src"),
            (&src, &bad_dst, "invalid dst dir"),
        ] {
            assert!(s.copy_to(d).is_err(), "copy_to({ctx})");
        }
    }
}
