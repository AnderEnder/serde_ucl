//! UCL value model.
//!
//! The model follows libucl's object tree. A key in an object holds an [`Entry`] of one or more
//! values: more than one is libucl's *implicit array* (`UCL_OBJECT_MULTIVALUE`), created when a
//! key repeats. An explicit array written with `[...]` is a single [`UclValue::Array`] value, so
//! the two stay distinct:
//!
//! ```text
//! k = [1, 2]; k = 3    →  entry "k" with two values: Array[1, 2], Integer 3
//! ```
//!
//! Each value in an entry carries the priority of the chunk it came from (0–15, as in libucl) and
//! whether it was copied by `.inherit`. Both only matter while duplicate keys are resolved; see
//! [`UclObject::insert_with_strategy`].
//!
//! # Borrowed and owned values
//!
//! [`Value<'a>`] is a value whose keys and strings may borrow from text that lives for `'a`
//! ([`Str`]); [`UclValue`] is `Value<'static>`, a value that owns them (or borrows only
//! `'static` text). The parser ([`crate::parse`]) and every other function of the crate that
//! gives a value give a `UclValue`. The serde entry points parse the caller's text into a value
//! that borrows from it, so that targets that borrow, such as `&str` and `Cow<str>` fields, can
//! take keys and strings without a copy ([`crate::de`]). [`Value::into_owned`] turns a borrowed
//! value into an owned one.
//!
//! Cloning, comparing, dropping and [`Value::into_owned`] do not recurse: they take the same
//! stack at any depth of nesting.

use std::fmt;

mod map;
mod string;
mod text;

use map::Map;
pub(crate) use map::Probe;
pub use map::{Entries, IntoIter, Iter, IterMut, Keys};
pub use string::Str;
pub(crate) use text::KeyCopy;

/// A UCL value whose keys and strings may borrow from text that lives for `'a` (see the
/// [module documentation](self#borrowed-and-owned-values)).
///
/// Cloning, comparing and dropping do not recurse: they take the same stack at any depth of
/// nesting. Formatting it with `Debug` recurses once per level.
#[derive(Debug)]
pub enum Value<'a> {
    Object(Object<'a>),
    Array(Array<'a>),
    Integer(i64),
    Float(f64),
    /// Time in seconds (`10s`, `5min`, `10ms`): libucl's `UCL_TIME`.
    Time(f64),
    String(Str<'a>),
    Boolean(bool),
    Null,
}

/// A UCL value that owns its keys and strings: what the parser gives.
pub type UclValue = Value<'static>;

/// An object that owns its keys and strings.
pub type UclObject = Object<'static>;

/// An explicit array that owns its keys and strings.
pub type UclArray = Array<'static>;

/// Written out rather than derived, which would recurse once per level of nesting: `.inherit`
/// copies values nested as deep as the parser allows (spec §9.7, §11.2), and a derived clone of
/// such a value overflows a 2 MiB stack in a debug build. The copy keeps each value's priority
/// and marks.
impl Clone for Value<'_> {
    fn clone(&self) -> Self {
        copy_value(self, Str::clone)
    }
}

/// A copy of `value` whose strings and keys are `text` of `value`'s: clones, or owned copies.
fn copy_value<'a, 'b>(value: &Value<'a>, text: fn(&Str<'a>) -> Str<'b>) -> Value<'b> {
    match value {
        Value::Object(_) | Value::Array(_) => clone_tree(value, text),
        scalar => copy_scalar(scalar, text),
    }
}

/// A copy of `scalar`, as [`copy_value`] makes it.
fn copy_scalar<'a, 'b>(scalar: &Value<'a>, text: fn(&Str<'a>) -> Str<'b>) -> Value<'b> {
    match scalar {
        Value::Integer(i) => Value::Integer(*i),
        Value::Float(f) => Value::Float(*f),
        Value::Time(t) => Value::Time(*t),
        Value::String(s) => Value::String(text(s)),
        Value::Boolean(b) => Value::Boolean(*b),
        Value::Null => Value::Null,
        Value::Object(_) | Value::Array(_) => unreachable!("not a scalar"),
    }
}

/// Written out rather than derived, which would recurse once per level of nesting: `==` compares
/// as the derived comparisons of [`Value`], [`Object`], [`Entry`] and [`Slot`] would, with a heap
/// stack. Values of different kinds differ; floats and times compare as `f64` (a NaN equals
/// nothing); objects compare as maps, their keys in any order; the values of an entry compare in
/// order, each with its priority and marks. Borrowed and owned strings compare by their text.
impl<'b> PartialEq<Value<'b>> for Value<'_> {
    fn eq(&self, other: &Value<'b>) -> bool {
        let mut pending = Vec::new();
        push_values(self, other, &mut pending) && equal_pending(pending)
    }
}

/// See [`Value`]'s `PartialEq`.
impl<'b> PartialEq<Object<'b>> for Object<'_> {
    fn eq(&self, other: &Object<'b>) -> bool {
        let mut pending = Vec::new();
        push_objects(self, other, &mut pending) && equal_pending(pending)
    }
}

/// See [`Value`]'s `PartialEq`: element by element.
impl<'b> PartialEq<Array<'b>> for Array<'_> {
    fn eq(&self, other: &Array<'b>) -> bool {
        let mut pending = Vec::new();
        pending.extend(self.0.iter().zip(&other.0));
        self.0.len() == other.0.len() && equal_pending(pending)
    }
}

/// See [`Value`]'s `PartialEq`.
impl<'b> PartialEq<Entry<'b>> for Entry<'_> {
    fn eq(&self, other: &Entry<'b>) -> bool {
        let mut pending = Vec::new();
        push_entries(self, other, &mut pending) && equal_pending(pending)
    }
}

/// See [`Value`]'s `PartialEq`.
impl<'b> PartialEq<Slot<'b>> for Slot<'_> {
    fn eq(&self, other: &Slot<'b>) -> bool {
        let mut pending = Vec::new();
        push_slots(self, other, &mut pending) && equal_pending(pending)
    }
}

/// Pairs of values still to compare.
type Pending<'p, 'a, 'b> = Vec<(&'p Value<'a>, &'p Value<'b>)>;

/// Whether every pair in `pending`, and every pair of values inside them, is equal.
fn equal_pending(mut pending: Pending<'_, '_, '_>) -> bool {
    while let Some((a, b)) = pending.pop() {
        if !push_values(a, b, &mut pending) {
            return false;
        }
    }
    true
}

/// Compares `a` and `b` without the values inside them, which it pushes to `pending` in pairs.
fn push_values<'p, 'a, 'b>(
    a: &'p Value<'a>,
    b: &'p Value<'b>,
    pending: &mut Pending<'p, 'a, 'b>,
) -> bool {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => push_objects(x, y, pending),
        (Value::Array(x), Value::Array(y)) => {
            pending.extend(x.0.iter().zip(&y.0));
            x.0.len() == y.0.len()
        }
        (Value::Integer(x), Value::Integer(y)) => x == y,
        (Value::Float(x), Value::Float(y)) | (Value::Time(x), Value::Time(y)) => x == y,
        (Value::String(x), Value::String(y)) => x.as_str() == y.as_str(),
        (Value::Boolean(x), Value::Boolean(y)) => x == y,
        (Value::Null, Value::Null) => true,
        _ => false,
    }
}

fn push_objects<'p, 'a, 'b>(
    x: &'p Object<'a>,
    y: &'p Object<'b>,
    pending: &mut Pending<'p, 'a, 'b>,
) -> bool {
    x.entries.len() == y.entries.len()
        && x.entries.iter().all(|(key, entry)| {
            y.entries
                .get(key.as_str())
                .is_some_and(|other| push_entries(entry, other, pending))
        })
}

fn push_entries<'p, 'a, 'b>(
    x: &'p Entry<'a>,
    y: &'p Entry<'b>,
    pending: &mut Pending<'p, 'a, 'b>,
) -> bool {
    x.slots.len() == y.slots.len()
        && x.slots
            .iter()
            .zip(y.slots.iter())
            .all(|(s, t)| push_slots(s, t, pending))
}

fn push_slots<'p, 'a, 'b>(
    s: &'p Slot<'a>,
    t: &'p Slot<'b>,
    pending: &mut Pending<'p, 'a, 'b>,
) -> bool {
    pending.push((&s.value, &t.value));
    (s.priority, s.inherited, s.collected) == (t.priority, t.inherited, t.collected)
}

/// A copy of the container `root`, as [`copy_value`] makes it, built with a heap stack: every
/// value is copied after the values inside it, which wait on `done` in order until their
/// container is built.
fn clone_tree<'a, 'b>(root: &Value<'a>, text: fn(&Str<'a>) -> Str<'b>) -> Value<'b> {
    enum Step<'r, 'a> {
        /// Copy this value: a scalar at once, a container after its values.
        Copy(&'r Value<'a>),
        /// Build the copy of this container from the copies of its values on `done`.
        Build(&'r Value<'a>),
    }
    let mut todo = vec![Step::Copy(root)];
    let mut done: Vec<Value<'b>> = Vec::new();
    while let Some(step) = todo.pop() {
        match step {
            Step::Copy(value @ Value::Object(object)) => {
                todo.push(Step::Build(value));
                todo.extend(
                    object
                        .entries()
                        .flat_map(Entry::values)
                        .rev()
                        .map(Step::Copy),
                );
            }
            Step::Copy(value @ Value::Array(items)) => {
                todo.push(Step::Build(value));
                todo.extend(items.iter().rev().map(Step::Copy));
            }
            Step::Copy(scalar) => done.push(copy_scalar(scalar, text)),
            Step::Build(Value::Object(object)) => {
                let count = object.entries().map(Entry::len).sum::<usize>();
                let mut values = done.split_off(done.len() - count).into_iter();
                let mut entries = Map::with_capacity(object.entries.len());
                for (key, entry) in object.entries.iter() {
                    let slots = entry
                        .slots
                        .iter()
                        .map(|slot| slot.with_value(values.next().expect("copied")));
                    entries.push_unique(text(key), Entry::from_slots(slots));
                }
                done.push(Value::Object(Object { entries }));
            }
            Step::Build(Value::Array(items)) => {
                let elements = done.split_off(done.len() - items.len());
                done.push(Value::Array(Array(elements)));
            }
            Step::Build(_) => unreachable!("only containers are built"),
        }
    }
    done.pop().expect("the root was copied")
}

