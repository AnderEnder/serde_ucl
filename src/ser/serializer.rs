//! The serializer that turns any `Serialize` value into a [`UclValue`] (see [`super::to_value`]).

use super::marker;
use crate::de::enter;
use crate::error::{SerdeError, UclError};
use crate::handoff;
use crate::value::{UclObject, UclValue};
use serde::ser::{self, Impossible, Serialize};

fn unrepresentable(what: String) -> UclError {
    UclError::Serde(SerdeError::Unrepresentable(what))
}

fn custom(message: impl std::fmt::Display) -> UclError {
    <UclError as ser::Error>::custom(message)
}

/// Serializes a Rust value into a [`UclValue`].
///
/// | serde | UCL |
/// | --- | --- |
/// | `bool`, `char`, `str` | boolean, one-character string, string |
/// | integers | integer; outside the `i64` range an error |
/// | `f32`, `f64` | float (`f32` widened exactly) |
/// | unit, unit struct, `None` | `null` |
/// | `Some(v)`, newtype struct | `v` |
/// | bytes | array of integers 0–255 |
/// | sequence, tuple, tuple struct | array |
/// | map, struct | object, fields and entries in order |
/// | unit variant | the variant's name |
/// | newtype, tuple and struct variant | object with one key, the variant's name |
///
/// A [`UclValue`] or [`UclObject`] is copied as it is, in one step at any depth (see
/// [`crate::handoff`]). Other values are serialized by recursion through their `Serialize`
/// impls, and the serializer enters at most [`crate::de::MAX_SERDE_NESTING`] maps and sequences
/// inside one another: the object of a newtype, tuple or struct variant counts, and so does the
/// array or object inside a tuple or struct variant.
pub(super) struct ValueSerializer {
    /// How many maps and sequences hold the value: 0 for the root.
    depth: usize,
}

impl ValueSerializer {
    /// The serializer of a root value.
    pub(super) fn new() -> Self {
        Self::nested(0)
    }

    fn nested(depth: usize) -> Self {
        Self { depth }
    }
}

impl ser::Serializer for ValueSerializer {
    type Ok = UclValue;
    type Error = UclError;
    type SerializeSeq = SeqSerializer;
    type SerializeTuple = SeqSerializer;
    type SerializeTupleStruct = SeqSerializer;
    type SerializeTupleVariant = VariantSerializer<SeqSerializer>;
    type SerializeMap = MapSerializer;
    type SerializeStruct = MapSerializer;
    type SerializeStructVariant = VariantSerializer<MapSerializer>;

    fn serialize_bool(self, v: bool) -> Result<UclValue, UclError> {
        Ok(UclValue::Boolean(v))
    }

    fn serialize_i8(self, v: i8) -> Result<UclValue, UclError> {
        self.serialize_i64(v.into())
    }

    fn serialize_i16(self, v: i16) -> Result<UclValue, UclError> {
        self.serialize_i64(v.into())
    }

    fn serialize_i32(self, v: i32) -> Result<UclValue, UclError> {
        self.serialize_i64(v.into())
    }

    fn serialize_i64(self, v: i64) -> Result<UclValue, UclError> {
        Ok(UclValue::Integer(v))
    }

    fn serialize_i128(self, v: i128) -> Result<UclValue, UclError> {
        match i64::try_from(v) {
            Ok(v) => self.serialize_i64(v),
            Err(_) => Err(out_of_range(v)),
        }
    }

    fn serialize_u8(self, v: u8) -> Result<UclValue, UclError> {
        self.serialize_i64(v.into())
    }

    fn serialize_u16(self, v: u16) -> Result<UclValue, UclError> {
        self.serialize_i64(v.into())
    }

    fn serialize_u32(self, v: u32) -> Result<UclValue, UclError> {
        self.serialize_i64(v.into())
    }

    fn serialize_u64(self, v: u64) -> Result<UclValue, UclError> {
        match i64::try_from(v) {
            Ok(v) => self.serialize_i64(v),
            Err(_) => Err(out_of_range(v)),
        }
    }

    fn serialize_u128(self, v: u128) -> Result<UclValue, UclError> {
        match i64::try_from(v) {
            Ok(v) => self.serialize_i64(v),
            Err(_) => Err(out_of_range(v)),
        }
    }

