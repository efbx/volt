use super::StrExt;
use super::normalizer::NORMALIZER;

/// In-place edits on `String` that avoid reallocating where they can.
pub trait StringExt {
    /// Removes leading and trailing whitespace without allocating.
    fn trim_in_place(&mut self);
    /// Trims, then replaces typographic symbols with their ASCII forms, allocating
    /// only when there is something to replace.
    fn normalize_symbols_in_place(&mut self);
    /// Uppercases as [`StrExt::simple_uppercase`] does, in place for ASCII strings.
    fn simple_uppercase_in_place(&mut self);
    /// Replaces the string with `value` unless `value` is blank.
    fn set_if_not_blank(&mut self, value: String);
}

impl StringExt for String {
    fn set_if_not_blank(&mut self, value: String) {
        if value.is_not_blank() {
            *self = value;
        }
    }

    fn trim_in_place(&mut self) {
        self.truncate(self.trim_end().len());
        let start = self.len() - self.trim_start().len();
        self.drain(..start);
    }

    fn normalize_symbols_in_place(&mut self) {
        self.trim_in_place();
        if let Some(normalized) = NORMALIZER.normalized(self) {
            *self = normalized;
        }
    }

    fn simple_uppercase_in_place(&mut self) {
        if self.is_ascii() {
            self.make_ascii_uppercase();
        } else {
            *self = self.simple_uppercase();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_if_not_blank_keeps_the_old_value_for_blank_input() {
        for (start, incoming, expected) in [
            ("old", "new", "new"),
            ("old", "", "old"),
            ("old", "  \t\n", "old"),
            ("", "  x  ", "  x  "),
        ] {
            let mut value = start.to_owned();
            value.set_if_not_blank(incoming.to_owned());
            assert_eq!(value, expected, "start={start:?} incoming={incoming:?}");
        }
    }

    #[test]
    fn trim_in_place_matches_trim() {
        for input in [
            "",
            "   ",
            "hello",
            "  hello",
            "hello  ",
            "  hello  ",
            "\t a b \n",
            "héllo 世界 ",
        ] {
            let mut s = input.to_owned();
            s.trim_in_place();
            assert_eq!(s, input.trim(), "input: {input:?}");
        }
    }

    #[test]
    fn normalize_symbols_in_place_changed() {
        let mut s = " \u{2018}test\u{2019} ".to_string();
        s.normalize_symbols_in_place();
        assert_eq!(s, "'test'");
    }

    #[test]
    fn normalize_symbols_in_place_no_change() {
        let mut s = "already normal".to_string();
        let original_ptr = s.as_ptr();
        s.normalize_symbols_in_place();
        assert_eq!(s.as_ptr(), original_ptr);
        assert_eq!(s, "already normal");
    }

    #[test]
    fn simple_uppercase_in_place_matches_simple_uppercase() {
        for input in [
            "pygmalion_username",
            "café",
            "straße",
            "MiXeD 123 !?",
            "ǆ",
            "",
        ] {
            let mut s = input.to_owned();
            s.simple_uppercase_in_place();
            assert_eq!(s, input.simple_uppercase(), "input: {input:?}");
        }
    }

    #[test]
    fn simple_uppercase_in_place_ascii_does_not_reallocate() {
        let mut s = "pygmalion_username".to_owned();
        let before = s.as_ptr();
        s.simple_uppercase_in_place();
        assert_eq!(s.as_ptr(), before);
    }
}