/// `root` with every key and string it borrows copied, built with a heap stack as
/// [`clone_tree`] builds a copy, but taking the values out of `root` rather than copying them.
fn owned_tree(root: Value<'_>) -> UclValue {
    /// The shape of a container whose values are being made owned: the keys of an object with
    /// the priority and marks of each value, or the length of an array.
    enum Shape {
        Object(Vec<(Str<'static>, Vec<Slot<'static>>)>, usize),
        Array(usize),
    }
    enum Step<'a> {
        Take(Value<'a>),
        Build(Shape),
    }
    let mut todo = vec![Step::Take(root)];
    let mut done: Vec<UclValue> = Vec::new();
    while let Some(step) = todo.pop() {
        match step {
            Step::Take(Value::Object(mut object)) => {
                let entries = std::mem::take(&mut object.entries);
                let mut shape = Vec::with_capacity(entries.len());
                let mut values = Vec::new();
                for (key, entry) in entries {
                    let marks = entry
                        .slots
                        .iter()
                        .map(|slot| slot.with_value(Value::Null))
                        .collect();
                    shape.push((key.into_owned(), marks));
                    values.extend(entry.into_values());
                }
                let count = values.len();
                todo.push(Step::Build(Shape::Object(shape, count)));
                todo.extend(values.into_iter().rev().map(Step::Take));
            }
            Step::Take(Value::Array(mut items)) => {
                let items = std::mem::take(&mut items.0);
                todo.push(Step::Build(Shape::Array(items.len())));
                todo.extend(items.into_iter().rev().map(Step::Take));
            }
            Step::Take(scalar) => done.push(match scalar {
                Value::Integer(i) => Value::Integer(i),
                Value::Float(f) => Value::Float(f),
                Value::Time(t) => Value::Time(t),
                Value::String(s) => Value::String(s.into_owned()),
                Value::Boolean(b) => Value::Boolean(b),
                Value::Null => Value::Null,
                Value::Object(_) | Value::Array(_) => unreachable!("containers are taken apart"),
            }),
            Step::Build(Shape::Object(shape, count)) => {
                let mut values = done.split_off(done.len() - count).into_iter();
                let mut entries = Map::with_capacity(shape.len());
                for (key, marks) in shape {
                    let slots = marks
                        .into_iter()
                        .map(|mark| mark.with_value(values.next().expect("made owned")));
                    entries.push_unique(key, Entry::from_slots(slots));
                }
                done.push(Value::Object(Object { entries }));
            }
            Step::Build(Shape::Array(len)) => {
                let elements = done.split_off(done.len() - len);
                done.push(Value::Array(Array(elements)));
            }
        }
    }
    done.pop().expect("the root was made owned")
}

/// Whether dropping `value` could recurse: it is a container that holds something.
fn holds_values(value: &Value<'_>) -> bool {
    match value {
        Value::Object(object) => !object.entries.is_empty(),
        Value::Array(items) => !items.0.is_empty(),
        _ => false,
    }
}

/// Moves the containers that hold values out of the values of `object`'s entries onto
/// `stack`, leaving `null` in their place, so that dropping `object` recurses no further.
fn take_nested<'a>(object: &mut Object<'a>, stack: &mut Vec<Value<'a>>) {
    for entry in object.entries.values_mut() {
        for slot in entry.slots.as_mut_slice() {
            if holds_values(&slot.value) {
                stack.push(std::mem::replace(&mut slot.value, Value::Null));
            }
        }
    }
}

/// [`take_nested`] for the elements of an array.
fn take_nested_elements<'a>(items: &mut [Value<'a>], stack: &mut Vec<Value<'a>>) {
    for item in items {
        if holds_values(item) {
            stack.push(std::mem::replace(item, Value::Null));
        }
    }
}

/// Drops the containers on `stack` one at a time, each after the containers inside it have been
/// moved onto `stack` too, so that no drop recurses.
fn drop_stack(mut stack: Vec<Value<'_>>) {
    while let Some(mut value) = stack.pop() {
        match &mut value {
            Value::Object(object) => take_nested(object, &mut stack),
            Value::Array(items) => take_nested_elements(&mut items.0, &mut stack),
            _ => {}
        }
    }
}

/// Drops `value` with a heap stack ([`Value`] drops that way).
pub(crate) fn discard(value: Value<'_>) {
    drop(value);
}

/// Keeps only the first value of each entry whose first value is an object or an array, in
/// every object inside `value`, `value` itself included: the rule of `.inherit` copies at every
/// level of the copy (spec §9.7). Entries whose first value is anything else keep all their
/// values. Walks with a heap stack.
pub(crate) fn keep_first_container_values(value: &mut Value<'_>) {
    let mut stack = vec![value];
    while let Some(value) = stack.pop() {
        match value {
            Value::Object(object) => {
                for entry in object.entries.values_mut() {
                    if entry.slots.len() > 1 && is_container(&entry.slots.as_slice()[0].value) {
                        entry.slots.truncate_to_first();
                    }
                    stack.extend(entry.slots.as_mut_slice().iter_mut().map(Slot::value_mut));
                }
            }
            Value::Array(items) => stack.extend(items.0.iter_mut()),
            _ => {}
        }
    }
}

/// The most containers (objects and arrays) nested inside one another in `value`, itself
/// included: 0 for a scalar, 1 for a container that holds only scalars. Counted with a heap
/// stack.
pub(crate) fn nesting(value: &Value<'_>) -> usize {
    let mut deepest = 0;
    let mut stack = vec![(value, 1)];
    while let Some((value, depth)) = stack.pop() {
        match value {
            Value::Object(object) => {
                deepest = deepest.max(depth);
                stack.extend(
                    object
                        .entries()
                        .flat_map(Entry::values)
                        .map(|v| (v, depth + 1)),
                );
            }
            Value::Array(items) => {
                deepest = deepest.max(depth);
                stack.extend(items.iter().map(|v| (v, depth + 1)));
            }
            _ => {}
        }
    }
    deepest
}

impl<'a> Value<'a> {
    /// Returns true if the value is an object
    pub fn is_object(&self) -> bool {
        matches!(self, Value::Object(_))
    }

    /// Returns true if the value is an explicit array
    pub fn is_array(&self) -> bool {
        matches!(self, Value::Array(_))
    }

    /// Returns true if the value is a string
    pub fn is_string(&self) -> bool {
        matches!(self, Value::String(_))
    }

    /// Returns true if the value is a time
    pub fn is_time(&self) -> bool {
        matches!(self, Value::Time(_))
    }

    /// Returns true if this is a Null variant
    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }

    /// Returns the object if this is an Object variant
    pub fn as_object(&self) -> Option<&Object<'a>> {
        match self {
            Value::Object(obj) => Some(obj),
            _ => None,
        }
    }

    /// Returns the object mutably if this is an Object variant
    pub fn as_object_mut(&mut self) -> Option<&mut Object<'a>> {
        match self {
            Value::Object(obj) => Some(obj),
            _ => None,
        }
    }

    /// Returns the array if this is an Array variant
    pub fn as_array(&self) -> Option<&Array<'a>> {
        match self {
            Value::Array(arr) => Some(arr),
            _ => None,
        }
    }

    /// Returns the array mutably if this is an Array variant
    pub fn as_array_mut(&mut self) -> Option<&mut Array<'a>> {
        match self {
            Value::Array(arr) => Some(arr),
            _ => None,
        }
    }

    /// Returns the string if this is a String variant
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s.as_str()),
            _ => None,
        }
    }

    /// Returns the integer if this is an Integer variant
    pub fn as_integer(&self) -> Option<i64> {
        match self {
            Value::Integer(i) => Some(*i),
            _ => None,
        }
    }

    /// Returns the float if this is a Float variant
    pub fn as_float(&self) -> Option<f64> {
        match self {
            Value::Float(f) => Some(*f),
            _ => None,
        }
    }

    /// Returns the number of seconds if this is a Time variant
    pub fn as_time(&self) -> Option<f64> {
        match self {
            Value::Time(t) => Some(*t),
            _ => None,
        }
    }

    /// Returns the boolean if this is a Boolean variant
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Boolean(b) => Some(*b),
            _ => None,
        }
    }

    /// Name of the value's type, as libucl spells it (`ucl_object_type_to_string`).
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Object(_) => "object",
            Value::Array(_) => "array",
            Value::Integer(_) => "int",
            Value::Float(_) => "float",
            Value::Time(_) => "time",
            Value::String(_) => "string",
            Value::Boolean(_) => "boolean",
            Value::Null => "null",
        }
    }

    /// A copy of the value that borrows nothing: [`Value::into_owned`] of a clone, in one step.
    pub(crate) fn owned_copy(&self) -> UclValue {
        copy_value(self, |s| s.clone().into_owned())
    }

    /// The value with every key and string it borrows copied, so that it borrows nothing, with
    /// the same stack at any depth. Priorities and marks are kept.
    pub fn into_owned(self) -> UclValue {
        match self {
            Value::Object(_) | Value::Array(_) => owned_tree(self),
            Value::Integer(i) => Value::Integer(i),
            Value::Float(f) => Value::Float(f),
            Value::Time(t) => Value::Time(t),
            Value::String(s) => Value::String(s.into_owned()),
            Value::Boolean(b) => Value::Boolean(b),
            Value::Null => Value::Null,
        }
    }
}

/// How a key that is already present is resolved: libucl's `ucl_duplicate_strategy`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum DuplicateStrategy {
    /// Priority-aware (libucl's default): equal priority adds the value to the entry (implicit
    /// array), higher priority replaces the entry, lower priority is dropped.
    #[default]
    Append,
    /// Merge containers: a new object's keys go into the existing object, a new array's
    /// elements into the existing array. Scalars follow [`DuplicateStrategy::Append`].
    Merge,
    /// Replace the existing entry regardless of priority.
    Rewrite,
    /// Fail on the duplicate.
    Error,
}