    fn serialize_f32(self, v: f32) -> Result<UclValue, UclError> {
        self.serialize_f64(v.into())
    }

    fn serialize_f64(self, v: f64) -> Result<UclValue, UclError> {
        Ok(UclValue::Float(v))
    }

    fn serialize_char(self, v: char) -> Result<UclValue, UclError> {
        Ok(UclValue::String(v.into()))
    }

    fn serialize_str(self, v: &str) -> Result<UclValue, UclError> {
        Ok(UclValue::String(v.into()))
    }

    fn serialize_bytes(self, v: &[u8]) -> Result<UclValue, UclError> {
        let bytes = v.iter().map(|&b| UclValue::Integer(b.into())).collect();
        Ok(UclValue::Array(bytes))
    }

    fn serialize_none(self) -> Result<UclValue, UclError> {
        self.serialize_unit()
    }

    fn serialize_some<T: ?Sized + Serialize>(self, value: &T) -> Result<UclValue, UclError> {
        value.serialize(self)
    }

    fn serialize_unit(self) -> Result<UclValue, UclError> {
        Ok(UclValue::Null)
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<UclValue, UclError> {
        self.serialize_unit()
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _index: u32,
        variant: &'static str,
    ) -> Result<UclValue, UclError> {
        self.serialize_str(variant)
    }

    /// A newtype struct is its content, except for the private names of [`marker`]: a time,
    /// and a whole [`UclValue`] or [`UclObject`], which is copied as it is.
    fn serialize_newtype_struct<T: ?Sized + Serialize>(
        self,
        name: &'static str,
        value: &T,
    ) -> Result<UclValue, UclError> {
        if name == marker::TIME {
            match value.serialize(self)? {
                UclValue::Float(seconds) => Ok(UclValue::Time(seconds)),
                _ => Err(custom("a UCL time holds a number of seconds as an f64")),
            }
        } else if name == marker::VALUE {
            let depth = self.depth;
            match handoff::request(|| value.serialize(ValueSerializer::nested(depth))) {
                (Ok(_), Some(tree)) => Ok(tree),
                (Ok(_), None) => Err(custom("a UCL value that did not hand itself over")),
                (Err(e), _) => Err(e),
            }
        } else {
            value.serialize(self)
        }
    }

    fn serialize_newtype_variant<T: ?Sized + Serialize>(
        self,
        _name: &'static str,
        _index: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<UclValue, UclError> {
        let depth = enter(self.depth)?;
        let mut object = UclObject::new();
        object.append(variant, value.serialize(ValueSerializer::nested(depth))?);
        Ok(UclValue::Object(object))
    }

    fn serialize_seq(self, len: Option<usize>) -> Result<SeqSerializer, UclError> {
        Ok(SeqSerializer {
            items: Vec::with_capacity(len.unwrap_or(0).min(4096)),
            depth: enter(self.depth)?,
        })
    }

    fn serialize_tuple(self, len: usize) -> Result<SeqSerializer, UclError> {
        self.serialize_seq(Some(len))
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        len: usize,
    ) -> Result<SeqSerializer, UclError> {
        self.serialize_seq(Some(len))
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<VariantSerializer<SeqSerializer>, UclError> {
        // The variant's object holds the array.
        let inner = ValueSerializer::nested(enter(self.depth)?).serialize_seq(Some(len))?;
        Ok(VariantSerializer { variant, inner })
    }

    fn serialize_map(self, _len: Option<usize>) -> Result<MapSerializer, UclError> {
        Ok(MapSerializer {
            object: UclObject::new(),
            key: None,
            depth: enter(self.depth)?,
        })
    }

    fn serialize_struct(self, _name: &'static str, len: usize) -> Result<MapSerializer, UclError> {
        self.serialize_map(Some(len))
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<VariantSerializer<MapSerializer>, UclError> {
        // The variant's object holds the struct's object.
        let inner = ValueSerializer::nested(enter(self.depth)?).serialize_map(Some(len))?;
        Ok(VariantSerializer { variant, inner })
    }
}

fn out_of_range(v: impl std::fmt::Display) -> UclError {
    unrepresentable(format!(
        "the integer {v}: UCL integers are 64-bit signed (spec §5.3)"
    ))
}

/// Elements of a sequence, tuple or tuple struct.
pub(super) struct SeqSerializer {
    items: Vec<UclValue>,
    /// The nesting of the sequence, which holds the elements.
    depth: usize,
}

impl SeqSerializer {
    fn push<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), UclError> {
        self.items
            .push(value.serialize(ValueSerializer::nested(self.depth))?);
        Ok(())
    }

    fn finish(self) -> UclValue {
        UclValue::Array(self.items.into())
    }
}

impl ser::SerializeSeq for SeqSerializer {
    type Ok = UclValue;
    type Error = UclError;

    fn serialize_element<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), UclError> {
        self.push(value)
    }

    fn end(self) -> Result<UclValue, UclError> {
        Ok(self.finish())
    }
}

impl ser::SerializeTuple for SeqSerializer {
    type Ok = UclValue;
    type Error = UclError;

    fn serialize_element<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), UclError> {
        self.push(value)
    }

    fn end(self) -> Result<UclValue, UclError> {
        Ok(self.finish())
    }
}

impl ser::SerializeTupleStruct for SeqSerializer {
    type Ok = UclValue;
    type Error = UclError;

