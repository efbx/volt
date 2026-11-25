use std::rc::Rc;

use serde_json::Value;
use tokio::sync::oneshot::error::TryRecvError;
use tokio::sync::oneshot::{self, Receiver};
use url::Url;
use volt_browser::Cookie;

use super::{Pages, Setup};
use crate::Error;
use crate::platform::Fake;
use crate::script::Script;

/// A session's pages on the test thread, with page 1 open.
struct Opened;

impl Opened {
    /// Returns pages that open as `user_agent` says, without opening any.
    fn empty(user_agent: Option<&str>) -> Pages {
        Pages::new(Setup {
            visible: false,
            user_agent: user_agent.map(str::to_owned),
        })
    }

    /// Returns pages whose page 1 has opened.
    fn pages() -> Pages {
        let pages = Self::empty(None);
        let (reply, mut answer) = oneshot::channel();
        pages.open(1, reply);
        assert!(matches!(answer.try_recv(), Ok(Ok(()))));
        pages
    }

    /// Starts a goto to `url` on page 1 and returns where its answer arrives.
    fn goto(pages: &Pages, url: &str) -> Receiver<Result<(), Error>> {
        let (reply, answer) = oneshot::channel();
        pages.goto(1, url, reply);
        answer
    }

    /// Evaluates `expression` on page 1 and returns where its value arrives.
    fn evaluate(pages: &Pages, expression: &str) -> Receiver<Result<Value, Error>> {
        let (reply, answer) = oneshot::channel();
        pages.evaluate(1, &Script::wrap(expression), reply);
        answer
    }

    /// Reads the cookies page 1's store sends to `example.com` and returns where they arrive.
    fn cookies(pages: &Pages) -> Receiver<Result<Vec<Cookie>, Error>> {
        let (reply, answer) = oneshot::channel();
        let url = Url::parse("https://example.com/").unwrap();
        pages.cookies(1, &url, reply);
        answer
    }
}

#[test]
fn a_page_that_cannot_open_fails_its_caller() {
    let pages = Opened::empty(Some(Fake::REFUSED_AGENT));
    let (reply, mut answer) = oneshot::channel();

    pages.open(1, reply);
    assert!(matches!(answer.try_recv(), Ok(Err(Error::Webview(_)))));
    assert!(pages.tab(1).is_none());
}

#[test]
fn a_goto_is_answered_when_its_load_ends() {
    let pages = Opened::pages();

    let mut answer = Opened::goto(&pages, "https://example.com/");
    assert!(matches!(answer.try_recv(), Ok(Ok(()))));
}

#[test]
fn a_goto_that_never_ends_is_replaced_by_the_next() {
    let pages = Opened::pages();
    let mut stuck = Opened::goto(&pages, Fake::STUCK_URL);
    assert!(stuck.try_recv().is_err());

    let mut next = Opened::goto(&pages, "https://example.com/");
    assert!(matches!(stuck.try_recv(), Ok(Err(Error::Replaced))));
    assert!(matches!(next.try_recv(), Ok(Ok(()))));
}

#[test]
fn a_refused_url_fails_its_goto_and_leaves_no_wait_behind() {
    let pages = Opened::pages();

    let mut refused = Opened::goto(&pages, Fake::REFUSED_URL);
    assert!(matches!(refused.try_recv(), Ok(Err(Error::Webview(_)))));
    let mut next = Opened::goto(&pages, "https://example.com/");
    assert!(matches!(next.try_recv(), Ok(Ok(()))));
}

#[test]
fn a_failed_load_fails_its_goto() {
    let pages = Opened::pages();

    let mut failed = Opened::goto(&pages, Fake::FAILING_URL);
    assert!(matches!(failed.try_recv(), Ok(Err(Error::LoadFailed(_)))));
}

#[test]
fn a_goto_answered_with_the_blank_page_fails() {
    let pages = Opened::pages();

    let mut refused = Opened::goto(&pages, Fake::BLANKED_URL);
    assert!(matches!(refused.try_recv(), Ok(Err(Error::LoadFailed(_)))));
}

#[test]
fn a_goto_to_the_blank_page_is_answered_by_it() {
    let pages = Opened::pages();

    let mut blank = Opened::goto(&pages, Fake::BLANK_URL);
    assert!(matches!(blank.try_recv(), Ok(Ok(()))));
}

#[test]
fn the_end_of_the_content_process_fails_the_goto() {
    let pages = Opened::pages();

    let mut crashed = Opened::goto(&pages, Fake::CRASHING_URL);
    assert!(matches!(crashed.try_recv(), Ok(Err(Error::Crashed))));
}

