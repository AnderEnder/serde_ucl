//! Deserializing a [`UclValue`] into Rust values: [`from_value`], the value deserializer behind
//! every serde entry point of the crate, and `Deserialize` for [`UclValue`] and [`UclObject`].
//! Nothing here depends on a parser. The mapping is described in the documentation of
//! [`crate::de`].

use crate::error::UclError;
use crate::ser::marker;
use crate::value::{Entry, UclArray, UclObject, UclValue};
use serde::de::{self, Deserialize, DeserializeOwned, DeserializeSeed, Unexpected, Visitor};
use std::fmt;

/// Deserializes a `T` from a UCL value, such as one returned by [`crate::parse::Parser::parse`].
///
/// ```
/// use serde::Deserialize;
///
/// #[derive(Debug, Deserialize, PartialEq)]
/// struct Server {
///     host: String,
///     ports: Vec<u16>,
/// }
///
/// let value = ucl_lexer::parse::parse(b"host = example.org\nports = [80, 443]").unwrap();
/// let server: Server = ucl_lexer::from_value(value).unwrap();
/// assert_eq!(server, Server { host: "example.org".into(), ports: vec![80, 443] });
/// ```
pub fn from_value<T: DeserializeOwned>(value: UclValue) -> Result<T, UclError> {
    T::deserialize(ValueDeserializer::new(value))
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
pub(crate) struct ValueDeserializer {
    value: UclValue,
}

impl ValueDeserializer {
    pub(crate) fn new(value: UclValue) -> Self {
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

    /// Bytes are a string's bytes, or an array of integers 0–255, which is how
    /// [`crate::ser`] writes them.
    fn deserialize_bytes<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, UclError> {
        match self.value {
            UclValue::String(s) => visitor.visit_string(s),
            UclValue::Array(array) => Self::visit_array(array, visitor),
            other => Err(invalid_type(&other, &visitor)),
        }
    }

    fn deserialize_byte_buf<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, UclError> {
        self.deserialize_bytes(visitor)
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

    /// A newtype struct reads as its content. `Deserialize for UclValue` asks for the private
    /// name [`marker::VALUE`], and gets a time as an enum variant named [`marker::TIME`], so that
    /// it stays a time.
    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        name: &'static str,
        visitor: V,
    ) -> Result<V::Value, UclError> {
        match self.value {
            UclValue::Time(t) if name == marker::VALUE => {
                visitor.visit_enum(MarkerAccess::new(marker::TIME, UclValue::Float(t)))
            }
            _ if name == marker::VALUE => self.deserialize_any(visitor),
            _ => visitor.visit_newtype_struct(self),
        }
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
pub(crate) struct SeqAccess {
    items: std::vec::IntoIter<UclValue>,
}

impl SeqAccess {
    pub(crate) fn new(array: UclArray) -> Self {
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

/// Map access over an object. A key with several values reads as a sequence, except for
/// `Deserialize for UclValue`, which gets every value (see [`EntryDeserializer`]).
struct MapAccess {
    entries: indexmap::map::IntoIter<String, Entry>,
    entry: Option<Entry>,
}

impl MapAccess {
    fn new(object: UclObject) -> Self {
        Self {
            entries: object.into_iter(),
            entry: None,
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
                self.entry = Some(entry);
                seed.deserialize(KeyDeserializer { key }).map(Some)
            }
            None => Ok(None),
        }
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, UclError>
    where
        V: DeserializeSeed<'de>,
    {
        match self.entry.take() {
            Some(entry) => seed.deserialize(EntryDeserializer { entry }),
            None => Err(<UclError as de::Error>::custom(
                "map value requested before its key",
            )),
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

/// Forwards each `Deserializer` method to a [`ValueDeserializer`] made by `$make`.
macro_rules! forward_to {
    ($make:ident; $($method:ident($($arg:ident: $ty:ty),*))*) => {
        $(
            fn $method<V>(self, $($arg: $ty,)* visitor: V) -> Result<V::Value, UclError>
            where
                V: Visitor<'de>,
            {
                self.$make().$method($($arg,)* visitor)
            }
        )*
    };
}

/// Deserializer for the value of an object's key: the entry's single value, or for an entry
/// with several values an explicit array of them. Only `Deserialize for UclValue`, which asks for
/// the private name [`marker::ENTRY`], gets the values one by one, as an enum variant named
/// [`marker::MULTI`].
struct EntryDeserializer {
    entry: Entry,
}

impl EntryDeserializer {
    fn value(self) -> ValueDeserializer {
        ValueDeserializer::new(self.entry.into_value())
    }
}

impl<'de> de::Deserializer<'de> for EntryDeserializer {
    type Error = UclError;

    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        name: &'static str,
        visitor: V,
    ) -> Result<V::Value, UclError> {
        if name == marker::ENTRY {
            let values = UclValue::Array(self.entry.into_values().collect());
            visitor.visit_enum(MarkerAccess::new(marker::MULTI, values))
        } else {
            self.value().deserialize_newtype_struct(name, visitor)
        }
    }

    forward_to! { value;
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

/// Deserializer for an object key. A key is a string, but a target that wants an integer, a
/// float, a `bool` or a `char` gets the key's text parsed as one, so that maps such as
/// `BTreeMap<u32, T>` read back what [`crate::ser`] writes for them.
struct KeyDeserializer {
    key: String,
}

impl KeyDeserializer {
    fn value(self) -> ValueDeserializer {
        ValueDeserializer::new(UclValue::String(self.key))
    }
}

/// Parses the key with `$ty` and visits the result with `$visit`.
macro_rules! parse_key {
    ($($method:ident => $ty:ty, $visit:ident;)*) => {
        $(
            fn $method<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, UclError> {
                match self.key.parse::<$ty>() {
                    Ok(v) => visitor.$visit(v),
                    Err(_) => Err(de::Error::invalid_value(Unexpected::Str(&self.key), &visitor)),
                }
            }
        )*
    };
}

impl<'de> de::Deserializer<'de> for KeyDeserializer {
    type Error = UclError;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, UclError> {
        visitor.visit_string(self.key)
    }

    parse_key! {
        deserialize_bool => bool, visit_bool;
        deserialize_i8 => i64, visit_i64;
        deserialize_i16 => i64, visit_i64;
        deserialize_i32 => i64, visit_i64;
        deserialize_i64 => i64, visit_i64;
        deserialize_i128 => i128, visit_i128;
        deserialize_u8 => u64, visit_u64;
        deserialize_u16 => u64, visit_u64;
        deserialize_u32 => u64, visit_u64;
        deserialize_u64 => u64, visit_u64;
        deserialize_u128 => u128, visit_u128;
        deserialize_f32 => f64, visit_f64;
        deserialize_f64 => f64, visit_f64;
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, UclError> {
        visitor.visit_some(self)
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, UclError> {
        visitor.visit_newtype_struct(self)
    }

    forward_to! { value;
        deserialize_char()
        deserialize_str()
        deserialize_string()
        deserialize_bytes()
        deserialize_byte_buf()
        deserialize_unit()
        deserialize_unit_struct(name: &'static str)
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

/// The enum variant by which the crate's deserializer hands `Deserialize for UclValue` what
/// serde's data model has no place for: a newtype variant named [`marker::TIME`] holding the
/// seconds, or one named [`marker::MULTI`] holding the values of an entry.
struct MarkerAccess {
    variant: &'static str,
    value: UclValue,
}

impl MarkerAccess {
    fn new(variant: &'static str, value: UclValue) -> Self {
        Self { variant, value }
    }
}

impl<'de> de::EnumAccess<'de> for MarkerAccess {
    type Error = UclError;
    type Variant = Self;

    fn variant_seed<V>(self, seed: V) -> Result<(V::Value, Self), UclError>
    where
        V: DeserializeSeed<'de>,
    {
        let name = de::value::StrDeserializer::<UclError>::new(self.variant);
        Ok((seed.deserialize(name)?, self))
    }
}

impl<'de> de::VariantAccess<'de> for MarkerAccess {
    type Error = UclError;

    fn unit_variant(self) -> Result<(), UclError> {
        Err(de::Error::invalid_type(
            Unexpected::NewtypeVariant,
            &"unit variant",
        ))
    }

    fn newtype_variant_seed<T>(self, seed: T) -> Result<T::Value, UclError>
    where
        T: DeserializeSeed<'de>,
    {
        seed.deserialize(ValueDeserializer::new(self.value))
    }

    fn tuple_variant<V: Visitor<'de>>(
        self,
        _len: usize,
        _visitor: V,
    ) -> Result<V::Value, UclError> {
        Err(de::Error::invalid_type(
            Unexpected::NewtypeVariant,
            &"tuple variant",
        ))
    }

    fn struct_variant<V: Visitor<'de>>(
        self,
        _fields: &'static [&'static str],
        _visitor: V,
    ) -> Result<V::Value, UclError> {
        Err(de::Error::invalid_type(
            Unexpected::NewtypeVariant,
            &"struct variant",
        ))
    }
}

impl<'de> Deserialize<'de> for UclValue {
    /// From this crate's deserializer ([`from_value`], and the text entry points), a time stays
    /// a time and every value of a multi-value entry is kept. From other deserializers, the
    /// value their format gives: numbers, strings, `bool`, unit and `None` as `Null`, sequences
    /// as arrays and maps as objects (a key that repeats becomes a multi-value entry).
    fn deserialize<D: de::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_newtype_struct(marker::VALUE, ValueVisitor)
    }
}

impl<'de> Deserialize<'de> for UclObject {
    /// A [`UclValue`] that must be an object.
    fn deserialize<D: de::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match UclValue::deserialize(deserializer)? {
            UclValue::Object(object) => Ok(object),
            other => Err(de::Error::invalid_type(unexpected(&other), &"a UCL object")),
        }
    }
}

/// Builds a [`UclValue`] from whatever a deserializer offers.
struct ValueVisitor;

impl ValueVisitor {
    /// The marker variants of [`MarkerAccess`]: a time, or the values of an entry.
    fn marker<'de, A: de::EnumAccess<'de>>(data: A) -> Result<Vec<UclValue>, A::Error> {
        use de::VariantAccess;
        let (name, variant): (String, _) = data.variant()?;
        if name == marker::TIME {
            Ok(vec![UclValue::Time(variant.newtype_variant()?)])
        } else if name == marker::MULTI {
            variant.newtype_variant()
        } else {
            Err(de::Error::custom(format!(
                "the enum variant `{name}`: a UCL value is an object, an array or a scalar"
            )))
        }
    }
}

impl<'de> Visitor<'de> for ValueVisitor {
    type Value = UclValue;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a UCL value")
    }

    fn visit_bool<E: de::Error>(self, v: bool) -> Result<UclValue, E> {
        Ok(UclValue::Boolean(v))
    }

    fn visit_i64<E: de::Error>(self, v: i64) -> Result<UclValue, E> {
        Ok(UclValue::Integer(v))
    }

    fn visit_i128<E: de::Error>(self, v: i128) -> Result<UclValue, E> {
        i64::try_from(v)
            .map(UclValue::Integer)
            .map_err(|_| E::invalid_value(Unexpected::Other("128-bit integer"), &self))
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> Result<UclValue, E> {
        i64::try_from(v)
            .map(UclValue::Integer)
            .map_err(|_| E::invalid_value(Unexpected::Unsigned(v), &"an integer up to i64::MAX"))
    }

    fn visit_u128<E: de::Error>(self, v: u128) -> Result<UclValue, E> {
        i64::try_from(v)
            .map(UclValue::Integer)
            .map_err(|_| E::invalid_value(Unexpected::Other("128-bit integer"), &self))
    }

    fn visit_f64<E: de::Error>(self, v: f64) -> Result<UclValue, E> {
        Ok(UclValue::Float(v))
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<UclValue, E> {
        Ok(UclValue::String(v.to_owned()))
    }

    fn visit_string<E: de::Error>(self, v: String) -> Result<UclValue, E> {
        Ok(UclValue::String(v))
    }

    fn visit_bytes<E: de::Error>(self, v: &[u8]) -> Result<UclValue, E> {
        match std::str::from_utf8(v) {
            Ok(s) => Ok(UclValue::String(s.to_owned())),
            Err(_) => Err(E::invalid_value(Unexpected::Bytes(v), &"UTF-8 text")),
        }
    }

    fn visit_unit<E: de::Error>(self) -> Result<UclValue, E> {
        Ok(UclValue::Null)
    }

    fn visit_none<E: de::Error>(self) -> Result<UclValue, E> {
        Ok(UclValue::Null)
    }

    fn visit_some<D: de::Deserializer<'de>>(self, deserializer: D) -> Result<UclValue, D::Error> {
        UclValue::deserialize(deserializer)
    }

    fn visit_newtype_struct<D: de::Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> Result<UclValue, D::Error> {
        deserializer.deserialize_any(self)
    }

    fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<UclValue, A::Error> {
        let mut items = Vec::with_capacity(seq.size_hint().unwrap_or(0).min(4096));
        while let Some(item) = seq.next_element()? {
            items.push(item);
        }
        Ok(UclValue::Array(items))
    }

    fn visit_map<A: de::MapAccess<'de>>(self, mut map: A) -> Result<UclValue, A::Error> {
        let mut object = UclObject::new();
        while let Some(key) = map.next_key::<String>()? {
            for value in map.next_value_seed(EntrySeed)? {
                object.append(key.as_str(), value);
            }
        }
        Ok(UclValue::Object(object))
    }

    fn visit_enum<A: de::EnumAccess<'de>>(self, data: A) -> Result<UclValue, A::Error> {
        let mut values = Self::marker(data)?;
        match values.len() {
            1 => Ok(values.remove(0)),
            _ => Err(de::Error::custom("several values where one is expected")),
        }
    }
}

/// The values of one object key, for [`ValueVisitor::visit_map`]: all values of the entry from
/// this crate's deserializer, the one value from others.
struct EntrySeed;

impl<'de> DeserializeSeed<'de> for EntrySeed {
    type Value = Vec<UclValue>;

    fn deserialize<D: de::Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_newtype_struct(marker::ENTRY, EntryVisitor)
    }
}

/// [`ValueVisitor`] for an entry: one value, or all values of a multi-value entry.
struct EntryVisitor;

/// Forwards visitor methods to [`ValueVisitor`], wrapping the value in a one-element `Vec`.
macro_rules! one_value {
    ($($method:ident($($arg:ident: $ty:ty),*);)*) => {
        $(
            fn $method<E: de::Error>(self, $($arg: $ty),*) -> Result<Vec<UclValue>, E> {
                ValueVisitor.$method($($arg),*).map(|v| vec![v])
            }
        )*
    };
}

impl<'de> Visitor<'de> for EntryVisitor {
    type Value = Vec<UclValue>;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a UCL value")
    }

    one_value! {
        visit_bool(v: bool);
        visit_i64(v: i64);
        visit_i128(v: i128);
        visit_u64(v: u64);
        visit_u128(v: u128);
        visit_f64(v: f64);
        visit_str(v: &str);
        visit_string(v: String);
        visit_bytes(v: &[u8]);
        visit_unit();
        visit_none();
    }

    fn visit_some<D: de::Deserializer<'de>>(self, d: D) -> Result<Vec<UclValue>, D::Error> {
        ValueVisitor.visit_some(d).map(|v| vec![v])
    }

    fn visit_newtype_struct<D: de::Deserializer<'de>>(
        self,
        d: D,
    ) -> Result<Vec<UclValue>, D::Error> {
        UclValue::deserialize(d).map(|v| vec![v])
    }

    fn visit_seq<A: de::SeqAccess<'de>>(self, seq: A) -> Result<Vec<UclValue>, A::Error> {
        ValueVisitor.visit_seq(seq).map(|v| vec![v])
    }

    fn visit_map<A: de::MapAccess<'de>>(self, map: A) -> Result<Vec<UclValue>, A::Error> {
        ValueVisitor.visit_map(map).map(|v| vec![v])
    }

    fn visit_enum<A: de::EnumAccess<'de>>(self, data: A) -> Result<Vec<UclValue>, A::Error> {
        ValueVisitor::marker(data)
    }
}