    fn serialize_field<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), UclError> {
        self.push(value)
    }

    fn end(self) -> Result<UclValue, UclError> {
        Ok(self.finish())
    }
}

/// Entries of a map or fields of a struct. A key that is already present gets another value,
/// as a repeated key does when parsing (spec §8.2).
pub(super) struct MapSerializer {
    object: UclObject,
    key: Option<String>,
    /// The nesting of the map, which holds the values.
    depth: usize,
}

impl MapSerializer {
    fn field<T: ?Sized + Serialize>(&mut self, key: &str, value: &T) -> Result<(), UclError> {
        let value = value.serialize(ValueSerializer::nested(self.depth))?;
        self.object.append(key, value);
        Ok(())
    }

    fn finish(self) -> UclValue {
        UclValue::Object(self.object)
    }
}

impl ser::SerializeMap for MapSerializer {
    type Ok = UclValue;
    type Error = UclError;

    fn serialize_key<T: ?Sized + Serialize>(&mut self, key: &T) -> Result<(), UclError> {
        self.key = Some(key.serialize(KeySerializer)?);
        Ok(())
    }

    fn serialize_value<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), UclError> {
        let key = self
            .key
            .take()
            .ok_or_else(|| custom("a map value was serialized before its key"))?;
        self.field(&key, value)
    }

    fn end(self) -> Result<UclValue, UclError> {
        Ok(self.finish())
    }
}

impl ser::SerializeStruct for MapSerializer {
    type Ok = UclValue;
    type Error = UclError;

    fn serialize_field<T: ?Sized + Serialize>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), UclError> {
        self.field(key, value)
    }

    fn end(self) -> Result<UclValue, UclError> {
        Ok(self.finish())
    }
}

/// The content of a tuple or struct variant, which becomes the value of the variant's name.
pub(super) struct VariantSerializer<S> {
    variant: &'static str,
    inner: S,
}

impl<S> VariantSerializer<S> {
    fn wrap(variant: &'static str, value: UclValue) -> UclValue {
        let mut object = UclObject::new();
        object.append(variant, value);
        UclValue::Object(object)
    }
}

impl ser::SerializeTupleVariant for VariantSerializer<SeqSerializer> {
    type Ok = UclValue;
    type Error = UclError;

    fn serialize_field<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), UclError> {
        self.inner.push(value)
    }

    fn end(self) -> Result<UclValue, UclError> {
        Ok(Self::wrap(self.variant, self.inner.finish()))
    }
}

impl ser::SerializeStructVariant for VariantSerializer<MapSerializer> {
    type Ok = UclValue;
    type Error = UclError;

    fn serialize_field<T: ?Sized + Serialize>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), UclError> {
        self.inner.field(key, value)
    }

    fn end(self) -> Result<UclValue, UclError> {
        Ok(Self::wrap(self.variant, self.inner.finish()))
    }
}

/// Serializes a map key into the key's text. Strings and chars are themselves; integers and
/// booleans their decimal and `true`/`false` text, which the deserializer parses back; a unit
/// variant its name; a newtype struct its content. Other keys are an error.
struct KeySerializer;

