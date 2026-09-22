//! Serde deserializer (PLAN.md §2.3).
//!
//! UCL text is parsed into a [`UclValue`] tree, and one value deserializer maps that tree onto
//! serde. The document-level [`UclDeserializer`] only parses and hands the result over, so the
//! document and every nested value follow the same rules:
//!
//! | UCL | serde |
//! | --- | --- |
//! | entry with one value | that value |
//! | entry with several values (implicit array) | sequence |
//! | single value into a sequence target (`Vec<T>`) | one-element sequence |
//! | object into a sequence target | error; use a map (`HashMap`, `IndexMap`) |
//! | `Time` | `f64` seconds (see [`crate::time`] for `Duration` fields) |
//! | `Float` into an integer target | only if integral and in range |
//! | `Null` into `Option<T>` | `None`; anything else is `Some` |
//! | string into an enum | unit variant; an object with one key is a data variant |

use crate::error::UclError;
use crate::lexer::LexerConfig;
use crate::parser::{
    EnvironmentVariableHandler, MapVariableHandler, UclParser, UclParserBuilder, VariableHandler,
};
use crate::value::{Entry, UclArray, UclObject, UclValue};
use serde::de::{self, Deserialize, DeserializeSeed, Unexpected, Visitor};

/// Deserializes UCL text: parses the document, then deserializes the resulting value.
pub struct UclDeserializer<'a> {
    parser: UclParser<'a>,
    value: Option<UclValue>,
}

impl<'a> UclDeserializer<'a> {
    /// Creates a new deserializer from UCL text
    pub fn new(input: &'a str) -> Self {
        Self::from_parser(UclParser::new(input))
    }

    /// Creates a deserializer with custom lexer configuration
    pub fn with_lexer_config(input: &'a str, config: LexerConfig) -> Self {
        Self::from_parser(UclParser::with_lexer_config(input, config))
    }

    /// Creates a deserializer with a variable handler
    pub fn with_variable_handler(input: &'a str, handler: Box<dyn VariableHandler>) -> Self {
        Self::from_parser(UclParser::with_variable_handler(input, handler))
    }

    /// Creates a deserializer from an existing parser
    pub fn from_parser(parser: UclParser<'a>) -> Self {
        Self {
            parser,
            value: None,
        }
    }

    /// Returns a reference to the underlying parser
    pub fn parser(&self) -> &UclParser<'a> {
        &self.parser
    }

    /// Returns a mutable reference to the underlying parser
    pub fn parser_mut(&mut self) -> &mut UclParser<'a> {
        &mut self.parser
    }

    /// Parses the document unless a value is already present.
    fn into_value(mut self) -> Result<ValueDeserializer, UclError> {
        let value = match self.value.take() {
            Some(value) => value,
            None => self.parser.parse_document()?,
        };
        Ok(ValueDeserializer::new(value))
    }
}

/// Forwards each `Deserializer` method of `UclDeserializer` to the value deserializer.
macro_rules! forward_to_value {
    ($($method:ident($($arg:ident: $ty:ty),*))*) => {
        $(
            fn $method<V>(self, $($arg: $ty,)* visitor: V) -> Result<V::Value, UclError>
            where
                V: Visitor<'de>,
            {
                self.into_value()?.$method($($arg,)* visitor)
            }
        )*
    };
}

