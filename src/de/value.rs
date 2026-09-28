//! Deserializing a [`Value`] into Rust values: [`from_value`], the value deserializer behind
//! every serde entry point of the crate, and `Deserialize` for [`UclValue`] and [`UclObject`].
//! Nothing here depends on a parser. The mapping is described in the documentation of
//! [`crate::de`].
//!
//! Borrowing. The value may borrow its keys and strings from the text it was parsed from, which
//! the text entry points arrange (clean-room work item C13): a borrowed string is offered to a
//! visitor that asks for a `&str` with `visit_borrowed_str`, so that `&'de str` and
//! `Cow<'de, str>` targets take it without a copy. What each method offers:
//!
//! - A string value: `deserialize_str` offers a borrowed string with `visit_borrowed_str`; every
//!   other method, and every string that is not borrowed, is offered with `visit_string`, as
//!   before.
//! - A key: `deserialize_str`, `deserialize_identifier` and `deserialize_any` offer a borrowed
//!   key with `visit_borrowed_str`; a key that is not borrowed is offered as before
//!   (`visit_string` without paths, `visit_str` with them), and `deserialize_string` always
//!   offers `visit_string`.
//!
//! Depth. `Deserialize for UclValue` takes the whole value from the crate's deserializer in one
//! step ([`crate::handoff`]), so a `UclValue` is read at any depth with the same stack. Other
//! types are read level by level, by recursion through their `Deserialize` impls; for them the
//! deserializer counts the maps and sequences it enters and fails past
//! [`MAX_SERDE_NESTING`](crate::de::MAX_SERDE_NESTING).
//!
//! Errors. The deserializer comes in two variants, `ValueDeserializer<'_, false>` and
//! `ValueDeserializer<'_, true>`. The second builds the path of an error as it goes up from the
//! value it is about: each map and sequence access it passes through puts its step in front of
//! the error's path (`UclError::add_step`). For that it keeps each key until its value is read,
//! and lends keys to the target, which gets a copy when it asks for an owned string. The first
//! does none of this, and is what the text entry points use first: when it fails, they parse
//! the document again and run the second one ([`crate::de`]). [`crate::UclDeserializer`], which
//! cannot run its visitor twice, uses the second one; [`from_value`], which consumes its value,
//! the first.

use crate::de::enter;
use crate::error::{Step, UclError};
use crate::handoff;
use crate::ser::marker;
use crate::value::{Array, Entry, Object, Str, UclObject, UclValue, Value};
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
/// let value = serde_ucl::parse::parse(b"host = example.org\nports = [80, 443]").unwrap();
/// let server: Server = serde_ucl::from_value(value).unwrap();
/// assert_eq!(server, Server { host: "example.org".into(), ports: vec![80, 443] });
/// ```
///
/// An error that the target type reports is [`UclError::Deserialize`] without a path or a
/// position: finding them would cost time while deserialization succeeds, since the value is
/// consumed and cannot be read a second time. The text entry points ([`crate::from_str`] and the
/// others) and [`crate::UclDeserializer`] give both.
pub fn from_value<T: DeserializeOwned>(value: UclValue) -> Result<T, UclError> {
    T::deserialize(ValueDeserializer::<false>::new(value)).map_err(UclError::in_document)
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

fn unexpected<'u>(value: &'u Value<'_>) -> Unexpected<'u> {
    match value {
        Value::String(s) => Unexpected::Str(s),
        Value::Integer(i) => Unexpected::Signed(*i),
        Value::Float(f) => Unexpected::Float(*f),
        Value::Time(_) => Unexpected::Other("time value"),
        Value::Boolean(b) => Unexpected::Bool(*b),
        Value::Null => Unexpected::Unit,
        Value::Object(_) => Unexpected::Map,
        Value::Array(_) => Unexpected::Seq,
    }
}

fn invalid_type<'de, V: Visitor<'de>>(value: &Value<'_>, visitor: &V) -> UclError {
    de::Error::invalid_type(unexpected(value), visitor)
}

/// What gives `Deserialize for UclValue` the owned value it takes, from the value a deserializer
/// holds ([`crate::handoff`]): the value itself when it is owned, and a copy of what it borrows
/// when it borrows ([`Value::into_owned`]).
type Owner<'v> = fn(Value<'v>) -> UclValue;

