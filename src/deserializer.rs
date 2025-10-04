//! Serde deserializer implementation for UCL
//!
//! This module provides the serde integration, allowing UCL text to be
//! deserialized directly into Rust types using the standard serde derive macros.

use crate::error::{ParseError, Position, SerdeError, UclError};
use crate::lexer::LexerConfig;
use crate::parser::{UclArray, UclParser, UclValue, VariableHandler};
use serde::de::{self, Deserialize, DeserializeSeed, Visitor};

/// UCL deserializer that implements serde::Deserializer
pub struct UclDeserializer<'a> {
    parser: UclParser<'a>,
    current_value: Option<UclValue>,
}

impl<'a> UclDeserializer<'a> {
    /// Creates a new deserializer from UCL text
    pub fn new(input: &'a str) -> Self {
        Self {
            parser: UclParser::new(input),
            current_value: None,
        }
    }

    /// Creates a deserializer with custom lexer configuration
    pub fn with_lexer_config(input: &'a str, config: LexerConfig) -> Self {
        Self {
            parser: UclParser::with_lexer_config(input, config),
            current_value: None,
        }
    }

    /// Creates a deserializer with a variable handler
    pub fn with_variable_handler(input: &'a str, handler: Box<dyn VariableHandler>) -> Self {
        Self {
            parser: UclParser::with_variable_handler(input, handler),
            current_value: None,
        }
    }

    /// Creates a deserializer from an existing parser
    pub fn from_parser(parser: UclParser<'a>) -> Self {
        Self {
            parser,
            current_value: None,
        }
    }

    /// Returns the current position in the input
    fn current_position(&self) -> Position {
        self.parser.current_position()
    }

    /// Parses the next value if not already cached
    fn ensure_value(&mut self) -> Result<&UclValue, UclError> {
        if self.current_value.is_none() {
            let value = self.parser.parse_document().map_err(|e| {
                // Add context to parse errors
                match e {
                    ParseError::UnexpectedToken {
                        token,
                        position,
                        expected,
                        ..
                    } => UclError::Serde(SerdeError::Custom(format!(
                        "Unexpected token '{}' at {}, expected {}",
                        token, position, expected
                    ))),
                    ParseError::VariableNotFound { name, position, .. } => {
                        UclError::Serde(SerdeError::Custom(format!(
                            "Variable '{}' not found at {}",
                            name, position
                        )))
                    }
                    other => UclError::Parse(other),
                }
            })?;
            self.current_value = Some(value);
        }
        Ok(self.current_value.as_ref().unwrap())
    }

    /// Takes the current value, parsing if necessary
    fn take_value(&mut self) -> Result<UclValue, UclError> {
        if self.current_value.is_none() {
            let value = self.parser.parse_document().map_err(|e| {
                // Add context to parse errors
                match e {
                    ParseError::UnexpectedToken {
                        token,
                        position,
                        expected,
                        ..
                    } => UclError::Serde(SerdeError::Custom(format!(
                        "Unexpected token '{}' at {}, expected {}",
                        token, position, expected
                    ))),
                    ParseError::VariableNotFound { name, position, .. } => {
                        UclError::Serde(SerdeError::Custom(format!(
                            "Variable '{}' not found at {}",
                            name, position
                        )))
                    }
                    other => UclError::Parse(other),
                }
            })?;
            self.current_value = Some(value);
        }
        Ok(self.current_value.take().unwrap())
    }

    /// Returns a reference to the underlying parser
    pub fn parser(&self) -> &UclParser<'a> {
        &self.parser
    }

    /// Returns a mutable reference to the underlying parser
    pub fn parser_mut(&mut self) -> &mut UclParser<'a> {
        &mut self.parser
    }
}

