use volt_browser::{
    Blocker, By, Choice, Click, Error, Keystroke, Match, Modifiers, NamedKey, Point, Session, State,
};

use super::checks::{BRIEF, Checks, Flaw, Outcome, SETTLED};
use super::site::Site;

/// The function that lists the values of a `<select>`'s chosen options.
const CHOSEN: &str = "function (select) { return Array.from(select.selectedOptions).map(function (option) { return option.value; }); }";

impl Checks {
    /// Every kind of search finds what it should.
    pub(super) async fn selectors(session: &Session, site: &Site) -> Outcome {
        let page = Self::open(session, site, "/list").await?;
        Flaw::same("class", &page.find(By::class("tea")).count().await?, &4)?;
        Flaw::same("tag", &page.find(By::tag("li")).count().await?, &4)?;
        Flaw::same("id", &page.find(By::id("teas")).tag().await?, "ul")?;
        Flaw::same(
            "css",
            &page.find(By::css("ul > li.tea[hidden]")).count().await?,
            &1,
        )?;
        let second = page.find(By::xpath("//ul[@id='teas']/li[2]"));
        Flaw::same("xpath", &second.text().await?, "Black tea")?;
        Flaw::same("text", &page.find(By::text("Black tea")).tag().await?, "li")?;
        Flaw::same(
            "innermost text",
            &page.find(By::text("tea")).tag().await?,
            "b",
        )?;
        let green = page.find(By::text_containing("GREEN"));
        Flaw::same("text containing", &green.tag().await?, "li")?;
        Flaw::same(
            "button text",
            &page.find(By::text("Brew")).tag().await?,
            "input",
        )?;
        let link = page.find(By::link("Next page"));
        Flaw::same(
            "link",
            &link.attribute("href").await?.as_deref(),
            &Some("/next"),
        )?;
        Flaw::same(
            "link containing",
            &page.find(By::link_containing("NEXT")).count().await?,
            &1,
        )?;
        Flaw::same("alt", &page.find(By::alt("cup of")).tag().await?, "img")?;
        let broken = page.find(By::css("li[")).count().await;
        Flaw::expect(
            "broken selector",
            &broken,
            matches!(broken, Err(Error::InvalidSelector(_, _))),
        )?;
        page.close().await?;

        let page = Self::open(session, site, "/form").await?;
        let email = page.find(By::label("email ADDRESS"));
        Flaw::same(
            "label for",
            &email.attribute("name").await?.as_deref(),
            &Some("email"),
        )?;
        let password = page.find(By::label("password"));
        Flaw::same(
            "wrapping label",
            &password.attribute("name").await?.as_deref(),
            &Some("password"),
        )?;
        Flaw::same(
            "aria label",
            &page.find(By::label("note for")).tag().await?,
            "textarea",
        )?;
        let hinted = page.find(By::placeholder("you@"));
        Flaw::same(
            "placeholder",
            &hinted.attribute("id").await?.as_deref(),
            &Some("email"),
        )?;
        Flaw::same(
            "title",
            &page.find(By::title("counts clicks")).tag().await?,
            "button",
        )?;
        Flaw::same(
            "test id",
            &page.find(By::test_id("counter")).tag().await?,
            "button",
        )?;
        Flaw::same(
            "name",
            &page
                .find(By::name("age"))
                .attribute("type")
                .await?
                .as_deref(),
            &Some("number"),
        )?;
        Ok(page.close().await?)
    }

