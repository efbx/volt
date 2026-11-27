use std::mem;

use volt_browser::Cause;

use crate::Error;

/// How a page's load is going, as its webview reports it.
#[derive(Debug)]
pub(crate) enum LoadEvent {
    /// A load has started: committed on macOS, content loading on Windows, started in
    /// `WebKitGTK`.
    Started,
    /// The load under way opened `about:blank`, which a webview also does in place of an address
    /// it refuses to load.
    Blank,
    /// A load has ended: with no reason when it finished, with the webview's reason when it
    /// failed, which only `WebKitGTK` reports. `WebView2` ends a failed load as a finished one,
    /// on its error page, which the goto then looks for.
    Ended(Option<Cause>),
    /// The page's web content process ended; reported on macOS, Linux and the BSDs.
    Crashed,
}

/// How the load a caller waits for ended.
#[derive(Debug)]
pub(super) enum End {
    /// It finished.
    Loaded,
    /// It failed, for this reason.
    Failed(Cause),
    /// The webview opened `about:blank` in place of the address the goto asked for.
    Refused,
    /// The page's web content process ended.
    Crashed,
}

impl End {
    /// Returns what the caller who waited for the load is told.
    pub(super) fn outcome(self) -> Result<(), Error> {
        match self {
            Self::Loaded => Ok(()),
            Self::Failed(cause) => Err(Error::LoadFailed(cause)),
            Self::Refused => Err(Error::LoadFailed(Cause::from(
                "the webview refused the address and opened a blank page",
            ))),
            Self::Crashed => Err(Error::Crashed),
        }
    }
}

/// The load a page's caller waits for, holding the reply `R` its end answers.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Waiting<R> {
    /// No caller waits for a load.
    Idle,
    /// The new page waits for `about:blank` to finish loading.
    Open(R),
    /// A goto waits for a load that starts after it, then ends.
    Goto {
        /// Where the goto's answer goes.
        reply: R,
        /// What a load has done since the goto.
        seen: Seen,
        /// Whether the goto asked for a page of the webview's own, as `about:blank` is.
        about: bool,
    },
}

/// What a load has done since the goto that waits for it.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Seen {
    /// Nothing: no load has started.
    Nothing,
    /// It has started.
    Start,
    /// It has started and opened `about:blank`, where the goto asked for another address.
    Blank,
}

impl<R> Waiting<R> {
    /// Waits for the load a goto starts next, which asked for a page of the webview's own when
    /// `about` says so, returning the reply of the wait it replaces.
    pub(super) fn goto(&mut self, reply: R, about: bool) -> Option<R> {
        mem::replace(
            self,
            Self::Goto {
                reply,
                seen: Seen::Nothing,
                about,
            },
        )
        .into_reply()
    }

    /// Applies `event`, returning the reply whose load it ends and how it ended. An end while
    /// nothing is awaited, as when a page reloads itself, ends nothing; so does one before the
    /// awaited load has started, which belongs to an earlier load. A load that opened a blank
    /// page for a goto that asked for another address ends that goto as refused. The end of the
    /// web content process ends any wait.
    pub(super) fn advance(&mut self, event: LoadEvent) -> Option<(R, End)> {
        match (mem::replace(self, Self::Idle), event) {
            (
                Self::Goto {
                    reply,
                    seen: Seen::Nothing,
                    about,
                },
                LoadEvent::Started,
            ) => {
                *self = Self::Goto {
                    reply,
                    seen: Seen::Start,
                    about,
                };
                None
            }
            (
                Self::Goto {
                    reply,
                    seen: Seen::Start,
                    about: false,
                },
                LoadEvent::Blank,
            ) => {
                *self = Self::Goto {
                    reply,
                    seen: Seen::Blank,
                    about: false,
                };
                None
            }
            (
                Self::Goto {
                    reply,
                    seen: Seen::Blank,
                    ..
                },
                LoadEvent::Ended(_),
            ) => Some((reply, End::Refused)),
            (
                Self::Open(reply)
                | Self::Goto {
                    reply,
                    seen: Seen::Start,
                    ..
                },
                LoadEvent::Ended(failure),
            ) => Some((reply, failure.map_or(End::Loaded, End::Failed))),
            (Self::Open(reply) | Self::Goto { reply, .. }, LoadEvent::Crashed) => {
                Some((reply, End::Crashed))
            }
            (waiting, _) => {
                *self = waiting;
                None
            }
        }
    }

    /// Ends the wait, returning the reply it still owes.
    pub(super) fn abandon(&mut self) -> Option<R> {
        mem::replace(self, Self::Idle).into_reply()
    }

