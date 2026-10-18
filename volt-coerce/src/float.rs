use serde::de::Error;

use super::rule::Coerce;

impl Coerce<'_> for f64 {
    fn read_bool<E: Error>(value: bool) -> Result<Option<Self>, E> {
        Ok(Some(if value { 1.0 } else { 0.0 }))
    }

    fn read_i64<E: Error>(value: i64) -> Result<Option<Self>, E> {
        Ok(Some(value as f64))
    }

    fn read_u64<E: Error>(value: u64) -> Result<Option<Self>, E> {
        Ok(Some(value as f64))
    }

    fn read_f64<E: Error>(value: f64) -> Result<Option<Self>, E> {
        Ok(Some(value))
    }

    fn read_str<E: Error>(value: &str) -> Result<Option<Self>, E> {
        Ok(value.parse().ok())
    }
}

impl Coerce<'_> for f32 {
    fn read_bool<E: Error>(value: bool) -> Result<Option<Self>, E> {
        Ok(Some(if value { 1.0 } else { 0.0 }))
    }

    fn read_i64<E: Error>(value: i64) -> Result<Option<Self>, E> {
        Ok(Some(value as f32))
    }

    fn read_u64<E: Error>(value: u64) -> Result<Option<Self>, E> {
        Ok(Some(value as f32))
    }

    fn read_f64<E: Error>(value: f64) -> Result<Option<Self>, E> {
        Ok(Some(value as f32))
    }

    fn read_str<E: Error>(value: &str) -> Result<Option<Self>, E> {
        Ok(value.parse().ok())
    }
}
