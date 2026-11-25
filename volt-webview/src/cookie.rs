#[cfg(not(any(target_os = "macos", target_os = "windows")))]
use std::time::Duration;
use std::time::SystemTime;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
use gtk::glib::DateTime;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
use soup::SameSitePolicy;
use volt_browser::{Cookie, SameSite};
#[cfg(any(target_os = "macos", target_os = "windows"))]
use wry::cookie::{self, CookieBuilder, time::OffsetDateTime};

/// The cookies of the platform's store, turned into the shared kind and back.
pub(crate) struct Stored;

#[cfg(any(target_os = "macos", target_os = "windows"))]
impl Stored {
    /// Returns the store's `cookie` as the shared kind; an attribute the store does not report
    /// is empty or false.
    pub(crate) fn shared(cookie: &cookie::Cookie<'_>) -> Cookie {
        Cookie {
            name: cookie.name().to_owned(),
            value: cookie.value().to_owned(),
            domain: cookie.domain().unwrap_or_default().to_owned(),
            path: cookie.path().unwrap_or_default().to_owned(),
            secure: cookie.secure().unwrap_or_default(),
            http_only: cookie.http_only().unwrap_or_default(),
            same_site: cookie.same_site().map(|same_site| match same_site {
                cookie::SameSite::Strict => SameSite::Strict,
                cookie::SameSite::Lax => SameSite::Lax,
                cookie::SameSite::None => SameSite::None,
            }),
            expires: cookie.expires_datetime().map(SystemTime::from),
        }
    }

    /// Returns the shared `cookie` as the store takes one; an empty path means every path. A
    /// flag that is off is left out, since `WKWebView`'s store reads a flag that is merely
    /// present as on.
    pub(crate) fn native(cookie: &Cookie) -> cookie::Cookie<'_> {
        let path = if cookie.path.is_empty() {
            "/"
        } else {
            cookie.path.as_str()
        };
        let mut native = CookieBuilder::new(cookie.name.as_str(), cookie.value.as_str())
            .domain(cookie.domain.as_str())
            .path(path);
        if cookie.secure {
            native = native.secure(true);
        }
        if cookie.http_only {
            native = native.http_only(true);
        }
        if let Some(same_site) = cookie.same_site {
            native = native.same_site(match same_site {
                SameSite::Strict => cookie::SameSite::Strict,
                SameSite::Lax => cookie::SameSite::Lax,
                SameSite::None => cookie::SameSite::None,
            });
        }
        if let Some(expires) = cookie.expires {
            native = native.expires(OffsetDateTime::from(expires));
        }
        native.build()
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
impl Stored {
    /// Returns the store's `cookie` as the shared kind, without the leading dot of a domain
    /// cookie.
    pub(crate) fn shared(mut cookie: soup::Cookie) -> Cookie {
        let domain = cookie.domain().unwrap_or_default();
        let expires = cookie
            .expires()
            .and_then(|expires| u64::try_from(expires.to_unix()).ok())
            .and_then(|seconds| SystemTime::UNIX_EPOCH.checked_add(Duration::from_secs(seconds)));
        Cookie {
            name: cookie.name().map(String::from).unwrap_or_default(),
            value: cookie.value().map(String::from).unwrap_or_default(),
            domain: domain.strip_prefix('.').unwrap_or(&domain).to_owned(),
            path: cookie.path().map(String::from).unwrap_or_default(),
            secure: cookie.is_secure(),
            http_only: cookie.is_http_only(),
            same_site: match cookie.same_site_policy() {
                SameSitePolicy::Strict => Some(SameSite::Strict),
                SameSitePolicy::Lax => Some(SameSite::Lax),
                SameSitePolicy::None => Some(SameSite::None),
                _ => None,
            },
            expires,
        }
    }

    /// Returns the shared `cookie` as the store takes one; an empty path means every path.
    pub(crate) fn native(cookie: &Cookie) -> soup::Cookie {
        let path = if cookie.path.is_empty() {
            "/"
        } else {
            cookie.path.as_str()
        };
        let mut native = soup::Cookie::new(&cookie.name, &cookie.value, &cookie.domain, path, -1);
        native.set_secure(cookie.secure);
        native.set_http_only(cookie.http_only);
        if let Some(same_site) = cookie.same_site {
            native.set_same_site_policy(match same_site {
                SameSite::Strict => SameSitePolicy::Strict,
                SameSite::Lax => SameSitePolicy::Lax,
                SameSite::None => SameSitePolicy::None,
            });
        }
        let ends = cookie
            .expires
            .and_then(|expires| expires.duration_since(SystemTime::UNIX_EPOCH).ok())
            .and_then(|since| i64::try_from(since.as_secs()).ok())
            .and_then(|seconds| DateTime::from_unix_utc(seconds).ok());
        if let Some(ends) = ends {
            native.set_expires(&ends);
        }
        native
    }
}

#[cfg(test)]
mod tests {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    use std::time::{Duration, SystemTime};

    use volt_browser::Cookie;
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    use volt_browser::SameSite;
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    use wry::cookie;

    use super::Stored;

    /// The `HttpOnly` cookie `token=abc` for `example.com` on every path.
    struct Token;

    impl Token {
        /// Returns the cookie.
        fn cookie() -> Cookie {
            Cookie {
                secure: true,
                http_only: true,
                ..Cookie::new("token", "abc", "example.com")
            }
        }
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    #[test]
    fn a_wry_cookie_keeps_its_parts_and_flags_without_the_leading_dot() {
        let cookie = cookie::Cookie::build(("token", "abc"))
            .domain(".example.com")
            .path("/")
            .secure(true)
            .http_only(true)
            .build();

        assert_eq!(Stored::shared(&cookie), Token::cookie());
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    #[test]
    fn an_attribute_the_store_does_not_report_is_empty_or_false() {
        let cookie = Stored::shared(&cookie::Cookie::new("bare", "1"));

        assert_eq!(
            cookie,
            Cookie {
                path: String::new(),
                ..Cookie::new("bare", "1", "")
            }
        );
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    #[test]
    fn a_shared_cookie_survives_the_trip_into_the_store_and_back() {
        let lasting = Cookie {
            same_site: Some(SameSite::Strict),
            expires: Some(SystemTime::UNIX_EPOCH + Duration::from_secs(784_111_777)),
            ..Token::cookie()
        };
        assert_eq!(Stored::shared(&Stored::native(&lasting)), lasting);
        assert_eq!(
            Stored::shared(&Stored::native(&Token::cookie())),
            Token::cookie()
        );
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    #[test]
    fn an_empty_path_is_stored_as_every_path() {
        let pathless = Cookie {
            path: String::new(),
            ..Token::cookie()
        };
        assert_eq!(Stored::native(&pathless).path(), Some("/"));
    }

    #[cfg(any(
        target_os = "linux",
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd"
    ))]
    #[test]
    fn a_soup_cookie_keeps_its_parts_and_flags_without_the_leading_dot() {
        let mut cookie = soup::Cookie::new("token", "abc", ".example.com", "/", -1);
        cookie.set_secure(true);
        cookie.set_http_only(true);

        let shared = Stored::shared(cookie);
        assert_eq!(
            Cookie {
                same_site: None,
                ..shared
            },
            Token::cookie()
        );
    }

    #[cfg(any(
        target_os = "linux",
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd"
    ))]
    #[test]
    fn a_shared_cookie_becomes_a_soup_cookie_with_its_flags() {
        let mut native = Stored::native(&Token::cookie());
        assert_eq!(native.domain().as_deref(), Some("example.com"));
        assert!(native.is_secure() && native.is_http_only());
    }
}
