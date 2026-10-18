use serde::de::Error;
use serde::{Deserialize, Serialize};
use serde_json::{Deserializer, Map, Value};
use volt_coerce::{Coerce, Json, Reader, coerce};

#[coerce(take_from)]
#[derive(Debug, PartialEq, Serialize)]
#[serde(default)]
pub struct Bar {
    pub id: i64,
    pub ratio: f64,
    pub label: String,
}

impl Default for Bar {
    fn default() -> Self {
        Self {
            id: 10,
            ratio: 100.0,
            label: "unnamed".to_owned(),
        }
    }
}

#[coerce]
#[derive(Debug, Default, Serialize)]
#[serde(default)]
pub struct Foo {
    #[serde(rename = "qty", alias = "count")]
    pub quantity: i64,
    pub tags: Vec<String>,
    pub bar: Option<Box<Bar>>,
    pub bars: Vec<Bar>,
    pub logic: Logic,
    pub level: Level,
    #[reject]
    pub version: i64,
    #[strict]
    pub note: String,
    #[strict]
    #[serde(flatten)]
    pub unknown: Map<String, Value>,
}

#[coerce(custom)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Serialize)]
pub enum Logic {
    #[default]
    AndAny,
    NotAll,
}

impl Coerce<'_> for Logic {
    fn read_i64<E: Error>(value: i64) -> Result<Option<Self>, E> {
        Ok(Some(if value == 1 {
            Self::NotAll
        } else {
            Self::AndAny
        }))
    }

    fn read_str<E: Error>(value: &str) -> Result<Option<Self>, E> {
        Ok(Some(match value.trim().to_ascii_lowercase().as_str() {
            "notall" => Self::NotAll,
            _ => Self::AndAny,
        }))
    }
}

#[coerce]
#[derive(Debug, Default, Clone, Copy, PartialEq, Serialize)]
#[serde(from = "i64", into = "i64")]
pub enum Level {
    #[default]
    Low,
    High,
}

impl From<i64> for Level {
    fn from(value: i64) -> Self {
        if value > 5 { Self::High } else { Self::Low }
    }
}

impl From<Level> for i64 {
    fn from(value: Level) -> Self {
        value as i64
    }
}

#[coerce]
#[derive(Debug, Default)]
#[serde(default)]
pub struct RawUser {
    pub name: String,
    pub username: Option<String>,
    pub login: Option<String>,
}

#[coerce]
#[derive(Debug, PartialEq, Serialize)]
#[serde(from = "RawUser")]
pub struct User {
    pub name: String,
    pub username: String,
}

impl From<RawUser> for User {
    fn from(raw: RawUser) -> Self {
        Self {
            name: raw.name,
            username: raw.username.or(raw.login).unwrap_or_default(),
        }
    }
}

impl Default for User {
    fn default() -> Self {
        RawUser::default().into()
    }
}

#[coerce(after = Self::renumber)]
#[derive(Debug, Default, Serialize)]
#[serde(default)]
pub struct Shelf {
    pub name: String,
    pub bars: Vec<Bar>,
}

impl Shelf {
    fn renumber(&mut self) {
        for (index, bar) in self.bars.iter_mut().enumerate() {
            bar.id = i64::try_from(index).unwrap();
        }
    }
}

#[coerce(after = Self::check)]
#[derive(Debug, Default)]
#[serde(default)]
pub struct Order {
    pub amount: f64,
}

impl Order {
    fn check(&mut self) -> Result<(), String> {
        if self.amount < 0.0 {
            return Err(format!("amount {} is negative", self.amount));
        }
        Ok(())
    }
}

#[coerce]
#[derive(Debug, Default)]
#[serde(default)]
pub struct Holder {
    pub shelf: Shelf,
    pub shelves: Vec<Shelf>,
    pub user: User,
}

#[coerce(take_from)]
#[derive(Debug)]
pub struct Required {
    pub id: i64,
    pub maybe: Option<i64>,
}