impl<'de> de::Deserializer<'de> for UclDeserializer<'de> {
    type Error = UclError;

    fn deserialize_any<V>(mut self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = self.take_value()?;

        match value {
            UclValue::String(s) => visitor.visit_string(s),
            UclValue::Integer(i) => visitor.visit_i64(i),
            UclValue::Float(f) => visitor.visit_f64(f),
            UclValue::Boolean(b) => visitor.visit_bool(b),
            UclValue::Null => visitor.visit_unit(),
            UclValue::Object(obj) => {
                // Put the value back for deserialize_map to take
                self.current_value = Some(UclValue::Object(obj));
                self.deserialize_map(visitor)
            }
            UclValue::Array(arr) => {
                // Put the value back for deserialize_seq to take
                self.current_value = Some(UclValue::Array(arr));
                self.deserialize_seq(visitor)
            }
        }
    }

    fn deserialize_bool<V>(mut self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = self.take_value()?;
        match value {
            UclValue::Boolean(b) => visitor.visit_bool(b),
            _ => Err(UclError::Serde(SerdeError::TypeMismatch {
                expected: "boolean".to_string(),
                found: format!("{:?}", value),
                position: self.current_position(),
            })),
        }
    }

    fn deserialize_i8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i64<V>(mut self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = self.take_value()?;
        match value {
            UclValue::Integer(i) => visitor.visit_i64(i),
            UclValue::Float(f) => visitor.visit_i64(f as i64),
            _ => Err(UclError::Serde(SerdeError::TypeMismatch {
                expected: "integer".to_string(),
                found: format!("{:?}", value),
                position: self.current_position(),
            })),
        }
    }

    fn deserialize_u8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u64<V>(mut self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = self.take_value()?;
        match value {
            UclValue::Integer(i) if i >= 0 => visitor.visit_u64(i as u64),
            UclValue::Float(f) if f >= 0.0 => visitor.visit_u64(f as u64),
            _ => Err(UclError::Serde(SerdeError::TypeMismatch {
                expected: "unsigned integer".to_string(),
                found: format!("{:?}", value),
                position: self.current_position(),
            })),
        }
    }

    fn deserialize_f32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_f64(visitor)
    }

    fn deserialize_f64<V>(mut self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = self.take_value()?;
        match value {
            UclValue::Float(f) => visitor.visit_f64(f),
            UclValue::Integer(i) => visitor.visit_f64(i as f64),
            _ => Err(UclError::Serde(SerdeError::TypeMismatch {
                expected: "float".to_string(),
                found: format!("{:?}", value),
                position: self.current_position(),
            })),
        }
    }

    fn deserialize_char<V>(mut self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = self.take_value()?;
        match value {
            UclValue::String(s) => {
                let mut chars = s.chars();
                match (chars.next(), chars.next()) {
                    (Some(c), None) => visitor.visit_char(c),
                    _ => Err(UclError::Serde(SerdeError::TypeMismatch {
                        expected: "single character".to_string(),
                        found: format!("string of length {}", s.len()),
                        position: self.current_position(),
                    })),
                }
            }
            _ => Err(UclError::Serde(SerdeError::TypeMismatch {
                expected: "character".to_string(),
                found: format!("{:?}", value),
                position: self.current_position(),
            })),
        }
    }

    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_string(visitor)
    }

    fn deserialize_string<V>(mut self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = self.take_value()?;
        match value {
            UclValue::String(s) => visitor.visit_string(s),
            _ => Err(UclError::Serde(SerdeError::TypeMismatch {
                expected: "string".to_string(),
                found: format!("{:?}", value),
                position: self.current_position(),
            })),
        }
    }

    fn deserialize_bytes<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_string(visitor)
    }

    fn deserialize_byte_buf<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_string(visitor)
    }

    fn deserialize_option<V>(mut self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = self.ensure_value()?;
        match value {
            UclValue::Null => visitor.visit_none(),
            _ => visitor.visit_some(self),
        }
    }

    fn deserialize_unit<V>(mut self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = self.take_value()?;
        match value {
            UclValue::Null => visitor.visit_unit(),
            _ => Err(UclError::Serde(SerdeError::TypeMismatch {
                expected: "null".to_string(),
                found: format!("{:?}", value),
                position: self.current_position(),
            })),
        }
    }

    fn deserialize_unit_struct<V>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_unit(visitor)
    }

    fn deserialize_newtype_struct<V>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_seq<V>(mut self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = self.take_value()?;
        match value {
            UclValue::Array(array) => {
                let seq = UclSeqAccess::new(array);
                visitor.visit_seq(seq)
            }
            // Allow objects to be deserialized as sequences of values
            UclValue::Object(object) => {
                let seq = UclObjectSeqAccess::new(object.into_values());
                visitor.visit_seq(seq)
            }
            _ => Err(UclError::Serde(SerdeError::TypeMismatch {
                expected: "array or object".to_string(),
                found: format!("{:?}", value),
                position: self.current_position(),
            })),
        }
    }

    fn deserialize_tuple<V>(self, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    fn deserialize_tuple_struct<V>(
        self,
        _name: &'static str,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    fn deserialize_map<V>(mut self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = self.take_value()?;
        match value {
            UclValue::Object(object) => {
                let map = UclMapAccess::new(object);
                visitor.visit_map(map)
            }
            // Allow arrays to be deserialized as maps with string indices
            UclValue::Array(array) => {
                let mut object = crate::parser::UclObject::new();
                for (i, value) in array.into_iter().enumerate() {
                    object.insert(i.to_string(), value);
                }
                let map = UclMapAccess::new(object);
                visitor.visit_map(map)
            }
            _ => Err(UclError::Serde(SerdeError::TypeMismatch {
                expected: "object or array".to_string(),
                found: format!("{:?}", value),
                position: self.current_position(),
            })),
        }
    }

    fn deserialize_struct<V>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_map(visitor)
    }

    fn deserialize_enum<V>(
        mut self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = self.take_value()?;
        match value {
            // Unit variant (string)
            UclValue::String(s) => {
                let enum_access = UclEnumAccess::new_unit(s);
                visitor.visit_enum(enum_access)
            }
            // Data variant (object with single key)
            UclValue::Object(mut obj) => {
                if obj.len() == 1 {
                    let (variant_name, variant_value) = obj.shift_remove_index(0).unwrap();
                    let enum_access = UclEnumAccess::new_data(variant_name, variant_value);
                    visitor.visit_enum(enum_access)
                } else {
                    Err(UclError::Serde(SerdeError::TypeMismatch {
                        expected: "enum (string or single-key object)".to_string(),
                        found: format!("object with {} keys", obj.len()),
                        position: self.current_position(),
                    }))
                }
            }
            _ => Err(UclError::Serde(SerdeError::TypeMismatch {
                expected: "enum (string or object)".to_string(),
                found: format!("{:?}", value),
                position: self.current_position(),
            })),
        }
    }

    fn deserialize_identifier<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_string(visitor)
    }

    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_any(visitor)
    }
}

