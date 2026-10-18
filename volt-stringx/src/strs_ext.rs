use super::StrExt;

/// Joins over slices of `&str` that skip entries.
pub trait StrsExt {
    /// Joins the trimmed entries that `keep` accepts with `separator`, allocating
    /// once.
    fn join_where(&self, separator: &str, keep: impl Fn(&str) -> bool) -> String;
    /// Joins the trimmed, non-blank entries with `separator`.
    fn join_non_blank(&self, separator: &str) -> String;
}

impl StrsExt for [&str] {
    fn join_where(&self, separator: &str, keep: impl Fn(&str) -> bool) -> String {
        let capacity = self.iter().map(|s| s.len() + separator.len()).sum();
        let mut out = String::new();
        for s in self.iter().filter(|&&s| keep(s)).map(|s| s.trim()) {
            if out.is_empty() {
                out.reserve(capacity);
            } else {
                out.push_str(separator);
            }
            out.push_str(s);
        }
        out
    }

    fn join_non_blank(&self, separator: &str) -> String {
        self.join_where(separator, str::is_not_blank)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_non_blank() {
        assert_eq!(
            ["one", "  ", "two", "", " three "].join_non_blank(", "),
            "one, two, three"
        );
        assert_eq!([" ", "\t", ""].join_non_blank(", "), "");
        assert_eq!(<[&str]>::join_non_blank(&[], ", "), "");
        assert_eq!(["  hello  ", " world "].join_non_blank("-"), "hello-world");
    }

    #[test]
    fn join_where_custom_filter() {
        assert_eq!(
            ["apple", "cat", "apricot", "dog", "cherry"].join_where(" | ", |s| s.len() > 3),
            "apple | apricot | cherry"
        );
    }

    #[test]
    fn join_empty_separator() {
        assert_eq!(["a", "b", "c"].join_non_blank(""), "abc");
        assert_eq!(["a", " ", "b"].join_non_blank(""), "ab");
        assert_eq!(["", "x", ""].join_non_blank(""), "x");
    }

    #[test]
    fn join_all_blank_with_any_separator() {
        for sep in ["", ", ", "\n----\n"] {
            assert_eq!(["", "  ", "\t\n"].join_non_blank(sep), "");
        }
    }
}
