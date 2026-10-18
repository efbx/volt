use std::collections::{BTreeMap, HashMap};
use std::hash::BuildHasher;

use serde::Deserialize;
use serde::de::MapAccess;
use serde::de::value::MapAccessDeserializer;
use serde_json::{Map, Value};

use super::capacity::Capacity;
use super::reader::Reader;
use super::rule::Coerce;

impl<'de, V: Coerce<'de>, S: BuildHasher + Default> Coerce<'de> for HashMap<String, V, S> {
    fn read_object<A: MapAccess<'de>>(mut object: A) -> Result<Option<Self>, A::Error> {
        let mut entries = HashMap::with_capacity_and_hasher(
            Capacity::cautious::<(String, V)>(object.size_hint()),
            S::default(),
        );
        while let Some(key) = object.next_key::<String>()? {
            if let Some(value) = object.next_value_seed(Reader::<V>::default())? {
                entries.insert(key, value);
            }
        }
        Ok(Some(entries))
    }
}

impl<'de, V: Coerce<'de>> Coerce<'de> for BTreeMap<String, V> {
    fn read_object<A: MapAccess<'de>>(mut object: A) -> Result<Option<Self>, A::Error> {
        let mut entries = BTreeMap::new();
        while let Some(key) = object.next_key::<String>()? {
            if let Some(value) = object.next_value_seed(Reader::<V>::default())? {
                entries.insert(key, value);
            }
        }
        Ok(Some(entries))
    }
}

impl<'de> Coerce<'de> for Map<String, Value> {
    fn read_object<A: MapAccess<'de>>(object: A) -> Result<Option<Self>, A::Error> {
        Ok(Some(Map::deserialize(MapAccessDeserializer::new(object))?))
    }
}