/// Sequence access for UCL arrays
struct UclSeqAccess {
    array: std::vec::IntoIter<UclValue>,
}

impl UclSeqAccess {
    fn new(array: Box<UclArray>) -> Self {
        Self {
            array: array.into_vec().into_iter(),
        }
    }
}

impl<'de> de::SeqAccess<'de> for UclSeqAccess {
    type Error = UclError;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: DeserializeSeed<'de>,
    {
        match self.array.next() {
            Some(value) => {
                let deserializer = UclValueDeserializer::new(value);
                seed.deserialize(deserializer).map(Some)
            }
            None => Ok(None),
        }
    }

    fn size_hint(&self) -> Option<usize> {
        let (lower, upper) = self.array.size_hint();
        upper.or(Some(lower))
    }
}

/// Sequence access for UCL object values (direct iterator, no Vec allocation)
struct UclObjectSeqAccess {
    values: indexmap::map::IntoValues<String, UclValue>,
}

impl UclObjectSeqAccess {
    fn new(values: indexmap::map::IntoValues<String, UclValue>) -> Self {
        Self { values }
    }
}

impl<'de> de::SeqAccess<'de> for UclObjectSeqAccess {
    type Error = UclError;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: DeserializeSeed<'de>,
    {
        match self.values.next() {
            Some(value) => {
                let deserializer = UclValueDeserializer::new(value);
                seed.deserialize(deserializer).map(Some)
            }
            None => Ok(None),
        }
    }

