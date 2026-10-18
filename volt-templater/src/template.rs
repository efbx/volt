use std::fmt::{self, Debug};

use aho_corasick::{AhoCorasick, MatchKind};

use super::compiled::Piece;
use super::{CompiledTemplate, Token};

/// The set of tokens a program offers, with one automaton that finds any of their keys.
pub struct Templater<T> {
    matcher: AhoCorasick,
    tokens: Vec<Token<T>>,
}

impl<T> Debug for Templater<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Templater")
            .field("tokens", &self.tokens)
            .finish_non_exhaustive()
    }
}

impl<T> Templater<T> {
    /// Builds a templater over `tokens`, dropping any token whose key is empty.
    #[must_use]
    pub fn new(mut tokens: Vec<Token<T>>) -> Self {
        tokens.retain(|token| !token.key.is_empty());
        let matcher = AhoCorasick::builder()
            .match_kind(MatchKind::LeftmostLongest)
            .build(tokens.iter().map(|token| token.key.as_bytes()))
            .expect("token keys form a valid automaton");
        Self { matcher, tokens }
    }

    /// Returns the tokens in the order given, without the ones dropped for an empty key.
    #[must_use]
    pub fn tokens(&self) -> &[Token<T>] {
        &self.tokens
    }

    /// Splits `template` into text and tokens in one pass. Where keys overlap the leftmost
    /// match wins, and at the same position the longest key; unknown text stays as it is.
    #[must_use]
    pub fn compile(&self, template: &str) -> CompiledTemplate<T> {
        let mut pieces = Vec::new();
        let mut last = 0;
        for found in self.matcher.find_iter(template) {
            if found.start() > last {
                pieces.push(Piece::Text(template[last..found.start()].to_owned()));
            }
            pieces.push(Piece::Token(
                self.tokens[found.pattern().as_usize()].extractor,
            ));
            last = found.end();
        }
        if last < template.len() {
            pieces.push(Piece::Text(template[last..].to_owned()));
        }
        let literal_len = pieces
            .iter()
            .map(|piece| match piece {
                Piece::Text(text) => text.len(),
                Piece::Token(_) => 0,
            })
            .sum();
        CompiledTemplate {
            pieces,
            literal_len,
        }
    }

    /// Fills `template` for `input` in one pass without compiling it, for a template used once.
    pub fn execute(&self, template: &str, input: &T) -> String {
        let mut out = String::with_capacity(template.len());
        self.matcher
            .replace_all_with(template, &mut out, |found, _, dst| {
                dst.push_str(&(self.tokens[found.pattern().as_usize()].extractor)(input));
                true
            });
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct User {
        name: String,
        email: String,
        age: u32,
    }

    fn user(name: &str) -> User {
        User {
            name: name.into(),
            email: "alice@test.com".into(),
            age: 5,
        }
    }

    fn templater() -> Templater<User> {
        Templater::new(vec![
            Token::new("{{name}}", |u: &User| u.name.as_str().into()),
            Token::new("{{email}}", |u: &User| u.email.as_str().into()),
            Token::with_description(
                "{{age}}",
                |u: &User| u.age.to_string().into(),
                "Age in years",
            ),
        ])
    }

    #[test]
    fn renders_text_and_tokens() {
        let t = templater();
        let alice = user("Alice");
        for (template, expected) in [
            ("Hello, {{name}}!", "Hello, Alice!"),
            ("{{name}} <{{email}}>, {{age}}", "Alice <alice@test.com>, 5"),
            ("{{name}} loves {{name}}", "Alice loves Alice"),
            ("{{name}}{{email}}", "Alicealice@test.com"),
            ("{{name}}", "Alice"),
            ("plain text", "plain text"),
            ("{{name", "{{name"),
            ("Hello {{unknown}} world", "Hello {{unknown}} world"),
            ("{{name}} 50% off", "Alice 50% off"),
            ("", ""),
        ] {
            assert_eq!(t.execute(template, &alice), expected, "{template:?}");
        }
    }

    #[test]
    fn longest_key_wins() {
        let t = Templater::new(vec![
            Token::new("{{n}}", |_: &User| "SHORT".into()),
            Token::new("{{name}}", |u: &User| u.name.as_str().into()),
        ]);
        assert_eq!(t.execute("{{n}} {{name}}", &user("Alice")), "SHORT Alice");
    }

    #[test]
    fn compiled_template_is_reusable() {
        let compiled = templater().compile("Hello, {{name}}!");
        for name in ["Alice", "Bob", "Charlie"] {
            assert_eq!(compiled.execute(&user(name)), format!("Hello, {name}!"));
        }
    }

    #[test]
    fn handles_unicode() {
        let t = Templater::new(vec![Token::new("{{名前}}", |u: &User| {
            u.name.as_str().into()
        })]);
        assert_eq!(
            t.execute("ユーザー: {{名前}} です", &user("太郎")),
            "ユーザー: 太郎 です"
        );
    }

    #[test]
    fn handles_degenerate_tokens() {
        let none: Templater<()> = Templater::new(vec![]);
        assert_eq!(none.execute("plain {{x}}", &()), "plain {{x}}");

        let empty_output = Templater::new(vec![Token::new("{{x}}", |(): &()| "".into())]);
        assert_eq!(empty_output.execute("a{{x}}b", &()), "ab");

        let empty_key = Templater::new(vec![
            Token::new("", |(): &()| "!".into()),
            Token::new("{{x}}", |(): &()| "X".into()),
        ]);
        assert_eq!(empty_key.tokens().len(), 1);
        assert_eq!(empty_key.execute("a{{x}}b", &()), "aXb");
    }

    #[test]
    fn exposes_token_metadata() {
        let t = templater();
        let keys: Vec<&str> = t.tokens().iter().map(|token| &*token.key).collect();
        assert_eq!(keys, ["{{name}}", "{{email}}", "{{age}}"]);
        assert!(t.tokens()[0].description.is_none());
        assert_eq!(t.tokens()[2].description.as_deref(), Some("Age in years"));
    }
}
