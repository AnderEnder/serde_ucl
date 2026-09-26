//! Serde serialization (clean-room work item C4b; spec §10.8).
//!
//! Any `Serialize` value can be written as UCL text: [`to_string`] and [`to_writer`] in the
//! config format, [`to_json_string`], [`to_json_string_compact`] and [`to_yaml_string`] in the
//! other formats of spec §10. [`to_value`] gives the [`UclValue`] tree instead of text, and
//! [`crate::from_value`] turns a tree back into a Rust value. Errors are [`UclError`]; values that
//! have no form are [`SerdeError::Unrepresentable`].
//!
//! ```
//! use serde::{Deserialize, Serialize};
//! use std::time::Duration;
//!
//! #[derive(Debug, PartialEq, Serialize, Deserialize)]
//! struct Server {
//!     host: String,
//!     ports: Vec<u16>,
//!     ratio: f64,
//!     #[serde(with = "ucl_lexer::time")]
//!     timeout: Duration,
//! }
//!
//! let server = Server {
//!     host: "example.org".into(),
//!     ports: vec![80, 443],
//!     ratio: 0.1,
//!     timeout: Duration::from_millis(1500),
//! };
//! let text = ucl_lexer::to_string(&server).unwrap();
//! assert_eq!(
//!     text,
//!     "host = \"example.org\";\nports [\n    80,\n    443,\n]\nratio = 0.1;\ntimeout = 1.5s;\n"
//! );
//! let value = ucl_lexer::parse::parse(text.as_bytes()).unwrap();
//! assert_eq!(ucl_lexer::from_value::<Server>(value).unwrap(), server);
//! ```
//!
//! # Round trip
//!
//! The text has the layouts of libucl's output (spec §10.4–§10.6), but every value is written in
//! a form that libucl and [`crate::parse`] read back as exactly the value written (spec §10.8),
//! floats included, so deserializing the text gives back the same Rust value. The one exception
//! is a time in JSON, which is written as its number of seconds and reads back as a float (see
//! *JSON* below). The reader must use
//! the default flags and the `append` strategy (spec §8, §12): `key-lowercase` lowercases keys,
//! `no-time` reads times as strings, and `no-implicit-arrays` or another strategy changes
//! repeated keys. In JSON, compact JSON and YAML it must also not register variables that the
//! strings refer to (see *Variables* below).
//!
//! | Value | Written as |
//! | --- | --- |
//! | integer | decimal |
//! | float | the fewest digits that identify the double, with a `.` or an exponent: `0.1`, `1.0`, `-0.0`, `1e16`, `2.2250738585072014e-308`. NaN is `nan` (its sign and payload are not kept), +∞ `inf`, −∞ `-1e308k`; JSON has none of the three |
//! | time | the digits of a float followed by `s`: `1.5s`, `0.001s`; +∞ and −∞ are `1e308ks` and `-1e308ks`; a subnormal time, which only `ms` gives, is a normal float followed by `ms`: `1e-307ms`, `2.2250738585072014e-308ms` (spec §5.4, §10.8). In JSON, the digits of its seconds alone: `1.5`, `0.001` |
//! | string | double-quoted, with the escapes of spec §6.1 for `"`, `\`, control bytes and DEL; in the config format, a string that contains `$` is single-quoted, with `'` written `\'` |
//! | key | bare where spec §3.1 allows it (config and YAML), otherwise double-quoted with those escapes; always double-quoted in JSON |
//! | entry with several values ([`UclValue`] only) | one entry per value, all with the same key, in every format (libucl's JSON and YAML write an array instead, §10.7) |
//! | bool, null, empty object, empty array | `true`, `false`, `null`, `{}`, `[]` |
//!
//! Variables. A double-quoted string expands references to variables registered when it is read
//! (spec §7), and there is no escape that prevents it (§7.6). libucl and [`crate::parse`] define
//! `FILENAME` and `CURDIR` for every document by default (§7.8), and an application may register
//! more.
//!
//! - The config format writes every string that contains `$` in single quotes, which never
//!   expand (§6.2), so its output reads back exactly whatever variables the reader has.
//! - JSON, compact JSON and YAML write every string double-quoted, as JSON requires. A string
//!   that refers to `FILENAME` or `CURDIR` (`$FILENAME`, `$CURDIR`, `${FILENAME}`, `${CURDIR}`,
//!   also with more text after an unbraced name) would expand with the default settings, so it
//!   is an error there; the check is conservative and also rejects a reference that `$$` keeps
//!   as written (§7.5). A reference to a variable that the application registers when reading
//!   expands too: the output reads back exactly only with a reader that registers none of the
//!   variables its strings refer to.
//!
//! JSON. [`to_json_string`] and [`to_json_string_compact`] always write valid JSON (RFC 8259),
//! unlike libucl's own JSON output, which writes `nan` and `inf` (WORKLIST.md C4, decision 2):
//!
//! - a time is written as its number of seconds, a JSON number, and reads back as a float. A
//!   Rust value whose deserializer takes a number of seconds still round-trips: a `Duration`
//!   through [`crate::time`] does, while a time in a [`UclValue`] comes back as a float;
//! - a NaN or infinite float or time has no JSON number and is an error, as is a subnormal time,
//!   whose number of seconds no literal reads back as (spec §5.3). The config and YAML formats
//!   write all three.
//!
//! An entry with several values gives repeated member names, which JSON's grammar allows (RFC
//! 8259 advises unique names; readers such as `serde_json` keep the last value).
//!
//! # Errors
//!
//! [`SerdeError::Unrepresentable`] for a value that has no form that reads back as itself:
//!
//! - an integer outside the 64-bit signed range, which [`to_value`] rejects too (spec §5.3);
//! - a subnormal float: every literal below the normal range is an error (§5.3);
//! - a subnormal time closer to zero than `2.2250738585069563e-311`, the smallest that `ms` gives
//!   from a normal float (§10.8), and in JSON every subnormal time;
//! - a NaN time, which no text gives (§5.4);
//! - in JSON and compact JSON, a NaN or infinite float or time;
//! - the empty key, which parsing rejects (§3.2);
//! - a root that is not a map, a struct or a sequence: a UCL document is an object or an array
//!   (§1.1);
//! - more than 1024 containers nested inside one another, the root included (§11.2);
//! - in the config format, a string that contains `$` and a backslash before `'`, a line break
//!   or its end: double quotes would expand it and single quotes cannot hold it (§6.2, §10.8);
//! - in JSON, compact JSON and YAML, a string that refers to `FILENAME` or `CURDIR` (see
//!   *Variables*);
//! - a map key that is not a string, a `char`, an integer, a `bool` or a unit variant.
//!
//! [`crate::time::serialize`] reports a `Duration` that a 64-bit float of seconds cannot hold
//! exactly as [`SerdeError::Custom`].
//!
//! # Depth
//!
//! A [`UclValue`] or [`UclObject`], also as a field of another type, is copied as it is, with
//! the same stack at any depth: [`to_value`] of one gives an equal value, priorities and marks
//! included, and the text functions write any such value up to 1024 containers deep (spec
//! §11.2), which every value the parser returns is unless its `.inherit` depth limit is raised
//! ([`Parser::set_inherit_depth_limit`](crate::parse::Parser::set_inherit_depth_limit)). Deeper
//! text could not be parsed again. Any other type is serialized by recursion through its `Serialize` impl, one
//! level per map or sequence, and there serialization enters at most
//! [`MAX_SERDE_NESTING`](crate::MAX_SERDE_NESTING) maps and sequences inside one another, the
//! outermost included; deeper, it fails with [`SerdeError::TooDeep`]. The object of an enum
//! variant counts as a map, and the array or object inside a tuple or struct variant counts too.
//!
//! # Serde's own limits
//!
//! `Some(None)` and `Some(())` are written `null` and read back as `None`, as in every
//! self-describing format. Enums are externally tagged: a unit variant is its name, the other
//! variants an object whose one key is the name.