/// Deserializer for one UCL value, which may borrow from text that lives for `'v`. With
/// `PATHS`, errors get the path of the value they are about.
pub(crate) struct ValueDeserializer<'v, const PATHS: bool> {
    value: Value<'v>,
    /// How many maps and sequences hold the value: 0 for the document.
    depth: usize,
    owner: Owner<'v>,
}

impl<const PATHS: bool> ValueDeserializer<'static, PATHS> {
    /// A deserializer for an owned value, which `Deserialize for UclValue` takes as it is.
    pub(crate) fn new(value: UclValue) -> Self {
        Self {
            value,
            depth: 0,
            owner: std::convert::identity,
        }
    }
}

impl<'v, const PATHS: bool> ValueDeserializer<'v, PATHS> {
    /// A deserializer for a value that may borrow its keys and strings; `Deserialize for
    /// UclValue` takes a copy of what it borrows.
    pub(crate) fn borrowed(value: Value<'v>) -> Self {
        Self {
            value,
            depth: 0,
            owner: Value::into_owned,
        }
    }

    fn nested(value: Value<'v>, depth: usize, owner: Owner<'v>) -> Self {
        Self {
            value,
            depth,
            owner,
        }
    }

    /// Visits `object`, a value inside `depth` maps and sequences, as a map.
    ///
    /// Past the limit, the object is dropped, which does not recurse ([`Value`]): it may be as
    /// deep as the parser allows, and a drop that recursed would add its depth to the stack of
    /// the recursion that stops here. The same holds for arrays.
    fn visit_object<'de, V: Visitor<'de>>(
        object: Object<'v>,
        depth: usize,
        visitor: V,
        (keys, owner): (Keys, Owner<'v>),
    ) -> Result<V::Value, UclError>
    where
        'v: 'de,
    {
        match enter(depth) {
            Ok(depth) => visitor.visit_map(MapAccess::<PATHS>::new(object, depth, keys, owner)),
            Err(e) => {
                crate::value::discard(Value::Object(object));
                Err(e)
            }
        }
    }

    /// Visits `array`, a value inside `depth` maps and sequences, as a sequence. With `single`,
    /// the array holds one value offered as a sequence of one, and an error of its element is the
    /// error of that value.
    fn visit_array<'de, V: Visitor<'de>>(
        array: Array<'v>,
        depth: usize,
        visitor: V,
        (single, owner): (bool, Owner<'v>),
    ) -> Result<V::Value, UclError>
    where
        'v: 'de,
    {
        match enter(depth) {
            Ok(depth) => visitor.visit_seq(SeqAccess::<PATHS>::new(array, depth, single, owner)),
            Err(e) => {
                crate::value::discard(Value::Array(array));
                Err(e)
            }
        }
    }
}

impl<'de, 'v: 'de, const PATHS: bool> de::Deserializer<'de> for ValueDeserializer<'v, PATHS> {
    type Error = UclError;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, UclError> {
        match self.value {
            Value::String(s) => visitor.visit_string(s.into_string()),
            Value::Integer(i) => visitor.visit_i64(i),
            Value::Float(f) | Value::Time(f) => visitor.visit_f64(f),
            Value::Boolean(b) => visitor.visit_bool(b),
            Value::Null => visitor.visit_unit(),
            Value::Object(obj) => {
                Self::visit_object(obj, self.depth, visitor, (Keys::Object, self.owner))
            }
            Value::Array(arr) => Self::visit_array(arr, self.depth, visitor, (false, self.owner)),
        }
    }

    fn deserialize_bool<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, UclError> {
        match self.value {
            Value::Boolean(b) => visitor.visit_bool(b),
            other => Err(invalid_type(&other, &visitor)),
        }
    }

    fn deserialize_i64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, UclError> {
        match self.value {
            Value::Integer(i) => visitor.visit_i64(i),
            Value::Float(f) | Value::Time(f) => match float_to_i64(f) {
                Some(i) => visitor.visit_i64(i),
                None => Err(de::Error::invalid_value(Unexpected::Float(f), &visitor)),
            },
            other => Err(invalid_type(&other, &visitor)),
        }
    }

    fn deserialize_u64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, UclError> {
        match self.value {
            // Negative values are rejected by the visitor's own range check.
            Value::Integer(i) => visitor.visit_i64(i),
            Value::Float(f) | Value::Time(f) => match float_to_u64(f) {
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
            Value::Float(f) | Value::Time(f) => visitor.visit_f64(f),
            Value::Integer(i) => visitor.visit_f64(i as f64),
            other => Err(invalid_type(&other, &visitor)),
        }
    }

    fn deserialize_char<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, UclError> {
        match self.value {
            Value::String(s) => {
                let mut chars = s.chars();
                match (chars.next(), chars.next()) {
                    (Some(c), None) => visitor.visit_char(c),
                    _ => Err(de::Error::invalid_value(Unexpected::Str(&s), &visitor)),
                }
            }
            other => Err(invalid_type(&other, &visitor)),
        }
    }

    /// A string borrowed from the text is offered with `visit_borrowed_str`, so that a target
    /// that borrows takes it without a copy; any other string as [`Self::deserialize_string`]
    /// offers it.
    fn deserialize_str<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, UclError> {
        match self.value {
            Value::String(s) => match s.as_borrowed() {
                Some(text) => visitor.visit_borrowed_str(text),
                None => visitor.visit_string(s.into_string()),
            },
            other => Err(invalid_type(&other, &visitor)),
        }
    }

    fn deserialize_string<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, UclError> {
        match self.value {
            Value::String(s) => visitor.visit_string(s.into_string()),
            other => Err(invalid_type(&other, &visitor)),
        }
    }

    /// Bytes are a string's bytes, or an array of integers 0–255, which is how
    /// [`crate::ser`] writes them.
    fn deserialize_bytes<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, UclError> {
        match self.value {
            Value::String(s) => visitor.visit_string(s.into_string()),
            Value::Array(array) => {
                Self::visit_array(array, self.depth, visitor, (false, self.owner))
            }
            other => Err(invalid_type(&other, &visitor)),
        }
    }

    fn deserialize_byte_buf<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, UclError> {
        self.deserialize_bytes(visitor)
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, UclError> {
        match self.value {
            Value::Null => visitor.visit_none(),
            _ => visitor.visit_some(self),
        }
    }

    fn deserialize_unit<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, UclError> {
        match self.value {
            Value::Null => visitor.visit_unit(),
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
    /// name [`marker::VALUE`], and gets the whole value, moved as it is, times, multi-value
    /// entries, priorities and marks included (see [`crate::handoff`]); a value that borrows
    /// from the text is copied into one that does not first.
    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        name: &'static str,
        visitor: V,
    ) -> Result<V::Value, UclError> {
        if name != marker::VALUE {
            return visitor.visit_newtype_struct(self);
        }
        handoff::put((self.owner)(self.value));
        let result = visitor.visit_enum(TreeAccess);
        // `ValueVisitor` has taken the value; any other visitor leaves it.
        drop(handoff::take());
        result
    }

    fn deserialize_seq<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, UclError> {
        match self.value {
            Value::Array(array) => {
                Self::visit_array(array, self.depth, visitor, (false, self.owner))
            }
            // An object is not a sequence: reading its values would drop the keys.
            Value::Object(_) => Err(de::Error::invalid_type(Unexpected::Map, &visitor)),
            // One-or-many: a key written once reads like a key written several times.
            single => {
                Self::visit_array(vec![single].into(), self.depth, visitor, (true, self.owner))
            }
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
            Value::Object(object) => {
                Self::visit_object(object, self.depth, visitor, (Keys::Object, self.owner))
            }
            // An array reads as a map from string indices
            Value::Array(array) => {
                let object = array
                    .into_iter()
                    .enumerate()
                    .map(|(i, v)| (i.to_string(), v))
                    .collect();
                Self::visit_object(object, self.depth, visitor, (Keys::Indices, self.owner))
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
            Value::String(s) => visitor.visit_enum(EnumAccess::<PATHS>::unit(s, self.owner)),
            // Data variant: an object with a single key, which counts as a map
            Value::Object(mut obj) if obj.len() == 1 => match enter(self.depth) {
                Ok(depth) => {
                    let (name, entry) = obj.remove_index(0).unwrap();
                    let access = EnumAccess::<PATHS>::data(name, entry, depth, self.owner);
                    visitor.visit_enum(access)
                }
                Err(e) => {
                    crate::value::discard(Value::Object(obj));
                    Err(e)
                }
            },
            Value::Object(obj) => Err(de::Error::invalid_length(
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
pub(crate) struct SeqAccess<'v, const PATHS: bool> {
    items: std::vec::IntoIter<Value<'v>>,
    /// The nesting of the sequence, which holds the elements.
    depth: usize,
    /// The number of elements, for the index of an element that fails; `None` for one value
    /// offered as a sequence of one, whose errors are those of the value.
    len: Option<usize>,
    owner: Owner<'v>,
}

impl<'v, const PATHS: bool> SeqAccess<'v, PATHS> {
    fn new(array: Array<'v>, depth: usize, single: bool, owner: Owner<'v>) -> Self {
        Self {
            len: (!single).then_some(array.len()),
            items: array.into_iter(),
            depth,
            owner,
        }
    }
}

impl<'de, 'v: 'de, const PATHS: bool> de::SeqAccess<'de> for SeqAccess<'v, PATHS> {
    type Error = UclError;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, UclError>
    where
        T: DeserializeSeed<'de>,
    {
        let Some(value) = self.items.next() else {
            return Ok(None);
        };
        let value = ValueDeserializer::<PATHS>::nested(value, self.depth, self.owner);
        if !PATHS {
            return seed.deserialize(value).map(Some);
        }
        let mut result = seed.deserialize(value);
        // The step goes in front of the error's path as the error goes up.
        if let (Err(e), Some(len)) = (&mut result, self.len) {
            e.add_step(Step::Index(len - self.items.len() - 1));
        }
        result.map(Some)
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.items.len())
    }
}

/// What the keys of a map the deserializer offers are, for the paths of errors.
#[derive(Debug, Clone, Copy)]
enum Keys {
    /// The keys of an object.
    Object,
    /// The indices of the elements of an array read as a map.
    Indices,
}

/// Map access over an object. A key with several values reads as a sequence of them.
struct MapAccess<'v, const PATHS: bool> {
    entries: crate::value::IntoIter<'v>,
    /// The entry whose key was read last, until its value is read.
    entry: Option<Entry<'v>>,
    /// The key read last, for the path of an error in its value (with `PATHS`).
    key: Str<'v>,
    /// The nesting of the map, which holds the values.
    depth: usize,
    keys: Keys,
    owner: Owner<'v>,
}

impl<'v, const PATHS: bool> MapAccess<'v, PATHS> {
    fn new(object: Object<'v>, depth: usize, keys: Keys, owner: Owner<'v>) -> Self {
        Self {
            entries: object.into_iter(),
            entry: None,
            key: Str::new(),
            depth,
            keys,
            owner,
        }
    }

    /// The step to the value of entry `key`, or with `on_key` to the key itself. `multi` says
    /// that the entry has several values.
    fn step<'k>(&self, key: &'k str, on_key: bool, multi: bool) -> Step<'k> {
        match self.keys {
            Keys::Object if on_key => Step::Key(key),
            Keys::Object if multi => Step::Values(key),
            Keys::Object => Step::Entry(key),
            // The keys are the indices written out.
            Keys::Indices => Step::Index(key.parse().unwrap_or_default()),
        }
    }
}

impl<'de, 'v: 'de, const PATHS: bool> de::MapAccess<'de> for MapAccess<'v, PATHS> {
    type Error = UclError;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, UclError>
    where
        K: DeserializeSeed<'de>,
    {
        let Some((key, entry)) = self.entries.next() else {
            return Ok(None);
        };
        self.entry = Some(entry);
        if !PATHS {
            let key = OwnedKey {
                key,
                owner: self.owner,
            };
            return seed.deserialize(key).map(Some);
        }
        let mut result = seed.deserialize(KeyDeserializer {
            key: &key,
            owner: self.owner,
        });
        if let Err(e) = &mut result {
            e.add_step(self.step(&key, true, false));
        }
        self.key = key;
        result.map(Some)
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, UclError>
    where
        V: DeserializeSeed<'de>,
    {
        let Some(entry) = self.entry.take() else {
            return Err(<UclError as de::Error>::custom(
                "map value requested before its key",
            ));
        };
        let multi = PATHS && entry.is_multi();
        let value = ValueDeserializer::<PATHS>::nested(entry.into_value(), self.depth, self.owner);
        if !PATHS {
            return seed.deserialize(value);
        }
        let mut result = seed.deserialize(value);
        if let Err(e) = &mut result {
            e.add_step(self.step(&self.key, false, multi));
        }
        result
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.entries.len())
    }
}

/// Enum access: the variant name, and its data for a non-unit variant: the value of the entry
/// `{ name = data }`, or its values if it has several.
struct EnumAccess<'v, const PATHS: bool> {
    name: Str<'v>,
    data: Option<Entry<'v>>,
    /// How many maps and sequences hold the data: the object `{ name = data }` among them.
    depth: usize,
    owner: Owner<'v>,
}

impl<'v, const PATHS: bool> EnumAccess<'v, PATHS> {
    fn unit(name: Str<'v>, owner: Owner<'v>) -> Self {
        Self {
            name,
            data: None,
            depth: 0,
            owner,
        }
    }

    fn data(name: Str<'v>, data: Entry<'v>, depth: usize, owner: Owner<'v>) -> Self {
        Self {
            name,
            data: Some(data),
            depth,
            owner,
        }
    }
}

impl<'de, 'v: 'de, const PATHS: bool> de::EnumAccess<'de> for EnumAccess<'v, PATHS> {
    type Error = UclError;
    type Variant = VariantAccess<'v, PATHS>;

    fn variant_seed<V>(self, seed: V) -> Result<(V::Value, VariantAccess<'v, PATHS>), UclError>
    where
        V: DeserializeSeed<'de>,
    {
        let multi = self.data.as_ref().is_some_and(Entry::is_multi);
        let data = self.data.map(Entry::into_value);
        let (depth, owner) = (self.depth, self.owner);
        let Some(data) = data else {
            // A unit variant: the name is a string value.
            let name = ValueDeserializer::<PATHS>::nested(Value::String(self.name), 0, owner);
            let access = VariantAccess {
                data: None,
                multi,
                depth,
                name: Str::new(),
                owner,
            };
            return Ok((seed.deserialize(name)?, access));
        };
        // The variant of `{ name = data }` is its key.
        let (variant, name) = if PATHS {
            let variant = seed
                .deserialize(KeyDeserializer {
                    key: &self.name,
                    owner,
                })
                .map_err(|e| e.at_key(&self.name))?;
            (variant, self.name)
        } else {
            let key = OwnedKey {
                key: self.name,
                owner,
            };
            (seed.deserialize(key)?, Str::new())
        };
        let access = VariantAccess {
            data: Some(data),
            multi,
            depth,
            name,
            owner,
        };
        Ok((variant, access))
    }
}

struct VariantAccess<'v, const PATHS: bool> {
    data: Option<Value<'v>>,
    /// The data is the values of `{ name = … }`, which has several.
    multi: bool,
    depth: usize,
    /// The key of `{ name = data }`, for the paths of the data's errors.
    name: Str<'v>,
    owner: Owner<'v>,
}

impl<const PATHS: bool> VariantAccess<'_, PATHS> {
    /// Puts the step to the data in front of the path of `result`'s error.
    fn locate<T>(&self, mut result: Result<T, UclError>) -> Result<T, UclError> {
        if PATHS && let Err(e) = &mut result {
            e.add_step(if self.multi {
                Step::Values(&self.name)
            } else {
                Step::Entry(&self.name)
            });
        }
        result
    }
}