#[test]
fn load_events_that_arrive_during_a_later_call_still_answer_their_goto() {
    let pages = Opened::pages();
    let mut late = Opened::goto(&pages, Fake::LATE_URL);
    assert!(late.try_recv().is_err());

    let mut cookies = Opened::cookies(&pages);
    assert!(matches!(late.try_recv(), Ok(Ok(()))));
    assert!(matches!(
        cookies.try_recv(),
        Ok(Ok(cookies)) if cookies == [Fake::cookie()]
    ));
}

#[test]
fn a_start_left_by_a_replaced_load_counts_for_the_goto_that_replaced_it() {
    let pages = Opened::pages();
    let mut late = Opened::goto(&pages, Fake::LATE_URL);

    let mut next = Opened::goto(&pages, Fake::STUCK_URL);
    assert!(matches!(late.try_recv(), Ok(Err(Error::Replaced))));
    assert!(matches!(next.try_recv(), Ok(Ok(()))));
}

#[test]
fn an_expression_is_answered_with_its_value() {
    let pages = Opened::pages();

    let mut two = Opened::evaluate(&pages, "2");
    assert!(matches!(two.try_recv(), Ok(Ok(value)) if value == 2));
    let mut nothing = Opened::evaluate(&pages, "undefined");
    assert!(matches!(nothing.try_recv(), Ok(Ok(Value::Null))));
}

#[test]
fn an_expression_the_webview_drops_drops_its_answer() {
    let pages = Opened::pages();

    let mut answer = Opened::evaluate(&pages, Fake::DROPPED);
    assert!(matches!(answer.try_recv(), Err(TryRecvError::Closed)));
}

#[test]
fn an_expression_the_webview_refuses_is_answered_with_its_error() {
    let pages = Opened::pages();

    let mut answer = Opened::evaluate(&pages, Fake::REFUSED);
    assert!(matches!(answer.try_recv(), Ok(Err(Error::Webview(_)))));
}

#[test]
fn closing_a_page_fails_its_goto_and_every_later_call() {
    let pages = Opened::pages();
    let mut stuck = Opened::goto(&pages, Fake::STUCK_URL);

    pages.close(1).unwrap();
    assert!(matches!(stuck.try_recv(), Ok(Err(Error::Closed))));
    let mut later = Opened::goto(&pages, "https://example.com/");
    assert!(matches!(later.try_recv(), Ok(Err(Error::Closed))));
    assert!(matches!(
        Opened::cookies(&pages).try_recv(),
        Ok(Err(Error::Closed))
    ));
    assert!(matches!(pages.set_visible(1, true), Err(Error::Closed)));
    assert!(matches!(pages.close(1), Err(Error::Closed)));
}

#[test]
fn a_page_in_use_outlives_its_removal() {
    let pages = Opened::pages();
    let in_use = pages.tab(1).unwrap();

    pages.close(1).unwrap();
    assert_eq!(Rc::strong_count(&in_use), 1);
    in_use.surface.set_visible(true);
    assert!(in_use.surface.is_visible());
}

#[test]
fn closing_the_session_fails_every_wait_and_refuses_new_pages() {
    let pages = Opened::pages();
    let mut stuck = Opened::goto(&pages, Fake::STUCK_URL);

    pages.close_all();
    assert!(matches!(stuck.try_recv(), Ok(Err(Error::Closed))));
    let (reply, mut answer) = oneshot::channel();
    pages.open(2, reply);
    assert!(matches!(answer.try_recv(), Ok(Err(Error::Closed))));
    assert!(pages.tab(2).is_none());
}

#[test]
fn a_hidden_window_can_be_shown_and_hidden_again() {
    let pages = Opened::pages();
    let tab = pages.tab(1).unwrap();
    assert!(!tab.surface.is_visible());

    pages.set_visible(1, true).unwrap();
    assert!(tab.surface.is_visible());
    pages.set_visible(1, false).unwrap();
    assert!(!tab.surface.is_visible());
}

#[test]
fn a_broken_store_fails_its_cookie_reads() {
    let pages = Opened::empty(Some(Fake::BROKEN_AGENT));
    let (reply, mut answer) = oneshot::channel();
    pages.open(1, reply);
    assert!(matches!(answer.try_recv(), Ok(Ok(()))));

    assert!(matches!(
        Opened::cookies(&pages).try_recv(),
        Ok(Err(Error::Webview(_)))
    ));
}

#[test]
fn an_unknown_page_is_closed() {
    let pages = Opened::pages();

    let (reply, mut answer) = oneshot::channel();
    pages.goto(9, "https://example.com/", reply);
    assert!(matches!(answer.try_recv(), Ok(Err(Error::Closed))));
    let (reply, mut answer) = oneshot::channel();
    pages.evaluate(9, &Script::wrap("2"), reply);
    assert!(matches!(answer.try_recv(), Ok(Err(Error::Closed))));
}