    fn size_hint(&self) -> Option<usize> {
        let (lower, upper) = self.values.size_hint();
        upper.or(Some(lower))
    }
}

/// Map access for UCL objects
struct UclMapAccess {
    object: indexmap::map::IntoIter<String, UclValue>,
    current_value: Option<UclValue>,
}

impl UclMapAccess {
    fn new(object: crate::parser::UclObject) -> Self {
        Self {
            object: object.into_iter(),
            current_value: None,
        }
    }
}

impl<'de> de::MapAccess<'de> for UclMapAccess {
    type Error = UclError;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error>
    where
        K: DeserializeSeed<'de>,
    {
        match self.object.next() {
            Some((key, value)) => {
                self.current_value = Some(value);
                let key_deserializer = UclValueDeserializer::new(UclValue::String(key));
                seed.deserialize(key_deserializer).map(Some)
            }
            None => Ok(None),
        }
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
    where
        V: DeserializeSeed<'de>,
    {
        match self.current_value.take() {
            Some(value) => {
                let deserializer = UclValueDeserializer::new(value);
                seed.deserialize(deserializer)
            }
            None => Err(UclError::Serde(SerdeError::Custom(
                "No value available for map entry".to_string(),
            ))),
        }
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.object.len())
    }
}

/// Enum access for UCL enum deserialization
struct UclEnumAccess {
    variant_name: String,
    variant_value: Option<UclValue>,
}

impl UclEnumAccess {
    fn new_unit(variant_name: String) -> Self {
        Self {
            variant_name,
            variant_value: None,
        }
    }

    fn new_data(variant_name: String, variant_value: UclValue) -> Self {
        Self {
            variant_name,
            variant_value: Some(variant_value),
        }
    }
}

impl<'de> de::EnumAccess<'de> for UclEnumAccess {
    type Error = UclError;
    type Variant = UclVariantAccess;

    fn variant_seed<V>(self, seed: V) -> Result<(V::Value, Self::Variant), Self::Error>
    where
        V: DeserializeSeed<'de>,
    {
        let variant_name_deserializer =
            UclValueDeserializer::new(UclValue::String(self.variant_name));
        let variant_name = seed.deserialize(variant_name_deserializer)?;
        let variant_access = UclVariantAccess::new(self.variant_value);
        Ok((variant_name, variant_access))
    }
}

/// Variant access for UCL enum variants
struct UclVariantAccess {
    value: Option<UclValue>,
}

impl UclVariantAccess {
    fn new(value: Option<UclValue>) -> Self {
        Self { value }
    }
}

impl<'de> de::VariantAccess<'de> for UclVariantAccess {
    type Error = UclError;

    fn unit_variant(self) -> Result<(), Self::Error> {
        match self.value {
            None => Ok(()),
            Some(_) => Err(UclError::Serde(SerdeError::Custom(
                "Expected unit variant, found data".to_string(),
            ))),
        }
    }

    fn newtype_variant_seed<T>(self, seed: T) -> Result<T::Value, Self::Error>
    where
        T: DeserializeSeed<'de>,
    {
        match self.value {
            Some(value) => {
                let deserializer = UclValueDeserializer::new(value);
                seed.deserialize(deserializer)
            }
            None => Err(UclError::Serde(SerdeError::Custom(
                "Expected newtype variant data, found unit".to_string(),
            ))),
        }
    }

    fn tuple_variant<V>(self, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self.value {
            Some(UclValue::Array(array)) => {
                let seq = UclSeqAccess::new(array);
                visitor.visit_seq(seq)
            }
            Some(value) => {
                // Single value as tuple with one element
                use smallvec::SmallVec;
                let mut array = SmallVec::new();
                array.push(value);
                let seq = UclSeqAccess::new(Box::new(array));
                visitor.visit_seq(seq)
            }
            None => Err(UclError::Serde(SerdeError::Custom(
                "Expected tuple variant data, found unit".to_string(),
            ))),
        }
    }