impl<'de, 'v: 'de, const PATHS: bool> de::VariantAccess<'de> for VariantAccess<'v, PATHS> {
    type Error = UclError;

    fn unit_variant(self) -> Result<(), UclError> {
        match &self.data {
            None => Ok(()),
            Some(value) => {
                let e: UclError = de::Error::invalid_type(unexpected(value), &"unit variant");
                self.locate(Err(e))
            }
        }
    }

    fn newtype_variant_seed<T>(mut self, seed: T) -> Result<T::Value, UclError>
    where
        T: DeserializeSeed<'de>,
    {
        match self.data.take() {
            Some(value) => {
                let value = ValueDeserializer::<PATHS>::nested(value, self.depth, self.owner);
                let result = seed.deserialize(value);
                self.locate(result)
            }
            None => Err(de::Error::invalid_type(
                Unexpected::UnitVariant,
                &"newtype variant",
            )),
        }
    }

    fn tuple_variant<V>(mut self, _len: usize, visitor: V) -> Result<V::Value, UclError>
    where
        V: Visitor<'de>,
    {
        let result = match self.data.take() {
            Some(Value::Array(array)) => ValueDeserializer::<PATHS>::visit_array(
                array,
                self.depth,
                visitor,
                (false, self.owner),
            ),
            // A single value is a tuple with one element
            Some(value) => ValueDeserializer::<PATHS>::visit_array(
                vec![value].into(),
                self.depth,
                visitor,
                (true, self.owner),
            ),
            None => {
                return Err(de::Error::invalid_type(
                    Unexpected::UnitVariant,
                    &"tuple variant",
                ));
            }
        };
        self.locate(result)
    }

    fn struct_variant<V>(
        mut self,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, UclError>
    where
        V: Visitor<'de>,
    {
        let result = match self.data.take() {
            Some(Value::Object(object)) => ValueDeserializer::<PATHS>::visit_object(
                object,
                self.depth,
                visitor,
                (Keys::Object, self.owner),
            ),
            Some(value) => Err(de::Error::invalid_type(
                unexpected(&value),
                &"struct variant",
            )),
            None => {
                return Err(de::Error::invalid_type(
                    Unexpected::UnitVariant,
                    &"struct variant",
                ));
            }
        };
        self.locate(result)
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

/// Deserializer for an object key. A key is a string, but a target that wants an integer, a
/// float, a `bool` or a `char` gets the key's text parsed as one, so that maps such as
/// `BTreeMap<u32, T>` read back what [`crate::ser`] writes for them.
///
/// The key is lent, not given: the map access keeps it for the path of an error in its value.
/// A target that asks for an owned string gets a copy; one that asks for a `&str` gets the key
/// borrowed from the text when it is.
struct KeyDeserializer<'k, 'v> {
    key: &'k Str<'v>,
    owner: Owner<'v>,
}

impl<'v> KeyDeserializer<'_, 'v> {
    fn value(self) -> ValueDeserializer<'v, false> {
        ValueDeserializer::nested(Value::String(self.key.clone()), 0, self.owner)
    }

    /// The key for a target that asks for a `&str`: borrowed when it is borrowed from the text.
    fn visit_key<'de, V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, UclError>
    where
        'v: 'de,
    {
        match self.key.as_borrowed() {
            Some(text) => visitor.visit_borrowed_str(text),
            None => visitor.visit_str(self.key),
        }
    }
}

