use std::fmt;
use std::time::SystemTime;

use httpdate::HttpDate;
use serde::{Deserialize, Serialize};
use url::{Host, Url};

/// A cookie of a page's store, the same on every engine.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cookie {
    /// The cookie's name.
    pub name: String,
    /// The cookie's value.
    pub value: String,
    /// The domain the cookie belongs to, without the leading dot some stores write. A cookie set
    /// through any engine counts for its domain and the hosts under it. When reading, Chrome and
    /// `WebKitGTK` give a cookie a site set for its own host only to that host, while the
    /// webviews of macOS and Windows give it to the hosts under it too.
    pub domain: String,
    /// The path the cookie is sent for; empty or `/` for every path.
    pub path: String,
    /// Whether the cookie is only sent over HTTPS.
    pub secure: bool,
    /// Whether the cookie is hidden from scripts.
    pub http_only: bool,
    /// The cookie's rule for requests from other sites, where it carries one.
    pub same_site: Option<SameSite>,
    /// When the cookie ends; `None` for one that ends with the browser session.
    pub expires: Option<SystemTime>,
}

impl Cookie {
    /// Returns the session cookie `name=value` for `domain`, on every path.
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        value: impl Into<String>,
        domain: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
            domain: domain.into(),
            path: "/".to_owned(),
            secure: false,
            http_only: false,
            same_site: None,
            expires: None,
        }
    }

    /// Whether a browser sends the cookie with a request to `url`: the host, the path and the
    /// scheme all fit.
    #[must_use]
    pub fn applies_to(&self, url: &Url) -> bool {
        self.covers(url) && self.reaches(url.path()) && (!self.secure || Self::protects(url))
    }

    /// The cookie as the value of a `Set-Cookie` header, for a cookie jar that takes those.
    #[must_use]
    pub fn header(&self) -> SetCookie<'_> {
        SetCookie(self)
    }

    /// Whether the cookie's domain is `url`'s host or a parent of it.
    fn covers(&self, url: &Url) -> bool {
        let Some(host) = url.host_str() else {
            return false;
        };
        if host.eq_ignore_ascii_case(&self.domain) {
            return true;
        }
        let Some(cut) = host.len().checked_sub(self.domain.len() + 1) else {
            return false;
        };
        host.is_char_boundary(cut)
            && host[cut..].starts_with('.')
            && host[cut + 1..].eq_ignore_ascii_case(&self.domain)
    }

    /// Whether the cookie's path is `path` or a folder above it.
    fn reaches(&self, path: &str) -> bool {
        let own = self.path.as_str();
        own.is_empty()
            || own == "/"
            || path
                .strip_prefix(own)
                .is_some_and(|rest| rest.is_empty() || own.ends_with('/') || rest.starts_with('/'))
    }

    /// Whether `url` may carry a `Secure` cookie: HTTPS, or this machine itself.
    fn protects(url: &Url) -> bool {
        matches!(url.scheme(), "https" | "wss")
            || match url.host() {
                Some(Host::Domain(domain)) => domain.eq_ignore_ascii_case("localhost"),
                Some(Host::Ipv4(address)) => address.is_loopback(),
                Some(Host::Ipv6(address)) => address.is_loopback(),
                None => false,
            }
    }
}

impl fmt::Display for Cookie {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}={}", self.name, self.value)
    }
}

/// A cookie's rule for requests that come from another site.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SameSite {
    /// Never sent with requests from another site.
    Strict,
    /// Sent when a person follows a link from another site.
    Lax,
    /// Sent with every request.
    None,
}

impl fmt::Display for SameSite {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Strict => "Strict",
            Self::Lax => "Lax",
            Self::None => "None",
        })
    }
}

/// A cookie written as the value of a `Set-Cookie` header.
#[derive(Clone, Copy, Debug)]
pub struct SetCookie<'a>(&'a Cookie);