pub(crate) mod marker;
mod serializer;

use crate::emit::{Emitter, Format};
use crate::error::{SerdeError, UclError};
use crate::handoff;
use crate::value::{Entry, UclObject, UclValue};
use serde::ser::{Serialize, SerializeMap, Serializer};
use serializer::ValueSerializer;
use std::io;

/// Serializes `value` into a [`UclValue`] tree, in the mapping of the table below. The tree can
/// be of any kind; only the text functions require an object or an array at the root.
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
/// | map, struct | object, with entries and fields in order |
/// | unit variant | the variant's name |
/// | newtype, tuple and struct variant | object with one key, the variant's name |
/// | `Duration` with [`crate::time`] | time |
pub fn to_value<T: ?Sized + Serialize>(value: &T) -> Result<UclValue, UclError> {
    value.serialize(ValueSerializer::new())
}

fn to_text<T: ?Sized + Serialize>(value: &T, format: Format) -> Result<String, UclError> {
    let tree = to_value(value)?;
    let text = Emitter::new(format).round_trip().try_emit(&tree);
    // A tree built in code may be deeper than the text formats allow (the emitter rejects it),
    // and deeper than a recursive drop could take.
    crate::value::discard(tree);
    text.map_err(|what| UclError::Serde(SerdeError::Unrepresentable(what)))
}