    /// Narrowing steps keep, pick and move between matches, and one match is expected.
    pub(super) async fn narrowing(session: &Session, site: &Site) -> Outcome {
        let page = Self::open(session, site, "/list").await?;
        let teas = page.find(By::class("tea"));
        Flaw::same("visible", &teas.clone().visible().count().await?, &3)?;
        Flaw::same("first", &teas.clone().first().text().await?, "Green tea")?;
        Flaw::same("nth", &teas.clone().nth(1).text().await?, "Black tea")?;
        Flaw::same("last", &teas.clone().last().text().await?, "Next page")?;
        Flaw::same(
            "with text",
            &teas.clone().with_text("BLACK").count().await?,
            &1,
        )?;
        Flaw::same(
            "without text",
            &teas.clone().without_text("tea").text().await?,
            "Next page",
        )?;
        Flaw::same(
            "having",
            &teas.clone().having(By::tag("b")).text().await?,
            "Green tea",
        )?;
        Flaw::same(
            "not having",
            &teas.clone().not_having(By::tag("a")).count().await?,
            &3,
        )?;
        Flaw::same(
            "inside",
            &teas.clone().find(By::tag("a")).text().await?,
            "Next page",
        )?;
        Flaw::same(
            "parent",
            &page.find(By::tag("b")).parent().tag().await?,
            "li",
        )?;
        let list = page.find(By::tag("b")).closest(By::tag("ul"));
        Flaw::same(
            "closest",
            &list.attribute("id").await?.as_deref(),
            &Some("teas"),
        )?;
        let css = page.find(By::tag("b")).closest(By::css("li.tea"));
        Flaw::same("closest css", &css.text().await?, "Green tea")?;
        let either = page.find(By::tag("h1")).or(page.find(By::tag("b")));
        Flaw::same("or", &either.texts().await?, &["Teas", "tea"][..])?;
        let shown = teas.clone().visible().texts().await?;
        Flaw::same(
            "texts",
            &shown,
            &["Green tea", "Black tea", "Next page"][..],
        )?;
        let each = teas.all().await?;
        Flaw::same("all", &each.len(), &4)?;
        Flaw::same("each of all", &each[1].text().await?, "Black tea")?;
        Flaw::same("hidden", &teas.clone().nth(2).is_visible().await?, &false)?;
        Flaw::same(
            "raw text",
            &teas.clone().nth(2).text_content().await?,
            "Hidden tea",
        )?;
        let several = teas.text().await;
        Flaw::expect(
            "several matches",
            &several,
            matches!(several, Err(Error::Ambiguous(_, 4))),
        )?;
        Flaw::same(
            "missing",
            &page.find(By::id("nope")).exists().await?,
            &false,
        )?;
        Flaw::same(
            "missing is not visible",
            &page.find(By::id("nope")).is_visible().await?,
            &false,
        )?;
        let absent = page.find(By::id("nope")).timeout(BRIEF).text().await;
        Flaw::expect(
            "never found",
            &absent,
            matches!(absent, Err(Error::NotFound(_))),
        )?;
        Ok(page.close().await?)
    }

    /// Commands that go by an element's text go by its visible text, not by a hidden part.
    pub(super) async fn visible_text(session: &Session, site: &Site) -> Outcome {
        let page = Self::open(session, site, "/list").await?;
        let note = page.find(By::id("note"));
        Flaw::same("text", &note.text().await?, "Fresh")?;
        Flaw::same(
            "with text, a hidden part",
            &note.clone().with_text("stale").count().await?,
            &0,
        )?;
        Flaw::same(
            "without text, a hidden part",
            &note.clone().without_text("stale").count().await?,
            &1,
        )?;
        let waited = note.timeout(BRIEF).wait_for_text("stale").await;
        Flaw::expect(
            "waiting for a hidden part",
            &waited,
            matches!(waited, Err(Error::Timeout(_))),
        )?;
        let brew = page.find(By::tag("input"));
        Flaw::same("a button's value is no text", &brew.text().await?, "")?;
        Flaw::same(
            "with text, a button's value",
            &brew.with_text("Brew").count().await?,
            &0,
        )?;
        Ok(page.close().await?)
    }

