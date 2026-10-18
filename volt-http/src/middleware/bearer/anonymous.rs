/// A request extension that makes [`Bearer`](crate::middleware::Bearer) leave the request
/// alone: no token is attached and a `401` is neither refreshed nor replayed.
#[derive(Debug, Clone, Copy)]
pub struct Anonymous;
