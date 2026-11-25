use std::time::{Duration, SystemTime};

use http::StatusCode;
use serde_json::{Value, json};
use volt_browser::{Area, By, Cookie, Error, Load, Match, Session};

use super::checks::{Checks, Flaw, Outcome};
use super::site::Site;

/// The script that puts one answer into a cache called `pages`.
const CACHE: &str = "caches.open('pages').then(function (cache) { return cache.put('/cached', new Response('hello', { headers: { 'content-type': 'text/plain' } })); })";

impl Checks {
    /// The caller's own scripts run with their arguments, and promises are waited for.
    pub(super) async fn scripts(session: &Session, site: &Site) -> Outcome {
        let page = Self::open(session, site, "/late").await?;
        Flaw::same("a number", &page.evaluate::<u32>("20 + 22").await?, &42)?;
        Flaw::same(
            "a text",
            &page.evaluate::<String>("document.title").await?,
            "Late",
        )?;
        Flaw::same(
            "nothing",
            &page.evaluate::<Option<u8>>("undefined").await?,
            &None,
        )?;
        Flaw::same(
            "a promise",
            &page.evaluate::<u32>("later(150, 7)").await?,
            &7,
        )?;
        let refused = page.evaluate::<Value>("refuse()").await;
        Flaw::expect(
            "a rejected promise",
            &refused,
            matches!(&refused, Err(Error::Script(reason)) if reason.contains("refused")),
        )?;
        let thrown = page.evaluate::<Value>("nowhere.nothing").await;
        Flaw::expect(
            "a thrown error",
            &thrown,
            matches!(thrown, Err(Error::Script(_))),
        )?;
        let cyclic = page.evaluate::<Value>("window").await;
        Flaw::expect(
            "a value JSON cannot hold",
            &cyclic,
            matches!(cyclic, Err(Error::Script(_))),
        )?;
        let product: u32 = page
            .call("function (pair) { return pair[0] * pair[1]; }", &[6, 7])
            .await?;
        Flaw::same("a function", &product, &42)?;
        let quoted: String = page
            .call("function (text) { return text; }", "it's \"quoted\"\n")
            .await?;
        Flaw::same("an argument with quotes", &quoted, "it's \"quoted\"\n")?;
        let eager: String = page
            .call(
                "async function (word) { await later(50); return word + '!'; }",
                "hi",
            )
            .await?;
        Flaw::same("an async function", &eager, "hi!")?;
        let named: String = page
            .find(By::id("soon"))
            .call(
                "function (button, suffix) { return button.id + suffix; }",
                "-x",
            )
            .await?;
        Flaw::same("a function on an element", &named, "soon-x")?;
        page.evaluate::<()>("void document.addEventListener('ping', function (event) { document.title = 'ping ' + event.detail; })")
            .await?;
        page.find(By::tag("body"))
            .dispatch("ping", &json!({"detail": 3}))
            .await?;
        Flaw::same("a sent event", &page.title().await?, "ping 3")?;
        let agent = page.user_agent().await?;
        Flaw::expect("user agent", &agent, agent.contains("Mozilla"))?;
        Ok(page.close().await?)
    }

    /// Pages load, go back and forth, reload and take new HTML.
    pub(super) async fn navigation(session: &Session, site: &Site) -> Outcome {
        let page = Self::open(session, site, "/list").await?;
        Flaw::same("title", &page.title().await?, "List")?;
        Flaw::same("url", page.url().await?.path(), "/list")?;
        let source = page.html().await?;
        Flaw::expect(
            "html",
            &source.len(),
            source.contains("<title>List</title>"),
        )?;
        page.find(By::link("Next page")).click().await?;
        page.wait_for_url(Match::Exact(&site.url("/next"))).await?;
        page.wait_for_load(Load::Complete).await?;
        Flaw::same("a followed link", &page.title().await?, "Next")?;
        page.back().await?;
        Flaw::same("back", &page.title().await?, "List")?;
        page.forward().await?;
        Flaw::same("forward", &page.title().await?, "Next")?;
        let before: String = page.evaluate("String(performance.timeOrigin)").await?;
        page.reload().await?;
        let after: String = page.evaluate("String(performance.timeOrigin)").await?;
        Flaw::expect("reload", &after, before != after)?;
        Flaw::same("reloaded page", &page.title().await?, "Next")?;
        page.goto(&site.url_elsewhere("/list")).await?;
        page.back().await?;
        Flaw::same("back from another site", &page.title().await?, "Next")?;
        page.forward().await?;
        Flaw::same("forward to another site", &page.title().await?, "List")?;
        page.wait_for_url(Match::Prefix("http://")).await?;
        page.wait_for_load(Load::Interactive).await?;
        page.stop().await?;
        let unparsed = page.goto("nowhere").await;
        Flaw::expect(
            "a URL that does not parse",
            &unparsed,
            matches!(unparsed, Err(Error::InvalidUrl)),
        )?;
        page.set_html("<title>Fresh</title><p id=\"new\">written</p>")
            .await?;
        Flaw::same("new html title", &page.title().await?, "Fresh")?;
        Flaw::same(
            "new html",
            &page.find(By::id("new")).text().await?,
            "written",
        )?;
        page.close().await?;
        let first = Self::open(session, site, "/list").await?;
        first.back().await?;
        Flaw::same(
            "back from the first page",
            first.url().await?.as_str(),
            "about:blank",
        )?;
        Ok(first.close().await?)
    }