    /// Reading commands give what the page holds.
    pub(super) async fn reading(session: &Session, site: &Site) -> Outcome {
        let page = Self::open(session, site, "/form").await?;
        let counter = page.find(By::id("count"));
        Flaw::same("text", &counter.text().await?, "Clicked 0 times")?;
        Flaw::same("inner html", &counter.html().await?, "Clicked 0 times")?;
        let outer = counter.outer_html().await?;
        Flaw::expect(
            "outer html",
            &outer,
            outer.starts_with("<button") && outer.ends_with("</button>"),
        )?;
        Flaw::same("tag", &counter.tag().await?, "button")?;
        Flaw::same(
            "attribute",
            &counter.attribute("type").await?.as_deref(),
            &Some("button"),
        )?;
        Flaw::same("no attribute", &counter.attribute("href").await?, &None)?;
        let rect = counter.rect().await?;
        Flaw::expect("rect", &rect, rect.width > 0.0 && rect.height > 0.0)?;
        Flaw::same(
            "style",
            &page.find(By::id("log")).style("color").await?,
            "rgb(1, 2, 3)",
        )?;
        let locked = page.find(By::name("locked"));
        Flaw::same("value", &locked.value().await?, "fixed")?;
        Flaw::same(
            "property",
            &locked.property::<bool>("readOnly").await?,
            &true,
        )?;
        Flaw::same(
            "read-only is not editable",
            &locked.is_editable().await?,
            &false,
        )?;
        Flaw::same(
            "disabled",
            &page.find(By::name("off")).is_enabled().await?,
            &false,
        )?;
        Flaw::same(
            "editable",
            &page.find(By::id("email")).is_editable().await?,
            &true,
        )?;
        Flaw::same(
            "checked",
            &page.find(By::css("input[value=free]")).is_checked().await?,
            &true,
        )?;
        let english = page.find(By::css("select[name=language] option")).first();
        Flaw::same("selected", &english.is_selected().await?, &true)?;
        let unvalued = page.find(By::id("log")).value().await;
        Flaw::expect(
            "no value",
            &unvalued,
            matches!(unvalued, Err(Error::WrongElement(_, _))),
        )?;
        Ok(page.close().await?)
    }

    /// Form fields take values the way a person gives them.
    pub(super) async fn forms(session: &Session, site: &Site) -> Outcome {
        let page = Self::open(session, site, "/form").await?;
        let email = page.find(By::id("email"));
        email.fill("ada@example.com").await?;
        Flaw::same("fill", &email.value().await?, "ada@example.com")?;
        let log = page.find(By::id("log")).text().await?;
        Flaw::expect(
            "fill events",
            &log,
            log.contains("focus") && log.contains("input") && log.contains("change"),
        )?;
        email.clear().await?;
        Flaw::same("clear", &email.value().await?, "")?;
        let password = page.find(By::name("password"));
        password.type_text("s3cret").await?;
        Flaw::same("type", &password.value().await?, "s3cret")?;
        let age = page.find(By::name("age"));
        age.type_text("42").await?;
        Flaw::same("type a number", &age.value().await?, "42")?;
        let note = page.find(By::name("note"));
        note.type_text("a\nb").await?;
        Flaw::same("type lines", &note.value().await?, "a\nb")?;
        let rich = page.find(By::id("rich"));
        rich.fill("Hello").await?;
        rich.type_text(" world").await?;
        let written = rich.text_content().await?.replace('\u{a0}', " ");
        Flaw::same("rich text", &written, "Hello world")?;

        let news = page.find(By::name("news"));
        news.check().await?;
        Flaw::same("check", &news.is_checked().await?, &true)?;
        news.check().await?;
        Flaw::same("check again", &news.is_checked().await?, &true)?;
        news.uncheck().await?;
        Flaw::same("uncheck", &news.is_checked().await?, &false)?;
        let paid = page.find(By::css("input[value=paid]"));
        paid.check().await?;
        Flaw::same("radio", &paid.is_checked().await?, &true)?;
        Flaw::same(
            "other radio",
            &page.find(By::css("input[value=free]")).is_checked().await?,
            &false,
        )?;

        let language = page.find(By::name("language"));
        language.select(Choice::Label("German")).await?;
        Flaw::same("select by label", &language.value().await?, "de")?;
        language.select(Choice::Value("fr")).await?;
        Flaw::same("select by value", &language.value().await?, "fr")?;
        language.select(Choice::Index(0)).await?;
        Flaw::same("select by index", &language.value().await?, "en")?;
        let toppings = page.find(By::name("toppings"));
        toppings
            .select_all(&[Choice::Value("a"), Choice::Index(2)])
            .await?;
        let chosen: Vec<String> = toppings.call(CHOSEN, &()).await?;
        Flaw::same("select several", &chosen, &["a", "c"][..])?;

        page.find(By::id("login")).submit().await?;
        page.wait_for_url(Match::Containing("/submitted")).await?;
        let query = page.find(By::id("query")).text().await?;
        Flaw::expect(
            "submit",
            &query,
            query.contains("language=en") && query.contains("toppings=c"),
        )?;
        Ok(page.close().await?)
    }