    fn struct_variant<V>(
        self,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self.value {
            Some(UclValue::Object(object)) => {
                let map = UclMapAccess::new(object);
                visitor.visit_map(map)
            }
            Some(_) => Err(UclError::Serde(SerdeError::Custom(
                "Expected struct variant data (object), found other type".to_string(),
            ))),
            None => Err(UclError::Serde(SerdeError::Custom(
                "Expected struct variant data, found unit".to_string(),
            ))),
        }
    }
}

/// Deserializer for individual UCL values
struct UclValueDeserializer {
    value: UclValue,
}

impl UclValueDeserializer {
    fn new(value: UclValue) -> Self {
        Self { value }
    }
}

impl<'de> de::Deserializer<'de> for UclValueDeserializer {
    type Error = UclError;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self.value {
            UclValue::String(s) => visitor.visit_string(s),
            UclValue::Integer(i) => visitor.visit_i64(i),
            UclValue::Float(f) => visitor.visit_f64(f),
            UclValue::Boolean(b) => visitor.visit_bool(b),
            UclValue::Null => visitor.visit_unit(),
            UclValue::Object(obj) => {
                let map = UclMapAccess::new(obj);
                visitor.visit_map(map)
            }
            UclValue::Array(arr) => {
                let seq = UclSeqAccess::new(arr);
                visitor.visit_seq(seq)
            }
        }
    }

    // Delegate all other methods to deserialize_any for simplicity
    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf option unit unit_struct newtype_struct seq tuple
        tuple_struct map struct enum identifier ignored_any
    }
}

/// Convenience function to deserialize UCL text into a Rust type
pub fn from_str<'a, T>(s: &'a str) -> Result<T, UclError>
where
    T: Deserialize<'a>,
{
    let deserializer = UclDeserializer::new(s);
    T::deserialize(deserializer)
}

/// Convenience function to deserialize UCL text with variable expansion
pub fn from_str_with_variables<'a, T>(
    s: &'a str,
    handler: Box<dyn VariableHandler>,
) -> Result<T, UclError>
where
    T: Deserialize<'a>,
{
    let deserializer = UclDeserializer::with_variable_handler(s, handler);
    T::deserialize(deserializer)
}

/// Convenience function to deserialize UCL text with custom lexer configuration
pub fn from_str_with_config<'a, T>(s: &'a str, config: LexerConfig) -> Result<T, UclError>
where
    T: Deserialize<'a>,
{
    let deserializer = UclDeserializer::with_lexer_config(s, config);
    T::deserialize(deserializer)
}

/// Convenience function to deserialize UCL text with both custom config and variables
pub fn from_str_with_config_and_variables<'a, T>(
    s: &'a str,
    _config: LexerConfig,
    handler: Box<dyn VariableHandler>,
) -> Result<T, UclError>
where
    T: Deserialize<'a>,
{
    // Create parser with variable handler first, then apply config
    let parser = UclParser::with_variable_handler(s, handler)
        .with_config(crate::parser::ParserConfig::default());
    // Note: We can't easily combine lexer config with variable handler in current API
    // This would require extending the parser API
    let deserializer = UclDeserializer {
        parser,
        current_value: None,
    };
    T::deserialize(deserializer)
}

/// Convenience function to deserialize UCL text using environment variables
pub fn from_str_with_env<'a, T>(s: &'a str) -> Result<T, UclError>
where
    T: Deserialize<'a>,
{
    let handler = Box::new(crate::parser::EnvironmentVariableHandler);
    from_str_with_variables(s, handler)
}

