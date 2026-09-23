//! The serializer that turns any `Serialize` value into a [`UclValue`] (see [`super::to_value`]).

use super::marker;
use crate::error::{SerdeError, UclError};
use crate::value::{UclObject, UclValue};
use serde::ser::{self, Impossible, Serialize};

fn unrepresentable(what: String) -> UclError {
    UclError::Serde(SerdeError::Unrepresentable(what))
}

fn custom(message: impl std::fmt::Display) -> UclError {
    <UclError as ser::Error>::custom(message)
}

/// What serializing one value gives: the value, or, for the private newtype struct
/// [`marker::MULTI`], every value of a multi-value entry.
pub(super) enum Node {
    One(UclValue),
    Many(Vec<UclValue>),
}

impl Node {
    /// The node as one value: the values of a multi-value entry outside an object become an
    /// explicit array.
    pub(super) fn into_value(self) -> UclValue {
        match self {
            Node::One(value) => value,
            Node::Many(values) => UclValue::Array(values),
        }
    }
}

/// Adds the value(s) of `node` to `key` of `object`. A key that is already present gets another
/// value, as a repeated key does when parsing (spec §8.2).
fn add(object: &mut UclObject, key: String, node: Node) -> Result<(), UclError> {
    match node {
        Node::One(value) => object.append(key, value),
        Node::Many(values) if values.is_empty() => {
            return Err(custom(format!(
                "the key {key:?} with no values: an entry holds at least one value"
            )));
        }
        Node::Many(values) => {
            for value in values {
                object.append(key.as_str(), value);
            }
        }
    }
    Ok(())
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
pub(super) struct ValueSerializer;

impl ser::Serializer for ValueSerializer {
    type Ok = Node;
    type Error = UclError;
    type SerializeSeq = SeqSerializer;
    type SerializeTuple = SeqSerializer;
    type SerializeTupleStruct = SeqSerializer;
    type SerializeTupleVariant = VariantSerializer<SeqSerializer>;
    type SerializeMap = MapSerializer;
    type SerializeStruct = MapSerializer;
    type SerializeStructVariant = VariantSerializer<MapSerializer>;

    fn serialize_bool(self, v: bool) -> Result<Node, UclError> {
        Ok(Node::One(UclValue::Boolean(v)))
    }

    fn serialize_i8(self, v: i8) -> Result<Node, UclError> {
        self.serialize_i64(v.into())
    }

    fn serialize_i16(self, v: i16) -> Result<Node, UclError> {
        self.serialize_i64(v.into())
    }

    fn serialize_i32(self, v: i32) -> Result<Node, UclError> {
        self.serialize_i64(v.into())
    }

    fn serialize_i64(self, v: i64) -> Result<Node, UclError> {
        Ok(Node::One(UclValue::Integer(v)))
    }

    fn serialize_i128(self, v: i128) -> Result<Node, UclError> {
        match i64::try_from(v) {
            Ok(v) => self.serialize_i64(v),
            Err(_) => Err(out_of_range(v)),
        }
    }

    fn serialize_u8(self, v: u8) -> Result<Node, UclError> {
        self.serialize_i64(v.into())
    }

    fn serialize_u16(self, v: u16) -> Result<Node, UclError> {
        self.serialize_i64(v.into())
    }

    fn serialize_u32(self, v: u32) -> Result<Node, UclError> {
        self.serialize_i64(v.into())
    }

    fn serialize_u64(self, v: u64) -> Result<Node, UclError> {
        match i64::try_from(v) {
            Ok(v) => self.serialize_i64(v),
            Err(_) => Err(out_of_range(v)),
        }
    }

    fn serialize_u128(self, v: u128) -> Result<Node, UclError> {
        match i64::try_from(v) {
            Ok(v) => self.serialize_i64(v),
            Err(_) => Err(out_of_range(v)),
        }
    }

    fn serialize_f32(self, v: f32) -> Result<Node, UclError> {
        self.serialize_f64(v.into())
    }

    fn serialize_f64(self, v: f64) -> Result<Node, UclError> {
        Ok(Node::One(UclValue::Float(v)))
    }

    fn serialize_char(self, v: char) -> Result<Node, UclError> {
        Ok(Node::One(UclValue::String(v.to_string())))
    }

    fn serialize_str(self, v: &str) -> Result<Node, UclError> {
        Ok(Node::One(UclValue::String(v.to_owned())))
    }

    fn serialize_bytes(self, v: &[u8]) -> Result<Node, UclError> {
        let bytes = v.iter().map(|&b| UclValue::Integer(b.into())).collect();
        Ok(Node::One(UclValue::Array(bytes)))
    }

    fn serialize_none(self) -> Result<Node, UclError> {
        self.serialize_unit()
    }

    fn serialize_some<T: ?Sized + Serialize>(self, value: &T) -> Result<Node, UclError> {
        value.serialize(self)
    }

    fn serialize_unit(self) -> Result<Node, UclError> {
        Ok(Node::One(UclValue::Null))
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<Node, UclError> {
        self.serialize_unit()
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _index: u32,
        variant: &'static str,
    ) -> Result<Node, UclError> {
        self.serialize_str(variant)
    }

    /// A newtype struct is its content, except for the private names of [`marker`]: a time, and
    /// the values of a multi-value entry.
    fn serialize_newtype_struct<T: ?Sized + Serialize>(
        self,
        name: &'static str,
        value: &T,
    ) -> Result<Node, UclError> {
        if name == marker::TIME {
            match value.serialize(self)? {
                Node::One(UclValue::Float(seconds)) => Ok(Node::One(UclValue::Time(seconds))),
                _ => Err(custom("a UCL time holds a number of seconds as an f64")),
            }
        } else if name == marker::MULTI {
            match value.serialize(self)? {
                Node::One(UclValue::Array(values)) => Ok(Node::Many(values)),
                _ => Err(custom("the values of a multi-value entry are a sequence")),
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
    ) -> Result<Node, UclError> {
        let mut object = UclObject::new();
        add(&mut object, variant.to_owned(), value.serialize(self)?)?;
        Ok(Node::One(UclValue::Object(object)))
    }

    fn serialize_seq(self, len: Option<usize>) -> Result<SeqSerializer, UclError> {
        Ok(SeqSerializer {
            items: Vec::with_capacity(len.unwrap_or(0).min(4096)),
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
        Ok(VariantSerializer {
            variant,
            inner: self.serialize_seq(Some(len))?,
        })
    }

    fn serialize_map(self, _len: Option<usize>) -> Result<MapSerializer, UclError> {
        Ok(MapSerializer {
            object: UclObject::new(),
            key: None,
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
        Ok(VariantSerializer {
            variant,
            inner: self.serialize_map(Some(len))?,
        })
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
}

impl SeqSerializer {
    fn push<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), UclError> {
        self.items
            .push(value.serialize(ValueSerializer)?.into_value());
        Ok(())
    }

    fn finish(self) -> UclValue {
        UclValue::Array(self.items)
    }
}

impl ser::SerializeSeq for SeqSerializer {
    type Ok = Node;
    type Error = UclError;

    fn serialize_element<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), UclError> {
        self.push(value)
    }

    fn end(self) -> Result<Node, UclError> {
        Ok(Node::One(self.finish()))
    }
}

impl ser::SerializeTuple for SeqSerializer {
    type Ok = Node;
    type Error = UclError;

    fn serialize_element<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), UclError> {
        self.push(value)
    }

    fn end(self) -> Result<Node, UclError> {
        Ok(Node::One(self.finish()))
    }
}

impl ser::SerializeTupleStruct for SeqSerializer {
    type Ok = Node;
    type Error = UclError;

    fn serialize_field<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), UclError> {
        self.push(value)
    }

    fn end(self) -> Result<Node, UclError> {
        Ok(Node::One(self.finish()))
    }
}

/// Entries of a map or fields of a struct.
pub(super) struct MapSerializer {
    object: UclObject,
    key: Option<String>,
}

impl MapSerializer {
    fn field<T: ?Sized + Serialize>(&mut self, key: &str, value: &T) -> Result<(), UclError> {
        add(
            &mut self.object,
            key.to_owned(),
            value.serialize(ValueSerializer)?,
        )
    }

    fn finish(self) -> UclValue {
        UclValue::Object(self.object)
    }
}

impl ser::SerializeMap for MapSerializer {
    type Ok = Node;
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
        add(&mut self.object, key, value.serialize(ValueSerializer)?)
    }

    fn end(self) -> Result<Node, UclError> {
        Ok(Node::One(self.finish()))
    }
}

impl ser::SerializeStruct for MapSerializer {
    type Ok = Node;
    type Error = UclError;

    fn serialize_field<T: ?Sized + Serialize>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), UclError> {
        self.field(key, value)
    }

    fn end(self) -> Result<Node, UclError> {
        Ok(Node::One(self.finish()))
    }
}

/// The content of a tuple or struct variant, which becomes the value of the variant's name.
pub(super) struct VariantSerializer<S> {
    variant: &'static str,
    inner: S,
}

impl<S> VariantSerializer<S> {
    fn wrap(variant: &'static str, value: UclValue) -> Node {
        let mut object = UclObject::new();
        object.append(variant, value);
        Node::One(UclValue::Object(object))
    }
}

impl ser::SerializeTupleVariant for VariantSerializer<SeqSerializer> {
    type Ok = Node;
    type Error = UclError;

    fn serialize_field<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), UclError> {
        self.inner.push(value)
    }

    fn end(self) -> Result<Node, UclError> {
        Ok(Self::wrap(self.variant, self.inner.finish()))
    }
}

impl ser::SerializeStructVariant for VariantSerializer<MapSerializer> {
    type Ok = Node;
    type Error = UclError;

    fn serialize_field<T: ?Sized + Serialize>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), UclError> {
        self.inner.field(key, value)
    }

    fn end(self) -> Result<Node, UclError> {
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