/// Parser flags: libucl's `ucl_parser_flags`, bit for bit.
///
/// Only [`ParserFlags::KEY_LOWERCASE`] and [`ParserFlags::NO_IMPLICIT_ARRAYS`] affect
/// [`UclObject::insert_with_strategy`]; the others are read by the parser.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct ParserFlags(u32);

impl ParserFlags {
    /// No flags (`UCL_PARSER_DEFAULT`).
    pub const DEFAULT: Self = Self(0);
    /// Convert all keys to lower case.
    pub const KEY_LOWERCASE: Self = Self(1 << 0);
    /// Parse input in zero-copy mode if possible.
    pub const ZEROCOPY: Self = Self(1 << 1);
    /// Do not parse time suffixes; treat such values as strings.
    pub const NO_TIME: Self = Self(1 << 2);
    /// Collect repeated keys into an explicit array instead of an implicit one.
    pub const NO_IMPLICIT_ARRAYS: Self = Self(1 << 3);
    /// Save comments in the parser context.
    pub const SAVE_COMMENTS: Self = Self(1 << 4);
    /// Reject macros as syntax errors and switch off variable expansion (spec §12.6).
    pub const DISABLE_MACRO: Self = Self(1 << 5);
    /// Do not set the `FILENAME` and `CURDIR` variables.
    pub const NO_FILEVARS: Self = Self(1 << 6);

    const ALL: u32 = (1 << 7) - 1;

    /// No flags.
    pub const fn empty() -> Self {
        Self(0)
    }

    /// The raw bits, equal to libucl's values.
    pub const fn bits(self) -> u32 {
        self.0
    }

    /// Builds flags from raw bits, dropping unknown bits.
    pub const fn from_bits_truncate(bits: u32) -> Self {
        Self(bits & Self::ALL)
    }

    /// Returns true if every flag in `other` is set.
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// Returns true if no flag is set.
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Sets the flags in `other`.
    pub fn insert(&mut self, other: Self) {
        self.0 |= other.0;
    }

    /// Clears the flags in `other`.
    pub fn remove(&mut self, other: Self) {
        self.0 &= !other.0;
    }
}

impl std::ops::BitOr for ParserFlags {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl std::ops::BitOrAssign for ParserFlags {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl std::ops::BitAnd for ParserFlags {
    type Output = Self;

    fn bitand(self, rhs: Self) -> Self {
        Self(self.0 & rhs.0)
    }
}

/// Returned by [`UclObject::insert_with_strategy`] when a repeated key cannot take another value:
/// under [`DuplicateStrategy::Error`], for a container type mismatch under
/// [`DuplicateStrategy::Merge`], and for the `NO_IMPLICIT_ARRAYS` repeat of spec §8.5.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateKeyError {
    pub key: String,
}

impl fmt::Display for DuplicateKeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "key '{}' cannot take another value", self.key)
    }
}

impl std::error::Error for DuplicateKeyError {}

/// The highest priority, 15. Priorities are kept modulo 16, as libucl keeps them (spec §8.3):
/// only their 4 least significant bits are used.
pub const MAX_PRIORITY: u8 = 0x0f;

/// Where [`UclObject::insert_slot_placed`] put a value. A parser that fills containers in place
/// uses it to find the container it has just inserted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    /// The value is value `n` of the key's entry.
    Slot(usize),
    /// The value is element `n` of the explicit array that is the entry's first value: a repeat
    /// collected under [`ParserFlags::NO_IMPLICIT_ARRAYS`] (spec §8.5).
    Collected(usize),
    /// The value, a container, was merged into the entry's first value, a container of the same
    /// type (spec §8.4).
    Merged,
    /// The value was discarded: the existing value has a higher priority (spec §8.3).
    Dropped,
}

/// One value of an [`Entry`], with its priority and `.inherit` flag.
#[derive(Debug, Clone)]
pub struct Slot<'a> {
    value: Value<'a>,
    priority: u8,
    inherited: bool,
    /// Set on an explicit array built from repeated keys under `NO_IMPLICIT_ARRAYS`.
    collected: bool,
}

impl<'a> Slot<'a> {
    /// A value with the given priority (masked to 0–15).
    pub fn new(value: Value<'a>, priority: u8) -> Self {
        Self {
            value,
            priority: priority & MAX_PRIORITY,
            inherited: false,
            collected: false,
        }
    }

    /// A value copied from another object by `.inherit`.
    #[cfg(test)]
    pub(crate) fn inherited(value: Value<'a>, priority: u8) -> Self {
        Self::new(value, priority).into_inherited()
    }

    /// An explicit array that collects a key's repeats under `NO_IMPLICIT_ARRAYS` (spec §8.5),
    /// at priority 0.
    pub(crate) fn collection(value: Value<'a>) -> Self {
        Self {
            collected: true,
            ..Self::new(value, 0)
        }
    }

    /// A slot with this slot's priority and marks that holds `value`.
    pub(crate) fn with_value<'b>(&self, value: Value<'b>) -> Slot<'b> {
        Slot {
            value,
            priority: self.priority,
            inherited: self.inherited,
            collected: self.collected,
        }
    }

    /// The slot marked as copied by `.inherit` (spec §9.7). Its value, priority and
    /// `NO_IMPLICIT_ARRAYS` collection mark are kept.
    pub(crate) fn into_inherited(self) -> Self {
        Self {
            inherited: true,
            ..self
        }
    }

    pub fn value(&self) -> &Value<'a> {
        &self.value
    }

    pub fn value_mut(&mut self) -> &mut Value<'a> {
        &mut self.value
    }

    pub fn into_value(self) -> Value<'a> {
        self.value
    }

    /// Priority 0–15.
    pub fn priority(&self) -> u8 {
        self.priority
    }

    /// True if the value was copied by `.inherit`.
    pub fn is_inherited(&self) -> bool {
        self.inherited
    }

    /// True for the explicit array that collects a key's repeats under `NO_IMPLICIT_ARRAYS`.
    pub(crate) fn is_collected(&self) -> bool {
        self.collected
    }
}

/// The slots of an [`Entry`]: one kept in place, more in a vector, so that an entry of one value
/// allocates nothing for it. Written out rather than a `SmallVec`, which would make [`Value`]
/// invariant in its lifetime: an owned value could not then go into a borrowed tree.
#[derive(Clone)]
enum Slots<'a> {
    One(Slot<'a>),
    Many(Vec<Slot<'a>>),
}

impl fmt::Debug for Slots<'_> {
    /// As a list of the slots, the form of the `SmallVec` this replaces.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.as_slice()).finish()
    }
}

impl<'a> Slots<'a> {
    fn as_slice(&self) -> &[Slot<'a>] {
        match self {
            Slots::One(slot) => std::slice::from_ref(slot),
            Slots::Many(slots) => slots,
        }
    }

    fn as_mut_slice(&mut self) -> &mut [Slot<'a>] {
        match self {
            Slots::One(slot) => std::slice::from_mut(slot),
            Slots::Many(slots) => slots,
        }
    }

    fn len(&self) -> usize {
        match self {
            Slots::One(_) => 1,
            Slots::Many(slots) => slots.len(),
        }
    }

    fn iter(&self) -> std::slice::Iter<'_, Slot<'a>> {
        self.as_slice().iter()
    }

    fn push(&mut self, slot: Slot<'a>) {
        match self {
            Slots::Many(slots) => slots.push(slot),
            Slots::One(_) => {
                let Slots::One(first) = std::mem::replace(self, Slots::Many(Vec::new())) else {
                    unreachable!("matched above")
                };
                *self = Slots::Many(vec![first, slot]);
            }
        }
    }

    /// Keeps the first slot only.
    fn truncate_to_first(&mut self) {
        if let Slots::Many(slots) = self {
            slots.truncate(1);
        }
    }

    /// The first slot, taking the others with it.
    fn into_first(self) -> Slot<'a> {
        match self {
            Slots::One(slot) => slot,
            Slots::Many(slots) => slots.into_iter().next().expect("an entry holds a value"),
        }
    }

    fn into_iter(self) -> SlotsIntoIter<'a> {
        match self {
            Slots::One(slot) => SlotsIntoIter::One(Some(slot)),
            Slots::Many(slots) => SlotsIntoIter::Many(slots.into_iter()),
        }
    }
}

/// The slots of an entry, by value.
enum SlotsIntoIter<'a> {
    One(Option<Slot<'a>>),
    Many(std::vec::IntoIter<Slot<'a>>),
}

impl<'a> Iterator for SlotsIntoIter<'a> {
    type Item = Slot<'a>;

    fn next(&mut self) -> Option<Slot<'a>> {
        match self {
            SlotsIntoIter::One(slot) => slot.take(),
            SlotsIntoIter::Many(slots) => slots.next(),
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        match self {
            SlotsIntoIter::One(slot) => (usize::from(slot.is_some()), Some(1)),
            SlotsIntoIter::Many(slots) => slots.size_hint(),
        }
    }
}

/// The values of one key: always at least one. More than one is an implicit array.
#[derive(Debug, Clone)]
pub struct Entry<'a> {
    slots: Slots<'a>,
}

impl<'a> Entry<'a> {
    /// An entry with one value at priority 0.
    pub fn new(value: Value<'a>) -> Self {
        Self::from_slot(Slot::new(value, 0))
    }

    /// An entry with one slot.
    pub fn from_slot(slot: Slot<'a>) -> Self {
        Self {
            slots: Slots::One(slot),
        }
    }

    /// An entry of `slots`, of which there is at least one.
    fn from_slots(mut slots: impl Iterator<Item = Slot<'a>>) -> Self {
        let mut entry = Entry::from_slot(slots.next().expect("an entry holds a value"));
        for slot in slots {
            entry.slots.push(slot);
        }
        entry
    }

