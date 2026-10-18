use serde::de::{Error, MapAccess, SeqAccess};

use super::rule::Coerce;

impl<'de, T: Coerce<'de>> Coerce<'de> for Option<T> {
    fn read_null<E: Error>() -> Result<Option<Self>, E> {
        Ok(Some(None))
    }

    fn read_bool<E: Error>(value: bool) -> Result<Option<Self>, E> {
        Ok(T::read_bool(value)?.map(Some))
    }

    fn read_i64<E: Error>(value: i64) -> Result<Option<Self>, E> {
        Ok(T::read_i64(value)?.map(Some))
    }

    fn read_u64<E: Error>(value: u64) -> Result<Option<Self>, E> {
        Ok(T::read_u64(value)?.map(Some))
    }

    fn read_f64<E: Error>(value: f64) -> Result<Option<Self>, E> {
        Ok(T::read_f64(value)?.map(Some))
    }

    fn read_str<E: Error>(value: &str) -> Result<Option<Self>, E> {
        Ok(T::read_str(value)?.map(Some))
    }

    fn read_borrowed_str<E: Error>(value: &'de str) -> Result<Option<Self>, E> {
        Ok(T::read_borrowed_str(value)?.map(Some))
    }

    fn read_string<E: Error>(value: String) -> Result<Option<Self>, E> {
        Ok(T::read_string(value)?.map(Some))
    }

    fn read_list<A: SeqAccess<'de>>(list: A) -> Result<Option<Self>, A::Error> {
        Ok(T::read_list(list)?.map(Some))
    }

    fn read_object<A: MapAccess<'de>>(object: A) -> Result<Option<Self>, A::Error> {
        Ok(T::read_object(object)?.map(Some))
    }
}