fn key_error() -> UclError {
    unrepresentable(
        "a map key that is not a string, a char, an integer, a bool or a unit variant".to_owned(),
    )
}

impl ser::Serializer for KeySerializer {
    type Ok = String;
    type Error = UclError;
    type SerializeSeq = Impossible<String, UclError>;
    type SerializeTuple = Impossible<String, UclError>;
    type SerializeTupleStruct = Impossible<String, UclError>;
    type SerializeTupleVariant = Impossible<String, UclError>;
    type SerializeMap = Impossible<String, UclError>;
    type SerializeStruct = Impossible<String, UclError>;
    type SerializeStructVariant = Impossible<String, UclError>;

    fn serialize_bool(self, v: bool) -> Result<String, UclError> {
        Ok(v.to_string())
    }

    fn serialize_i8(self, v: i8) -> Result<String, UclError> {
        Ok(v.to_string())
    }

    fn serialize_i16(self, v: i16) -> Result<String, UclError> {
        Ok(v.to_string())
    }

    fn serialize_i32(self, v: i32) -> Result<String, UclError> {
        Ok(v.to_string())
    }

    fn serialize_i64(self, v: i64) -> Result<String, UclError> {
        Ok(v.to_string())
    }

    fn serialize_i128(self, v: i128) -> Result<String, UclError> {
        Ok(v.to_string())
    }

    fn serialize_u8(self, v: u8) -> Result<String, UclError> {
        Ok(v.to_string())
    }

    fn serialize_u16(self, v: u16) -> Result<String, UclError> {
        Ok(v.to_string())
    }

    fn serialize_u32(self, v: u32) -> Result<String, UclError> {
        Ok(v.to_string())
    }

    fn serialize_u64(self, v: u64) -> Result<String, UclError> {
        Ok(v.to_string())
    }

    fn serialize_u128(self, v: u128) -> Result<String, UclError> {
        Ok(v.to_string())
    }

    fn serialize_f32(self, _v: f32) -> Result<String, UclError> {
        Err(key_error())
    }

    fn serialize_f64(self, _v: f64) -> Result<String, UclError> {
        Err(key_error())
    }

    fn serialize_char(self, v: char) -> Result<String, UclError> {
        Ok(v.to_string())
    }

    fn serialize_str(self, v: &str) -> Result<String, UclError> {
        Ok(v.to_owned())
    }

    fn serialize_bytes(self, _v: &[u8]) -> Result<String, UclError> {
        Err(key_error())
    }

    fn serialize_none(self) -> Result<String, UclError> {
        Err(key_error())
    }

    fn serialize_some<T: ?Sized + Serialize>(self, _value: &T) -> Result<String, UclError> {
        Err(key_error())
    }

    fn serialize_unit(self) -> Result<String, UclError> {
        Err(key_error())
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<String, UclError> {
        Err(key_error())
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _index: u32,
        variant: &'static str,
    ) -> Result<String, UclError> {
        Ok(variant.to_owned())
    }

    fn serialize_newtype_struct<T: ?Sized + Serialize>(
        self,
        _name: &'static str,
        value: &T,
    ) -> Result<String, UclError> {
        value.serialize(self)
    }

    fn serialize_newtype_variant<T: ?Sized + Serialize>(
        self,
        _name: &'static str,
        _index: u32,
        _variant: &'static str,
        _value: &T,
    ) -> Result<String, UclError> {
        Err(key_error())
    }

    fn serialize_seq(self, _len: Option<usize>) -> Result<Self::SerializeSeq, UclError> {
        Err(key_error())
    }

    fn serialize_tuple(self, _len: usize) -> Result<Self::SerializeTuple, UclError> {
        Err(key_error())
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleStruct, UclError> {
        Err(key_error())
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleVariant, UclError> {
        Err(key_error())
    }

    fn serialize_map(self, _len: Option<usize>) -> Result<Self::SerializeMap, UclError> {
        Err(key_error())
    }

    fn serialize_struct(
        self,
        _name: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStruct, UclError> {
        Err(key_error())
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStructVariant, UclError> {
        Err(key_error())
    }
}