impl fmt::Display for SetCookie<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let cookie = self.0;
        write!(f, "{cookie}")?;
        if !cookie.domain.is_empty() {
            write!(f, "; Domain={}", cookie.domain)?;
        }
        if !cookie.path.is_empty() {
            write!(f, "; Path={}", cookie.path)?;
        }
        if cookie.secure {
            f.write_str("; Secure")?;
        }
        if cookie.http_only {
            f.write_str("; HttpOnly")?;
        }
        if let Some(same_site) = cookie.same_site {
            write!(f, "; SameSite={same_site}")?;
        }
        if let Some(expires) = cookie.expires {
            write!(f, "; Expires={}", HttpDate::from(expires))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, SystemTime};

    use url::Url;

    use super::{Cookie, SameSite};

    /// Cookies and the URLs they are checked against.
    struct Jar;

    impl Jar {
        /// Returns the cookie `token=abc` of `example.com`.
        fn domain_cookie() -> Cookie {
            Cookie::new("token", "abc", "example.com")
        }

        /// Whether `cookie` is sent to `url`.
        fn sends(cookie: &Cookie, url: &str) -> bool {
            cookie.applies_to(&Url::parse(url).unwrap())
        }
    }

    #[test]
    fn a_cookie_reaches_its_domain_and_the_hosts_under_it_only() {
        let cookie = Jar::domain_cookie();
        assert!(Jar::sends(&cookie, "https://example.com/"));
        assert!(Jar::sends(&cookie, "https://EXAMPLE.com/any/path"));
        assert!(!Jar::sends(&cookie, "https://example.org/"));
        assert!(Jar::sends(&cookie, "https://www.example.com/"));
        assert!(Jar::sends(&cookie, "https://a.b.example.com/"));
        assert!(!Jar::sends(&cookie, "https://notexample.com/"));
        assert!(!Jar::sends(&cookie, "https://com/"));
        assert!(!Jar::sends(&cookie, "data:text/plain,none"));
    }

    #[test]
    fn a_path_covers_itself_and_the_folders_below() {
        let cookie = Cookie {
            path: "/shop".to_owned(),
            ..Jar::domain_cookie()
        };
        assert!(Jar::sends(&cookie, "https://example.com/shop"));
        assert!(Jar::sends(&cookie, "https://example.com/shop/cart"));
        assert!(!Jar::sends(&cookie, "https://example.com/shopping"));
        assert!(!Jar::sends(&cookie, "https://example.com/"));
        let folder = Cookie {
            path: "/shop/".to_owned(),
            ..Jar::domain_cookie()
        };
        assert!(Jar::sends(&folder, "https://example.com/shop/cart"));
        let everywhere = Cookie {
            path: String::new(),
            ..Jar::domain_cookie()
        };
        assert!(Jar::sends(&everywhere, "https://example.com/shop"));
    }

    #[test]
    fn a_secure_cookie_needs_https_or_this_machine() {
        let cookie = Cookie {
            secure: true,
            ..Cookie::new("token", "abc", "example.com")
        };
        assert!(Jar::sends(&cookie, "https://example.com/"));
        assert!(!Jar::sends(&cookie, "http://example.com/"));
        let local = Cookie {
            secure: true,
            ..Cookie::new("token", "abc", "127.0.0.1")
        };
        assert!(Jar::sends(&local, "http://127.0.0.1:8080/"));
        let named = Cookie {
            secure: true,
            ..Cookie::new("token", "abc", "localhost")
        };
        assert!(Jar::sends(&named, "http://localhost/"));
    }

    #[test]
    fn a_cookie_shows_as_its_pair_and_as_a_set_cookie_header() {
        let plain = Cookie::new("token", "abc", "example.com");
        assert_eq!(plain.to_string(), "token=abc");
        assert_eq!(
            plain.header().to_string(),
            "token=abc; Domain=example.com; Path=/"
        );
        let bare = Cookie {
            domain: String::new(),
            path: String::new(),
            ..Cookie::new("token", "abc", "")
        };
        assert_eq!(bare.header().to_string(), "token=abc");
        let full = Cookie {
            secure: true,
            http_only: true,
            same_site: Some(SameSite::Lax),
            expires: Some(SystemTime::UNIX_EPOCH + Duration::from_secs(784_111_777)),
            ..Jar::domain_cookie()
        };
        assert_eq!(
            full.header().to_string(),
            "token=abc; Domain=example.com; Path=/; Secure; HttpOnly; SameSite=Lax; Expires=Sun, 06 Nov 1994 08:49:37 GMT"
        );
    }
}
