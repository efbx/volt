use std::fmt;
use std::marker::PhantomData;

use serde::Deserializer;
use serde::de::{DeserializeSeed, Error, MapAccess, SeqAccess, Visitor};

use super::rule::Coerce;

/// Reads one `T` through its [`Coerce`] rules from any serde deserializer. Its functions fit
/// `#[serde(deserialize_with = "…")]`, and as a `DeserializeSeed` it yields `None` for junk.
#[derive(Debug)]
pub struct Reader<T>(PhantomData<T>);

impl<T> Default for Reader<T> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<T> Reader<T> {
    /// Returns the value `d` holds, or `None` when it is junk for `T`. Fails only when `d` itself
    /// fails or a rule of `T` rejects the value.
    pub fn read<'de, D: Deserializer<'de>>(d: D) -> Result<Option<T>, D::Error>
    where
        T: Coerce<'de>,
    {
        d.deserialize_any(Self::default())
    }

    /// Returns the value `d` holds, or `T::default()` when it is junk.
    pub fn coerce<'de, D: Deserializer<'de>>(d: D) -> Result<T, D::Error>
    where
        T: Coerce<'de> + Default,
    {
        Ok(Self::read(d)?.unwrap_or_default())
    }

    /// Returns the value `d` holds, or an `unsupported value` error when it is junk.
    pub fn demand<'de, D: Deserializer<'de>>(d: D) -> Result<T, D::Error>
    where
        T: Coerce<'de>,
    {
        Self::read(d)?.ok_or_else(|| D::Error::custom("unsupported value"))
    }
}

impl<'de, T: Coerce<'de>> DeserializeSeed<'de> for Reader<T> {
    type Value = Option<T>;

    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Option<T>, D::Error> {
        Self::read(d)
    }
}

impl<'de, T: Coerce<'de>> Visitor<'de> for Reader<T> {
    type Value = Option<T>;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("any JSON value")
    }

    fn visit_unit<E: Error>(self) -> Result<Option<T>, E> {
        T::read_null()
    }

    fn visit_none<E: Error>(self) -> Result<Option<T>, E> {
        T::read_null()
    }

    fn visit_some<D: Deserializer<'de>>(self, d: D) -> Result<Option<T>, D::Error> {
        Self::read(d)
    }

    fn visit_bool<E: Error>(self, value: bool) -> Result<Option<T>, E> {
        T::read_bool(value)
    }

    fn visit_i64<E: Error>(self, value: i64) -> Result<Option<T>, E> {
        T::read_i64(value)
    }

    fn visit_u64<E: Error>(self, value: u64) -> Result<Option<T>, E> {
        T::read_u64(value)
    }

    fn visit_f64<E: Error>(self, value: f64) -> Result<Option<T>, E> {
        T::read_f64(value)
    }

    fn visit_str<E: Error>(self, value: &str) -> Result<Option<T>, E> {
        T::read_str(value)
    }

    fn visit_borrowed_str<E: Error>(self, value: &'de str) -> Result<Option<T>, E> {
        T::read_borrowed_str(value)
    }

    fn visit_string<E: Error>(self, value: String) -> Result<Option<T>, E> {
        T::read_string(value)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, list: A) -> Result<Option<T>, A::Error> {
        T::read_list(list)
    }

    fn visit_map<A: MapAccess<'de>>(self, object: A) -> Result<Option<T>, A::Error> {
        T::read_object(object)
    }
}