impl<'de, 'v: 'de> de::Deserializer<'de> for KeyDeserializer<'_, 'v> {
    type Error = UclError;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, UclError> {
        self.visit_key(visitor)
    }

    fn deserialize_str<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, UclError> {
        self.visit_key(visitor)
    }

    fn deserialize_string<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, UclError> {
        visitor.visit_string(self.key.to_string())
    }

    fn deserialize_identifier<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, UclError> {
        self.visit_key(visitor)
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
        deserialize_ignored_any()
    }
}

/// [`KeyDeserializer`] for a key given away: the map access without paths does not keep keys.
struct OwnedKey<'v> {
    key: Str<'v>,
    owner: Owner<'v>,
}

impl<'v> OwnedKey<'v> {
    fn value(self) -> ValueDeserializer<'v, false> {
        ValueDeserializer::nested(Value::String(self.key), 0, self.owner)
    }

    /// The key for a target that asks for a `&str` or an identifier: borrowed when it is
    /// borrowed from the text, and given away otherwise.
    fn visit_key<'de, V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, UclError>
    where
        'v: 'de,
    {
        match self.key.as_borrowed() {
            Some(text) => visitor.visit_borrowed_str(text),
            None => visitor.visit_string(self.key.into_string()),
        }
    }
}

impl<'de, 'v: 'de> de::Deserializer<'de> for OwnedKey<'v> {
    type Error = UclError;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, UclError> {
        self.visit_key(visitor)
    }

    fn deserialize_str<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, UclError> {
        self.visit_key(visitor)
    }

    fn deserialize_string<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, UclError> {
        visitor.visit_string(self.key.into_string())
    }

    fn deserialize_identifier<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, UclError> {
        self.visit_key(visitor)
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
        deserialize_ignored_any()
    }
}

