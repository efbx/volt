use std::borrow::Cow;

use super::normalizer::NORMALIZER;

/// Read-only helpers on `str` that return new values.
pub trait StrExt {
    /// Whether the string is empty or only whitespace.
    fn is_blank(&self) -> bool;
    /// Whether the string has at least one non-whitespace character.
    fn is_not_blank(&self) -> bool;
    /// Keeps only the ASCII letters and digits, lowercased; every other character
    /// is dropped.
    fn alphanumeric_lower(&self) -> String;
    /// Whether what [`StrExt::alphanumeric_lower`] keeps equals `name`, compared without
    /// allocating; `name` is expected in that lowercase alphanumeric form.
    fn alphanumeric_eq(&self, name: &str) -> bool;
    /// Trims the string and replaces typographic quotes, apostrophes and commas
    /// with their ASCII forms, borrowing the trimmed text when nothing is replaced.
    fn normalize_symbols(&self) -> Cow<'_, str>;
    /// Uppercases each character whose uppercase form is a single character and
    /// keeps the rest, so the result has the same number of characters.
    fn simple_uppercase(&self) -> String;
    /// Decodes a URL query component: `+` becomes a space and `%XX` the byte it
    /// names. Returns `None` on a malformed escape or when the bytes are not UTF-8.
    fn query_unescape(&self) -> Option<String>;
}

impl StrExt for str {
    fn is_blank(&self) -> bool {
        self.chars().all(char::is_whitespace)
    }

    fn is_not_blank(&self) -> bool {
        !self.is_blank()
    }

    fn alphanumeric_lower(&self) -> String {
        let mut key = String::with_capacity(self.len());
        key.extend(
            self.bytes()
                .filter(u8::is_ascii_alphanumeric)
                .map(|b| char::from(b.to_ascii_lowercase())),
        );
        key
    }

    fn alphanumeric_eq(&self, name: &str) -> bool {
        self.bytes()
            .filter(u8::is_ascii_alphanumeric)
            .map(|byte| byte.to_ascii_lowercase())
            .eq(name.bytes())
    }