    /// A page closed through one clone fails at once through another.
    pub(super) async fn closing(session: &Session, site: &Site) -> Outcome {
        let page = Self::open(session, site, "/list").await?;
        let same = page.clone();
        page.close().await?;
        let moved = same.goto(&site.url("/next")).await;
        Flaw::expect(
            "a goto on a closed page",
            &moved,
            matches!(moved, Err(Error::Closed)),
        )?;
        let read = same.find(By::tag("h1")).text().await;
        Flaw::expect(
            "a read on a closed page",
            &read,
            matches!(read, Err(Error::Closed)),
        )
    }

    /// Cookies are read with the ones scripts cannot see, and set and removed.
    pub(super) async fn cookies(session: &Session, site: &Site) -> Outcome {
        let page = Self::open(session, site, "/state").await?;
        let token = page.cookie("token").await?;
        Flaw::expect(
            "an HttpOnly cookie",
            &token,
            token
                .as_ref()
                .is_some_and(|cookie| cookie.http_only && cookie.value == "abc"),
        )?;
        let theme = page.cookie("theme").await?;
        Flaw::expect(
            "a plain cookie",
            &theme,
            theme
                .as_ref()
                .is_some_and(|cookie| !cookie.http_only && cookie.domain == Site::HOST),
        )?;
        let seen: String = page.evaluate("document.cookie").await?;
        Flaw::expect(
            "what scripts see",
            &seen,
            seen.contains("theme=dark") && !seen.contains("token"),
        )?;
        let elsewhere = page.cookies_for(&site.url("/other/path")).await?;
        Flaw::same("cookies for a URL", &elsewhere.len(), &2)?;
        Flaw::same(
            "cookies of another site",
            &page.cookies_for("https://example.com/").await?.len(),
            &0,
        )?;

        page.set_cookie(&Cookie::new("added", "1", Site::HOST))
            .await?;
        let seen: String = page.evaluate("document.cookie").await?;
        Flaw::expect("a set cookie", &seen, seen.contains("added=1"))?;
        let kept = Cookie {
            http_only: true,
            expires: Some(SystemTime::now() + Duration::from_secs(3600)),
            ..Cookie::new("kept", "2", Site::HOST)
        };
        page.set_cookie(&kept).await?;
        let read = page.cookie("kept").await?;
        Flaw::expect(
            "a lasting cookie",
            &read,
            read.as_ref().is_some_and(|cookie| {
                cookie.http_only && cookie.expires.is_some() && cookie.value == "2"
            }),
        )?;
        page.set_cookie(&Cookie::new("added", "2", Site::HOST))
            .await?;
        Flaw::same(
            "a replaced cookie",
            &page
                .cookie("added")
                .await?
                .map(|cookie| cookie.value)
                .as_deref(),
            &Some("2"),
        )?;
        Flaw::same("all cookies", &page.cookies().await?.len(), &4)?;
        page.delete_cookie("theme").await?;
        Flaw::same("a deleted cookie", &page.cookie("theme").await?, &None)?;
        Flaw::same("the others stay", &page.cookies().await?.len(), &3)?;
        let elsewhere = Cookie {
            path: "/list".to_owned(),
            ..Cookie::new("pair", "2", Site::HOST)
        };
        page.set_cookie(&Cookie::new("pair", "1", Site::HOST))
            .await?;
        page.set_cookie(&elsewhere).await?;
        page.delete_cookie("pair").await?;
        let list = site.url("/list");
        let left = page.cookies_for(&list).await?;
        Flaw::same(
            "one of two cookies of a name, deleted by its path",
            &left
                .iter()
                .find(|cookie| cookie.name == "pair")
                .map(|cookie| cookie.value.as_str()),
            &Some("2"),
        )?;
        page.goto(&list).await?;
        page.clear_cookies().await?;
        Flaw::same("cleared cookies", &page.cookies().await?.len(), &0)?;
        Ok(page.close().await?)
    }