/// The answer of the crate's deserializer to `Deserialize for UclValue`: the unit variant
/// [`marker::TREE`], which says that the value waits in [`crate::handoff`].
struct TreeAccess;

impl<'de> de::EnumAccess<'de> for TreeAccess {
    type Error = UclError;
    type Variant = Self;

    fn variant_seed<V>(self, seed: V) -> Result<(V::Value, Self), UclError>
    where
        V: DeserializeSeed<'de>,
    {
        let name = de::value::StrDeserializer::<UclError>::new(marker::TREE);
        Ok((seed.deserialize(name)?, self))
    }
}

impl<'de> de::VariantAccess<'de> for TreeAccess {
    type Error = UclError;

    fn unit_variant(self) -> Result<(), UclError> {
        Ok(())
    }

    fn newtype_variant_seed<T>(self, _seed: T) -> Result<T::Value, UclError>
    where
        T: DeserializeSeed<'de>,
    {
        Err(de::Error::invalid_type(
            Unexpected::UnitVariant,
            &"newtype variant",
        ))
    }

    fn tuple_variant<V: Visitor<'de>>(
        self,
        _len: usize,
        _visitor: V,
    ) -> Result<V::Value, UclError> {
        Err(de::Error::invalid_type(
            Unexpected::UnitVariant,
            &"tuple variant",
        ))
    }

    fn struct_variant<V: Visitor<'de>>(
        self,
        _fields: &'static [&'static str],
        _visitor: V,
    ) -> Result<V::Value, UclError> {
        Err(de::Error::invalid_type(
            Unexpected::UnitVariant,
            &"struct variant",
        ))
    }
}

