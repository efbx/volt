use serde::de::{Error, MapAccess, SeqAccess};

use super::capacity::Capacity;
use super::reader::Reader;
use super::rule::Coerce;

impl<'de, T: Coerce<'de>> Coerce<'de> for Vec<T> {
    fn read_bool<E: Error>(value: bool) -> Result<Option<Self>, E> {
        Ok(T::read_bool(value)?.map(|item| vec![item]))
    }

    fn read_i64<E: Error>(value: i64) -> Result<Option<Self>, E> {
        Ok(T::read_i64(value)?.map(|item| vec![item]))
    }

    fn read_u64<E: Error>(value: u64) -> Result<Option<Self>, E> {
        Ok(T::read_u64(value)?.map(|item| vec![item]))
    }

    fn read_f64<E: Error>(value: f64) -> Result<Option<Self>, E> {
        Ok(T::read_f64(value)?.map(|item| vec![item]))
    }

    fn read_str<E: Error>(value: &str) -> Result<Option<Self>, E> {
        Ok(T::read_str(value)?.map(|item| vec![item]))
    }

    fn read_borrowed_str<E: Error>(value: &'de str) -> Result<Option<Self>, E> {
        Ok(T::read_borrowed_str(value)?.map(|item| vec![item]))
    }

    fn read_string<E: Error>(value: String) -> Result<Option<Self>, E> {
        Ok(T::read_string(value)?.map(|item| vec![item]))
    }

    fn read_list<A: SeqAccess<'de>>(mut list: A) -> Result<Option<Self>, A::Error> {
        let mut items = Vec::with_capacity(Capacity::cautious::<T>(list.size_hint()));
        while let Some(outcome) = list.next_element_seed(Reader::<T>::default())? {
            if let Some(item) = outcome {
                items.push(item);
            }
        }
        Ok(Some(items))
    }

    fn read_object<A: MapAccess<'de>>(object: A) -> Result<Option<Self>, A::Error> {
        Ok(T::read_object(object)?.map(|item| vec![item]))
    }
}
