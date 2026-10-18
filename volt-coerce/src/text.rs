use std::borrow::Cow;

use serde::Deserialize;
use serde::de::value::{MapAccessDeserializer, SeqAccessDeserializer};
use serde::de::{Error, MapAccess, SeqAccess};
use serde_json::{Number, Value};

use super::rule::Coerce;

impl<'de> Coerce<'de> for String {
    fn read_bool<E: Error>(value: bool) -> Result<Option<Self>, E> {
        Ok(Some(value.to_string()))
    }

    fn read_i64<E: Error>(value: i64) -> Result<Option<Self>, E> {
        Ok(Some(value.to_string()))
    }

    fn read_u64<E: Error>(value: u64) -> Result<Option<Self>, E> {
        Ok(Some(value.to_string()))
    }

    fn read_f64<E: Error>(value: f64) -> Result<Option<Self>, E> {
        Ok(Some(Number::from_f64(value).map_or_else(
            || value.to_string(),
            |number| number.to_string(),
        )))
    }

    fn read_str<E: Error>(value: &str) -> Result<Option<Self>, E> {
        Ok(Some(value.to_owned()))
    }

    fn read_string<E: Error>(value: String) -> Result<Option<Self>, E> {
        Ok(Some(value))
    }

    fn read_list<A: SeqAccess<'de>>(list: A) -> Result<Option<Self>, A::Error> {
        Ok(Some(
            Value::deserialize(SeqAccessDeserializer::new(list))?.to_string(),
        ))
    }

    fn read_object<A: MapAccess<'de>>(object: A) -> Result<Option<Self>, A::Error> {
        Ok(Some(
            Value::deserialize(MapAccessDeserializer::new(object))?.to_string(),
        ))
    }
}

impl<'de> Coerce<'de> for Cow<'de, str> {
    fn read_bool<E: Error>(value: bool) -> Result<Option<Self>, E> {
        Ok(Some(Cow::Borrowed(if value { "true" } else { "false" })))
    }

    fn read_i64<E: Error>(value: i64) -> Result<Option<Self>, E> {
        Ok(String::read_i64(value)?.map(Cow::Owned))
    }

    fn read_u64<E: Error>(value: u64) -> Result<Option<Self>, E> {
        Ok(String::read_u64(value)?.map(Cow::Owned))
    }

    fn read_f64<E: Error>(value: f64) -> Result<Option<Self>, E> {
        Ok(String::read_f64(value)?.map(Cow::Owned))
    }

    fn read_str<E: Error>(value: &str) -> Result<Option<Self>, E> {
        Ok(Some(Cow::Owned(value.to_owned())))
    }

    fn read_borrowed_str<E: Error>(value: &'de str) -> Result<Option<Self>, E> {
        Ok(Some(Cow::Borrowed(value)))
    }

    fn read_string<E: Error>(value: String) -> Result<Option<Self>, E> {
        Ok(Some(Cow::Owned(value)))
    }

    fn read_list<A: SeqAccess<'de>>(list: A) -> Result<Option<Self>, A::Error> {
        Ok(String::read_list(list)?.map(Cow::Owned))
    }

    fn read_object<A: MapAccess<'de>>(object: A) -> Result<Option<Self>, A::Error> {
        Ok(String::read_object(object)?.map(Cow::Owned))
    }
}