    /// Number of values (1 unless this is an implicit array).
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    /// Always false: an entry holds at least one value.
    pub fn is_empty(&self) -> bool {
        false
    }

    /// True if the key was repeated (libucl's implicit array).
    pub fn is_multi(&self) -> bool {
        self.slots.len() > 1
    }

    /// The first value, which is what libucl's `ucl_object_lookup` returns.
    pub fn first(&self) -> &Value<'a> {
        &self.slots.as_slice()[0].value
    }

    /// The first value, mutably.
    pub fn first_mut(&mut self) -> &mut Value<'a> {
        &mut self.slots.as_mut_slice()[0].value
    }

    /// The last value.
    pub fn last(&self) -> &Value<'a> {
        let slots = self.slots.as_slice();
        &slots[slots.len() - 1].value
    }

    /// All values in insertion order.
    pub fn values(&self) -> Values<'_, 'a> {
        Values(self.slots.iter())
    }

    /// All values with their priority and flags.
    pub fn slots(&self) -> &[Slot<'a>] {
        self.slots.as_slice()
    }

    /// Value `index` of the entry, mutably.
    pub fn value_at_mut(&mut self, index: usize) -> Option<&mut Value<'a>> {
        self.slots
            .as_mut_slice()
            .get_mut(index)
            .map(|s| &mut s.value)
    }

    /// Adds a value to the entry at priority 0, making it an implicit array.
    pub fn push(&mut self, value: Value<'a>) {
        self.slots.push(Slot::new(value, 0));
    }

    /// Adds a slot to the entry.
    pub fn push_slot(&mut self, slot: Slot<'a>) {
        self.slots.push(slot);
    }

    /// The single value, or an explicit array of all values for an implicit array.
    ///
    /// This is the view serde and JSON-like consumers get: a repeated key reads as a sequence.
    pub fn into_value(self) -> Value<'a> {
        match self.slots {
            Slots::One(slot) => slot.value,
            Slots::Many(slots) => Value::Array(Array(slots.into_iter().map(|s| s.value).collect())),
        }
    }

    /// All values in insertion order.
    pub fn into_values(self) -> impl Iterator<Item = Value<'a>> {
        self.slots.into_iter().map(|s| s.value)
    }

    /// The entry with every key and string it borrows copied ([`Value::into_owned`]).
    pub fn into_owned(self) -> Entry<'static> {
        let slots = self.slots.into_iter().map(|slot| {
            let marks = slot.with_value(Value::Null);
            marks.with_value(slot.value.into_owned())
        });
        Entry::from_slots(slots)
    }

    fn head(&self) -> &Slot<'a> {
        &self.slots.as_slice()[0]
    }

    /// Resolves a repeated key by priority (spec §8.3), comparing with the first value: a higher
    /// priority replaces the entry, a lower one is dropped, and an equal one is added (§8.2), or
    /// collected into an explicit array under `NO_IMPLICIT_ARRAYS` (§8.5). With
    /// `replace_inherited`, an inherited first value is replaced whatever the priorities.
    fn add_by_priority(
        &mut self,
        key: &str,
        slot: Slot<'a>,
        replace_inherited: bool,
        flags: ParserFlags,
    ) -> Result<Placement, DuplicateKeyError> {
        let head = self.head();
        if (replace_inherited && head.inherited) || slot.priority > head.priority {
            *self = Entry::from_slot(slot);
            Ok(Placement::Slot(0))
        } else if slot.priority == head.priority {
            if flags.contains(ParserFlags::NO_IMPLICIT_ARRAYS) {
                return self.collect(key, slot);
            }
            self.slots.push(slot);
            Ok(Placement::Slot(self.slots.len() - 1))
        } else {
            Ok(Placement::Dropped)
        }
    }

    /// Adds a repeated value under `NO_IMPLICIT_ARRAYS` (spec §8.5). The first repeat replaces the
    /// entry with an explicit array of the old and new values; later repeats are appended to it.
    /// Only the entry's first value goes into the array: other values, which only `.inherit`
    /// with `replace=true` can add under this flag, are dropped (QUESTIONS.md #25).
    ///
    /// Two details follow the oracle rather than the spec text (QUESTIONS.md #3, #4):
    /// - The collection array has priority 0, whatever its elements' priorities, so a later value
    ///   with a priority above 0 replaces it.
    /// - If `merge` has replaced the collection array with a scalar, a later repeat is an error.
    fn collect(&mut self, key: &str, slot: Slot<'a>) -> Result<Placement, DuplicateKeyError> {
        if self.head().collected {
            return match &mut self.slots.as_mut_slice()[0].value {
                Value::Array(items) => {
                    items.0.push(slot.value);
                    Ok(Placement::Collected(items.0.len() - 1))
                }
                _ => Err(DuplicateKeyError {
                    key: key.to_owned(),
                }),
            };
        }
        let slots = std::mem::replace(&mut self.slots, Slots::Many(Vec::new()));
        let head = slots.into_first();
        let items = Array(vec![head.value, slot.value]);
        *self = Entry::from_slot(Slot::collection(Value::Array(items)));
        Ok(Placement::Collected(1))
    }

    /// Resolves a repeated key under [`DuplicateStrategy::Merge`] (spec §8.4), by the type of the
    /// first value. Only the first value takes part; any other values of the entry are kept
    /// (QUESTIONS.md #1).
    ///
    /// - Object and object: the new object's values are inserted into the first value one by one,
    ///   with `Merge`. The first value keeps its priority.
    /// - Array and array: the new elements are appended. The array keeps its priority.
    /// - Object and array, or array and object: error.
    /// - Object or array, and a scalar (**quirk**): the scalar takes the container's place and
    ///   keeps the container's priority and inherited mark.
    /// - Scalar first: resolved by priority as under `Append`. An inherited first value is not
    ///   replaced unconditionally here (QUESTIONS.md #2).
    fn merge(
        &mut self,
        key: &str,
        slot: Slot<'a>,
        flags: ParserFlags,
    ) -> Result<Placement, DuplicateKeyError> {
        let head = &mut self.slots.as_mut_slice()[0];
        if !is_container(&head.value) {
            return self.add_by_priority(key, slot, false, flags);
        }
        if !is_container(&slot.value) {
            head.value = slot.value;
            return Ok(Placement::Slot(0));
        }
        match (&mut head.value, slot.value) {
            (Value::Object(target), Value::Object(source)) => {
                for (name, entry) in source {
                    for inner in entry.slots.into_iter() {
                        target.insert_slot_with_strategy(
                            name.clone(),
                            inner,
                            DuplicateStrategy::Merge,
                            flags,
                        )?;
                    }
                }
                Ok(Placement::Merged)
            }
            (Value::Array(target), Value::Array(source)) => {
                target.0.extend(source);
                Ok(Placement::Merged)
            }
            _ => Err(DuplicateKeyError {
                key: key.to_owned(),
            }),
        }
    }
}

fn is_container(value: &Value<'_>) -> bool {
    matches!(value, Value::Object(_) | Value::Array(_))
}

/// Iterator over the values of an [`Entry`].
#[derive(Debug, Clone)]
pub struct Values<'v, 'a>(std::slice::Iter<'v, Slot<'a>>);

impl<'v, 'a> Iterator for Values<'v, 'a> {
    type Item = &'v Value<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(|s| &s.value)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}

impl DoubleEndedIterator for Values<'_, '_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.0.next_back().map(|s| &s.value)
    }
}

impl ExactSizeIterator for Values<'_, '_> {}

/// An explicit array (`[...]`): a vector of values, which it derefs to.
///
/// Dropping an array does not recurse (see [`Value`]).
#[derive(Clone, Default)]
pub struct Array<'a>(Vec<Value<'a>>);

impl fmt::Debug for Array<'_> {
    /// As the list of its values.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(&self.0).finish()
    }
}

impl<'a> Array<'a> {
    /// An empty array.
    pub fn new() -> Self {
        Self(Vec::new())
    }

    /// An empty array with room for `capacity` values.
    pub fn with_capacity(capacity: usize) -> Self {
        Self(Vec::with_capacity(capacity))
    }

    /// The values, as a vector.
    pub fn into_vec(mut self) -> Vec<Value<'a>> {
        std::mem::take(&mut self.0)
    }
}

impl Drop for Array<'_> {
    /// Without recursion: the containers inside go through a heap stack, which is allocated only
    /// when there is one.
    fn drop(&mut self) {
        let mut stack = Vec::new();
        take_nested_elements(&mut self.0, &mut stack);
        if !stack.is_empty() {
            drop_stack(stack);
        }
    }
}

impl<'a> std::ops::Deref for Array<'a> {
    type Target = Vec<Value<'a>>;

    fn deref(&self) -> &Vec<Value<'a>> {
        &self.0
    }
}

impl std::ops::DerefMut for Array<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<'a> From<Vec<Value<'a>>> for Array<'a> {
    fn from(values: Vec<Value<'a>>) -> Self {
        Self(values)
    }
}

impl<'a> From<Array<'a>> for Vec<Value<'a>> {
    fn from(array: Array<'a>) -> Self {
        array.into_vec()
    }
}

impl<'a> FromIterator<Value<'a>> for Array<'a> {
    fn from_iter<I: IntoIterator<Item = Value<'a>>>(iter: I) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl<'a> IntoIterator for Array<'a> {
    type Item = Value<'a>;
    type IntoIter = std::vec::IntoIter<Value<'a>>;

    fn into_iter(self) -> Self::IntoIter {
        self.into_vec().into_iter()
    }
}

impl<'v, 'a> IntoIterator for &'v Array<'a> {
    type Item = &'v Value<'a>;
    type IntoIter = std::slice::Iter<'v, Value<'a>>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl<'a> Extend<Value<'a>> for Array<'a> {
    fn extend<I: IntoIterator<Item = Value<'a>>>(&mut self, iter: I) {
        self.0.extend(iter);
    }
}

impl<'b> PartialEq<Vec<Value<'b>>> for Array<'_> {
    /// As arrays compare.
    fn eq(&self, other: &Vec<Value<'b>>) -> bool {
        let mut pending = Vec::new();
        pending.extend(self.0.iter().zip(other));
        self.0.len() == other.len() && equal_pending(pending)
    }
}