impl<'de> Deserialize<'de> for UclValue {
    /// From this crate's deserializer ([`from_value`], and the text entry points), the value
    /// itself, moved in one step at any depth: times, multi-value entries, priorities and the
    /// marks of `.inherit` copies and `no-implicit-arrays` collections are kept. From other
    /// deserializers, the value their format gives: numbers, strings, `bool`, unit and `None` as
    /// `Null`, sequences as arrays and maps as objects (a key that repeats becomes a multi-value
    /// entry).
    fn deserialize<D: de::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_newtype_struct(marker::VALUE, ValueVisitor)
    }
}

impl<'de> Deserialize<'de> for UclObject {
    /// A [`UclValue`] that must be an object.
    fn deserialize<D: de::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match UclValue::deserialize(deserializer)? {
            Value::Object(object) => Ok(object),
            other => Err(de::Error::invalid_type(unexpected(&other), &"a UCL object")),
        }
    }
}

/// Builds a [`UclValue`] from whatever a deserializer offers.
struct ValueVisitor;

impl<'de> Visitor<'de> for ValueVisitor {
    type Value = UclValue;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a UCL value")
    }

    fn visit_bool<E: de::Error>(self, v: bool) -> Result<UclValue, E> {
        Ok(Value::Boolean(v))
    }

    fn visit_i64<E: de::Error>(self, v: i64) -> Result<UclValue, E> {
        Ok(Value::Integer(v))
    }

    fn visit_i128<E: de::Error>(self, v: i128) -> Result<UclValue, E> {
        i64::try_from(v)
            .map(Value::Integer)
            .map_err(|_| E::invalid_value(Unexpected::Other("128-bit integer"), &self))
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> Result<UclValue, E> {
        i64::try_from(v)
            .map(Value::Integer)
            .map_err(|_| E::invalid_value(Unexpected::Unsigned(v), &"an integer up to i64::MAX"))
    }

    fn visit_u128<E: de::Error>(self, v: u128) -> Result<UclValue, E> {
        i64::try_from(v)
            .map(Value::Integer)
            .map_err(|_| E::invalid_value(Unexpected::Other("128-bit integer"), &self))
    }

    fn visit_f64<E: de::Error>(self, v: f64) -> Result<UclValue, E> {
        Ok(Value::Float(v))
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<UclValue, E> {
        Ok(Value::String(v.into()))
    }

    fn visit_string<E: de::Error>(self, v: String) -> Result<UclValue, E> {
        Ok(Value::String(v.into()))
    }

    fn visit_bytes<E: de::Error>(self, v: &[u8]) -> Result<UclValue, E> {
        match std::str::from_utf8(v) {
            Ok(s) => Ok(Value::String(s.into())),
            Err(_) => Err(E::invalid_value(Unexpected::Bytes(v), &"UTF-8 text")),
        }
    }

    fn visit_unit<E: de::Error>(self) -> Result<UclValue, E> {
        Ok(Value::Null)
    }

    fn visit_none<E: de::Error>(self) -> Result<UclValue, E> {
        Ok(Value::Null)
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
        Ok(Value::Array(items.into()))
    }

    fn visit_map<A: de::MapAccess<'de>>(self, mut map: A) -> Result<UclValue, A::Error> {
        let mut object = UclObject::new();
        while let Some((key, value)) = map.next_entry::<String, UclValue>()? {
            object.append(key, value);
        }
        Ok(Value::Object(object))
    }

    /// The crate's deserializer answers with the variant [`marker::TREE`] and the value in
    /// [`crate::handoff`]. A UCL value is never an enum variant otherwise.
    fn visit_enum<A: de::EnumAccess<'de>>(self, data: A) -> Result<UclValue, A::Error> {
        use de::VariantAccess;
        let (name, variant): (String, _) = data.variant()?;
        if name == marker::TREE {
            variant.unit_variant()?;
            if let Some(value) = handoff::take() {
                return Ok(value);
            }
        }
        Err(de::Error::custom(format!(
            "the enum variant `{name}`: a UCL value is an object, an array or a scalar"
        )))
    }
}
