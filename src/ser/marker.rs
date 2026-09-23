//! Names of the newtype structs that carry what serde's data model has no place for: UCL time
//! values and multi-value entries (spec §8.2). The crate's own serializer and deserializer
//! recognise them; any other serializer or deserializer sees an ordinary newtype struct, so a
//! time is a plain `f64` and a multi-value entry a sequence. The names cannot clash with Rust
//! identifiers.

/// A UCL time: a newtype struct around the `f64` number of seconds.
pub(crate) const TIME: &str = "$__ucl_lexer_private_Time";

/// The values of a multi-value entry: a newtype struct around the sequence of values, written
/// by `Serialize for UclObject` as the value of the entry's key.
pub(crate) const MULTI: &str = "$__ucl_lexer_private_Multi";

/// Asked for by `Deserialize for UclValue`. The crate's deserializer answers with a time as an
/// enum variant named [`TIME`], and every other value as usual.
pub(crate) const VALUE: &str = "$__ucl_lexer_private_Value";

/// Asked for by `Deserialize for UclValue` for the value of an object's key. The crate's
/// deserializer answers with an enum variant named [`MULTI`] holding every value of the entry.
pub(crate) const ENTRY: &str = "$__ucl_lexer_private_Entry";
