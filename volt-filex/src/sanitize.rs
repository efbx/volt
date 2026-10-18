/// Text turned into a safe file name.
pub trait FileNameExt {
    /// Returns the text as a safe single path component: slashes become underscores, every
    /// character other than ASCII letters, digits, `-`, `_`, `.` and ASCII whitespace is
    /// dropped, and each run of whitespace becomes one hyphen, with none at either end.
    #[must_use]
    fn to_file_name(&self) -> String;
}

impl FileNameExt for str {
    fn to_file_name(&self) -> String {
        let mut path = String::with_capacity(self.len());
        for word in self.split([' ', '\t', '\n', '\x0C', '\r']) {
            let mut kept = word
                .chars()
                .filter_map(|c| match c {
                    '/' => Some('_'),
                    'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' => Some(c),
                    _ => None,
                })
                .peekable();
            if kept.peek().is_some() && !path.is_empty() {
                path.push('-');
            }
            path.extend(kept);
        }
        path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_path() {
        for (input, expected, ctx) in [
            ("clean-path", "clean-path", "already clean"),
            ("path/to/file", "path_to_file", "slashes become underscores"),
            (
                r#"a<b>c:d"e/f|g?h*i."#,
                "abcde_fghi.",
                "invalid chars removed",
            ),
            ("this has spaces", "this-has-spaces", "spaces become dashes"),
            (
                "  invalid / path with?  spaces  ",
                "invalid-_-path-with-spaces",
                "combination",
            ),
            ("", "", "empty string"),
            (
                "My\u{00A0}Card",
                "MyCard",
                "non-ascii whitespace removed not split",
            ),
            ("a\u{000B}b", "ab", "vertical tab removed not split"),
        ] {
            assert_eq!(
                input.to_file_name(),
                expected,
                "to_file_name({input:?}): {ctx}"
            );
        }
    }

    #[test]
    fn to_file_name_keeps_alnum_dash_underscore_dot() {
        for input in ["hello-world_123.txt", "MyFile.txt", "folder_name"] {
            assert_eq!(input.to_file_name(), input);
        }
    }

    #[test]
    fn to_file_name_drops_special_chars() {
        for (input, expected) in [
            ("file@name", "filename"),
            ("file:name", "filename"),
            ("file?name", "filename"),
            ("file*name", "filename"),
            ("emoji😀", "emoji"),
            ("\u{00A0}", ""),
            ("\u{000B}", ""),
            ("a ? b", "a-b"),
            ("a\t\n\x0C\r b", "a-b"),
        ] {
            assert_eq!(input.to_file_name(), expected, "to_file_name({input:?})");
        }
    }
}