/// A UCL object: keys in insertion order, each with one or more values.
///
/// Equality ignores key order, like `IndexMap`; the order of values inside an entry matters.
/// Comparing and dropping do not recurse ([`Value`]).
#[derive(Clone, Default)]
pub struct Object<'a> {
    entries: Map<'a>,
}

impl fmt::Debug for Object<'_> {
    /// As the struct `UclObject` with its entries, the form of the owned objects of 0.3.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UclObject")
            .field("entries", &self.entries)
            .finish()
    }
}

impl Drop for Object<'_> {
    /// Without recursion, as an [`Array`] drops.
    fn drop(&mut self) {
        let mut stack = Vec::new();
        take_nested(self, &mut stack);
        if !stack.is_empty() {
            drop_stack(stack);
        }
    }
}

impl<'a> Object<'a> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            entries: Map::with_capacity(capacity),
        }
    }

    /// Number of keys.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.entries.get_index_of(key).is_some()
    }

    /// The entry of `key`, a [`KeyCopy`], which is expected at position `index`: found there by
    /// comparing bytes, without hashing the key or checking its UTF-8, when it is, and otherwise
    /// looked up by the key as a `str` (clean-room work item C13).
    pub(crate) fn entry_at_mut(&mut self, index: usize, key: &KeyCopy) -> Option<&mut Entry<'a>> {
        let index = match self.entries.get_index(index) {
            Some((k, _)) if k.as_bytes() == key.as_bytes() => index,
            _ => self.entries.get_index_of(key.as_str())?,
        };
        self.entries.get_index_mut(index).map(|(_, entry)| entry)
    }

    /// The first value of `key`, as libucl's `ucl_object_lookup` returns it.
    pub fn get(&self, key: &str) -> Option<&Value<'a>> {
        self.entries.get(key).map(Entry::first)
    }

    /// The first value of `key`, mutably.
    pub fn get_mut(&mut self, key: &str) -> Option<&mut Value<'a>> {
        self.entries.get_mut(key).map(Entry::first_mut)
    }

    /// Every value of `key` in insertion order; empty if the key is absent.
    pub fn get_all(&self, key: &str) -> Values<'_, 'a> {
        match self.entries.get(key) {
            Some(entry) => entry.values(),
            None => Values([].iter()),
        }
    }

    pub fn entry(&self, key: &str) -> Option<&Entry<'a>> {
        self.entries.get(key)
    }

    pub fn entry_mut(&mut self, key: &str) -> Option<&mut Entry<'a>> {
        self.entries.get_mut(key)
    }

    /// The key and entry at position `index` in insertion order.
    pub fn get_index(&self, index: usize) -> Option<(&Str<'a>, &Entry<'a>)> {
        self.entries.get_index(index)
    }

    /// The key and entry at position `index` in insertion order, the entry mutable.
    pub fn get_index_mut(&mut self, index: usize) -> Option<(&Str<'a>, &mut Entry<'a>)> {
        self.entries.get_index_mut(index)
    }

    /// The position of `key` in insertion order.
    pub fn index_of(&self, key: &str) -> Option<usize> {
        self.entries.get_index_of(key)
    }

    /// Where `key` is in this object, or would go, for [`Object::push_probed`]: a key that the
    /// parser looks up and then inserts is searched for, or hashed, once (clean-room work items
    /// C11 and C13).
    pub(crate) fn probe(&self, key: &str) -> Probe {
        self.entries.probe(key)
    }

    /// Adds `key`, which [`Object::probe`] found absent, with `entry` at the end.
    pub(crate) fn push_probed(&mut self, probe: Probe, key: Str<'a>, entry: Entry<'a>) {
        self.entries.push_probed(probe, key, entry);
    }

    /// Changes the spelling of key `old` to `new`, keeping its position and entry. Returns false
    /// if `old` is absent or `new` is another key already present.
    pub fn rename_key(&mut self, old: &str, new: impl Into<Str<'a>>) -> bool {
        match self.entries.get_index_of(old) {
            Some(index) => self.entries.replace_index(index, new.into()).is_ok(),
            None => false,
        }
    }

    /// Sets `key` to a single value, replacing every existing value. An existing key keeps its
    /// position.
    pub fn insert(&mut self, key: impl Into<Str<'a>>, value: Value<'a>) -> Option<Entry<'a>> {
        self.entries.insert(key.into(), Entry::new(value))
    }

    /// Sets `key` to `entry`, replacing an existing entry in place.
    pub fn insert_entry(&mut self, key: impl Into<Str<'a>>, entry: Entry<'a>) -> Option<Entry<'a>> {
        self.entries.insert(key.into(), entry)
    }

    /// Adds a value to `key`: a new key, or one more value of an existing key (implicit array).
    pub fn append(&mut self, key: impl Into<Str<'a>>, value: Value<'a>) {
        let key = key.into();
        match self.entries.get_mut(key.as_str()) {
            Some(entry) => entry.push(value),
            None => {
                self.entries.insert(key, Entry::new(value));
            }
        }
    }

    /// Removes `key`, keeping the order of the others.
    pub fn remove(&mut self, key: &str) -> Option<Entry<'a>> {
        self.entries.shift_remove(key)
    }

    /// Removes and returns the entry at `index`, keeping the order of the others.
    pub fn remove_index(&mut self, index: usize) -> Option<(Str<'a>, Entry<'a>)> {
        self.entries.shift_remove_index(index)
    }

    /// Keys and entries in insertion order.
    pub fn iter(&self) -> Iter<'_, 'a> {
        self.entries.iter()
    }

    /// Keys and entries in insertion order, entries mutable.
    pub fn iter_mut(&mut self) -> IterMut<'_, 'a> {
        self.entries.iter_mut()
    }

    pub fn keys(&self) -> Keys<'_, 'a> {
        Keys(self.entries.iter())
    }

    /// Entries in insertion order.
    pub fn entries(&self) -> Entries<'_, 'a> {
        Entries(self.entries.iter())
    }

    /// A copy of the object that borrows nothing ([`Value::owned_copy`]).
    pub(crate) fn owned_copy(&self) -> Object<'static> {
        let mut entries = Map::with_capacity(self.entries.len());
        for (key, entry) in self.entries.iter() {
            let slots = entry
                .slots
                .iter()
                .map(|slot| slot.with_value(slot.value.owned_copy()));
            entries.push_unique(key.clone().into_owned(), Entry::from_slots(slots));
        }
        Object { entries }
    }

    /// The object with every key and string it borrows copied ([`Value::into_owned`]).
    pub fn into_owned(self) -> Object<'static> {
        match Value::Object(self).into_owned() {
            Value::Object(object) => object,
            _ => unreachable!("an object stays an object"),
        }
    }

    /// Inserts `value` under `key`, resolving an existing key by `strategy` and `priority`,
    /// and applying `flags`. Behaviour is specified in `docs/spec/08-duplicates.md`; see
    /// [`Object::insert_slot_with_strategy`].
    pub fn insert_with_strategy(
        &mut self,
        key: impl Into<Str<'a>>,
        value: Value<'a>,
        priority: u8,
        strategy: DuplicateStrategy,
        flags: ParserFlags,
    ) -> Result<(), DuplicateKeyError> {
        self.insert_slot_with_strategy(key, Slot::new(value, priority), strategy, flags)
    }

    /// [`Object::insert_with_strategy`] for a prepared slot.
    ///
    /// Under [`ParserFlags::KEY_LOWERCASE`] the key is lowercased first, ASCII letters only
    /// (spec §8.6, §12.1). A new key is added at the end. A key already present keeps its
    /// position, and `strategy` decides the outcome (spec §8.2–§8.5):
    ///
    /// - [`DuplicateStrategy::Append`]: an inherited first value is replaced. Otherwise the new
    ///   priority is compared with the first value's: higher replaces the entry, lower is
    ///   dropped, equal adds the value (or collects it under [`ParserFlags::NO_IMPLICIT_ARRAYS`]).
    /// - [`DuplicateStrategy::Rewrite`]: the entry is replaced by the new value.
    /// - [`DuplicateStrategy::Error`]: [`DuplicateKeyError`], with the object unchanged.
    /// - [`DuplicateStrategy::Merge`]: containers are merged, and a scalar first value is
    ///   resolved as under `Append` (see the rules on `Entry::merge` in the source).
    ///
    /// Under `Merge`, an object is merged into entry by entry, so when a nested insert fails the
    /// entries before it have already been merged.
    pub fn insert_slot_with_strategy(
        &mut self,
        key: impl Into<Str<'a>>,
        slot: Slot<'a>,
        strategy: DuplicateStrategy,
        flags: ParserFlags,
    ) -> Result<(), DuplicateKeyError> {
        self.insert_slot_placed(key, slot, strategy, flags)
            .map(|_| ())
    }

    /// [`Object::insert_slot_with_strategy`], reporting where the value went.
    ///
    /// The key the value is stored under is `key`, lowercased under
    /// [`ParserFlags::KEY_LOWERCASE`].
    pub fn insert_slot_placed(
        &mut self,
        key: impl Into<Str<'a>>,
        slot: Slot<'a>,
        strategy: DuplicateStrategy,
        flags: ParserFlags,
    ) -> Result<Placement, DuplicateKeyError> {
        let mut key = key.into();
        if flags.contains(ParserFlags::KEY_LOWERCASE) && key.bytes().any(|b| b.is_ascii_uppercase())
        {
            key = Str::from(key.to_ascii_lowercase());
        }
        let index = self.entries.get_index_of(key.as_str());
        self.insert_slot_at(index, key, slot, strategy, flags)
    }

    /// [`Object::insert_slot_placed`] for a key that is already lowercased where
    /// `KEY_LOWERCASE` asks for it, and whose position `index` the caller has looked up (`None`
    /// for a new key), so that the key is hashed once. A new key is stored as `key`.
    pub(crate) fn insert_slot_at(
        &mut self,
        index: Option<usize>,
        key: impl AsRef<str> + Into<Str<'a>>,
        slot: Slot<'a>,
        strategy: DuplicateStrategy,
        flags: ParserFlags,
    ) -> Result<Placement, DuplicateKeyError> {
        let Some(index) = index else {
            self.entries.insert(key.into(), Entry::from_slot(slot));
            return Ok(Placement::Slot(0));
        };
        let (_, entry) = self
            .entries
            .get_index_mut(index)
            .expect("the caller found the key at this index");
        let key = key.as_ref();
        match strategy {
            DuplicateStrategy::Append => entry.add_by_priority(key, slot, true, flags),
            DuplicateStrategy::Merge => entry.merge(key, slot, flags),
            DuplicateStrategy::Rewrite => {
                *entry = Entry::from_slot(slot);
                Ok(Placement::Slot(0))
            }
            DuplicateStrategy::Error => Err(DuplicateKeyError {
                key: key.to_owned(),
            }),
        }
    }
}