    /// Commands that cannot work say why instead of acting.
    pub(super) async fn refusals(session: &Session, site: &Site) -> Outcome {
        let page = Self::open(session, site, "/form").await?;
        let locked = page.find(By::name("locked")).timeout(BRIEF).fill("x").await;
        Flaw::expect(
            "read-only",
            &locked,
            matches!(locked, Err(Error::NotActionable(_, Blocker::ReadOnly))),
        )?;
        let off = page.find(By::name("off")).timeout(BRIEF).fill("x").await;
        Flaw::expect(
            "disabled",
            &off,
            matches!(off, Err(Error::NotActionable(_, Blocker::Disabled))),
        )?;
        let typed = page
            .find(By::name("off"))
            .timeout(BRIEF)
            .type_text("x")
            .await;
        Flaw::expect(
            "typing into a disabled field",
            &typed,
            matches!(typed, Err(Error::NotActionable(_, Blocker::Disabled))),
        )?;
        let pressed = page.find(By::name("off")).timeout(BRIEF).press('x').await;
        Flaw::expect(
            "a key on a disabled field",
            &pressed,
            matches!(pressed, Err(Error::NotActionable(_, Blocker::Disabled))),
        )?;
        let button = page.find(By::id("count")).fill("x").await;
        Flaw::expect(
            "fill a button",
            &button,
            matches!(button, Err(Error::WrongElement(_, _))),
        )?;
        let radio = page.find(By::css("input[value=free]")).uncheck().await;
        Flaw::expect(
            "uncheck a radio",
            &radio,
            matches!(radio, Err(Error::WrongElement(_, _))),
        )?;
        let unchosen = page.find(By::id("email")).select(Choice::Index(0)).await;
        Flaw::expect(
            "select in a field",
            &unchosen,
            matches!(unchosen, Err(Error::WrongElement(_, _))),
        )?;
        let absent = page
            .find(By::name("language"))
            .select(Choice::Value("xx"))
            .await;
        Flaw::expect(
            "no such option",
            &absent,
            matches!(absent, Err(Error::WrongElement(_, _))),
        )?;
        let formless = page.find(By::id("log")).submit().await;
        Flaw::expect(
            "submit outside a form",
            &formless,
            matches!(formless, Err(Error::WrongElement(_, _))),
        )?;
        page.close().await?;

        let page = Self::open(session, site, "/late").await?;
        let under = page.find(By::id("under")).timeout(SETTLED).click().await;
        Flaw::expect(
            "covered",
            &under,
            matches!(under, Err(Error::NotActionable(_, Blocker::Covered))),
        )?;
        let ghost = page.find(By::id("ghost")).timeout(BRIEF).click().await;
        Flaw::expect(
            "hidden",
            &ghost,
            matches!(ghost, Err(Error::NotActionable(_, Blocker::Hidden))),
        )?;
        Ok(page.close().await?)
    }

