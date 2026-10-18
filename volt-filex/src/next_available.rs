use std::path::{Path, PathBuf};

use volt_stringx::StrExt;

/// The numbering behind [`PathExt::next_available`](crate::PathExt::next_available).
pub(super) struct Numbering;

impl Numbering {
    // Returns `max_existing + 1`, NOT the first free slot — gaps are preserved by design.
    /// Returns `path` when nothing exists there, otherwise the same name numbered one above
    /// the highest already taken in its directory, before `ext` when the name ends with it.
    pub(super) fn next(path: &Path, ext: Option<&str>) -> PathBuf {
        if !path.exists() {
            return path.to_path_buf();
        }
        let file_name = path
            .file_name()
            .map(|name| name.to_string_lossy())
            .unwrap_or_default();
        let suffix = ext
            .filter(|candidate| candidate.is_not_blank() && file_name.ends_with(candidate))
            .unwrap_or("");
        let base = file_name.strip_suffix(suffix).unwrap_or(&file_name);
        let next = Self::highest_index(Self::parent_dir(path), base, suffix) + 1;
        path.with_file_name(format!("{base}{next}{suffix}"))
    }

    /// Returns the directory holding `path`, or `.` when it has none.
    fn parent_dir(path: &Path) -> &Path {
        match path.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => parent,
            _ => Path::new("."),
        }
    }

    /// Returns the highest number found between `base` and `suffix` among the names in
    /// `dir`, or zero when there is none or `dir` cannot be read.
    fn highest_index(dir: &Path, base: &str, suffix: &str) -> u64 {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return 0;
        };
        entries
            .flatten()
            .filter_map(|entry| Self::index_of(&entry.file_name().to_string_lossy(), base, suffix))
            .max()
            .unwrap_or(0)
    }

    /// Returns the number between `base` and `suffix` in `name`, if `name` has that shape.
    fn index_of(name: &str, base: &str, suffix: &str) -> Option<u64> {
        name.strip_prefix(base)?.strip_suffix(suffix)?.parse().ok()
    }
}

#[cfg(test)]
mod tests {
    use crate::PathExt;
    use crate::test_dir::TestDir;
    use std::fs;

    #[test]
    fn test_next_available_path_absent() {
        let t = TestDir::new();
        let path = t.0.join("brand_new.png");
        assert_eq!(path.next_available(None), path);
        assert_eq!(path.next_available(Some(".png")), path);
    }

    #[test]
    fn test_next_available_path_directory() {
        let t = TestDir::new();
        let subdir = t.0.join("mydir");
        fs::create_dir_all(&subdir).unwrap();
        assert_eq!(subdir.next_available(None), t.0.join("mydir1"));
    }

    #[test]
    fn test_next_available_path_cases() {
        for (existing, query, ext, expected, ctx) in [
            (
                &["foo.png"][..],
                "foo.png",
                None,
                "foo.png1",
                "no-ext, dotted name",
            ),
            (
                &["my.vault", "my.vault1", "my.vault2"][..],
                "my.vault",
                None,
                "my.vault3",
                "no-ext counting, dotted",
            ),
            (
                &["log", "log1", "log2", "log3"][..],
                "log",
                None,
                "log4",
                "no-ext counting",
            ),
            (&["noext"][..], "noext", None, "noext1", "no extension"),
            (
                &["img.png"][..],
                "img.png",
                Some(".png"),
                "img1.png",
                "explicit ext",
            ),
            (
                &["img.png", "img1.png", "img2.png", "img3.png"][..],
                "img.png",
                Some(".png"),
                "img4.png",
                "explicit ext counting",
            ),
            (
                &["p.png", "p1.png", "p5.png", "p10.png"][..],
                "p.png",
                Some(".png"),
                "p11.png",
                "explicit ext, gap preserved",
            ),
            (
                &["file.txt"][..],
                "file.txt",
                Some(".png"),
                "file.txt1",
                "wrong ext ignored",
            ),
        ] {
            let t = TestDir::new();
            for &f in existing {
                fs::write(t.0.join(f), "").unwrap();
            }
            assert_eq!(
                t.0.join(query).next_available(ext),
                t.0.join(expected),
                "next_available({ctx})"
            );
        }
    }
}
