use wreq::{StatusCode, Uri};

/// What a [`RedirectClassifier`](super::RedirectClassifier) sees about one redirect.
#[derive(Debug)]
pub struct RedirectContext<'a> {
    /// The redirect status: 301, 302, 303, 307 or 308.
    pub status: StatusCode,
    /// The `Location` target, resolved against the current URI.
    pub destination: &'a Uri,
    /// Every URI requested so far in this chain, the original first.
    pub history: &'a [Uri],
    /// The number of URIs requested so far, 1 on the first redirect.
    pub hop: usize,
}
