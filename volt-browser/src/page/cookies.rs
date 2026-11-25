use url::Url;

use super::Page;
use crate::{Cookie, Error};

impl Page {
    /// The cookies the browser sends to the page's current URL, `HttpOnly` ones included.
    pub async fn cookies(&self) -> Result<Vec<Cookie>, Error> {
        let url = self.url().await?;
        self.engine().cookies(&url).await
    }

    /// The cookies the browser would send to `url`, `HttpOnly` ones included.
    pub async fn cookies_for(&self, url: &str) -> Result<Vec<Cookie>, Error> {
        let url = Url::parse(url).map_err(|_| Error::InvalidUrl)?;
        self.engine().cookies(&url).await
    }

    /// The cookie called `name` among those of the page's current URL.
    pub async fn cookie(&self, name: &str) -> Result<Option<Cookie>, Error> {
        let cookies = self.cookies().await?;
        Ok(cookies.into_iter().find(|cookie| cookie.name == name))
    }

    /// Adds `cookie` to the page's store, in place of one with its name, domain and path.
    pub async fn set_cookie(&self, cookie: &Cookie) -> Result<(), Error> {
        self.engine().set_cookie(cookie).await
    }

    /// Removes every cookie called `name` among those of the page's current URL.
    pub async fn delete_cookie(&self, name: &str) -> Result<(), Error> {
        for cookie in self.cookies().await? {
            if cookie.name == name {
                self.engine().delete_cookie(&cookie).await?;
            }
        }
        Ok(())
    }

    /// Removes every cookie of the page's current URL.
    pub async fn clear_cookies(&self) -> Result<(), Error> {
        for cookie in self.cookies().await? {
            self.engine().delete_cookie(&cookie).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::json;

    use crate::fake::Fake;
    use crate::{Cookie, Error};

    /// A page on `https://www.example.com/shop` whose store holds three cookies.
    struct Shop;

    impl Shop {
        /// Returns the fake engine of the page, which answers the URL as often as asked.
        fn open() -> Arc<Fake> {
            let fake = Fake::repeating(
                json!({"status": "ok", "json": "\"https://www.example.com/shop\""}),
            );
            fake.store(Cookie::new("cart", "3", "www.example.com"));
            fake.store(Cookie {
                http_only: true,
                ..Cookie::new("session", "abc", "example.com")
            });
            fake.store(Cookie::new("other", "1", "example.org"));
            fake
        }
    }

    #[tokio::test]
    async fn the_cookies_of_the_current_url_are_read_from_the_engine() {
        let fake = Shop::open();
        let page = fake.page();
        let names: Vec<_> = page
            .cookies()
            .await
            .unwrap()
            .into_iter()
            .map(|cookie| cookie.name)
            .collect();
        assert_eq!(names, ["cart", "session"]);
        assert_eq!(page.cookie("session").await.unwrap().unwrap().value, "abc");
        assert_eq!(page.cookie("absent").await.unwrap(), None);
    }

    #[tokio::test]
    async fn the_cookies_of_another_url_need_no_page_there() {
        let fake = Shop::open();
        let page = fake.page();
        let elsewhere = page.cookies_for("https://example.org/").await.unwrap();
        assert_eq!(elsewhere.len(), 1);
        assert_eq!(elsewhere[0].name, "other");
        assert!(matches!(
            page.cookies_for("nowhere").await,
            Err(Error::InvalidUrl)
        ));
        assert_eq!(fake.commands().len(), 0);
    }

    #[tokio::test]
    async fn setting_and_deleting_change_the_store() {
        let fake = Shop::open();
        let page = fake.page();
        page.set_cookie(&Cookie::new("cart", "4", "www.example.com"))
            .await
            .unwrap();
        assert_eq!(page.cookie("cart").await.unwrap().unwrap().value, "4");
        page.delete_cookie("cart").await.unwrap();
        assert_eq!(page.cookie("cart").await.unwrap(), None);
        assert_eq!(fake.stored().len(), 2);
    }

    #[tokio::test]
    async fn clearing_removes_the_cookies_of_the_current_url_only() {
        let fake = Shop::open();
        fake.page().clear_cookies().await.unwrap();
        let left: Vec<_> = fake
            .stored()
            .into_iter()
            .map(|cookie| cookie.name)
            .collect();
        assert_eq!(left, ["other"]);
    }
}
