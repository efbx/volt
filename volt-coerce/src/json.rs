use serde::de::value::MapAccessDeserializer;
use serde::de::{DeserializeOwned, Error, MapAccess};
use serde::{Deserialize, Deserializer};

use super::{Coerce, Reader};

/// A `T` that arrives as a JSON document stored inside a string, or as an inline object.
/// Text that does not parse as `T` is junk and falls to the default, while an inline object is
/// decoded with `T`'s own `Deserialize`, so its errors fail the document.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Json<T>(pub T);

impl<'de, T: DeserializeOwned + Default> Coerce<'de> for Json<T> {
    fn read_str<E: Error>(value: &str) -> Result<Option<Self>, E> {
        Ok(serde_json::from_str(value).ok().map(Json))
    }

    fn read_object<A: MapAccess<'de>>(object: A) -> Result<Option<Self>, A::Error> {
        T::deserialize(MapAccessDeserializer::new(object))
            .map(Json)
            .map(Some)
    }
}

impl<'de, T: DeserializeOwned + Default> Deserialize<'de> for Json<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Reader::coerce(deserializer)
    }
}