/// Serializes `value` as UCL text in the config format (spec §10.5), which reads back exactly
/// with any registered variables. See the [module documentation](self) for the forms and the
/// errors.
pub fn to_string<T: ?Sized + Serialize>(value: &T) -> Result<String, UclError> {
    to_text(value, Format::Config)
}

/// Serializes `value` as pretty JSON (spec §10.4 layout), one member per line. The output is
/// valid JSON (RFC 8259): times are numbers of seconds, and NaN and infinite floats and times are
/// errors. See the [module documentation](self).
pub fn to_json_string<T: ?Sized + Serialize>(value: &T) -> Result<String, UclError> {
    to_text(value, Format::Json)
}

/// Serializes `value` as JSON without whitespace (spec §10.4 layout), valid JSON as
/// [`to_json_string`] writes it.
pub fn to_json_string_compact<T: ?Sized + Serialize>(value: &T) -> Result<String, UclError> {
    to_text(value, Format::JsonCompact)
}

/// Serializes `value` in libucl's YAML format (spec §10.6 layout).
pub fn to_yaml_string<T: ?Sized + Serialize>(value: &T) -> Result<String, UclError> {
    to_text(value, Format::Yaml)
}

/// Writes `value` to `writer` in the config format, as [`to_string`] does. Nothing is written
/// when serialization fails.
pub fn to_writer<W: io::Write, T: ?Sized + Serialize>(
    mut writer: W,
    value: &T,
) -> Result<(), UclError> {
    let text = to_string(value)?;
    writer.write_all(text.as_bytes())?;
    Ok(())
}

impl Serialize for UclValue {
    /// The crate's serializers copy the value as it is, in one step at any depth: times,
    /// multi-value entries, priorities and marks included. Other serializers see a newtype
    /// struct with a private name, which most formats write as its content, around the value:
    /// objects as maps, arrays as sequences, and scalars as themselves. A time is a newtype
    /// struct around its `f64` seconds, and an entry with several values a newtype struct around
    /// the sequence of its values, so they see a number and a sequence.
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_newtype_struct(marker::VALUE, &Tree::Value(self))
    }
}

impl Serialize for UclObject {
    /// As `Serialize for UclValue` serializes the object.
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_newtype_struct(marker::VALUE, &Tree::Object(self))
    }
}

/// The content of the newtype struct [`marker::VALUE`] that `Serialize for UclValue` and
/// `Serialize for UclObject` write. Asked by the crate's serializer, it hands a copy of the
/// value over (see [`crate::handoff`]); for any other serializer, it writes the value's
/// structure.
enum Tree<'a> {
    Value(&'a UclValue),
    Object(&'a UclObject),
}

impl Serialize for Tree<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if handoff::wanted() {
            handoff::put(match *self {
                Tree::Value(value) => value.clone(),
                Tree::Object(object) => UclValue::Object(object.clone()),
            });
            return serializer.serialize_unit();
        }
        match *self {
            Tree::Value(value) => Structure(value).serialize(serializer),
            Tree::Object(object) => ObjectStructure(object).serialize(serializer),
        }
    }
}

/// A value as serde's data model has it, for serializers other than the crate's.
struct Structure<'a>(&'a UclValue);

impl Serialize for Structure<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self.0 {
            UclValue::Object(object) => ObjectStructure(object).serialize(serializer),
            UclValue::Array(items) => serializer.collect_seq(items.iter().map(Structure)),
            UclValue::Integer(i) => serializer.serialize_i64(*i),
            UclValue::Float(f) => serializer.serialize_f64(*f),
            UclValue::Time(t) => serializer.serialize_newtype_struct(marker::TIME, t),
            UclValue::String(s) => serializer.serialize_str(s),
            UclValue::Boolean(b) => serializer.serialize_bool(*b),
            UclValue::Null => serializer.serialize_unit(),
        }
    }
}

/// An object as a map from each key to its value; the values of a key that has several are the
/// private newtype struct [`marker::MULTI`] around the sequence of them.
struct ObjectStructure<'a>(&'a UclObject);

impl Serialize for ObjectStructure<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (key, entry) in self.0.iter() {
            if entry.is_multi() {
                map.serialize_entry(key, &MultiValue(entry))?;
            } else {
                map.serialize_entry(key, &Structure(entry.first()))?;
            }
        }
        map.end()
    }
}

/// The values of a multi-value entry, as the private newtype struct [`marker::MULTI`].
struct MultiValue<'a>(&'a Entry);

impl Serialize for MultiValue<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_newtype_struct(marker::MULTI, &Values(self.0))
    }
}

struct Values<'a>(&'a Entry);

impl Serialize for Values<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.0.values().map(Structure))
    }
}
