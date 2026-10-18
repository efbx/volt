use serde::de::Error;

use super::rule::Coerce;

impl Coerce<'_> for i64 {
    fn read_bool<E: Error>(value: bool) -> Result<Option<Self>, E> {
        Ok(Some(i64::from(value)))
    }

    fn read_i64<E: Error>(value: i64) -> Result<Option<Self>, E> {
        Ok(Some(value))
    }

    fn read_u64<E: Error>(value: u64) -> Result<Option<Self>, E> {
        Ok(Some(i64::try_from(value).unwrap_or(i64::MAX)))
    }

    fn read_f64<E: Error>(value: f64) -> Result<Option<Self>, E> {
        Ok(Some(value as i64))
    }

    fn read_str<E: Error>(value: &str) -> Result<Option<Self>, E> {
        Ok(value
            .parse()
            .ok()
            .or_else(|| value.parse::<f64>().ok().map(|number| number as i64)))
    }
}

impl Coerce<'_> for u64 {
    fn read_bool<E: Error>(value: bool) -> Result<Option<Self>, E> {
        Ok(Some(u64::from(value)))
    }

    fn read_i64<E: Error>(value: i64) -> Result<Option<Self>, E> {
        Ok(Some(u64::try_from(value).unwrap_or(0)))
    }

    fn read_u64<E: Error>(value: u64) -> Result<Option<Self>, E> {
        Ok(Some(value))
    }

    fn read_f64<E: Error>(value: f64) -> Result<Option<Self>, E> {
        Ok(Some(value as u64))
    }

    fn read_str<E: Error>(value: &str) -> Result<Option<Self>, E> {
        Ok(value
            .parse()
            .ok()
            .or_else(|| value.parse::<f64>().ok().map(|number| number as u64)))
    }
}

impl Coerce<'_> for i32 {
    fn read_bool<E: Error>(value: bool) -> Result<Option<Self>, E> {
        Ok(Some(i32::from(value)))
    }

    fn read_i64<E: Error>(value: i64) -> Result<Option<Self>, E> {
        let saturated = if value < 0 { i32::MIN } else { i32::MAX };
        Ok(Some(i32::try_from(value).unwrap_or(saturated)))
    }

    fn read_u64<E: Error>(value: u64) -> Result<Option<Self>, E> {
        Ok(Some(i32::try_from(value).unwrap_or(i32::MAX)))
    }

    fn read_f64<E: Error>(value: f64) -> Result<Option<Self>, E> {
        Ok(Some(value as i32))
    }

    fn read_str<E: Error>(value: &str) -> Result<Option<Self>, E> {
        Ok(value
            .parse()
            .ok()
            .or_else(|| value.parse::<f64>().ok().map(|number| number as i32)))
    }
}

impl Coerce<'_> for u32 {
    fn read_bool<E: Error>(value: bool) -> Result<Option<Self>, E> {
        Ok(Some(u32::from(value)))
    }

    fn read_i64<E: Error>(value: i64) -> Result<Option<Self>, E> {
        let saturated = if value < 0 { 0 } else { u32::MAX };
        Ok(Some(u32::try_from(value).unwrap_or(saturated)))
    }

    fn read_u64<E: Error>(value: u64) -> Result<Option<Self>, E> {
        Ok(Some(u32::try_from(value).unwrap_or(u32::MAX)))
    }

    fn read_f64<E: Error>(value: f64) -> Result<Option<Self>, E> {
        Ok(Some(value as u32))
    }

    fn read_str<E: Error>(value: &str) -> Result<Option<Self>, E> {
        Ok(value
            .parse()
            .ok()
            .or_else(|| value.parse::<f64>().ok().map(|number| number as u32)))
    }
}
