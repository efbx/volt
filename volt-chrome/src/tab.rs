use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

use async_trait::async_trait;
use chromiumoxide::Page;
use chromiumoxide::cdp::browser_protocol::network::{
    Cookie, CookieParam, CookieSameSite, DeleteCookiesParams, GetCookiesParams, SetCookiesParams,
    TimeSinceEpoch,
};
use chromiumoxide::cdp::js_protocol::runtime::EvaluateParams;
use serde_json::Value;
use url::Url;
use volt_browser::{Engine, SameSite};

use super::error::Error;

/// One Chrome tab as a page of [`volt_browser`].
#[derive(Debug)]
pub(super) struct Tab {
    /// The chromiumoxide page.
    page: Page,
    /// Whether the browser has a window to bring forward.
    headed: bool,
    /// Whether the page was closed, which Chrome answers for only once the tab is gone.
    closed: AtomicBool,
}

impl Tab {
    /// Returns the tab around `page` of a browser that has a window when `headed`.
    pub(super) fn new(page: Page, headed: bool) -> Self {
        Self {
            page,
            headed,
            closed: AtomicBool::new(false),
        }
    }

    /// The chromiumoxide page, until it is closed.
    fn page(&self) -> Result<&Page, volt_browser::Error> {
        if self.closed.load(Ordering::Relaxed) {
            Err(volt_browser::Error::Closed)
        } else {
            Ok(&self.page)
        }
    }

    /// Returns Chrome's `cookie` as the shared kind: without the leading dot of a domain cookie,
    /// and with no end for a session cookie.
    fn shared(cookie: Cookie) -> volt_browser::Cookie {
        let mut domain = cookie.domain;
        if domain.starts_with('.') {
            domain.remove(0);
        }
        let expires = if cookie.session {
            None
        } else {
            Duration::try_from_secs_f64(cookie.expires)
                .ok()
                .and_then(|since| SystemTime::UNIX_EPOCH.checked_add(since))
        };
        volt_browser::Cookie {
            name: cookie.name,
            value: cookie.value,
            domain,
            path: cookie.path,
            secure: cookie.secure,
            http_only: cookie.http_only,
            same_site: cookie.same_site.map(|same_site| match same_site {
                CookieSameSite::Strict => SameSite::Strict,
                CookieSameSite::Lax => SameSite::Lax,
                CookieSameSite::None => SameSite::None,
            }),
            expires,
        }
    }

    /// Returns `cookie` as Chrome takes one to store.
    fn param(cookie: &volt_browser::Cookie) -> CookieParam {
        let mut param = CookieParam::new(cookie.name.as_str(), cookie.value.as_str());
        param.domain = Some(cookie.domain.clone());
        if !cookie.path.is_empty() {
            param.path = Some(cookie.path.clone());
        }
        param.secure = Some(cookie.secure);
        param.http_only = Some(cookie.http_only);
        param.same_site = cookie.same_site.map(|same_site| match same_site {
            SameSite::Strict => CookieSameSite::Strict,
            SameSite::Lax => CookieSameSite::Lax,
            SameSite::None => CookieSameSite::None,
        });
        param.expires = cookie
            .expires
            .and_then(|expires| expires.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map(|since| TimeSinceEpoch::new(since.as_secs_f64()));
        param
    }

    /// A URL `cookie` is sent to, which names the cookie to Chrome whether it keeps the domain
    /// with a leading dot or without.
    fn address(cookie: &volt_browser::Cookie) -> String {
        let scheme = if cookie.secure { "https" } else { "http" };
        format!("{scheme}://{}{}", cookie.domain, Self::path(cookie))
    }

    /// The path `cookie` is kept under: its own, or `/` where it names none.
    fn path(cookie: &volt_browser::Cookie) -> &str {
        if cookie.path.is_empty() {
            "/"
        } else {
            cookie.path.as_str()
        }
    }
}

#[async_trait]
impl Engine for Tab {
    async fn goto(&self, url: &Url) -> Result<(), volt_browser::Error> {
        self.page()?.goto(url.as_str()).await.map_err(Error::Cdp)?;
        Ok(())
    }

    async fn evaluate(&self, script: &str) -> Result<Value, volt_browser::Error> {
        let mut params = EvaluateParams::new(script);
        params.await_promise = Some(false);
        let answer = self
            .page()?
            .evaluate_expression(params)
            .await
            .map_err(Error::Cdp)?;
        Ok(answer.into_value().unwrap_or(Value::Null))
    }