impl<'de> de::Deserializer<'de> for UclDeserializer<'de> {
    type Error = UclError;

    forward_to_value! {
        deserialize_any()
        deserialize_bool()
        deserialize_i8()
        deserialize_i16()
        deserialize_i32()
        deserialize_i64()
        deserialize_i128()
        deserialize_u8()
        deserialize_u16()
        deserialize_u32()
        deserialize_u64()
        deserialize_u128()
        deserialize_f32()
        deserialize_f64()
        deserialize_char()
        deserialize_str()
        deserialize_string()
        deserialize_bytes()
        deserialize_byte_buf()
        deserialize_option()
        deserialize_unit()
        deserialize_unit_struct(name: &'static str)
        deserialize_newtype_struct(name: &'static str)
        deserialize_seq()
        deserialize_tuple(len: usize)
        deserialize_tuple_struct(name: &'static str, len: usize)
        deserialize_map()
        deserialize_struct(name: &'static str, fields: &'static [&'static str])
        deserialize_enum(name: &'static str, variants: &'static [&'static str])
        deserialize_identifier()
        deserialize_ignored_any()
    }
}

/// Converts a float for an integer target: only an integral value inside the `i64` range is
/// accepted. `as` would truncate `30.7` to `30` and saturate `1e300` to `i64::MAX`.
fn float_to_i64(f: f64) -> Option<i64> {
    // -2^63 and 2^63 are exact in f64; i64::MAX is not, so the upper bound is exclusive.
    if f.fract() == 0.0 && (-9_223_372_036_854_775_808.0..9_223_372_036_854_775_808.0).contains(&f)
    {
        Some(f as i64)
    } else {
        None
    }
}

/// Converts a float for an unsigned target: integral and inside the `u64` range only.
fn float_to_u64(f: f64) -> Option<u64> {
    if f.fract() == 0.0 && (0.0..18_446_744_073_709_551_616.0).contains(&f) {
        Some(f as u64)
    } else {
        None
    }
}

fn unexpected(value: &UclValue) -> Unexpected<'_> {
    match value {
        UclValue::String(s) => Unexpected::Str(s),
        UclValue::Integer(i) => Unexpected::Signed(*i),
        UclValue::Float(f) => Unexpected::Float(*f),
        UclValue::Time(_) => Unexpected::Other("time value"),
        UclValue::Boolean(b) => Unexpected::Bool(*b),
        UclValue::Null => Unexpected::Unit,
        UclValue::Object(_) => Unexpected::Map,
        UclValue::Array(_) => Unexpected::Seq,
    }
}

fn invalid_type<'de, V: Visitor<'de>>(value: &UclValue, visitor: &V) -> UclError {
    de::Error::invalid_type(unexpected(value), visitor)
}

/// Deserializer for one UCL value.
struct ValueDeserializer {
    value: UclValue,
}

impl ValueDeserializer {
    fn new(value: UclValue) -> Self {
        Self { value }
    }

    fn visit_object<'de, V: Visitor<'de>>(
        object: UclObject,
        visitor: V,
    ) -> Result<V::Value, UclError> {
        visitor.visit_map(MapAccess::new(object))
    }

    fn visit_array<'de, V: Visitor<'de>>(
        array: UclArray,
        visitor: V,
    ) -> Result<V::Value, UclError> {
        visitor.visit_seq(SeqAccess::new(array))
    }
}

impl<'de> de::Deserializer<'de> for ValueDeserializer {
    type Error = UclError;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, UclError> {
        match self.value {
            UclValue::String(s) => visitor.visit_string(s),
            UclValue::Integer(i) => visitor.visit_i64(i),
            UclValue::Float(f) | UclValue::Time(f) => visitor.visit_f64(f),
            UclValue::Boolean(b) => visitor.visit_bool(b),
            UclValue::Null => visitor.visit_unit(),
            UclValue::Object(obj) => Self::visit_object(obj, visitor),
            UclValue::Array(arr) => Self::visit_array(arr, visitor),
        }
    }

    fn deserialize_bool<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, UclError> {
        match self.value {
            UclValue::Boolean(b) => visitor.visit_bool(b),
            other => Err(invalid_type(&other, &visitor)),
        }
    }

    fn deserialize_i64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, UclError> {
        match self.value {
            UclValue::Integer(i) => visitor.visit_i64(i),
            UclValue::Float(f) | UclValue::Time(f) => match float_to_i64(f) {
                Some(i) => visitor.visit_i64(i),
                None => Err(de::Error::invalid_value(Unexpected::Float(f), &visitor)),
            },
            other => Err(invalid_type(&other, &visitor)),
        }
    }

    fn deserialize_u64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, UclError> {
        match self.value {
            // Negative values are rejected by the visitor's own range check.
            UclValue::Integer(i) => visitor.visit_i64(i),
            UclValue::Float(f) | UclValue::Time(f) => match float_to_u64(f) {
                Some(u) => visitor.visit_u64(u),
                None => Err(de::Error::invalid_value(Unexpected::Float(f), &visitor)),
            },
            other => Err(invalid_type(&other, &visitor)),
        }
    }

    // Narrower integer targets: serde range-checks the value in `visit_i64`/`visit_u64`.
    fn deserialize_i8<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, UclError> {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i16<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, UclError> {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, UclError> {
        self.deserialize_i64(visitor)
    }

    fn deserialize_i128<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, UclError> {
        self.deserialize_i64(visitor)
    }

    fn deserialize_u8<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, UclError> {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u16<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, UclError> {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, UclError> {
        self.deserialize_u64(visitor)
    }

    fn deserialize_u128<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, UclError> {
        self.deserialize_u64(visitor)
    }

    fn deserialize_f32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, UclError> {
        self.deserialize_f64(visitor)
    }

    fn deserialize_f64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, UclError> {
        match self.value {
            UclValue::Float(f) | UclValue::Time(f) => visitor.visit_f64(f),
            UclValue::Integer(i) => visitor.visit_f64(i as f64),
            other => Err(invalid_type(&other, &visitor)),
        }
    }

    fn deserialize_char<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, UclError> {
        match self.value {
            UclValue::String(s) => {
                let mut chars = s.chars();
                match (chars.next(), chars.next()) {
                    (Some(c), None) => visitor.visit_char(c),
                    _ => Err(de::Error::invalid_value(Unexpected::Str(&s), &visitor)),
                }
            }
            other => Err(invalid_type(&other, &visitor)),
        }
    }

    fn deserialize_str<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, UclError> {
        self.deserialize_string(visitor)
    }

    fn deserialize_string<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, UclError> {
        match self.value {
            UclValue::String(s) => visitor.visit_string(s),
            other => Err(invalid_type(&other, &visitor)),
        }
    }

    fn deserialize_bytes<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, UclError> {
        self.deserialize_string(visitor)
    }

    fn deserialize_byte_buf<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, UclError> {
        self.deserialize_string(visitor)
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, UclError> {
        match self.value {
            UclValue::Null => visitor.visit_none(),
            _ => visitor.visit_some(self),
        }
    }

    fn deserialize_unit<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, UclError> {
        match self.value {
            UclValue::Null => visitor.visit_unit(),
            other => Err(invalid_type(&other, &visitor)),
        }
    }

    fn deserialize_unit_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, UclError> {
        self.deserialize_unit(visitor)
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, UclError> {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_seq<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, UclError> {
        match self.value {
            UclValue::Array(array) => Self::visit_array(array, visitor),
            // An object is not a sequence: reading its values would drop the keys.
            UclValue::Object(_) => Err(de::Error::invalid_type(Unexpected::Map, &visitor)),
            // One-or-many: a key written once reads like a key written several times.
            single => Self::visit_array(vec![single], visitor),
        }
    }

    fn deserialize_tuple<V: Visitor<'de>>(
        self,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value, UclError> {
        self.deserialize_seq(visitor)
    }

    fn deserialize_tuple_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value, UclError> {
        self.deserialize_seq(visitor)
    }

    fn deserialize_map<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, UclError> {
        match self.value {
            UclValue::Object(object) => Self::visit_object(object, visitor),
            // An array reads as a map from string indices
            UclValue::Array(array) => {
                let object = array
                    .into_iter()
                    .enumerate()
                    .map(|(i, v)| (i.to_string(), v))
                    .collect();
                Self::visit_object(object, visitor)
            }
            other => Err(invalid_type(&other, &visitor)),
        }
    }

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, UclError> {
        self.deserialize_map(visitor)
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, UclError> {
        match self.value {
            // Unit variant
            UclValue::String(s) => visitor.visit_enum(EnumAccess::unit(s)),
            // Data variant: an object with a single key
            UclValue::Object(mut obj) if obj.len() == 1 => {
                let (name, entry) = obj.remove_index(0).unwrap();
                visitor.visit_enum(EnumAccess::data(name, entry.into_value()))
            }
            UclValue::Object(obj) => Err(de::Error::invalid_length(
                obj.len(),
                &"an object with exactly one key (the variant name)",
            )),
            other => Err(invalid_type(&other, &visitor)),
        }
    }

    fn deserialize_identifier<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, UclError> {
        self.deserialize_string(visitor)
    }

    fn deserialize_ignored_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, UclError> {
        visitor.visit_unit()
    }
}

/// Sequence access over an explicit array (or the values of an implicit one).
struct SeqAccess {
    items: std::vec::IntoIter<UclValue>,
}

impl SeqAccess {
    fn new(array: UclArray) -> Self {
        Self {
            items: array.into_iter(),
        }
    }
}

impl<'de> de::SeqAccess<'de> for SeqAccess {
    type Error = UclError;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, UclError>
    where
        T: DeserializeSeed<'de>,
    {
        match self.items.next() {
            Some(value) => seed.deserialize(ValueDeserializer::new(value)).map(Some),
            None => Ok(None),
        }
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.items.len())
    }
}

/// Map access over an object. A key with several values reads as a sequence.
struct MapAccess {
    entries: indexmap::map::IntoIter<String, Entry>,
    value: Option<UclValue>,
}

impl MapAccess {
    fn new(object: UclObject) -> Self {
        Self {
            entries: object.into_iter(),
            value: None,
        }
    }
}

impl<'de> de::MapAccess<'de> for MapAccess {
    type Error = UclError;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, UclError>
    where
        K: DeserializeSeed<'de>,
    {
        match self.entries.next() {
            Some((key, entry)) => {
                self.value = Some(entry.into_value());
                seed.deserialize(ValueDeserializer::new(UclValue::String(key)))
                    .map(Some)
            }
            None => Ok(None),
        }
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, UclError>
    where
        V: DeserializeSeed<'de>,
    {
        match self.value.take() {
            Some(value) => seed.deserialize(ValueDeserializer::new(value)),
            None => Err(de::Error::custom("map value requested before its key")),
        }
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.entries.len())
    }
}

/// Enum access: the variant name, and its data for a non-unit variant.
struct EnumAccess {
    name: String,
    data: Option<UclValue>,
}

impl EnumAccess {
    fn unit(name: String) -> Self {
        Self { name, data: None }
    }

    fn data(name: String, data: UclValue) -> Self {
        Self {
            name,
            data: Some(data),
        }
    }
}

impl<'de> de::EnumAccess<'de> for EnumAccess {
    type Error = UclError;
    type Variant = VariantAccess;

    fn variant_seed<V>(self, seed: V) -> Result<(V::Value, VariantAccess), UclError>
    where
        V: DeserializeSeed<'de>,
    {
        let variant = seed.deserialize(ValueDeserializer::new(UclValue::String(self.name)))?;
        Ok((variant, VariantAccess { data: self.data }))
    }
}

struct VariantAccess {
    data: Option<UclValue>,
}

impl<'de> de::VariantAccess<'de> for VariantAccess {
    type Error = UclError;

    fn unit_variant(self) -> Result<(), UclError> {
        match self.data {
            None => Ok(()),
            Some(value) => Err(de::Error::invalid_type(unexpected(&value), &"unit variant")),
        }
    }

    fn newtype_variant_seed<T>(self, seed: T) -> Result<T::Value, UclError>
    where
        T: DeserializeSeed<'de>,
    {
        match self.data {
            Some(value) => seed.deserialize(ValueDeserializer::new(value)),
            None => Err(de::Error::invalid_type(
                Unexpected::UnitVariant,
                &"newtype variant",
            )),
        }
    }

    fn tuple_variant<V>(self, _len: usize, visitor: V) -> Result<V::Value, UclError>
    where
        V: Visitor<'de>,
    {
        match self.data {
            Some(UclValue::Array(array)) => visitor.visit_seq(SeqAccess::new(array)),
            // A single value is a tuple with one element
            Some(value) => visitor.visit_seq(SeqAccess::new(vec![value])),
            None => Err(de::Error::invalid_type(
                Unexpected::UnitVariant,
                &"tuple variant",
            )),
        }
    }

    fn struct_variant<V>(
        self,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, UclError>
    where
        V: Visitor<'de>,
    {
        match self.data {
            Some(UclValue::Object(object)) => visitor.visit_map(MapAccess::new(object)),
            Some(value) => Err(de::Error::invalid_type(
                unexpected(&value),
                &"struct variant",
            )),
            None => Err(de::Error::invalid_type(
                Unexpected::UnitVariant,
                &"struct variant",
            )),
        }
    }
}

/// Convenience function to deserialize UCL text into a Rust type
pub fn from_str<'a, T>(s: &'a str) -> Result<T, UclError>
where
    T: Deserialize<'a>,
{
    T::deserialize(UclDeserializer::new(s))
}

/// Convenience function to deserialize UCL text with variable expansion
pub fn from_str_with_variables<'a, T>(
    s: &'a str,
    handler: Box<dyn VariableHandler>,
) -> Result<T, UclError>
where
    T: Deserialize<'a>,
{
    T::deserialize(UclDeserializer::with_variable_handler(s, handler))
}

/// Convenience function to deserialize UCL text with custom lexer configuration
pub fn from_str_with_config<'a, T>(s: &'a str, config: LexerConfig) -> Result<T, UclError>
where
    T: Deserialize<'a>,
{
    T::deserialize(UclDeserializer::with_lexer_config(s, config))
}

/// Convenience function to deserialize UCL text with both custom config and variables
pub fn from_str_with_config_and_variables<'a, T>(
    s: &'a str,
    config: LexerConfig,
    handler: Box<dyn VariableHandler>,
) -> Result<T, UclError>
where
    T: Deserialize<'a>,
{
    let parser = UclParserBuilder::new(s)
        .with_lexer_config(config)
        .with_variable_handler(handler)
        .build()?;
    T::deserialize(UclDeserializer::from_parser(parser))
}

/// Convenience function to deserialize UCL text using environment variables
pub fn from_str_with_env<'a, T>(s: &'a str) -> Result<T, UclError>
where
    T: Deserialize<'a>,
{
    from_str_with_variables(s, Box::new(EnvironmentVariableHandler))
}

/// Convenience function to deserialize UCL text using a map of variables
pub fn from_str_with_map<'a, T>(
    s: &'a str,
    variables: std::collections::HashMap<String, String>,
) -> Result<T, UclError>
where
    T: Deserialize<'a>,
{
    from_str_with_variables(s, Box::new(MapVariableHandler::from_map(variables)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ParseError;
    use serde::Deserialize;
    use std::collections::HashMap;

    /// A document-level deserializer over a prepared value (no parsing).
    fn with_value(value: UclValue) -> UclDeserializer<'static> {
        UclDeserializer {
            parser: UclParser::new(""),
            value: Some(value),
        }
    }

    #[test]
    fn test_lex_errors_surface_as_lex_with_their_position() {
        // Lex errors used to reach callers as `SerdeError::Custom` strings, or as
        // `ParseError::InvalidObject` with the position reset to 1:1 (REVIEW.md §5.4).
        let err = from_str::<serde_json::Value>("\nkey = \"unterminated").unwrap_err();
        match err {
            UclError::Lex(crate::error::LexError::UnterminatedString { position }) => {
                // The opening quote
                assert_eq!((position.line, position.column), (2, 7));
            }
            other => panic!("expected UnterminatedString, got {other:?}"),
        }

        let err = from_str::<serde_json::Value>("a = 1\nb = 0xZZ").unwrap_err();
        match err {
            UclError::Lex(crate::error::LexError::InvalidNumber { position, .. }) => {
                assert_eq!(position.line, 2);
            }
            other => panic!("expected InvalidNumber, got {other:?}"),
        }

        let err = from_str::<serde_json::Value>("key = @@@").unwrap_err();
        match err {
            UclError::Lex(crate::error::LexError::UnexpectedCharacter {
                character,
                position,
            }) => {
                assert_eq!(character, '@');
                assert_eq!((position.line, position.column), (1, 7));
            }
            other => panic!("expected UnexpectedCharacter, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_errors_stay_typed() {
        let err = from_str::<serde_json::Value>("a = [1, 2").unwrap_err();
        match err {
            UclError::Parse(ParseError::UnexpectedToken { position, .. }) => {
                assert_eq!((position.line, position.column), (1, 10));
            }
            other => panic!("expected ParseError::UnexpectedToken, got {other:?}"),
        }
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct UnsignedField {
        v: u64,
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct SignedField {
        v: i64,
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct ByteField {
        v: u8,
    }

    #[test]
    fn test_float_to_integer_only_when_integral_and_in_range() {
        // Integral floats and times convert exactly.
        assert_eq!(from_str::<UnsignedField>("v = 30.0").unwrap().v, 30);
        assert_eq!(from_str::<UnsignedField>("v = 30s").unwrap().v, 30);
        assert_eq!(from_str::<SignedField>("v = -30.0").unwrap().v, -30);
        assert_eq!(from_str::<SignedField>("v = 30").unwrap().v, 30);

        // Anything else is an error instead of a truncated or saturated value (REVIEW.md §5.5).
        for input in [
            "v = 30.7",
            "v = 1e300",
            "v = -1.0",
            "v = nan",
            "v = inf",
            "v = -1",
        ] {
            assert!(
                from_str::<UnsignedField>(input).is_err(),
                "u64 from {input:?}"
            );
        }
        for input in ["v = 30.7", "v = 1e300", "v = -1e300", "v = nan"] {
            assert!(
                from_str::<SignedField>(input).is_err(),
                "i64 from {input:?}"
            );
        }
        // Narrower targets are range-checked after the conversion.
        assert!(from_str::<ByteField>("v = 300.0").is_err());
        assert_eq!(from_str::<ByteField>("v = 255.0").unwrap().v, 255);
    }

    #[test]
    fn test_document_and_nested_values_follow_the_same_rules() {
        // The document-level deserializer hands its value to the value deserializer, so a
        // top-level value behaves exactly like a nested one.
        assert_eq!(
            u64::deserialize(with_value(UclValue::Float(30.0))).unwrap(),
            30
        );
        assert_eq!(
            i64::deserialize(with_value(UclValue::Time(-2.0))).unwrap(),
            -2
        );
        assert!(u64::deserialize(with_value(UclValue::Float(30.7))).is_err());
        assert!(u64::deserialize(with_value(UclValue::Float(1e300))).is_err());
        assert!(i64::deserialize(with_value(UclValue::Float(f64::NAN))).is_err());
        assert_eq!(
            Option::<u32>::deserialize(with_value(UclValue::Integer(7))).unwrap(),
            Some(7)
        );
        assert_eq!(
            Option::<u32>::deserialize(with_value(UclValue::Null)).unwrap(),
            None
        );
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct Port(u16);

    #[derive(Debug, Deserialize, PartialEq)]
    #[serde(rename_all = "lowercase")]
    enum Level {
        Debug,
        Info,
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct Fields {
        maybe: Option<u32>,
        #[serde(default)]
        absent: Option<u32>,
        nothing: Option<u32>,
        level: Level,
        port: Port,
    }

    #[test]
    fn test_option_enum_and_newtype_fields() {
        // Each of these failed for struct fields before P2.2: the value deserializer sent
        // `option`, `enum` and `newtype_struct` to `deserialize_any`.
        let parsed: Fields =
            from_str("maybe = 30; nothing = null; level = \"info\"; port = 8080").unwrap();
        assert_eq!(
            parsed,
            Fields {
                maybe: Some(30),
                absent: None,
                nothing: None,
                level: Level::Info,
                port: Port(8080),
            }
        );
        assert!(from_str::<Fields>("maybe = 1; nothing = null; level = trace; port = 1").is_err());
        let _ = Level::Debug;
    }

    #[test]
    fn test_time_reads_as_seconds() {
        #[derive(Deserialize)]
        struct Timeouts {
            connect: f64,
            read: f32,
        }
        let parsed: Timeouts = from_str("connect = 1.5min; read = 250ms").unwrap();
        assert_eq!(parsed.connect, 90.0);
        assert_eq!(parsed.read, 0.25);
    }

    #[test]
    fn test_repeated_key_reads_as_sequence() {
        #[derive(Deserialize)]
        struct Servers {
            server: Vec<String>,
        }
        let parsed: Servers = from_str("server = a; server = b; server = c").unwrap();
        assert_eq!(parsed.server, vec!["a", "b", "c"]);

        // A value repeated after an explicit array is a second value, not an extension.
        let value: serde_json::Value = from_str("k = [1, 2]\nk = 3").unwrap();
        assert_eq!(value["k"], serde_json::json!([[1, 2], 3]));
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct Repos {
        #[serde(default)]
        repo: Vec<String>,
        mirror: Option<Vec<String>>,
    }

    #[test]
    fn test_one_or_many_sequences() {
        let parse = |input: &str| from_str::<Repos>(input).unwrap();
        let strings = |items: &[&str]| items.iter().map(|s| s.to_string()).collect::<Vec<_>>();

        // Zero occurrences
        assert_eq!(
            parse(""),
            Repos {
                repo: vec![],
                mirror: None
            }
        );
        // One occurrence: a one-element sequence
        assert_eq!(
            parse("repo = a\nmirror = m1"),
            Repos {
                repo: strings(&["a"]),
                mirror: Some(strings(&["m1"]))
            }
        );
        // Two occurrences (implicit array)
        assert_eq!(
            parse("repo = a\nrepo = b\nmirror = m1\nmirror = m2"),
            Repos {
                repo: strings(&["a", "b"]),
                mirror: Some(strings(&["m1", "m2"]))
            }
        );
        // An explicit array is unchanged
        assert_eq!(parse(r#"repo = ["a", "b"]"#).repo, strings(&["a", "b"]));
        // `null` into `Option<Vec<T>>` is `None`, not a one-element sequence
        assert_eq!(parse("mirror = null").mirror, None);
        // A tuple target takes a single value as a one-element tuple
        assert_eq!(
            <(u32,)>::deserialize(with_value(UclValue::Integer(5))).unwrap(),
            (5,)
        );
    }

    #[test]
    fn test_object_is_not_a_sequence() {
        // It used to read as the sequence of its values, silently dropping the keys.
        #[derive(Debug, Deserialize)]
        struct Sections {
            #[allow(dead_code)]
            section: Vec<u32>,
        }
        let err = from_str::<Sections>("section { a = 1; b = 2 }").unwrap_err();
        assert!(err.to_string().contains("invalid type: map"), "{err}");

        // A map target keeps the keys; repeated sections still read as a sequence of objects.
        #[derive(Debug, Deserialize)]
        struct Named {
            section: HashMap<String, u32>,
        }
        let named: Named = from_str("section { a = 1; b = 2 }").unwrap();
        assert_eq!(named.section.len(), 2);
        let repeated: Vec<HashMap<String, u32>> =
            <Vec<HashMap<String, u32>>>::deserialize(with_value(UclValue::Array(vec![
                UclValue::Object([("a", UclValue::Integer(1))].into_iter().collect()),
                UclValue::Object([("b", UclValue::Integer(2))].into_iter().collect()),
            ])))
            .unwrap();
        assert_eq!(repeated.len(), 2);
    }

    #[test]
    fn test_type_errors_name_what_was_found() {
        let err = from_str::<UnsignedField>("v = \"30\"").unwrap_err();
        assert!(
            err.to_string().contains("invalid type: string \"30\""),
            "{err}"
        );
        let err = from_str::<SignedField>("v = 30s").map(|_| ()).err();
        assert!(err.is_none(), "an integral time converts");
        #[derive(Debug, Deserialize)]
        struct Flag {
            #[allow(dead_code)]
            f: bool,
        }
        let err = from_str::<Flag>("f = 1").unwrap_err();
        assert!(
            err.to_string().contains("invalid type: integer `1`"),
            "{err}"
        );
    }

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

    #[derive(Debug, Deserialize, PartialEq)]
    struct EnumField {
        e: TestEnum,
    }

    #[test]
    fn test_basic_struct_deserialization() {
        let parsed: TestStruct = from_str(r#"{ name = "Alice", age = 30 }"#).unwrap();
        assert_eq!(
            parsed,
            TestStruct {
                name: "Alice".into(),
                age: 30
            }
        );
    }

    #[test]
    fn test_nested_struct_deserialization() {
        let ucl = r#"{
            user = { name = "Bob", age = 25 },
            active = true
        }"#;
        let parsed: NestedStruct = from_str(ucl).unwrap();
        assert_eq!(parsed.user.name, "Bob");
        assert_eq!(parsed.user.age, 25);
        assert!(parsed.active);
    }

    #[test]
    fn test_array_deserialization() {
        let parsed: Vec<i32> = from_str("[1, 2, 3, 4, 5]").unwrap();
        assert_eq!(parsed, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_map_deserialization() {
        let parsed: HashMap<String, String> =
            from_str(r#"{ key1 = "value1", key2 = "value2" }"#).unwrap();
        assert_eq!(parsed.get("key1"), Some(&"value1".to_string()));
        assert_eq!(parsed.get("key2"), Some(&"value2".to_string()));
    }

    #[test]
    fn test_optional_fields() {
        let parsed: OptionalFields = from_str(r#"{ required = "test" }"#).unwrap();
        assert_eq!(parsed.required, "test");
        assert_eq!(parsed.optional, None);
        assert_eq!(parsed.number, 42);

        let parsed: OptionalFields = from_str(r#"{ required = "a", optional = "b" }"#).unwrap();
        assert_eq!(parsed.optional.as_deref(), Some("b"));
    }

    #[test]
    fn test_renamed_fields() {
        let parsed: RenamedFields =
            from_str(r#"{ first_name = "John", last_name = "Doe" }"#).unwrap();
        assert_eq!(parsed.first_name, "John");
        assert_eq!(parsed.last_name, "Doe");
    }

    #[test]
    fn test_enum_variants() {
        let parse = |input: &str| from_str::<EnumField>(input).map(|f| f.e);
        assert_eq!(parse(r#"e = "Unit""#).unwrap(), TestEnum::Unit);
        assert_eq!(
            parse(r#"e = { Newtype = "test_value" }"#).unwrap(),
            TestEnum::Newtype("test_value".into())
        );
        assert_eq!(
            parse(r#"e = { Tuple = ["a", 1] }"#).unwrap(),
            TestEnum::Tuple("a".into(), 1)
        );
        assert_eq!(
            parse(r#"e = { Struct = { field = "test" } }"#).unwrap(),
            TestEnum::Struct {
                field: "test".into()
            }
        );
        // The document itself can be a data variant
        assert_eq!(
            from_str::<TestEnum>(r#"{ Newtype = "x" }"#).unwrap(),
            TestEnum::Newtype("x".into())
        );
        assert!(parse(r#"e = { Unit = 1, Newtype = "x" }"#).is_err());
        assert!(parse("e = 1").is_err());
    }

    #[test]
    fn test_variable_expansion() {
        let mut variables = HashMap::new();
        variables.insert("name".to_string(), "World".to_string());

        let parsed: HashMap<String, String> =
            from_str_with_map(r#"{ greeting = "Hello ${name}!" }"#, variables).unwrap();
        assert_eq!(parsed.get("greeting"), Some(&"Hello World!".to_string()));
    }

    #[test]
    fn test_environment_variables() {
        unsafe {
            std::env::set_var("TEST_UCL_VAR", "test_value");
        }
        let result: Result<HashMap<String, String>, _> =
            from_str_with_env(r#"{ env_value = "${TEST_UCL_VAR}" }"#);
        unsafe {
            std::env::remove_var("TEST_UCL_VAR");
        }
        assert_eq!(
            result.unwrap().get("env_value"),
            Some(&"test_value".to_string())
        );
    }

    #[test]
    fn test_type_coercion() {
        #[derive(Debug, Deserialize)]
        struct TypeCoercion {
            as_i32: i32,
            as_f64: f64,
            as_string: String,
            as_bool: bool,
        }
        let ucl = r#"{
            as_i32 = 42,
            as_f64 = 42,
            as_string = "42",
            as_bool = true
        }"#;
        let parsed: TypeCoercion = from_str(ucl).unwrap();
        assert_eq!(parsed.as_i32, 42);
        assert_eq!(parsed.as_f64, 42.0);
        assert_eq!(parsed.as_string, "42");
        assert!(parsed.as_bool);
    }

    #[test]
    fn test_error_propagation() {
        let result: Result<TestStruct, _> = from_str(r#"{ invalid syntax }"#);
        let error = result.unwrap_err();
        assert!(!error.to_string().is_empty());
    }

    #[test]
    fn test_convenience_functions() {
        let ucl = r#"{ name = "test", age = 25 }"#;
        let expected = TestStruct {
            name: "test".into(),
            age: 25,
        };
        assert_eq!(from_str::<TestStruct>(ucl).unwrap(), expected);
        assert_eq!(
            from_str_with_config::<TestStruct>(ucl, LexerConfig::default()).unwrap(),
            expected
        );
        assert_eq!(
            from_str_with_map::<TestStruct>(ucl, HashMap::new()).unwrap(),
            expected
        );
        assert_eq!(from_str_with_env::<TestStruct>(ucl).unwrap(), expected);
    }

    #[test]
    fn test_config_and_variables_honours_the_lexer_config() {
        // The `LexerConfig` argument used to be ignored.
        let vars = || {
            let mut vars = MapVariableHandler::new();
            vars.insert("N".into(), "x".into());
            Box::new(vars)
        };
        let config = LexerConfig {
            allow_time_suffixes: false,
            ..LexerConfig::default()
        };
        let with_default: Result<serde_json::Value, _> = from_str_with_config_and_variables(
            "t = 10s\nv = \"${N}\"",
            LexerConfig::default(),
            vars(),
        );
        let with_config: Result<serde_json::Value, _> =
            from_str_with_config_and_variables("t = 10s\nv = \"${N}\"", config, vars());
        let with_default = with_default.unwrap();
        assert_eq!(with_default["t"], 10.0);
        assert_eq!(with_default["v"], "x");
        assert_ne!(
            with_config.ok().map(|v| v["t"].clone()),
            Some(serde_json::json!(10.0))
        );
    }

    #[test]
    fn test_deserializer_methods() {
        let mut deserializer = UclDeserializer::new("test");
        let _parser_ref = deserializer.parser();
        let _parser_mut = deserializer.parser_mut();
    }
}