    /// Clicks of every kind reach the page as a person's would.
    pub(super) async fn clicks(session: &Session, site: &Site) -> Outcome {
        let page = Self::open(session, site, "/form").await?;
        let counter = page.find(By::id("count"));
        let log = page.find(By::id("log"));
        counter.hover().await?;
        Flaw::same("hover", &log.text().await?, "over")?;
        counter.click().await?;
        Flaw::same("click", &counter.text().await?, "Clicked 1 times")?;
        counter.double_click().await?;
        Flaw::same("double click", &counter.text().await?, "Clicked 3 times")?;
        let heard = log.text().await?;
        Flaw::expect(
            "double click event",
            &heard,
            heard.ends_with("click:1 click:2 dblclick"),
        )?;
        counter.right_click().await?;
        Flaw::same(
            "right click leaves the count",
            &counter.text().await?,
            "Clicked 3 times",
        )?;
        let heard = log.text().await?;
        Flaw::expect("context menu", &heard, heard.ends_with("contextmenu"))?;
        let shifted = Click {
            modifiers: Modifiers::SHIFT,
            offset: Some(Point { x: 2.0, y: 2.0 }),
            ..Click::default()
        };
        counter.click_with(shifted).await?;
        let heard = log.text().await?;
        Flaw::expect("held shift", &heard, heard.ends_with("click:1:shift"))?;
        page.find(By::text("Sign in")).click().await?;
        page.wait_for_url(Match::Containing("/submitted")).await?;
        Flaw::same("a click submits", &page.title().await?, "Submitted")?;
        page.close().await?;

        let page = Self::open(session, site, "/list").await?;
        page.find(By::id("far")).click().await?;
        Flaw::same(
            "click below the fold",
            &page.find(By::id("out")).text().await?,
            "reached",
        )?;
        page.find(By::tag("html"))
            .scroll_to(Point { x: 0.0, y: 400.0 })
            .await?;
        Flaw::same(
            "scroll to",
            &page.evaluate::<f64>("window.scrollY").await?,
            &400.0,
        )?;
        page.find(By::tag("html"))
            .scroll_by(Point { x: 0.0, y: 50.0 })
            .await?;
        Flaw::same(
            "scroll by",
            &page.evaluate::<f64>("window.scrollY").await?,
            &450.0,
        )?;
        page.find(By::tag("h1")).scroll_into_view().await?;
        let top = page.find(By::tag("h1")).rect().await?;
        Flaw::expect("scroll into view", &top, top.y >= 0.0)?;
        Ok(page.close().await?)
    }

    /// Keys move the focus, edit text and submit forms.
    pub(super) async fn keys(session: &Session, site: &Site) -> Outcome {
        let page = Self::open(session, site, "/form").await?;
        let email = page.find(By::id("email"));
        let password = page.find(By::name("password"));
        email.focus().await?;
        Flaw::same("focus", &email.is_focused().await?, &true)?;
        email.press(NamedKey::Tab).await?;
        Flaw::same(
            "tab",
            &page.focused().attribute("name").await?.as_deref(),
            &Some("password"),
        )?;
        password
            .press(Keystroke::with(Modifiers::SHIFT, NamedKey::Tab))
            .await?;
        Flaw::same(
            "shift tab",
            &page.focused().attribute("name").await?.as_deref(),
            &Some("email"),
        )?;
        email.blur().await?;
        Flaw::same("blur", &email.is_focused().await?, &false)?;

        password.fill("abc").await?;
        password.press(NamedKey::Backspace).await?;
        Flaw::same("backspace", &password.value().await?, "ab")?;
        password.press(NamedKey::Home).await?;
        password.press('z').await?;
        Flaw::same("home then a letter", &password.value().await?, "zab")?;
        password.press(NamedKey::Delete).await?;
        Flaw::same("delete", &password.value().await?, "zb")?;
        password.press(NamedKey::End).await?;
        password.press(NamedKey::ArrowLeft).await?;
        password.press('-').await?;
        Flaw::same("arrow then a letter", &password.value().await?, "z-b")?;
        password
            .press(Keystroke::with(Modifiers::CONTROL, 'a'))
            .await?;
        password.press(NamedKey::Backspace).await?;
        Flaw::same("select all then erase", &password.value().await?, "")?;
        password.fill("old").await?;
        password.select_text().await?;
        password.press('q').await?;
        Flaw::same("typing over a selection", &password.value().await?, "q")?;

        let news = page.find(By::name("news"));
        news.press(' ').await?;
        Flaw::same("space checks a box", &news.is_checked().await?, &true)?;
        email.fill("ada@example.com").await?;
        let heard = page.find(By::id("log")).text().await?;
        email.press(NamedKey::Enter).await?;
        page.wait_for_url(Match::Containing("/submitted")).await?;
        let query = page.find(By::id("query")).text().await?;
        Flaw::expect(
            "enter submits",
            &query,
            query.contains("email=ada%40example.com") && query.contains("news=yes"),
        )?;
        Flaw::expect("key events", &heard, heard.contains("keydown:Tab"))?;
        Ok(page.close().await?)
    }

