mod caches;
mod snapshot;

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::page_script::Op;
use crate::{Error, Page};
pub use caches::Caches;
pub use snapshot::Snapshot;

/// One of the two key-value stores a page's site has in the browser.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Area {
    /// `localStorage`, which outlives the page.
    #[serde(rename = "localStorage")]
    Local,
    /// `sessionStorage`, which ends with the page.
    #[serde(rename = "sessionStorage")]
    Session,
}

/// One key of a store, and the value to put there for a command that writes.
#[derive(Serialize)]
struct Slot<'a> {
    /// The store.
    area: Area,
    /// The key; empty for a command about the whole store.
    key: &'a str,
    /// The value to store.
    value: &'a str,
}

/// A key-value store of the page's current site. A page on `about:blank` has none, and its
/// commands fail with [`Error::Script`].
#[derive(Clone, Debug)]
pub struct Storage {
    /// The page whose site the store belongs to.
    page: Page,
    /// Which of the two stores.
    area: Area,
}

impl Storage {
    /// The value stored under `key`.
    pub async fn get(&self, key: &str) -> Result<Option<String>, Error> {
        self.page.ask(Op::StorageGet, &self.slot(key, "")).await
    }

    /// Stores `value` under `key`.
    pub async fn set(&self, key: &str, value: &str) -> Result<(), Error> {
        self.page.ask(Op::StorageSet, &self.slot(key, value)).await
    }

    /// Removes what is stored under `key`.
    pub async fn remove(&self, key: &str) -> Result<(), Error> {
        self.page.ask(Op::StorageRemove, &self.slot(key, "")).await
    }

    /// Removes everything in the store.
    pub async fn clear(&self) -> Result<(), Error> {
        self.page.ask(Op::StorageClear, &self.slot("", "")).await
    }

    /// Every key with its value, in the store's own order.
    pub async fn entries(&self) -> Result<Vec<(String, String)>, Error> {
        self.page.ask(Op::StorageEntries, &self.slot("", "")).await
    }

    /// The value stored under `key`, read as the JSON many sites keep there.
    pub async fn get_json<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>, Error> {
        match self.get(key).await? {
            Some(text) => Ok(Some(serde_json::from_str(&text)?)),
            None => Ok(None),
        }
    }

    /// Stores `value` under `key` as JSON.
    pub async fn set_json<T: Serialize + ?Sized>(&self, key: &str, value: &T) -> Result<(), Error> {
        self.set(key, &serde_json::to_string(value)?).await
    }

    /// Returns `key` of this store with `value`, as a command reads it.
    fn slot<'a>(&self, key: &'a str, value: &'a str) -> Slot<'a> {
        Slot {
            area: self.area,
            key,
            value,
        }
    }
}

impl Page {
    /// The `area` store of the page's current site.
    #[must_use]
    pub fn storage(&self, area: Area) -> Storage {
        Storage {
            page: self.clone(),
            area,
        }
    }

    /// The caches the page's current site keeps through the browser's Cache Storage.
    #[must_use]
    pub fn caches(&self) -> Caches {
        Caches::new(self.clone())
    }
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};
    use serde_json::json;

    use super::Area;
    use crate::Error;
    use crate::fake::Fake;

    /// What a site might keep as JSON under one key.
    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Login {
        /// Who is signed in.
        user: String,
        /// How many times.
        visits: u32,
    }

    #[tokio::test]
    async fn each_command_names_its_store_key_and_value() {
        let fake = Fake::answering([
            json!({"status": "ok", "json": "\"dark\""}),
            json!({"status": "ok", "json": null}),
            json!({"status": "ok", "json": null}),
            json!({"status": "ok", "json": null}),
            json!({"status": "ok", "json": "[[\"theme\",\"dark\"],[\"lang\",\"en\"]]"}),
        ]);
        let page = fake.page();
        let local = page.storage(Area::Local);
        let session = page.storage(Area::Session);

        assert_eq!(local.get("theme").await.unwrap().as_deref(), Some("dark"));
        session.set("step", "2").await.unwrap();
        local.remove("theme").await.unwrap();
        session.clear().await.unwrap();
        assert_eq!(
            local.entries().await.unwrap(),
            [
                ("theme".to_owned(), "dark".to_owned()),
                ("lang".to_owned(), "en".to_owned())
            ]
        );

        let commands = fake.commands();
        assert_eq!(commands[0]["op"], "storage_get");
        assert_eq!(
            commands[0]["arg"],
            json!({"area": "localStorage", "key": "theme", "value": ""})
        );
        assert_eq!(commands[1]["op"], "storage_set");
        assert_eq!(
            commands[1]["arg"],
            json!({"area": "sessionStorage", "key": "step", "value": "2"})
        );
        assert_eq!(commands[2]["op"], "storage_remove");
        assert_eq!(commands[3]["op"], "storage_clear");
        assert_eq!(commands[4]["op"], "storage_entries");
    }

    #[tokio::test]
    async fn a_missing_key_is_none() {
        let fake = Fake::answering([json!({"status": "ok", "json": "null"})]);
        assert_eq!(
            fake.page()
                .storage(Area::Local)
                .get("absent")
                .await
                .unwrap(),
            None
        );
    }

    #[tokio::test]
    async fn json_values_are_read_and_written_as_their_type() {
        let fake = Fake::answering([
            json!({"status": "ok", "json": "\"{\\\"user\\\":\\\"ada\\\",\\\"visits\\\":3}\""}),
            json!({"status": "ok", "json": null}),
            json!({"status": "ok", "json": "\"not json\""}),
            json!({"status": "ok", "json": "null"}),
        ]);
        let store = fake.page().storage(Area::Local);
        let login = Login {
            user: "ada".to_owned(),
            visits: 3,
        };
        assert_eq!(
            store.get_json::<Login>("login").await.unwrap(),
            Some(Login {
                user: "ada".to_owned(),
                visits: 3
            })
        );
        store.set_json("login", &login).await.unwrap();
        assert_eq!(
            fake.commands()[1]["arg"]["value"],
            "{\"user\":\"ada\",\"visits\":3}"
        );
        assert!(matches!(
            store.get_json::<Login>("login").await,
            Err(Error::Json(_))
        ));
        assert_eq!(store.get_json::<Login>("absent").await.unwrap(), None);
    }
}
