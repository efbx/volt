use std::sync::LazyLock;

use aho_corasick::{AhoCorasick, MatchKind};

/// The ASCII character each group of typographic look-alikes becomes.
const NORMALIZATIONS: [(&str, &[&str]); 3] = [
    (
        "\"",
        &[
            "‹", "›", "〈", "〉", "“", "”", "«", "»", "「", "」", "｢", "｣", "《", "》", "„", "‟",
            "〝", "〞",
        ],
    ),
    ("'", &["‘", "‛", "’"]),
    (",", &["‚"]),
];

/// One Aho-Corasick automaton over every abnormal symbol, with the replacement
/// for each pattern in the same order.
pub(super) struct Normalizer {
    matcher: AhoCorasick,
    replacements: Vec<&'static str>,
}

/// The shared [`Normalizer`], built from [`NORMALIZATIONS`] on first use.
pub(super) static NORMALIZER: LazyLock<Normalizer> = LazyLock::new(|| {
    let abnormals = NORMALIZATIONS
        .iter()
        .flat_map(|(_, abnormals)| abnormals.iter().copied());
    let replacements = NORMALIZATIONS
        .iter()
        .flat_map(|(normal, abnormals)| abnormals.iter().map(move |_| *normal))
        .collect();
    Normalizer {
        matcher: AhoCorasick::builder()
            .match_kind(MatchKind::LeftmostFirst)
            .build(abnormals)
            .expect("abnormal symbols form a valid automaton"),
        replacements,
    }
});

impl Normalizer {
    /// Returns `value` with every abnormal symbol replaced, or `None` when it has none, scanning
    /// the text once.
    pub(super) fn normalized(&self, value: &str) -> Option<String> {
        let start = self.matcher.find(value)?.start();
        let mut normalized = String::with_capacity(value.len());
        normalized.push_str(&value[..start]);
        self.matcher
            .replace_all_with(&value[start..], &mut normalized, |found, _, dst| {
                dst.push_str(self.replacements[found.pattern().as_usize()]);
                true
            });
        Some(normalized)
    }
}
