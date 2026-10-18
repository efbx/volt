use serde::de::Error;

use super::rule::Coerce;

impl Coerce<'_> for bool {
    fn read_bool<E: Error>(value: bool) -> Result<Option<Self>, E> {
        Ok(Some(value))
    }

    fn read_i64<E: Error>(value: i64) -> Result<Option<Self>, E> {
        Ok(Some(value != 0))
    }

    fn read_u64<E: Error>(value: u64) -> Result<Option<Self>, E> {
        Ok(Some(value != 0))
    }

    fn read_f64<E: Error>(value: f64) -> Result<Option<Self>, E> {
        Ok(Some(value != 0.0))
    }

    fn read_str<E: Error>(value: &str) -> Result<Option<Self>, E> {
        Ok(match value {
            "1" | "t" | "T" | "TRUE" | "true" | "True" => Some(true),
            "0" | "f" | "F" | "FALSE" | "false" | "False" => Some(false),
            _ => None,
        })
    }
}
