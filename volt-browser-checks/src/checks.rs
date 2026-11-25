use std::fmt::{self, Debug};
use std::time::Duration;

use volt_browser::{Error, Page, Session};

use super::site::Site;

/// The most a command waits in a check that expects it to fail.
pub(super) const BRIEF: Duration = Duration::from_millis(300);

/// What a check gives: nothing, or why it did not hold.
pub(super) type Outcome = Result<(), Flaw>;

/// One check that did not hold.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    /// The name of the check.
    pub check: &'static str,
    /// What went wrong in it.
    pub reason: String,
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.check, self.reason)
    }
}

/// Why a check did not hold.
#[derive(Debug)]
pub(super) struct Flaw(String);

impl Flaw {
    /// Fails with `what` and both values unless `got` equals `want`.
    pub(super) fn same<T, U>(what: &str, got: &T, want: &U) -> Outcome
    where
        T: PartialEq<U> + Debug + ?Sized,
        U: Debug + ?Sized,
    {
        if got == want {
            Ok(())
        } else {
            Err(Self(format!("{what}: got {got:?}, wanted {want:?}")))
        }
    }

    /// Fails with `what` and the value `got` unless `holds`.
    pub(super) fn expect<T: Debug + ?Sized>(what: &str, got: &T, holds: bool) -> Outcome {
        if holds {
            Ok(())
        } else {
            Err(Self(format!("{what}: got {got:?}")))
        }
    }
}

impl From<Error> for Flaw {
    fn from(error: Error) -> Self {
        Self(format!("a command failed: {error}"))
    }
}

/// The checks every engine has to pass.
#[derive(Clone, Copy, Debug)]
pub struct Checks;

impl Checks {
    /// Runs every check on pages of `session` and returns the ones that did not hold; an engine
    /// passes when there are none. Each check opens a page of its own and closes it.
    ///
    /// # Errors
    ///
    /// Fails with [`Error::Engine`], holding the I/O error, when the local site cannot be
    /// started.
    pub async fn run(session: &Session) -> Result<Vec<Failure>, Error> {
        let site = Site::serve()
            .await
            .map_err(|error| Error::Engine(Box::new(error)))?;
        let mut failures = Vec::new();
        Self::note(
            &mut failures,
            "selectors",
            Self::selectors(session, &site).await,
        );
        Self::note(
            &mut failures,
            "narrowing",
            Self::narrowing(session, &site).await,
        );
        Self::note(
            &mut failures,
            "visible text",
            Self::visible_text(session, &site).await,
        );
        Self::note(
            &mut failures,
            "reading",
            Self::reading(session, &site).await,
        );
        Self::note(&mut failures, "forms", Self::forms(session, &site).await);
        Self::note(
            &mut failures,
            "refusals",
            Self::refusals(session, &site).await,
        );
        Self::note(&mut failures, "clicks", Self::clicks(session, &site).await);
        Self::note(&mut failures, "keys", Self::keys(session, &site).await);
        Self::note(
            &mut failures,
            "waiting",
            Self::waiting(session, &site).await,
        );
        Self::note(&mut failures, "frames", Self::frames(session, &site).await);
        Self::note(
            &mut failures,
            "scripts",
            Self::scripts(session, &site).await,
        );
        Self::note(
            &mut failures,
            "navigation",
            Self::navigation(session, &site).await,
        );
        Self::note(
            &mut failures,
            "closing",
            Self::closing(session, &site).await,
        );
        Self::note(
            &mut failures,
            "cookies",
            Self::cookies(session, &site).await,
        );
        Self::note(
            &mut failures,
            "storage",
            Self::storage(session, &site).await,
        );
        Self::note(&mut failures, "caches", Self::caches(session, &site).await);
        for _ in 0..60 {
            Self::note(
                &mut failures,
                "navigation again",
                Self::navigation(session, &site).await,
            );
        }
        Ok(failures)
    }

    /// Adds the check called `check` to `failures` when its `outcome` is a flaw.
    fn note(failures: &mut Vec<Failure>, check: &'static str, outcome: Outcome) {
        if let Err(Flaw(reason)) = outcome {
            failures.push(Failure { check, reason });
        }
    }

    /// Opens a new page of `session` on `path` of `site`.
    pub(super) async fn open(session: &Session, site: &Site, path: &str) -> Result<Page, Flaw> {
        let page = session.new_page().await?;
        page.goto(&site.url(path)).await?;
        Ok(page)
    }
}
