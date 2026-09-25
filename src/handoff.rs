//! Moving a whole [`UclValue`] between `Serialize`/`Deserialize for UclValue` and the crate's own
//! serializer and deserializer, past serde's data model.
//!
//! serde passes a value through its data model one level at a time: each nested map or sequence
//! is another call into the serializer or the visitor, so a document nested 1024 containers deep,
//! as the parser allows (spec §11.2), takes 1024 levels of recursion, with several stack frames
//! each. In a debug build that overflows a 2 MiB thread stack after a few hundred levels. For a
//! `UclValue`, the crate's serializer and deserializer skip the data model and move the value
//! itself, through this thread-local slot:
//!
//! - Deserializing: `Deserialize for UclValue` asks for the private newtype struct
//!   [`marker::VALUE`](crate::ser::marker::VALUE). The crate's deserializer [`put`]s its value
//!   here and answers with the private enum variant [`marker::TREE`](crate::ser::marker::TREE);
//!   the visitor [`take`]s the value. Other deserializers see an ordinary newtype struct.
//! - Serializing: `Serialize for UclValue` (and for `UclObject`) writes the private newtype
//!   struct [`marker::VALUE`](crate::ser::marker::VALUE). The crate's serializer asks for the
//!   tree ([`request`]); the value, seeing the request ([`wanted`]), [`put`]s a copy of itself
//!   here and writes a unit. Other serializers see an ordinary newtype struct around the value.
//!
//! Nothing runs between putting a value and taking it but the crate's own code, and each side
//! clears what the other did not take.

use crate::value::UclValue;
use std::cell::{Cell, RefCell};

thread_local! {
    /// The value being moved.
    static VALUE: RefCell<Option<UclValue>> = const { RefCell::new(None) };
    /// The crate's serializer has asked `Serialize for UclValue` for its tree.
    static WANTED: Cell<bool> = const { Cell::new(false) };
}

/// Puts `value` in the slot.
pub(crate) fn put(value: UclValue) {
    let old = VALUE.with(|slot| slot.replace(Some(value)));
    drop(old);
}

/// Takes the value out of the slot.
pub(crate) fn take() -> Option<UclValue> {
    VALUE.with(RefCell::take)
}

/// Runs `serialize`, which is to serialize a value that wrote the newtype struct
/// [`marker::VALUE`](crate::ser::marker::VALUE), with the tree asked for, and returns its result
/// and the tree the value put in the slot.
pub(crate) fn request<T, E>(
    serialize: impl FnOnce() -> Result<T, E>,
) -> (Result<T, E>, Option<UclValue>) {
    WANTED.with(|wanted| wanted.set(true));
    let result = serialize();
    WANTED.with(|wanted| wanted.set(false));
    (result, take())
}

/// Whether the crate's serializer asked for the tree. Clears the request, so that only the value
/// asked answers it.
pub(crate) fn wanted() -> bool {
    WANTED.with(|wanted| wanted.replace(false))
}