    /// Returns the reply this wait owes.
    fn into_reply(self) -> Option<R> {
        match self {
            Self::Idle => None,
            Self::Open(reply) | Self::Goto { reply, .. } => Some(reply),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{End, LoadEvent, Waiting};
    use crate::Error;

    /// The load events a webview reports.
    struct Events;

    impl Events {
        /// Returns the end of a load that finished.
        fn finished() -> LoadEvent {
            LoadEvent::Ended(None)
        }

        /// Returns the end of a load that failed.
        fn failed() -> LoadEvent {
            LoadEvent::Ended(Some(Box::from("unreachable")))
        }
    }

    #[test]
    fn a_goto_ends_when_a_load_starts_then_finishes() {
        let mut waiting = Waiting::Idle;
        assert_eq!(waiting.goto(4, false), None);
        assert!(waiting.advance(LoadEvent::Started).is_none());
        assert!(matches!(
            waiting.advance(Events::finished()),
            Some((4, End::Loaded))
        ));
        assert_eq!(waiting, Waiting::Idle);
    }

    #[test]
    fn an_end_before_the_start_belongs_to_an_earlier_load() {
        let mut waiting = Waiting::Idle;
        waiting.goto(4, false);
        assert!(waiting.advance(Events::finished()).is_none());
        assert!(waiting.advance(Events::failed()).is_none());
        assert!(waiting.advance(LoadEvent::Started).is_none());
        assert!(matches!(
            waiting.advance(Events::finished()),
            Some((4, End::Loaded))
        ));
    }

    #[test]
    fn a_redirect_keeps_the_goto_waiting_for_the_next_end() {
        let mut waiting = Waiting::Idle;
        waiting.goto(4, false);
        assert!(waiting.advance(LoadEvent::Started).is_none());
        assert!(waiting.advance(LoadEvent::Started).is_none());
        assert!(waiting.advance(Events::finished()).is_some());
        assert!(waiting.advance(Events::finished()).is_none());
    }

    #[test]
    fn a_failed_load_ends_its_goto_and_its_own_finish_ends_nothing() {
        let mut waiting = Waiting::Idle;
        waiting.goto(4, false);
        waiting.advance(LoadEvent::Started);
        assert!(matches!(
            waiting.advance(Events::failed()),
            Some((4, End::Failed(_)))
        ));
        assert!(waiting.advance(Events::finished()).is_none());
    }

    #[test]
    fn a_goto_answered_with_the_blank_page_was_refused() {
        let mut waiting = Waiting::Idle;
        waiting.goto(4, false);
        assert!(waiting.advance(LoadEvent::Started).is_none());
        assert!(waiting.advance(LoadEvent::Blank).is_none());
        assert!(matches!(
            waiting.advance(Events::finished()),
            Some((4, End::Refused))
        ));
        assert_eq!(waiting, Waiting::Idle);
    }

    #[test]
    fn a_goto_to_an_about_page_takes_the_blank_page_as_its_own() {
        let mut waiting = Waiting::Idle;
        waiting.goto(4, true);
        assert!(waiting.advance(LoadEvent::Started).is_none());
        assert!(waiting.advance(LoadEvent::Blank).is_none());
        assert!(matches!(
            waiting.advance(Events::finished()),
            Some((4, End::Loaded))
        ));
    }

    #[test]
    fn a_blank_page_before_the_start_belongs_to_an_earlier_load() {
        let mut waiting = Waiting::Idle;
        waiting.goto(4, false);
        assert!(waiting.advance(LoadEvent::Blank).is_none());
        assert!(waiting.advance(LoadEvent::Started).is_none());
        assert!(matches!(
            waiting.advance(Events::finished()),
            Some((4, End::Loaded))
        ));
    }

    #[test]
    fn an_opening_page_takes_the_blank_page_as_its_own() {
        let mut waiting = Waiting::Open(1);
        assert!(waiting.advance(LoadEvent::Started).is_none());
        assert!(waiting.advance(LoadEvent::Blank).is_none());
        assert!(matches!(
            waiting.advance(Events::finished()),
            Some((1, End::Loaded))
        ));
    }

    #[test]
    fn the_end_of_the_content_process_ends_any_wait() {
        let mut waiting = Waiting::Idle;
        waiting.goto(4, false);
        assert!(matches!(
            waiting.advance(LoadEvent::Crashed),
            Some((4, End::Crashed))
        ));
        let mut opening = Waiting::Open(1);
        assert!(matches!(
            opening.advance(LoadEvent::Crashed),
            Some((1, End::Crashed))
        ));
        assert!(Waiting::<u64>::Idle.advance(LoadEvent::Crashed).is_none());
    }

    #[test]
    fn an_open_ends_on_the_first_end() {
        let mut waiting = Waiting::Open(1);
        assert!(waiting.advance(LoadEvent::Started).is_none());
        assert!(matches!(
            waiting.advance(Events::finished()),
            Some((1, End::Loaded))
        ));
        assert_eq!(waiting, Waiting::Idle);
    }

    #[test]
    fn an_idle_page_ignores_its_own_reloads() {
        let mut waiting = Waiting::<u64>::Idle;
        assert!(waiting.advance(LoadEvent::Started).is_none());
        assert!(waiting.advance(Events::finished()).is_none());
        assert!(waiting.advance(Events::failed()).is_none());
        assert_eq!(waiting, Waiting::Idle);
    }

    #[test]
    fn a_goto_replaces_the_wait_still_pending_and_waits_for_its_own_start() {
        let mut waiting = Waiting::Idle;
        waiting.goto(4, false);
        waiting.advance(LoadEvent::Started);
        assert_eq!(waiting.goto(5, false), Some(4));
        assert!(waiting.advance(Events::finished()).is_none());
        assert!(waiting.advance(LoadEvent::Started).is_none());
        assert!(matches!(
            waiting.advance(Events::finished()),
            Some((5, End::Loaded))
        ));
    }

    #[test]
    fn abandoning_returns_the_reply_still_owed() {
        assert_eq!(Waiting::<u64>::Idle.abandon(), None);
        assert_eq!(Waiting::Open(1).abandon(), Some(1));
        let mut waiting = Waiting::Idle;
        waiting.goto(2, false);
        assert_eq!(waiting.abandon(), Some(2));
        assert_eq!(waiting, Waiting::Idle);
    }

    #[test]
    fn each_end_tells_its_caller_its_outcome() {
        assert!(End::Loaded.outcome().is_ok());
        assert!(matches!(
            End::Failed(Box::from("unreachable")).outcome(),
            Err(Error::LoadFailed(_))
        ));
        assert!(matches!(End::Refused.outcome(), Err(Error::LoadFailed(_))));
        assert!(matches!(End::Crashed.outcome(), Err(Error::Crashed)));
    }
}