    /// Commands wait for the page to be ready, and waits end when their state is reached.
    pub(super) async fn waiting(session: &Session, site: &Site) -> Outcome {
        let page = Self::open(session, site, "/late").await?;
        Flaw::same(
            "a late element",
            &page.find(By::id("arrived")).text().await?,
            "Arrived",
        )?;
        page.find(By::id("soon")).click().await?;
        let out = page.find(By::id("out"));
        Flaw::same("a late click", &out.text().await?, "clicked")?;
        out.wait_for_text("CLICK").await?;
        page.find(By::id("veil")).wait_for(State::Detached).await?;
        page.find(By::id("soon")).wait_for(State::Enabled).await?;
        page.find(By::id("ghost")).wait_for(State::Hidden).await?;
        page.find(By::id("under")).wait_for(State::Visible).await?;
        page.find(By::id("arrived")).wait().await?;
        page.wait_until(
            "function (id) { return !!document.getElementById(id); }",
            "arrived",
        )
        .await?;
        let never = page
            .find(By::id("ghost"))
            .timeout(BRIEF)
            .wait_for(State::Visible)
            .await;
        Flaw::expect(
            "a state never reached",
            &never,
            matches!(never, Err(Error::Timeout(_))),
        )?;
        let unsaid = out.clone().timeout(BRIEF).wait_for_text("never").await;
        Flaw::expect(
            "text never shown",
            &unsaid,
            matches!(unsaid, Err(Error::Timeout(_))),
        )?;
        Ok(page.close().await?)
    }

    /// Frames of the same site and open shadow roots can be entered.
    pub(super) async fn frames(session: &Session, site: &Site) -> Outcome {
        let page = Self::open(session, site, "/frame").await?;
        let inner = page.find(By::id("inner")).frame();
        inner.clone().find(By::name("inside")).fill("deep").await?;
        inner.clone().find(By::id("in")).click().await?;
        Flaw::same(
            "inside a frame",
            &inner.find(By::id("echo")).text().await?,
            "deep",
        )?;
        let root = page.find(By::id("host")).shadow();
        root.clone().find(By::id("deep")).click().await?;
        Flaw::same(
            "inside a shadow root",
            &root.find(By::id("said")).text().await?,
            "pressed",
        )?;
        let unframed = page
            .find(By::id("host"))
            .frame()
            .find(By::tag("p"))
            .count()
            .await;
        Flaw::expect(
            "not a frame",
            &unframed,
            matches!(unframed, Err(Error::NoSuchFrame(_, _))),
        )?;
        let rootless = page
            .find(By::id("inner"))
            .shadow()
            .find(By::tag("p"))
            .count()
            .await;
        Flaw::expect(
            "no shadow root",
            &rootless,
            matches!(rootless, Err(Error::NoSuchFrame(_, _))),
        )?;
        Ok(page.close().await?)
    }
}
