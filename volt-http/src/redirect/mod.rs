//! The redirect policy a [`Client`](crate::Client) applies to 3xx responses: a [`RedirectPolicy`]
//! with a hop limit, and a [`RedirectClassifier`] that decides from a [`RedirectContext`] whether
//! each hop is followed.

mod redirect_classifier;
mod redirect_context;
mod redirect_policy;

pub use redirect_classifier::{Classifier, RedirectClassifier};
pub use redirect_context::RedirectContext;
pub use redirect_policy::RedirectPolicy;