#[coerce(take_from, after = Self::check)]
#[derive(Debug, Default, PartialEq)]
#[serde(default)]
pub struct Settings {
    #[serde(rename = "selectiveLogic")]
    pub logic: Option<i64>,
    pub depth: Option<i64>,
    #[reject]
    pub version: i64,
    #[strict]
    pub note: String,
    #[serde(flatten)]
    pub unknown: Map<String, Value>,
}

impl Settings {
    fn check(&mut self) -> Result<(), String> {
        if self.version < 0 {
            return Err(format!("version {} is negative", self.version));
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
pub struct Plain {
    pub logic: Logic,
    pub user: User,
    #[serde(default, deserialize_with = "volt_coerce::Reader::coerce")]
    pub count: i64,
}

#[test]
fn junk_and_missing_both_use_your_default() {
    let bar: Bar = serde_json::from_str(r#"{"id":[],"ratio":"junk"}"#).unwrap();
    assert_eq!(bar, Bar::default());
    let bar: Bar = serde_json::from_str("{}").unwrap();
    assert_eq!(bar, Bar::default());
    let bar: Bar = serde_json::from_str(r#"{"id":"3","ratio":7,"label":9}"#).unwrap();
    assert_eq!(
        bar,
        Bar {
            id: 3,
            ratio: 7.0,
            label: "9".to_owned()
        }
    );
}

#[test]
fn every_field_is_lenient_unless_marked() {
    let foo: Foo = serde_json::from_str(
        r#"{"count":"3","tags":"solo","bar":42,"bars":[{"id":"1"},9],"logic":"notall","level":9,"version":"2","note":"kept","extra":1}"#,
    )
    .unwrap();
    assert_eq!(foo.quantity, 3);
    assert_eq!(foo.tags, vec!["solo"]);
    assert!(foo.bar.is_none());
    assert_eq!(foo.bars.len(), 1);
    assert_eq!(foo.logic, Logic::NotAll);
    assert_eq!(foo.level, Level::High);
    assert_eq!(foo.version, 2);
    assert_eq!(foo.note, "kept");
    assert_eq!(foo.unknown.len(), 1);
    assert!(serde_json::from_str::<Foo>(r#"{"note":5}"#).is_err());
    let error = serde_json::from_str::<Foo>(r#"{"version":[]}"#).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("unsupported value for `version`"),
        "{error}"
    );
}

#[test]
fn missing_without_a_default_still_fails_like_serde() {
    let required: Required = serde_json::from_str(r#"{"id":"4","maybe":"x"}"#).unwrap();
    assert_eq!((required.id, required.maybe), (4, None));
    assert!(serde_json::from_str::<Required>("{}").is_err());
}

#[test]
fn twins_and_scalar_twins_work() {
    let user: User = serde_json::from_str(r#"{"name":7,"login":"ada"}"#).unwrap();
    assert_eq!(
        user,
        User {
            name: "7".to_owned(),
            username: "ada".to_owned()
        }
    );
    assert_eq!(serde_json::from_str::<Level>("9").unwrap(), Level::High);
    let foo: Foo = serde_json::from_str(r#"{"level":"9"}"#).unwrap();
    assert_eq!(foo.level, Level::High);
}

#[test]
fn after_runs_on_every_path() {
    let shelf: Shelf = serde_json::from_str(r#"{"bars":[{"id":9},{"id":9}]}"#).unwrap();
    assert_eq!((shelf.bars[0].id, shelf.bars[1].id), (0, 1));
    let mut d = Deserializer::from_str(r#"{"bars":[{"id":9},{"id":9}]}"#);
    let shelf = Shelf::deserialize(&mut d).unwrap();
    assert_eq!((shelf.bars[0].id, shelf.bars[1].id), (0, 1));
    let holder: Holder = serde_json::from_str(
        r#"{"shelf":{"bars":[{"id":4},{"id":4}]},"shelves":[{"bars":[{"id":3},{"id":3}]}]}"#,
    )
    .unwrap();
    assert_eq!((holder.shelf.bars[0].id, holder.shelf.bars[1].id), (0, 1));
    assert_eq!(
        (holder.shelves[0].bars[0].id, holder.shelves[0].bars[1].id),
        (0, 1)
    );
    let shelf = Option::<Shelf>::deserialize(serde_json::json!({"bars": [{"id": 7}, {"id": 7}]}))
        .unwrap()
        .unwrap();
    assert_eq!((shelf.bars[0].id, shelf.bars[1].id), (0, 1));
    let error = serde_json::from_str::<Order>(r#"{"amount":-1}"#).unwrap_err();
    assert!(
        error.to_string().contains("amount -1 is negative"),
        "{error}"
    );
}

#[test]
fn coerced_types_work_inside_plain_serde() {
    let plain: Plain =
        serde_json::from_str(r#"{"logic":1,"user":{"name":2},"count":"5"}"#).unwrap();
    assert_eq!((plain.logic, plain.count), (Logic::NotAll, 5));
    assert_eq!(plain.user.name, "2");
    let mut d = Deserializer::from_str("1");
    assert_eq!(Reader::<Logic>::read(&mut d).unwrap(), Some(Logic::NotAll));
}

#[test]
fn serialization_is_untouched() {
    let bar = Bar {
        id: 1,
        ratio: 2.0,
        label: "x".to_owned(),
    };
    assert_eq!(
        serde_json::to_string(&bar).unwrap(),
        r#"{"id":1,"ratio":2.0,"label":"x"}"#
    );
    assert_eq!(serde_json::to_string(&Level::High).unwrap(), "1");
}

#[coerce]
#[derive(Debug, Default)]
#[serde(default)]
pub struct Wrapper {
    pub inner: Json<Bar>,
    pub maybe: Option<Json<Bar>>,
}

#[test]
fn json_in_a_string_decodes_into_the_type() {
    let wrapper: Wrapper = serde_json::from_str(
        r#"{"inner":"{\"id\":\"3\",\"ratio\":7}","maybe":"{\"label\":\"x\"}"}"#,
    )
    .unwrap();
    assert_eq!(
        (wrapper.inner.0.id, wrapper.inner.0.ratio),
        (3, 7.0),
        "text parsed, then coerced inside"
    );
    assert_eq!(wrapper.maybe.unwrap().0.label, "x");

    let inline: Wrapper = serde_json::from_str(r#"{"inner":{"id":4},"maybe":{"id":5}}"#).unwrap();
    assert_eq!(inline.inner.0.id, 4, "an inline object decodes directly");
    assert_eq!(inline.maybe.unwrap().0.id, 5);

    let junk: Wrapper = serde_json::from_str(r#"{"inner":"not json","maybe":42}"#).unwrap();
    assert_eq!(
        junk.inner.0,
        Bar::default(),
        "unparseable text falls back to the default"
    );
    assert!(junk.maybe.is_none(), "junk in an option is none");

    let plain: Json<Bar> = serde_json::from_str(r#""{\"id\":9}""#).unwrap();
    assert_eq!(plain.0.id, 9, "works as a plain serde type too");
}

#[coerce]
#[derive(Debug, Default)]
#[serde(default)]
pub struct Widths {
    pub total: u64,
    pub count: u32,
    pub offset: i32,
    pub ratio: f32,
    pub maybe: Option<u32>,
}

#[test]
fn other_number_widths_read_and_saturate() {
    let widths: Widths = serde_json::from_str(
        r#"{"total":18446744073709551615,"count":"4294967296","offset":-3000000000,"ratio":true,"maybe":5000000000.0}"#,
    )
    .unwrap();
    assert_eq!(widths.total, u64::MAX);
    assert_eq!(widths.count, u32::MAX);
    assert_eq!(widths.offset, i32::MIN);
    assert_eq!(widths.ratio, 1.0);
    assert_eq!(widths.maybe, Some(u32::MAX));

    let low: Widths =
        serde_json::from_str(r#"{"total":-1,"count":-5,"offset":"x","maybe":null}"#).unwrap();
    assert_eq!(
        (low.total, low.count, low.offset, low.maybe),
        (0, 0, 0, None)
    );
}

#[coerce]
#[derive(Debug, Default)]
#[serde(default)]
pub struct Tree {
    pub name: String,
    pub children: Vec<Self>,
    pub parent: Option<Box<Self>>,
}

#[test]
fn recursive_self_fields_work() {
    let tree: Tree =
        serde_json::from_str(r#"{"name":1,"children":[{"name":"a"},5],"parent":{"name":"p"}}"#)
            .unwrap();
    assert_eq!(tree.name, "1");
    assert_eq!(tree.children.len(), 1);
    assert_eq!(tree.parent.unwrap().name, "p");
}

#[test]
fn take_from_removes_only_its_own_keys() {
    let mut map: Map<String, Value> = serde_json::from_str(
        r#"{"selectiveLogic":"2","depth":null,"note":"kept","uid":7,"group":""}"#,
    )
    .unwrap();
    let settings = Settings::take_from(&mut map).unwrap();
    assert_eq!(
        settings,
        Settings {
            logic: Some(2),
            depth: None,
            version: 0,
            note: "kept".to_owned(),
            unknown: Map::new(),
        }
    );
    let left: Map<String, Value> = serde_json::from_str(r#"{"uid":7,"group":""}"#).unwrap();
    assert_eq!(map, left);
}

#[test]
fn take_from_reads_like_deserialize() {
    let json = r#"{"selectiveLogic":"x","depth":"4","version":"2","note":"n","extra":1}"#;
    let mut decoded: Settings = serde_json::from_str(json).unwrap();
    let mut map: Map<String, Value> = serde_json::from_str(json).unwrap();
    let taken = Settings::take_from(&mut map).unwrap();
    assert_eq!(
        decoded.unknown, map,
        "the keys Deserialize leaves over stay in the map"
    );
    decoded.unknown = Map::new();
    assert_eq!(taken, decoded);
}

#[test]
fn take_from_falls_back_like_deserialize() {
    let mut map: Map<String, Value> = serde_json::from_str(r#"{"id":[],"label":9}"#).unwrap();
    let bar = Bar::take_from(&mut map).unwrap();
    assert_eq!(
        bar,
        Bar {
            id: 10,
            ratio: 100.0,
            label: "9".to_owned()
        }
    );
    assert!(map.is_empty());
}

#[test]
fn take_from_fails_like_deserialize() {
    let mut map = Map::new();
    let error = Required::take_from(&mut map).unwrap_err();
    assert!(error.to_string().contains("missing field `id`"), "{error}");

    for (json, expected) in [
        (r#"{"version":[]}"#, "unsupported value for `version`"),
        (r#"{"version":-1}"#, "version -1 is negative"),
        (r#"{"note":5}"#, "invalid type"),
    ] {
        let mut map: Map<String, Value> = serde_json::from_str(json).unwrap();
        let error = Settings::take_from(&mut map).unwrap_err();
        assert!(error.to_string().contains(expected), "{json}: {error}");
    }
}

#[derive(Debug, PartialEq)]
pub struct Bomb(i64);

impl Default for Bomb {
    fn default() -> Self {
        panic!("built a default nothing fell back to")
    }
}

impl Coerce<'_> for Bomb {
    fn read_i64<E: Error>(value: i64) -> Result<Option<Self>, E> {
        Ok(Some(Self(value)))
    }
}

#[coerce]
#[derive(Debug, Default)]
#[serde(default)]
pub struct Armed {
    pub bomb: Bomb,
    pub count: i64,
}

#[test]
fn a_derived_default_builds_only_what_falls_back() {
    let armed: Armed = serde_json::from_str(r#"{"bomb":1,"count":"x"}"#).unwrap();
    assert_eq!((armed.bomb, armed.count), (Bomb(1), 0));
}
