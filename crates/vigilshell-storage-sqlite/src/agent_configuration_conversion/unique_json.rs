use serde::de::{Deserialize, Deserializer, Error, MapAccess, SeqAccess, Visitor};
use std::{collections::HashSet, fmt};

struct UniqueValue;

impl<'de> Deserialize<'de> for UniqueValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(Self)
    }
}

impl<'de> Visitor<'de> for UniqueValue {
    type Value = Self;
    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("JSON without duplicate object keys")
    }
    fn visit_bool<E: Error>(self, _: bool) -> Result<Self, E> {
        Ok(self)
    }
    fn visit_i64<E: Error>(self, _: i64) -> Result<Self, E> {
        Ok(self)
    }
    fn visit_u64<E: Error>(self, _: u64) -> Result<Self, E> {
        Ok(self)
    }
    fn visit_f64<E: Error>(self, _: f64) -> Result<Self, E> {
        Ok(self)
    }
    fn visit_str<E: Error>(self, _: &str) -> Result<Self, E> {
        Ok(self)
    }
    fn visit_unit<E: Error>(self) -> Result<Self, E> {
        Ok(self)
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Self, A::Error> {
        while sequence.next_element::<Self>()?.is_some() {}
        Ok(self)
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self, A::Error> {
        let mut names = HashSet::new();
        while let Some(name) = map.next_key::<String>()? {
            if !names.insert(name) {
                return Err(A::Error::custom("duplicate_configuration_field"));
            }
            map.next_value::<Self>()?;
        }
        Ok(self)
    }
}

pub(super) fn validate(text: &str) -> Result<(), serde_json::Error> {
    serde_json::from_str::<UniqueValue>(text).map(|_| ())
}
