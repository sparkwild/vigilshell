//! Schema-only adapters preserve nullable required fields independently of Rust Option.
use schemars::{JsonSchema, Schema, SchemaGenerator};
use std::{borrow::Cow, marker::PhantomData};

pub struct NullableRequired<T>(PhantomData<T>);
impl<T: JsonSchema> JsonSchema for NullableRequired<T> {
    fn schema_name() -> Cow<'static, str> {
        T::schema_name()
    }
    fn inline_schema() -> bool {
        true
    }
    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        <Option<T>>::json_schema(generator)
    }
}

/// A nullable wire field must still be present unless its DTO declares a default.
pub fn required_nullable<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    serde::Deserialize::deserialize(deserializer)
}