    async fn cookies(&self, url: &Url) -> Result<Vec<volt_browser::Cookie>, volt_browser::Error> {
        let params = GetCookiesParams {
            urls: Some(vec![url.as_str().to_owned()]),
        };
        let found = self.page()?.execute(params).await.map_err(Error::Cdp)?;
        Ok(found.result.cookies.into_iter().map(Self::shared).collect())
    }

    async fn set_cookie(&self, cookie: &volt_browser::Cookie) -> Result<(), volt_browser::Error> {
        let params = SetCookiesParams::new(vec![Self::param(cookie)]);
        self.page()?.execute(params).await.map_err(Error::Cdp)?;
        Ok(())
    }

    async fn delete_cookie(
        &self,
        cookie: &volt_browser::Cookie,
    ) -> Result<(), volt_browser::Error> {
        let mut params = DeleteCookiesParams::new(cookie.name.as_str());
        params.url = Some(Self::address(cookie));
        params.path = Some(Self::path(cookie).to_owned());
        self.page()?.execute(params).await.map_err(Error::Cdp)?;
        Ok(())
    }

    async fn set_visible(&self, visible: bool) -> Result<(), volt_browser::Error> {
        let page = self.page()?;
        match (visible, self.headed) {
            (true, true) => {
                page.bring_to_front().await.map_err(Error::Cdp)?;
                Ok(())
            }
            (false, false) => Ok(()),
            (true, false) => Err(volt_browser::Error::Unsupported(
                "showing a Chrome that runs without a window",
            )),
            (false, true) => Err(volt_browser::Error::Unsupported("hiding a Chrome window")),
        }
    }

    async fn close(&self) -> Result<(), volt_browser::Error> {
        self.page()?.clone().close().await.map_err(Error::Cdp)?;
        self.closed.store(true, Ordering::Relaxed);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, SystemTime};

    use chromiumoxide::cdp::browser_protocol::network::{Cookie, CookieSameSite};
    use serde_json::json;
    use volt_browser::SameSite;

    use super::Tab;

    /// Chrome's cookies, as its protocol words them.
    struct Wire;

    impl Wire {
        /// Returns the cookie `token=abc` Chrome reports for `domain`, ending at `expires` unless
        /// it is a session cookie.
        fn cookie(domain: &str, session: bool, expires: f64) -> Cookie {
            serde_json::from_value(json!({
                "name": "token",
                "value": "abc",
                "domain": domain,
                "path": "/",
                "expires": expires,
                "size": 8,
                "httpOnly": true,
                "secure": true,
                "session": session,
                "sameSite": "Lax",
                "priority": "Medium",
                "sourceScheme": "Secure",
                "sourcePort": 443
            }))
            .unwrap()
        }
    }

    #[test]
    fn a_session_cookie_reads_without_the_leading_dot_and_without_an_end() {
        let shared = Tab::shared(Wire::cookie(".example.com", true, -1.0));
        assert_eq!(shared.domain, "example.com");
        assert!(shared.http_only && shared.secure);
        assert_eq!(shared.same_site, Some(SameSite::Lax));
        assert_eq!(shared.expires, None);
    }

    #[test]
    fn a_lasting_cookie_reads_with_its_end() {
        let shared = Tab::shared(Wire::cookie("example.com", false, 784_111_777.0));
        assert_eq!(shared.domain, "example.com");
        assert_eq!(
            shared.expires,
            Some(SystemTime::UNIX_EPOCH + Duration::from_secs(784_111_777))
        );
    }

    #[test]
    fn a_cookie_is_stored_by_its_domain_and_named_by_a_url_it_is_sent_to() {
        let lasting = Tab::shared(Wire::cookie("example.com", false, 784_111_777.0));
        let param = Tab::param(&lasting);
        assert_eq!(param.url, None);
        assert_eq!(param.domain.as_deref(), Some("example.com"));
        assert_eq!(param.path.as_deref(), Some("/"));
        assert_eq!(param.same_site, Some(CookieSameSite::Lax));
        assert_eq!(*param.expires.unwrap().inner(), 784_111_777.0);
        assert_eq!(Tab::address(&lasting), "https://example.com/");

        let plain = volt_browser::Cookie {
            path: String::new(),
            ..volt_browser::Cookie::new("theme", "dark", "example.com")
        };
        let param = Tab::param(&plain);
        assert_eq!(param.path, None);
        assert!(param.expires.is_none());
        assert_eq!(Tab::address(&plain), "http://example.com/");
    }
}
