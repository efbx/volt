use serde::Deserializer;
use serde::de::{Error, IgnoredAny, MapAccess, SeqAccess};

use super::reader::Reader;

/// The lenient reading rules of a type: one method per JSON shape, each returning `Ok(Some(_))`
/// for a value it accepts, `Ok(None)` for junk and `Err` only to fail the whole document.
///
/// Every shape is junk unless a method is overridden. The crate implements it for `bool`,
/// `i32`, `i64`, `u32`, `u64`, `f32`, `f64`, [`String`], `Cow<str>`, [`Vec`], [`Option`],
/// [`Box`], `HashMap` and `BTreeMap` with [`String`] keys, `serde_json::Map` and
/// [`Json`](crate::Json), and `#[coerce]` implements it for the types it annotates. An integer
/// out of its type's range saturates at the nearest bound.
pub trait Coerce<'de>: Sized {
    /// Reads a JSON `null`; junk unless overridden.
    fn read_null<E: Error>() -> Result<Option<Self>, E> {
        Ok(None)
    }

    /// Reads a JSON boolean; junk unless overridden.
    fn read_bool<E: Error>(_: bool) -> Result<Option<Self>, E> {
        Ok(None)
    }

    /// Reads a signed JSON integer; junk unless overridden.
    fn read_i64<E: Error>(_: i64) -> Result<Option<Self>, E> {
        Ok(None)
    }

    /// Reads an unsigned JSON integer; by default forwards to [`Self::read_i64`] when `value` fits
    /// in `i64` and is junk otherwise.
    fn read_u64<E: Error>(value: u64) -> Result<Option<Self>, E> {
        match i64::try_from(value) {
            Ok(value) => Self::read_i64(value),
            Err(_) => Ok(None),
        }
    }

    /// Reads a floating-point JSON number; junk unless overridden.
    fn read_f64<E: Error>(_: f64) -> Result<Option<Self>, E> {
        Ok(None)
    }

    /// Reads a JSON string; junk unless overridden. The borrowed and owned string methods forward
    /// here by default.
    fn read_str<E: Error>(_: &str) -> Result<Option<Self>, E> {
        Ok(None)
    }

    /// Reads a string borrowed from the input for `'de`; forwards to [`Self::read_str`] by default.
    fn read_borrowed_str<E: Error>(value: &'de str) -> Result<Option<Self>, E> {
        Self::read_str(value)
    }

    /// Reads an owned string; forwards to [`Self::read_str`] by default.
    fn read_string<E: Error>(value: String) -> Result<Option<Self>, E> {
        Self::read_str(&value)
    }

    /// Reads a JSON array from `list`; by default skips every element and treats the array as junk.
    fn read_list<A: SeqAccess<'de>>(mut list: A) -> Result<Option<Self>, A::Error> {
        while list.next_element::<IgnoredAny>()?.is_some() {}
        Ok(None)
    }

    /// Reads a JSON object from `object`; by default skips every entry and treats the object as
    /// junk.
    fn read_object<A: MapAccess<'de>>(mut object: A) -> Result<Option<Self>, A::Error> {
        while object.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
        Ok(None)
    }

    /// Returns the value `d` holds, or `Self::default()` when it is junk, like [`Reader::coerce`].
    fn coerce<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error>
    where
        Self: Default,
    {
        Reader::<Self>::coerce(d)
    }
}