impl<'a> std::ops::Index<&str> for Object<'a> {
    type Output = Value<'a>;

    /// The first value of `key`. Panics if the key is absent.
    fn index(&self, key: &str) -> &Value<'a> {
        self.get(key)
            .unwrap_or_else(|| panic!("key '{key}' not found in UCL object"))
    }
}

impl<'a> IntoIterator for Object<'a> {
    type Item = (Str<'a>, Entry<'a>);
    type IntoIter = IntoIter<'a>;

    fn into_iter(mut self) -> Self::IntoIter {
        std::mem::take(&mut self.entries).into_iter()
    }
}

impl<'v, 'a> IntoIterator for &'v Object<'a> {
    type Item = (&'v Str<'a>, &'v Entry<'a>);
    type IntoIter = Iter<'v, 'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.entries.iter()
    }
}

impl<'a, K: Into<Str<'a>>> FromIterator<(K, Value<'a>)> for Object<'a> {
    /// Collects pairs with [`Object::append`]: a repeated key becomes an implicit array.
    fn from_iter<I: IntoIterator<Item = (K, Value<'a>)>>(iter: I) -> Self {
        let mut object = Object::new();
        for (k, v) in iter {
            object.append(k, v);
        }
        object
    }
}

impl<'a, K: Into<Str<'a>>> Extend<(K, Value<'a>)> for Object<'a> {
    fn extend<I: IntoIterator<Item = (K, Value<'a>)>>(&mut self, iter: I) {
        for (k, v) in iter {
            self.append(k, v);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn int(i: i64) -> UclValue {
        UclValue::Integer(i)
    }

    fn values(obj: &UclObject, key: &str) -> Vec<UclValue> {
        obj.get_all(key).cloned().collect()
    }

    #[test]
    fn clone_keeps_order_priorities_and_marks() {
        let mut inner = UclObject::new();
        inner.insert_entry("z", Entry::from_slot(Slot::inherited(int(1), 3)));
        inner.append("y", UclValue::Time(1.5));
        inner.append("y", UclValue::Array(vec![int(2), UclValue::Null].into()));
        let mut obj = UclObject::new();
        obj.insert("b", UclValue::Object(inner));
        obj.insert_entry(
            "a",
            Entry::from_slot(Slot::collection(UclValue::Array(
                vec![
                    UclValue::String("s".into()),
                    UclValue::Array(vec![].into()),
                    UclValue::Object(UclObject::new()),
                ]
                .into(),
            ))),
        );
        obj.append("a", UclValue::Float(-0.0));
        let value = UclValue::Object(obj);
        let copy = value.clone();
        // Derived `PartialEq` compares every slot's priority and marks too.
        assert_eq!(copy, value);
        let (copy, value) = (copy.as_object().unwrap(), value.as_object().unwrap());
        assert_eq!(copy.keys().collect::<Vec<_>>(), ["b", "a"]);
        let inner = copy["b"].as_object().unwrap();
        assert_eq!(inner.keys().collect::<Vec<_>>(), ["z", "y"]);
        let z = &inner.entry("z").unwrap().slots()[0];
        assert_eq!((z.priority(), z.is_inherited()), (3, true));
        assert!(copy.entry("a").unwrap().slots()[0].is_collected());
        assert!(value.entry("a").unwrap().slots()[0].is_collected());
    }

    #[test]
    fn clone_does_not_recurse() {
        // Objects and arrays alternating, nested 20,000 deep: a derived clone overflows a
        // 2 MiB stack at about 550 levels in a debug build.
        std::thread::Builder::new()
            .stack_size(2 << 20)
            .spawn(|| {
                let mut value = int(1);
                for depth in 0..20_000 {
                    value = if depth % 2 == 0 {
                        UclValue::Array(vec![value].into())
                    } else {
                        UclValue::Object([("k", value)].into_iter().collect())
                    };
                }
                let copy = value.clone();
                assert_eq!(nesting(&copy), 20_000);
                // Comparing does not recurse either.
                assert!(copy == value);
                // Dropping recurses; it is not under test.
                std::mem::forget((value, copy));
            })
            .unwrap()
            .join()
            .unwrap();
    }

    #[test]
    fn equality_is_that_of_the_derived_comparisons() {
        let obj = |entries: Vec<(&str, Vec<Slot<'static>>)>| {
            let mut o = UclObject::new();
            for (key, slots) in entries {
                let mut slots = slots.into_iter();
                let mut entry = Entry::from_slot(slots.next().unwrap());
                slots.for_each(|slot| entry.push_slot(slot));
                o.insert_entry(key.to_string(), entry);
            }
            UclValue::Object(o)
        };
        let s = |v: UclValue| Slot::new(v, 0);
        let a = obj(vec![
            ("x", vec![s(int(1))]),
            ("y", vec![s(int(2)), s(int(3))]),
        ]);
        // Keys in any order; values of an entry in order.
        let b = obj(vec![
            ("y", vec![s(int(2)), s(int(3))]),
            ("x", vec![s(int(1))]),
        ]);
        assert!(a == b);
        let c = obj(vec![
            ("x", vec![s(int(1))]),
            ("y", vec![s(int(3)), s(int(2))]),
        ]);
        assert!(a != c);
        // Priority and marks count.
        let d = obj(vec![
            ("x", vec![Slot::new(int(1), 2)]),
            ("y", vec![s(int(2)), s(int(3))]),
        ]);
        assert!(a != d);
        let e = obj(vec![
            ("x", vec![Slot::inherited(int(1), 0)]),
            ("y", vec![s(int(2)), s(int(3))]),
        ]);
        assert!(a != e);
        let f = obj(vec![
            ("x", vec![Slot::collection(int(1))]),
            ("y", vec![s(int(2)), s(int(3))]),
        ]);
        assert!(a != f);
        // A missing key, an extra one, a different kind.
        assert!(a != obj(vec![("x", vec![s(int(1))])]));
        assert!(obj(vec![("x", vec![s(int(1))])]) != a);
        assert!(int(1) != UclValue::Float(1.0));
        assert!(UclValue::Float(1.0) != UclValue::Time(1.0));
        assert!(UclValue::Time(1.5) == UclValue::Time(1.5));
        // A NaN equals nothing, itself included.
        assert!(UclValue::Float(f64::NAN) != UclValue::Float(f64::NAN));
        assert!(
            UclValue::Array(vec![UclValue::Float(f64::NAN)].into())
                != UclValue::Array(vec![UclValue::Float(f64::NAN)].into())
        );
        // Arrays by length and element.
        let arr = |items: Vec<UclValue>| UclValue::Array(items.into());
        assert!(arr(vec![int(1), arr(vec![])]) == arr(vec![int(1), arr(vec![])]));
        assert!(arr(vec![int(1)]) != arr(vec![int(1), int(1)]));
        assert!(arr(vec![arr(vec![int(1)])]) != arr(vec![arr(vec![int(2)])]));
        // The object, entry and slot comparisons agree with the value's.
        let (UclValue::Object(x), UclValue::Object(y)) = (&a, &b) else {
            unreachable!()
        };
        assert!(x == y);
        assert!(x.entry("y") == y.entry("y"));
        assert!(x.entry("x") != y.entry("y"));
        assert!(x.entry("y").unwrap().slots()[0] == y.entry("y").unwrap().slots()[0]);
        assert!(x.entry("y").unwrap().slots()[0] != y.entry("y").unwrap().slots()[1]);
    }

    fn insert(obj: &mut UclObject, key: &str, v: UclValue, pri: u8, s: DuplicateStrategy) {
        obj.insert_with_strategy(key, v, pri, s, ParserFlags::DEFAULT)
            .unwrap();
    }

    #[test]
    fn test_append_equal_priority_makes_implicit_array() {
        let mut obj = UclObject::new();
        insert(&mut obj, "k", int(1), 0, DuplicateStrategy::Append);
        insert(&mut obj, "k", int(2), 0, DuplicateStrategy::Append);
        assert_eq!(values(&obj, "k"), vec![int(1), int(2)]);
        assert!(obj.entry("k").unwrap().is_multi());
        assert_eq!(obj.get("k"), Some(&int(1)));
    }

    #[test]
    fn test_explicit_array_is_not_flattened() {
        // libucl: `k = [1,2]; k = 3` is two values, the first an explicit array.
        let mut obj = UclObject::new();
        insert(
            &mut obj,
            "k",
            UclValue::Array(vec![int(1), int(2)].into()),
            0,
            DuplicateStrategy::Append,
        );
        insert(&mut obj, "k", int(3), 0, DuplicateStrategy::Append);
        assert_eq!(
            values(&obj, "k"),
            vec![UclValue::Array(vec![int(1), int(2)].into()), int(3)]
        );
    }

    #[test]
    fn test_append_keeps_key_position() {
        let mut obj = UclObject::new();
        insert(&mut obj, "a", int(1), 0, DuplicateStrategy::Append);
        insert(&mut obj, "b", int(2), 0, DuplicateStrategy::Append);
        insert(&mut obj, "a", int(3), 0, DuplicateStrategy::Append);
        assert_eq!(obj.keys().collect::<Vec<_>>(), vec!["a", "b"]);
    }

    #[test]
    fn entry_at_mut_finds_a_key_at_its_position_or_elsewhere() {
        // A short ASCII key, a short non-ASCII one and a long one: inline and heap copies.
        let keys = ["a", "é-ü", "a key longer than twenty-two bytes"];
        let mut obj = UclObject::new();
        for (i, key) in keys.iter().enumerate() {
            obj.insert(*key, int(i as i64));
        }
        for (i, key) in keys.iter().enumerate() {
            let copy = KeyCopy::from(*key);
            // At its position, at another key's, and past the end.
            for index in [i, (i + 1) % keys.len(), keys.len()] {
                let entry = obj.entry_at_mut(index, &copy);
                assert_eq!(
                    entry.map(|e| e.first()),
                    Some(&int(i as i64)),
                    "{key:?} {index}"
                );
            }
        }
        for absent in ["b", "é", "a key longer than twenty-two bytes!"] {
            let copy = KeyCopy::from(absent);
            for index in 0..=keys.len() {
                assert!(
                    obj.entry_at_mut(index, &copy).is_none(),
                    "{absent:?} {index}"
                );
            }
        }
        assert!(
            UclObject::new()
                .entry_at_mut(0, &KeyCopy::from("a"))
                .is_none()
        );
    }

    #[test]
    fn test_append_priorities() {
        let mut obj = UclObject::new();
        insert(&mut obj, "k", int(1), 2, DuplicateStrategy::Append);
        // lower priority: dropped
        insert(&mut obj, "k", int(2), 1, DuplicateStrategy::Append);
        assert_eq!(values(&obj, "k"), vec![int(1)]);
        // higher priority: replaces the whole entry
        insert(&mut obj, "k", int(3), 0, DuplicateStrategy::Append);
        insert(&mut obj, "k", int(4), 5, DuplicateStrategy::Append);
        assert_eq!(values(&obj, "k"), vec![int(4)]);
        assert_eq!(obj.entry("k").unwrap().slots()[0].priority(), 5);
    }

    #[test]
    fn test_priority_is_masked_to_four_bits() {
        assert_eq!(Slot::new(UclValue::Null, 0x1f).priority(), 0x0f);
    }

    #[test]
    fn test_inherited_value_is_always_replaced() {
        let mut obj = UclObject::new();
        obj.insert_slot_with_strategy(
            "k",
            Slot::inherited(int(1), 3),
            DuplicateStrategy::Append,
            ParserFlags::DEFAULT,
        )
        .unwrap();
        insert(&mut obj, "k", int(2), 0, DuplicateStrategy::Append);
        assert_eq!(values(&obj, "k"), vec![int(2)]);
    }

    #[test]
    fn test_rewrite_and_error() {
        let mut obj = UclObject::new();
        insert(&mut obj, "k", int(1), 5, DuplicateStrategy::Rewrite);
        insert(&mut obj, "k", int(2), 0, DuplicateStrategy::Rewrite);
        assert_eq!(values(&obj, "k"), vec![int(2)]);

        let err = obj
            .insert_with_strategy(
                "k",
                int(3),
                0,
                DuplicateStrategy::Error,
                ParserFlags::DEFAULT,
            )
            .unwrap_err();
        assert_eq!(
            err,
            DuplicateKeyError {
                key: "k".to_string()
            }
        );
        assert_eq!(values(&obj, "k"), vec![int(2)]);
    }

    #[test]
    fn test_merge_objects_and_arrays() {
        let mut first = UclObject::new();
        first.insert("a", int(1));
        let mut second = UclObject::new();
        second.insert("b", int(2));
        second.insert("a", int(3));

        let mut obj = UclObject::new();
        insert(
            &mut obj,
            "o",
            UclValue::Object(first),
            0,
            DuplicateStrategy::Merge,
        );
        insert(
            &mut obj,
            "o",
            UclValue::Object(second),
            0,
            DuplicateStrategy::Merge,
        );
        let merged = obj.get("o").unwrap().as_object().unwrap();
        assert_eq!(values(merged, "a"), vec![int(1), int(3)]);
        assert_eq!(values(merged, "b"), vec![int(2)]);
        assert_eq!(obj.entry("o").unwrap().len(), 1);

        insert(
            &mut obj,
            "arr",
            UclValue::Array(vec![int(1)].into()),
            0,
            DuplicateStrategy::Merge,
        );
        insert(
            &mut obj,
            "arr",
            UclValue::Array(vec![int(2)].into()),
            0,
            DuplicateStrategy::Merge,
        );
        assert_eq!(
            obj.get("arr"),
            Some(&UclValue::Array(vec![int(1), int(2)].into()))
        );
    }

    #[test]
    fn test_no_implicit_arrays_collects_into_one_explicit_array() {
        let flags = ParserFlags::NO_IMPLICIT_ARRAYS;
        let mut obj = UclObject::new();
        for i in 1..=3 {
            obj.insert_with_strategy("k", int(i), 0, DuplicateStrategy::Append, flags)
                .unwrap();
        }
        assert_eq!(
            values(&obj, "k"),
            vec![UclValue::Array(vec![int(1), int(2), int(3)].into())]
        );

        // A user-written array is not the collection array: it becomes its first element.
        let mut obj = UclObject::new();
        let written = UclValue::Array(vec![int(1), int(2)].into());
        obj.insert_with_strategy("k", written.clone(), 0, DuplicateStrategy::Append, flags)
            .unwrap();
        obj.insert_with_strategy("k", int(3), 0, DuplicateStrategy::Append, flags)
            .unwrap();
        assert_eq!(
            values(&obj, "k"),
            vec![UclValue::Array(vec![written, int(3)].into())]
        );
    }

    #[test]
    fn test_key_lowercase_flag() {
        let mut obj = UclObject::new();
        obj.insert_with_strategy(
            "ALIAS",
            int(1),
            0,
            DuplicateStrategy::Append,
            ParserFlags::KEY_LOWERCASE,
        )
        .unwrap();
        assert!(obj.contains_key("alias"));
        assert!(!obj.contains_key("ALIAS"));
    }

    fn object(pairs: &[(&str, UclValue)]) -> UclValue {
        UclValue::Object(pairs.iter().cloned().collect())
    }

    fn priorities(obj: &UclObject, key: &str) -> Vec<u8> {
        obj.entry(key)
            .unwrap()
            .slots()
            .iter()
            .map(Slot::priority)
            .collect()
    }

    fn insert_slot(obj: &mut UclObject, key: &str, slot: Slot<'static>, s: DuplicateStrategy) {
        obj.insert_slot_with_strategy(key, slot, s, ParserFlags::DEFAULT)
            .unwrap();
    }

    #[test]
    fn test_rewrite_takes_the_new_priority() {
        let mut obj = UclObject::new();
        insert(&mut obj, "k", int(1), 3, DuplicateStrategy::Rewrite);
        insert(&mut obj, "k", int(2), 1, DuplicateStrategy::Rewrite);
        assert_eq!(values(&obj, "k"), vec![int(2)]);
        assert_eq!(priorities(&obj, "k"), vec![1]);
    }

    #[test]
    fn test_error_strategy_rejects_repeat_of_inherited_value() {
        let mut obj = UclObject::new();
        insert_slot(
            &mut obj,
            "k",
            Slot::inherited(int(1), 0),
            DuplicateStrategy::Error,
        );
        let err = obj
            .insert_with_strategy(
                "k",
                int(2),
                0,
                DuplicateStrategy::Error,
                ParserFlags::DEFAULT,
            )
            .unwrap_err();
        assert_eq!(err.key, "k");
        assert_eq!(values(&obj, "k"), vec![int(1)]);
    }

    #[test]
    fn test_merge_container_then_scalar_keeps_container_priority() {
        // spec §8.4, strategy_merge_scalar_keeps_container_priority
        let mut obj = UclObject::new();
        insert(
            &mut obj,
            "a",
            object(&[("x", int(1))]),
            3,
            DuplicateStrategy::Merge,
        );
        insert(&mut obj, "a", int(2), 1, DuplicateStrategy::Merge);
        assert_eq!(values(&obj, "a"), vec![int(2)]);
        assert_eq!(priorities(&obj, "a"), vec![3]);

        insert(
            &mut obj,
            "b",
            UclValue::Array(vec![int(1)].into()),
            1,
            DuplicateStrategy::Merge,
        );
        insert(&mut obj, "b", int(2), 3, DuplicateStrategy::Merge);
        assert_eq!(values(&obj, "b"), vec![int(2)]);
        assert_eq!(priorities(&obj, "b"), vec![1]);
    }

    #[test]
    fn test_merge_container_type_mismatch_is_an_error() {
        let arr = UclValue::Array(vec![int(1)].into());
        let obj_value = object(&[("x", int(1))]);
        for (first, second) in [(arr.clone(), obj_value.clone()), (obj_value, arr)] {
            let mut obj = UclObject::new();
            insert(&mut obj, "a", first, 0, DuplicateStrategy::Merge);
            let err = obj
                .insert_with_strategy(
                    "a",
                    second,
                    0,
                    DuplicateStrategy::Merge,
                    ParserFlags::DEFAULT,
                )
                .unwrap_err();
            assert_eq!(err.key, "a");
        }
    }

    #[test]
    fn test_merge_arrays_keep_the_existing_priority() {
        // spec §8.4, strategy_merge_arrays_ignore_priority
        let mut obj = UclObject::new();
        insert(
            &mut obj,
            "a",
            UclValue::Array(vec![int(1)].into()),
            3,
            DuplicateStrategy::Merge,
        );
        insert(
            &mut obj,
            "a",
            UclValue::Array(vec![int(2)].into()),
            1,
            DuplicateStrategy::Merge,
        );
        assert_eq!(
            values(&obj, "a"),
            vec![UclValue::Array(vec![int(1), int(2)].into())]
        );
        assert_eq!(priorities(&obj, "a"), vec![3]);
    }

    #[test]
    fn test_merge_nested_values_resolve_by_their_own_priority() {
        // spec §8.4, include_merge_lower_priority_still_merges; a nested scalar with a higher
        // priority replaces (oracle probe, QUESTIONS.md #1).
        let mut inner = UclObject::new();
        insert(&mut inner, "a", int(1), 3, DuplicateStrategy::Append);
        let mut obj = UclObject::new();
        insert(
            &mut obj,
            "x",
            UclValue::Object(inner),
            3,
            DuplicateStrategy::Append,
        );

        let mut lower = UclObject::new();
        insert(&mut lower, "b", int(2), 1, DuplicateStrategy::Append);
        insert(
            &mut obj,
            "x",
            UclValue::Object(lower),
            1,
            DuplicateStrategy::Merge,
        );
        let mut higher = UclObject::new();
        insert(&mut higher, "a", int(9), 5, DuplicateStrategy::Append);
        insert(
            &mut obj,
            "x",
            UclValue::Object(higher),
            5,
            DuplicateStrategy::Merge,
        );

        assert_eq!(priorities(&obj, "x"), vec![3]);
        let merged = obj.get("x").unwrap().as_object().unwrap();
        assert_eq!(merged.keys().collect::<Vec<_>>(), vec!["a", "b"]);
        assert_eq!(values(merged, "a"), vec![int(9)]);
        assert_eq!(priorities(merged, "a"), vec![5]);
        assert_eq!(priorities(merged, "b"), vec![1]);
    }

    #[test]
    fn test_merge_uses_only_the_first_value() {
        // Oracle probe, QUESTIONS.md #1: other values of the entry are kept.
        let mut obj = UclObject::new();
        insert(
            &mut obj,
            "a",
            object(&[("x", int(1))]),
            0,
            DuplicateStrategy::Append,
        );
        insert(
            &mut obj,
            "a",
            object(&[("y", int(2))]),
            0,
            DuplicateStrategy::Append,
        );
        insert(
            &mut obj,
            "a",
            object(&[("z", int(3))]),
            0,
            DuplicateStrategy::Merge,
        );
        assert_eq!(
            values(&obj, "a"),
            vec![
                object(&[("x", int(1)), ("z", int(3))]),
                object(&[("y", int(2))])
            ]
        );
        insert(&mut obj, "a", int(5), 0, DuplicateStrategy::Merge);
        assert_eq!(values(&obj, "a"), vec![int(5), object(&[("y", int(2))])]);
    }

    #[test]
    fn test_merge_scalar_first_follows_append_priorities() {
        // spec §8.4, strategy_merge_scalars_append, strategy_merge_scalar_then_object
        let mut obj = UclObject::new();
        insert(&mut obj, "a", int(1), 2, DuplicateStrategy::Merge);
        insert(
            &mut obj,
            "a",
            object(&[("y", int(2))]),
            2,
            DuplicateStrategy::Merge,
        );
        insert(&mut obj, "a", int(3), 1, DuplicateStrategy::Merge);
        assert_eq!(values(&obj, "a"), vec![int(1), object(&[("y", int(2))])]);
        insert(&mut obj, "a", int(4), 5, DuplicateStrategy::Merge);
        assert_eq!(values(&obj, "a"), vec![int(4)]);
    }

    #[test]
    fn test_merge_does_not_replace_inherited_values() {
        // Oracle probes, QUESTIONS.md #2: an inherited scalar gets another value, an inherited
        // object is merged into and stays inherited.
        let mut obj = UclObject::new();
        insert_slot(
            &mut obj,
            "s",
            Slot::inherited(int(1), 0),
            DuplicateStrategy::Append,
        );
        insert(&mut obj, "s", int(2), 0, DuplicateStrategy::Merge);
        assert_eq!(values(&obj, "s"), vec![int(1), int(2)]);

        let inherited = Slot::inherited(object(&[("x", int(1))]), 0);
        insert_slot(&mut obj, "o", inherited, DuplicateStrategy::Append);
        insert(
            &mut obj,
            "o",
            object(&[("y", int(2))]),
            0,
            DuplicateStrategy::Merge,
        );
        assert_eq!(
            values(&obj, "o"),
            vec![object(&[("x", int(1)), ("y", int(2))])]
        );
        insert(
            &mut obj,
            "o",
            object(&[("z", int(3))]),
            0,
            DuplicateStrategy::Append,
        );
        assert_eq!(values(&obj, "o"), vec![object(&[("z", int(3))])]);
    }

    #[test]
    fn test_no_implicit_arrays_priorities() {
        // Oracle probes, QUESTIONS.md #3: priorities are compared first; the collection array
        // has priority 0, so a later value with a higher priority replaces it.
        let flags = ParserFlags::NO_IMPLICIT_ARRAYS;
        let mut obj = UclObject::new();
        for (v, pri) in [(1, 0), (2, 3), (3, 1), (4, 3)] {
            obj.insert_with_strategy("a", int(v), pri, DuplicateStrategy::Append, flags)
                .unwrap();
        }
        assert_eq!(
            values(&obj, "a"),
            vec![UclValue::Array(vec![int(2), int(4)].into())]
        );
        assert_eq!(priorities(&obj, "a"), vec![0]);
        obj.insert_with_strategy("a", int(5), 3, DuplicateStrategy::Append, flags)
            .unwrap();
        assert_eq!(values(&obj, "a"), vec![int(5)]);
        assert_eq!(priorities(&obj, "a"), vec![3]);

        // An inherited value is replaced, not collected.
        let mut obj = UclObject::new();
        obj.insert_slot_with_strategy(
            "a",
            Slot::inherited(int(1), 0),
            DuplicateStrategy::Append,
            flags,
        )
        .unwrap();
        obj.insert_with_strategy("a", int(2), 0, DuplicateStrategy::Append, flags)
            .unwrap();
        assert_eq!(values(&obj, "a"), vec![int(2)]);
    }

    #[test]
    fn test_no_implicit_arrays_under_merge() {
        // Oracle probes, QUESTIONS.md #4: the collection array is an ordinary array for `merge`.
        let flags = ParserFlags::NO_IMPLICIT_ARRAYS;
        let merge = DuplicateStrategy::Merge;
        let mut obj = UclObject::new();
        for v in [int(1), int(2), UclValue::Array(vec![int(3)].into())] {
            obj.insert_with_strategy("b", v, 0, merge, flags).unwrap();
        }
        assert_eq!(
            values(&obj, "b"),
            vec![UclValue::Array(vec![int(1), int(2), int(3)].into())]
        );
        obj.insert_with_strategy("b", int(5), 0, merge, flags)
            .unwrap();
        assert_eq!(values(&obj, "b"), vec![int(5)]);
        // The scalar took the collection array's place; a later repeat is an error.
        assert!(
            obj.insert_with_strategy("b", int(6), 0, DuplicateStrategy::Append, flags)
                .is_err()
        );
    }

    #[test]
    fn test_key_lowercase_merges_keys_that_differ_in_case() {
        // spec §8.6, §12.1: ASCII letters only.
        let flags = ParserFlags::KEY_LOWERCASE;
        let mut obj = UclObject::new();
        for key in ["A", "a", "É"] {
            obj.insert_with_strategy(key, int(1), 0, DuplicateStrategy::Append, flags)
                .unwrap();
        }
        assert_eq!(obj.keys().collect::<Vec<_>>(), vec!["a", "É"]);
        assert_eq!(values(&obj, "a"), vec![int(1), int(1)]);
    }

    #[test]
    fn test_entry_into_value_views_implicit_array_as_sequence() {
        let mut entry = Entry::new(int(1));
        assert_eq!(entry.clone().into_value(), int(1));
        entry.push(int(2));
        assert_eq!(
            entry.into_value(),
            UclValue::Array(vec![int(1), int(2)].into())
        );
    }

    #[test]
    fn test_insert_slot_placed_reports_where_the_value_went() {
        let append = DuplicateStrategy::Append;
        let flags = ParserFlags::DEFAULT;
        let mut obj = UclObject::new();
        let place = |obj: &mut UclObject, v, pri, s, f| {
            obj.insert_slot_placed("k", Slot::new(v, pri), s, f)
                .unwrap()
        };
        assert_eq!(
            place(&mut obj, int(1), 1, append, flags),
            Placement::Slot(0)
        );
        assert_eq!(
            place(&mut obj, int(2), 1, append, flags),
            Placement::Slot(1)
        );
        assert_eq!(
            place(&mut obj, int(3), 0, append, flags),
            Placement::Dropped
        );
        assert_eq!(
            place(&mut obj, int(4), 2, append, flags),
            Placement::Slot(0)
        );

        let merge = DuplicateStrategy::Merge;
        let empty = || UclValue::Object(UclObject::new());
        let mut obj = UclObject::new();
        assert_eq!(
            place(&mut obj, empty(), 0, merge, flags),
            Placement::Slot(0)
        );
        assert_eq!(place(&mut obj, empty(), 0, merge, flags), Placement::Merged);

        let nia = ParserFlags::NO_IMPLICIT_ARRAYS;
        let mut obj = UclObject::new();
        assert_eq!(place(&mut obj, int(1), 0, append, nia), Placement::Slot(0));
        assert_eq!(
            place(&mut obj, empty(), 0, append, nia),
            Placement::Collected(1)
        );
        assert_eq!(
            place(&mut obj, empty(), 0, append, nia),
            Placement::Collected(2)
        );
        assert!(
            obj.entry_mut("k")
                .unwrap()
                .value_at_mut(0)
                .unwrap()
                .is_array()
        );
    }

    #[test]
    fn test_parser_flags_match_libucl_bits() {
        assert_eq!(ParserFlags::KEY_LOWERCASE.bits(), 1);
        assert_eq!(ParserFlags::NO_FILEVARS.bits(), 64);
        let flags = ParserFlags::NO_TIME | ParserFlags::DISABLE_MACRO;
        assert!(flags.contains(ParserFlags::NO_TIME));
        assert!(!flags.contains(ParserFlags::ZEROCOPY));
        assert_eq!(ParserFlags::from_bits_truncate(0xffff).bits(), 127);
    }
}