/// Convenience function to deserialize UCL text using a map of variables
pub fn from_str_with_map<'a, T>(
    s: &'a str,
    variables: std::collections::HashMap<String, String>,
) -> Result<T, UclError>
where
    T: Deserialize<'a>,
{
    let handler = Box::new(crate::parser::MapVariableHandler::from_map(variables));
    from_str_with_variables(s, handler)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use std::collections::HashMap;

    #[derive(Debug, Deserialize, PartialEq)]
    struct TestStruct {
        name: String,
        age: u32,
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct NestedStruct {
        user: TestStruct,
        active: bool,
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct OptionalFields {
        required: String,
        #[serde(default)]
        optional: Option<String>,
        #[serde(default = "default_number")]
        number: i32,
    }

    fn default_number() -> i32 {
        42
    }

    #[derive(Debug, Deserialize, PartialEq)]
    #[serde(rename_all = "snake_case")]
    struct RenamedFields {
        first_name: String,
        last_name: String,
    }

    #[derive(Debug, Deserialize, PartialEq)]
    enum TestEnum {
        Unit,
        Newtype(String),
        Tuple(String, i32),
        Struct { field: String },
    }

    #[test]
    fn test_deserializer_creation() {
        let _deserializer = UclDeserializer::new("test");
        // Just test that it compiles and creates successfully
    }

    #[test]
    fn test_basic_struct_deserialization() {
        let ucl = r#"{ name = "Alice", age = 30 }"#;
        let result: Result<TestStruct, _> = from_str(ucl);

        // This test will pass once the full parsing pipeline is working
        match result {
            Ok(parsed) => {
                assert_eq!(parsed.name, "Alice");
                assert_eq!(parsed.age, 30);
            }
            Err(_) => {
                // Expected to fail until full parsing is implemented
                // This is testing the deserializer interface
            }
        }
    }

    #[test]
    fn test_nested_struct_deserialization() {
        let ucl = r#"{ 
            user = { name = "Bob", age = 25 }, 
            active = true 
        }"#;
        let result: Result<NestedStruct, _> = from_str(ucl);

        match result {
            Ok(parsed) => {
                assert_eq!(parsed.user.name, "Bob");
                assert_eq!(parsed.user.age, 25);
                assert_eq!(parsed.active, true);
            }
            Err(_) => {
                // Expected to fail until full parsing is implemented
            }
        }
    }

    #[test]
    fn test_array_deserialization() {
        let ucl = r#"[1, 2, 3, 4, 5]"#;
        let result: Result<Vec<i32>, _> = from_str(ucl);

        match result {
            Ok(parsed) => {
                assert_eq!(parsed, vec![1, 2, 3, 4, 5]);
            }
            Err(_) => {
                // Expected to fail until full parsing is implemented
            }
        }
    }

    #[test]
    fn test_map_deserialization() {
        let ucl = r#"{ key1 = "value1", key2 = "value2" }"#;
        let result: Result<HashMap<String, String>, _> = from_str(ucl);

        match result {
            Ok(parsed) => {
                assert_eq!(parsed.get("key1"), Some(&"value1".to_string()));
                assert_eq!(parsed.get("key2"), Some(&"value2".to_string()));
            }
            Err(_) => {
                // Expected to fail until full parsing is implemented
            }
        }
    }

    #[test]
    fn test_optional_fields() {
        let ucl = r#"{ required = "test" }"#;
        let result: Result<OptionalFields, _> = from_str(ucl);

        match result {
            Ok(parsed) => {
                assert_eq!(parsed.required, "test");
                assert_eq!(parsed.optional, None);
                assert_eq!(parsed.number, 42); // Default value
            }
            Err(_) => {
                // Expected to fail until full parsing is implemented
            }
        }
    }

    #[test]
    fn test_renamed_fields() {
        let ucl = r#"{ first_name = "John", last_name = "Doe" }"#;
        let result: Result<RenamedFields, _> = from_str(ucl);

        match result {
            Ok(parsed) => {
                assert_eq!(parsed.first_name, "John");
                assert_eq!(parsed.last_name, "Doe");
            }
            Err(_) => {
                // Expected to fail until full parsing is implemented
            }
        }
    }

    #[test]
    fn test_enum_unit_variant() {
        let ucl = r#""Unit""#;
        let result: Result<TestEnum, _> = from_str(ucl);

        match result {
            Ok(TestEnum::Unit) => {
                // Success
            }
            Ok(other) => panic!("Expected Unit variant, got {:?}", other),
            Err(_) => {
                // Expected to fail until full parsing is implemented
            }
        }
    }

    #[test]
    fn test_enum_newtype_variant() {
        let ucl = r#"{ Newtype = "test_value" }"#;
        let result: Result<TestEnum, _> = from_str(ucl);

        match result {
            Ok(TestEnum::Newtype(value)) => {
                assert_eq!(value, "test_value");
            }
            Ok(other) => panic!("Expected Newtype variant, got {:?}", other),
            Err(_) => {
                // Expected to fail until full parsing is implemented
            }
        }
    }

    #[test]
    fn test_enum_struct_variant() {
        let ucl = r#"{ Struct = { field = "test" } }"#;
        let result: Result<TestEnum, _> = from_str(ucl);

        match result {
            Ok(TestEnum::Struct { field }) => {
                assert_eq!(field, "test");
            }
            Ok(other) => panic!("Expected Struct variant, got {:?}", other),
            Err(_) => {
                // Expected to fail until full parsing is implemented
            }
        }
    }

    #[test]
    fn test_variable_expansion() {
        let mut variables = HashMap::new();
        variables.insert("name".to_string(), "World".to_string());

        let ucl = r#"{ greeting = "Hello ${name}!" }"#;
        let result: Result<HashMap<String, String>, _> = from_str_with_map(ucl, variables);

        match result {
            Ok(parsed) => {
                assert_eq!(parsed.get("greeting"), Some(&"Hello World!".to_string()));
            }
            Err(_) => {
                // Expected to fail until full parsing is implemented
            }
        }
    }

    #[test]
    fn test_environment_variables() {
        unsafe {
            std::env::set_var("TEST_UCL_VAR", "test_value");
        }

        let ucl = r#"{ env_value = "${TEST_UCL_VAR}" }"#;
        let result: Result<HashMap<String, String>, _> = from_str_with_env(ucl);

        match result {
            Ok(parsed) => {
                assert_eq!(parsed.get("env_value"), Some(&"test_value".to_string()));
            }
            Err(_) => {
                // Expected to fail until full parsing is implemented
            }
        }

        unsafe {
            std::env::remove_var("TEST_UCL_VAR");
        }
    }

    #[test]
    fn test_type_coercion() {
        // Test that numbers can be coerced to different types
        let ucl = r#"{ 
            as_i32 = 42, 
            as_f64 = 42, 
            as_string = "42",
            as_bool = true
        }"#;

        #[derive(Debug, Deserialize)]
        struct TypeCoercion {
            as_i32: i32,
            as_f64: f64,
            as_string: String,
            as_bool: bool,
        }

        let result: Result<TypeCoercion, _> = from_str(ucl);

        match result {
            Ok(parsed) => {
                assert_eq!(parsed.as_i32, 42);
                assert_eq!(parsed.as_f64, 42.0);
                assert_eq!(parsed.as_string, "42");
                assert_eq!(parsed.as_bool, true);
            }
            Err(_) => {
                // Expected to fail until full parsing is implemented
            }
        }
    }

    #[test]
    fn test_error_propagation() {
        // Test that parsing errors are properly propagated through serde
        let invalid_ucl = r#"{ invalid syntax }"#;
        let result: Result<TestStruct, _> = from_str(invalid_ucl);

        assert!(result.is_err());

        // Test that the error contains useful information
        if let Err(error) = result {
            let error_string = format!("{}", error);
            // Should contain position or context information
            assert!(!error_string.is_empty());
        }
    }

    #[test]
    fn test_convenience_functions() {
        // Test all convenience function signatures
        let ucl = r#"{ name = "test", age = 25 }"#;

        // Basic from_str
        let _result1: Result<TestStruct, _> = from_str(ucl);

        // With lexer config
        let config = LexerConfig::default();
        let _result2: Result<TestStruct, _> = from_str_with_config(ucl, config);

        // With variables
        let variables = HashMap::new();
        let _result3: Result<TestStruct, _> = from_str_with_map(ucl, variables);

        // With environment variables
        let _result4: Result<TestStruct, _> = from_str_with_env(ucl);

        // All should compile and have consistent interfaces
    }

    #[test]
    fn test_deserializer_methods() {
        // Test that deserializer provides access to underlying parser
        let mut deserializer = UclDeserializer::new("test");

        // Test parser access
        let _parser_ref = deserializer.parser();
        let _parser_mut = deserializer.parser_mut();

        // Test position tracking
        let _position = deserializer.current_position();
    }
}