    /// Both stores are read and written, and a snapshot carries everything out and back.
    pub(super) async fn storage(session: &Session, site: &Site) -> Outcome {
        let page = Self::open(session, site, "/state").await?;
        let local = page.storage(Area::Local);
        let kept = page.storage(Area::Session);
        Flaw::same(
            "stored json",
            &local.get_json::<Value>("user").await?,
            &Some(json!({"name": "ada"})),
        )?;
        Flaw::same(
            "session value",
            &kept.get("step").await?.as_deref(),
            &Some("1"),
        )?;
        Flaw::same("missing key", &local.get("absent").await?, &None)?;
        local.set("theme", "dark").await?;
        local.set_json("visits", &[1, 2]).await?;
        Flaw::same(
            "set then get",
            &local.get("theme").await?.as_deref(),
            &Some("dark"),
        )?;
        Flaw::same(
            "json round trip",
            &local.get_json::<Vec<u8>>("visits").await?,
            &Some(vec![1, 2]),
        )?;
        Flaw::same("entries", &local.entries().await?.len(), &3)?;
        local.remove("visits").await?;
        Flaw::same("removed", &local.get("visits").await?, &None)?;

        let snapshot = page.snapshot().await?;
        Flaw::same(
            "snapshot url",
            snapshot.url.as_str(),
            site.url("/state").as_str(),
        )?;
        Flaw::same("snapshot cookies", &snapshot.cookies.len(), &2)?;
        Flaw::same("snapshot local", &snapshot.local.len(), &2)?;
        Flaw::same(
            "snapshot session",
            &snapshot.session,
            &[("step".to_owned(), "1".to_owned())][..],
        )?;
        local.clear().await?;
        kept.clear().await?;
        page.clear_cookies().await?;
        Flaw::same("cleared store", &local.entries().await?.len(), &0)?;
        page.restore(&snapshot).await?;
        Flaw::same(
            "restored local",
            &local.get("theme").await?.as_deref(),
            &Some("dark"),
        )?;
        Flaw::same(
            "restored session",
            &kept.get("step").await?.as_deref(),
            &Some("1"),
        )?;
        let token = page.cookie("token").await?;
        Flaw::expect(
            "restored cookie",
            &token,
            token.as_ref().is_some_and(|cookie| cookie.http_only),
        )?;
        Ok(page.close().await?)
    }

    /// The site's caches are listed, read and removed.
    pub(super) async fn caches(session: &Session, site: &Site) -> Outcome {
        let page = Self::open(session, site, "/state").await?;
        let caches = page.caches();
        Flaw::same("no caches yet", &caches.names().await?.len(), &0)?;
        page.evaluate::<()>(CACHE).await?;
        Flaw::same("names", &caches.names().await?, &["pages"][..])?;
        let address = site.url("/cached");
        Flaw::same(
            "urls",
            &caches.urls("pages").await?,
            &[address.as_str()][..],
        )?;
        Flaw::same("urls of no cache", &caches.urls("absent").await?.len(), &0)?;
        let answer = caches.get("pages", &address).await?;
        let Some(answer) = answer else {
            return Flaw::expect("a cached answer", &"none", false);
        };
        Flaw::same("cached status", &answer.status(), &StatusCode::OK)?;
        Flaw::same(
            "cached header",
            &answer
                .headers()
                .get("content-type")
                .and_then(|kind| kind.to_str().ok()),
            &Some("text/plain"),
        )?;
        Flaw::same("cached body", answer.body().as_ref(), &b"hello"[..])?;
        Flaw::same(
            "nothing cached",
            &caches.get("pages", &site.url("/other")).await?.is_none(),
            &true,
        )?;
        Flaw::same(
            "nothing in no cache",
            &caches.get("absent", &address).await?.is_none(),
            &true,
        )?;
        Flaw::same("remove", &caches.remove("pages", &address).await?, &true)?;
        Flaw::same(
            "remove again",
            &caches.remove("pages", &address).await?,
            &false,
        )?;
        Flaw::same("delete", &caches.delete("pages").await?, &true)?;
        Flaw::same("delete again", &caches.delete("pages").await?, &false)?;
        Ok(page.close().await?)
    }
}
