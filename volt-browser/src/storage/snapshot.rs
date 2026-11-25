use serde::{Deserialize, Serialize};

use super::Area;
use crate::{Cookie, Error, Page};

/// Everything a site keeps in the browser for a page, taken in one go: the way to carry a
/// signed-in state out of a browser, save it, and put it back later. It holds secrets, so treat
/// a saved one like a password.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    /// The URL the page was on.
    pub url: String,
    /// The cookies the browser sends to that URL, `HttpOnly` ones included.
    pub cookies: Vec<Cookie>,
    /// The site's `localStorage`.
    pub local: Vec<(String, String)>,
    /// The site's `sessionStorage`.
    pub session: Vec<(String, String)>,
}

impl Page {
    /// Takes the cookies and both stores of the page's current site.
    pub async fn snapshot(&self) -> Result<Snapshot, Error> {
        let url = self.url().await?;
        Ok(Snapshot {
            cookies: self.engine().cookies(&url).await?,
            url: url.into(),
            local: self.storage(Area::Local).entries().await?,
            session: self.storage(Area::Session).entries().await?,
        })
    }

    /// Puts the cookies and stored values of `snapshot` back, on top of what is there. The page
    /// has to be on the snapshot's site already, since a store belongs to the site a page is on.
    pub async fn restore(&self, snapshot: &Snapshot) -> Result<(), Error> {
        for cookie in &snapshot.cookies {
            self.set_cookie(cookie).await?;
        }
        let local = self.storage(Area::Local);
        for (key, value) in &snapshot.local {
            local.set(key, value).await?;
        }
        let session = self.storage(Area::Session);
        for (key, value) in &snapshot.session {
            session.set(key, value).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::Snapshot;
    use crate::Cookie;
    use crate::fake::Fake;

    #[tokio::test]
    async fn a_snapshot_reads_the_url_once_and_holds_its_cookies_and_both_stores() {
        let fake = Fake::answering([
            json!({"status": "ok", "json": "\"https://example.com/home\""}),
            json!({"status": "ok", "json": "[[\"theme\",\"dark\"]]"}),
            json!({"status": "ok", "json": "[]"}),
        ]);
        fake.store(Cookie::new("session", "abc", "example.com"));
        let snapshot = fake.page().snapshot().await.unwrap();
        assert_eq!(
            snapshot,
            Snapshot {
                url: "https://example.com/home".to_owned(),
                cookies: vec![Cookie::new("session", "abc", "example.com")],
                local: vec![("theme".to_owned(), "dark".to_owned())],
                session: Vec::new(),
            }
        );
    }

    #[tokio::test]
    async fn a_saved_snapshot_restores_what_it_held() {
        let saved = serde_json::to_string(&Snapshot {
            url: "https://example.com/home".to_owned(),
            cookies: vec![Cookie::new("session", "abc", "example.com")],
            local: vec![("theme".to_owned(), "dark".to_owned())],
            session: vec![("step".to_owned(), "2".to_owned())],
        })
        .unwrap();
        let snapshot: Snapshot = serde_json::from_str(&saved).unwrap();
        let fake = Fake::repeating(json!({"status": "ok", "json": null}));
        fake.page().restore(&snapshot).await.unwrap();
        assert_eq!(
            fake.stored(),
            [Cookie::new("session", "abc", "example.com")]
        );
        let commands = fake.commands();
        assert_eq!(
            commands[0]["arg"],
            json!({"area": "localStorage", "key": "theme", "value": "dark"})
        );
        assert_eq!(
            commands[1]["arg"],
            json!({"area": "sessionStorage", "key": "step", "value": "2"})
        );
    }
}