    fn normalize_symbols(&self) -> Cow<'_, str> {
        let trimmed = self.trim();
        NORMALIZER
            .normalized(trimmed)
            .map_or(Cow::Borrowed(trimmed), Cow::Owned)
    }

    fn simple_uppercase(&self) -> String {
        if self.is_ascii() {
            return self.to_ascii_uppercase();
        }
        let mut upper = String::with_capacity(self.len());
        upper.extend(self.chars().map(|c| {
            let mut mapped = c.to_uppercase();
            match mapped.len() {
                1 => mapped.next().unwrap_or(c),
                _ => c,
            }
        }));
        upper
    }

    fn query_unescape(&self) -> Option<String> {
        let mut out = Vec::with_capacity(self.len());
        let mut rest = self.as_bytes();
        while let Some((&byte, tail)) = rest.split_first() {
            let (decoded, tail) = match byte {
                b'+' => (b' ', tail),
                b'%' => {
                    let ([high, low], tail) = tail.split_first_chunk()?;
                    let value = (*high as char).to_digit(16)? * 16 + (*low as char).to_digit(16)?;
                    (u8::try_from(value).ok()?, tail)
                }
                _ => (byte, tail),
            };
            out.push(decoded);
            rest = tail;
        }
        String::from_utf8(out).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_blank() {
        for (input, expected, ctx) in [
            ("", true, "empty"),
            (" ", true, "space"),
            ("\t", true, "tab"),
            ("\n", true, "newline"),
            ("\r", true, "carriage return"),
            (" \t \n \r ", true, "mixed whitespace"),
            ("\u{2002}\u{2003}", true, "unicode spaces"),
            ("hello", false, "word"),
            (" hello ", false, "padded word"),
        ] {
            assert_eq!(input.is_blank(), expected, "is_blank({input:?}): {ctx}");
            assert_eq!(
                input.is_not_blank(),
                !expected,
                "is_not_blank({input:?}): {ctx}"
            );
        }
    }

    #[test]
    fn alphanumeric_lower() {
        for (input, expected, ctx) in [
            ("BeforeChar", "beforechar", "camelcase lowered"),
            ("before_char", "beforechar", "underscores removed"),
            ("before-@#$char", "beforechar", "symbols removed"),
            ("  at depth  ", "atdepth", "spaces removed"),
            ("AND_ANY", "andany", "upper + underscore"),
            ("123abc", "123abc", "alphanumeric kept"),
            (
                "h\u{00E9}llo",
                "hllo",
                "non-ascii letter stripped (Go ASCII-regex parity)",
            ),
            ("at\u{00B2}depth", "atdepth", "unicode digit stripped"),
            ("", "", "empty"),
        ] {
            assert_eq!(
                input.alphanumeric_lower(),
                expected,
                "alphanumeric_lower({input:?}): {ctx}"
            );
            assert!(
                input.alphanumeric_eq(expected),
                "alphanumeric_eq({input:?}): {ctx}"
            );
        }
        assert!(!"before_char".alphanumeric_eq("beforecha"), "longer input");
        assert!(!"before_cha".alphanumeric_eq("beforechar"), "shorter input");
        assert!(
            !"after_char".alphanumeric_eq("beforechar"),
            "different name"
        );
    }

    #[test]
    fn normalize_symbols_full() {
        let input = "\u{301D}opposites\u{301E} \u{2039}angular\u{203A} \u{3008}gills\u{3009} \u{2018}smart\u{2019} \u{201C}curly\u{201D} \u{00AB}guillemets\u{00BB} \u{201E}low\u{201F} \u{300C}corner\u{300D} \u{300A}brackets\u{300B} ";
        let expected = "\"opposites\" \"angular\" \"gills\" 'smart' \"curly\" \"guillemets\" \"low\" \"corner\" \"brackets\"";
        assert_eq!(input.normalize_symbols(), expected);
    }

    #[test]
    fn normalize_symbols_maps_every_abnormal_char() {
        let mapping = [
            ('\u{2039}', '"'),
            ('\u{203A}', '"'),
            ('\u{3008}', '"'),
            ('\u{3009}', '"'),
            ('\u{201C}', '"'),
            ('\u{201D}', '"'),
            ('\u{00AB}', '"'),
            ('\u{00BB}', '"'),
            ('\u{300C}', '"'),
            ('\u{300D}', '"'),
            ('\u{FF62}', '"'),
            ('\u{FF63}', '"'),
            ('\u{300A}', '"'),
            ('\u{300B}', '"'),
            ('\u{201E}', '"'),
            ('\u{201F}', '"'),
            ('\u{301D}', '"'),
            ('\u{301E}', '"'),
            ('\u{2018}', '\''),
            ('\u{201B}', '\''),
            ('\u{2019}', '\''),
            ('\u{201A}', ','),
        ];
        assert_eq!(mapping.len(), 22);
        for (abnormal, expected) in mapping {
            assert_eq!(
                abnormal.to_string().normalize_symbols(),
                expected.to_string(),
                "U+{:04X} should map to {expected:?}",
                abnormal as u32
            );
        }
    }

    #[test]
    fn normalize_symbols_cases() {
        for (input, expected, ctx) in [
            ("  hello  ", "hello", "trims whitespace"),
            (" \u{2018}test\u{2019} ", "'test'", "trims and normalizes"),
            ("\u{201A}", ",", "comma normalization"),
            ("hello world 123", "hello world 123", "ascii passthrough"),
            ("", "", "empty string"),
            ("   ", "", "whitespace-only"),
        ] {
            assert_eq!(
                input.normalize_symbols(),
                expected,
                "normalize_symbols({input:?}): {ctx}"
            );
        }
    }

    #[test]
    fn normalize_symbols_borrows_unless_it_replaces() {
        assert!(matches!(
            "  plain text  ".normalize_symbols(),
            Cow::Borrowed("plain text")
        ));
        assert!(matches!(
            " \u{2018}quoted\u{2019} ".normalize_symbols(),
            Cow::Owned(text) if text == "'quoted'"
        ));
    }

    #[test]
    fn simple_uppercase_maps_one_to_one_only() {
        for (input, expected, ctx) in [
            ("pygmalion_username", "PYGMALION_USERNAME", "ascii"),
            ("café", "CAFÉ", "one-to-one non-ascii"),
            ("straße", "STRAßE", "multi-char expansion refused"),
            (
                "MiXeD 123 !?",
                "MIXED 123 !?",
                "already upper / digits / punctuation untouched",
            ),
            ("ǆ", "Ǆ", "digraph maps one-to-one"),
            ("", "", "empty"),
        ] {
            assert_eq!(
                input.simple_uppercase(),
                expected,
                "simple_uppercase({input:?}): {ctx}"
            );
        }
    }

    #[test]
    fn query_unescape() {
        for (input, expected, ctx) in [
            ("Violete%20V4", Some("Violete V4"), "percent space"),
            ("a+b", Some("a b"), "plus becomes space"),
            ("caf%C3%A9", Some("café"), "multibyte utf8 sequence"),
            ("%2b", Some("+"), "lowercase hex"),
            ("plain/name", Some("plain/name"), "untouched"),
            ("", Some(""), "empty"),
            ("bad%zz", None, "malformed hex"),
            ("trunc%4", None, "truncated escape"),
            ("trunc%", None, "bare percent at end"),
            ("%FF", None, "invalid utf8 result"),
        ] {
            assert_eq!(
                input.query_unescape().as_deref(),
                expected,
                "query_unescape({input:?}): {ctx}"
            );
        }
    }
}
