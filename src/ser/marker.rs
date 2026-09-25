//! Private names by which the crate's serde types carry what serde's data model has no place
//! for: UCL time values, multi-value entries (spec §8.2), and whole [`UclValue`](crate::UclValue)
//! trees (see [`crate::handoff`]). The crate's own serializer and deserializer recognise them;
//! any other serializer or deserializer sees an ordinary newtype struct, so a time is a plain
//! `f64`, a multi-value entry a sequence and a tree its content. The names cannot clash with Rust
//! identifiers.

/// A UCL time: a newtype struct around the `f64` number of seconds, written by
/// [`crate::time::serialize`] and, for other serializers, by `Serialize for UclValue`.
pub(crate) const TIME: &str = "$__ucl_lexer_private_Time";

/// The values of a multi-value entry: a newtype struct around the sequence of values, which
/// `Serialize for UclObject` writes, for serializers other than the crate's, as the value of the
/// entry's key.
pub(crate) const MULTI: &str = "$__ucl_lexer_private_Multi";

/// A whole value: the newtype struct that `Serialize for UclValue` and `Serialize for UclObject`
/// write, and that `Deserialize for UclValue` asks for. The crate's serializer and deserializer
/// then move the value itself (see [`crate::handoff`]).
pub(crate) const VALUE: &str = "$__ucl_lexer_private_Value";

/// The enum variant by which the crate's deserializer answers a request for [`VALUE`]: the value
/// is waiting in [`crate::handoff`].
pub(crate) const TREE: &str = "$__ucl_lexer_private_Tree";
