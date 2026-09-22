//! UCL value model (PLAN.md §2.2).
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

use indexmap::IndexMap;
use smallvec::SmallVec;
use std::fmt;

/// Explicit array (`[...]`).
pub type UclArray = Vec<UclValue>;

/// A UCL value.
#[derive(Debug, Clone, PartialEq)]
pub enum UclValue {
    Object(UclObject),
    Array(UclArray),
    Integer(i64),
    Float(f64),
    /// Time in seconds (`10s`, `5min`, `10ms`): libucl's `UCL_TIME`.
    Time(f64),
    String(String),
    Boolean(bool),
    Null,
}

impl UclValue {
    /// Returns true if the value is an object
    pub fn is_object(&self) -> bool {
        matches!(self, UclValue::Object(_))
    }

    /// Returns true if the value is an explicit array
    pub fn is_array(&self) -> bool {
        matches!(self, UclValue::Array(_))
    }

    /// Returns true if the value is a string
    pub fn is_string(&self) -> bool {
        matches!(self, UclValue::String(_))
    }

    /// Returns true if the value is a time
    pub fn is_time(&self) -> bool {
        matches!(self, UclValue::Time(_))
    }

    /// Returns true if this is a Null variant
    pub fn is_null(&self) -> bool {
        matches!(self, UclValue::Null)
    }

    /// Returns the object if this is an Object variant
    pub fn as_object(&self) -> Option<&UclObject> {
        match self {
            UclValue::Object(obj) => Some(obj),
            _ => None,
        }
    }

    /// Returns the object mutably if this is an Object variant
    pub fn as_object_mut(&mut self) -> Option<&mut UclObject> {
        match self {
            UclValue::Object(obj) => Some(obj),
            _ => None,
        }
    }

    /// Returns the array if this is an Array variant
    pub fn as_array(&self) -> Option<&UclArray> {
        match self {
            UclValue::Array(arr) => Some(arr),
            _ => None,
        }
    }

    /// Returns the array mutably if this is an Array variant
    pub fn as_array_mut(&mut self) -> Option<&mut UclArray> {
        match self {
            UclValue::Array(arr) => Some(arr),
            _ => None,
        }
    }

    /// Returns the string if this is a String variant
    pub fn as_str(&self) -> Option<&str> {
        match self {
            UclValue::String(s) => Some(s),
            _ => None,
        }
    }

    /// Returns the integer if this is an Integer variant
    pub fn as_integer(&self) -> Option<i64> {
        match self {
            UclValue::Integer(i) => Some(*i),
            _ => None,
        }
    }

    /// Returns the float if this is a Float variant
    pub fn as_float(&self) -> Option<f64> {
        match self {
            UclValue::Float(f) => Some(*f),
            _ => None,
        }
    }

    /// Returns the number of seconds if this is a Time variant
    pub fn as_time(&self) -> Option<f64> {
        match self {
            UclValue::Time(t) => Some(*t),
            _ => None,
        }
    }

    /// Returns the boolean if this is a Boolean variant
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            UclValue::Boolean(b) => Some(*b),
            _ => None,
        }
    }

    /// Name of the value's type, as libucl spells it (`ucl_object_type_to_string`).
    pub fn type_name(&self) -> &'static str {
        match self {
            UclValue::Object(_) => "object",
            UclValue::Array(_) => "array",
            UclValue::Integer(_) => "int",
            UclValue::Float(_) => "float",
            UclValue::Time(_) => "time",
            UclValue::String(_) => "string",
            UclValue::Boolean(_) => "boolean",
            UclValue::Null => "null",
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
    /// Treat macros as comments.
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

/// Returned by [`UclObject::insert_with_strategy`] under [`DuplicateStrategy::Error`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateKeyError {
    pub key: String,
}

impl fmt::Display for DuplicateKeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "duplicate element for key '{}' found", self.key)
    }
}

impl std::error::Error for DuplicateKeyError {}

/// Highest libucl priority: only the 4 least significant bits are used.
pub const MAX_PRIORITY: u8 = 0x0f;

/// One value of an [`Entry`], with its priority and `.inherit` flag.
#[derive(Debug, Clone, PartialEq)]
pub struct Slot {
    value: UclValue,
    priority: u8,
    inherited: bool,
    /// Set on an explicit array built from repeated keys under `NO_IMPLICIT_ARRAYS`.
    collected: bool,
}

impl Slot {
    /// A value with the given priority (masked to 0–15).
    pub fn new(value: UclValue, priority: u8) -> Self {
        Self {
            value,
            priority: priority & MAX_PRIORITY,
            inherited: false,
            collected: false,
        }
    }

    /// A value copied from another object by `.inherit`.
    #[cfg_attr(not(test), allow(dead_code))] // used by `.inherit` (PLAN.md P4.5)
    pub(crate) fn inherited(value: UclValue, priority: u8) -> Self {
        Self {
            inherited: true,
            ..Self::new(value, priority)
        }
    }

    pub fn value(&self) -> &UclValue {
        &self.value
    }

    pub fn value_mut(&mut self) -> &mut UclValue {
        &mut self.value
    }

    pub fn into_value(self) -> UclValue {
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
}

/// The values of one key: always at least one. More than one is an implicit array.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    slots: SmallVec<[Slot; 1]>,
}

impl Entry {
    /// An entry with one value at priority 0.
    pub fn new(value: UclValue) -> Self {
        Self::from_slot(Slot::new(value, 0))
    }

    /// An entry with one slot.
    pub fn from_slot(slot: Slot) -> Self {
        let mut slots = SmallVec::new();
        slots.push(slot);
        Self { slots }
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
    pub fn first(&self) -> &UclValue {
        &self.slots[0].value
    }

    /// The first value, mutably.
    pub fn first_mut(&mut self) -> &mut UclValue {
        &mut self.slots[0].value
    }

    /// The last value.
    pub fn last(&self) -> &UclValue {
        &self.slots[self.slots.len() - 1].value
    }

    /// All values in insertion order.
    pub fn values(&self) -> Values<'_> {
        Values(self.slots.iter())
    }

    /// All values with their priority and flags.
    pub fn slots(&self) -> &[Slot] {
        &self.slots
    }

    /// Adds a value to the entry at priority 0, making it an implicit array.
    pub fn push(&mut self, value: UclValue) {
        self.slots.push(Slot::new(value, 0));
    }

    /// Adds a slot to the entry.
    pub fn push_slot(&mut self, slot: Slot) {
        self.slots.push(slot);
    }

    /// The single value, or an explicit array of all values for an implicit array.
    ///
    /// This is the view serde and JSON-like consumers get: a repeated key reads as a sequence.
    pub fn into_value(self) -> UclValue {
        if self.slots.len() == 1 {
            self.slots.into_iter().next().unwrap().value
        } else {
            UclValue::Array(self.slots.into_iter().map(|s| s.value).collect())
        }
    }

    /// All values in insertion order.
    pub fn into_values(self) -> impl Iterator<Item = UclValue> {
        self.slots.into_iter().map(|s| s.value)
    }

    fn head(&self) -> &Slot {
        &self.slots[0]
    }

    /// Resolves a repeated key by priority (spec §8.3), comparing with the first value: a higher
    /// priority replaces the entry, a lower one is dropped, and an equal one is added (§8.2), or
    /// collected into an explicit array under `NO_IMPLICIT_ARRAYS` (§8.5). With
    /// `replace_inherited`, an inherited first value is replaced whatever the priorities.
    fn add_by_priority(
        &mut self,
        key: &str,
        slot: Slot,
        replace_inherited: bool,
        flags: ParserFlags,
    ) -> Result<(), DuplicateKeyError> {
        let head = self.head();
        if (replace_inherited && head.inherited) || slot.priority > head.priority {
            *self = Entry::from_slot(slot);
        } else if slot.priority == head.priority {
            if flags.contains(ParserFlags::NO_IMPLICIT_ARRAYS) {
                return self.collect(key, slot);
            }
            self.slots.push(slot);
        }
        Ok(())
    }

    /// Adds a repeated value under `NO_IMPLICIT_ARRAYS` (spec §8.5). The first repeat replaces the
    /// entry with an explicit array of the old and new values; later repeats are appended to it.
    ///
    /// Two details follow the oracle rather than the spec text (QUESTIONS.md #3, #4):
    /// - The collection array has priority 0, whatever its elements' priorities, so a later value
    ///   with a priority above 0 replaces it.
    /// - If `merge` has replaced the collection array with a scalar, a later repeat is an error.
    fn collect(&mut self, key: &str, slot: Slot) -> Result<(), DuplicateKeyError> {
        if self.head().collected {
            return match &mut self.slots[0].value {
                UclValue::Array(items) => {
                    items.push(slot.value);
                    Ok(())
                }
                _ => Err(DuplicateKeyError {
                    key: key.to_owned(),
                }),
            };
        }
        let mut items: UclArray = std::mem::take(&mut self.slots)
            .into_iter()
            .map(|s| s.value)
            .collect();
        items.push(slot.value);
        *self = Entry::from_slot(Slot {
            collected: true,
            ..Slot::new(UclValue::Array(items), 0)
        });
        Ok(())
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
        slot: Slot,
        flags: ParserFlags,
    ) -> Result<(), DuplicateKeyError> {
        let head = &mut self.slots[0];
        if !is_container(&head.value) {
            return self.add_by_priority(key, slot, false, flags);
        }
        if !is_container(&slot.value) {
            head.value = slot.value;
            return Ok(());
        }
        match (&mut head.value, slot.value) {
            (UclValue::Object(target), UclValue::Object(source)) => {
                for (name, entry) in source {
                    for inner in entry.slots {
                        target.insert_slot_with_strategy(
                            name.as_str(),
                            inner,
                            DuplicateStrategy::Merge,
                            flags,
                        )?;
                    }
                }
                Ok(())
            }
            (UclValue::Array(target), UclValue::Array(source)) => {
                target.extend(source);
                Ok(())
            }
            _ => Err(DuplicateKeyError {
                key: key.to_owned(),
            }),
        }
    }
}

fn is_container(value: &UclValue) -> bool {
    matches!(value, UclValue::Object(_) | UclValue::Array(_))
}

/// Iterator over the values of an [`Entry`].
#[derive(Debug, Clone)]
pub struct Values<'a>(std::slice::Iter<'a, Slot>);

impl<'a> Iterator for Values<'a> {
    type Item = &'a UclValue;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(|s| &s.value)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}

impl DoubleEndedIterator for Values<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.0.next_back().map(|s| &s.value)
    }
}

impl ExactSizeIterator for Values<'_> {}

/// A UCL object: keys in insertion order, each with one or more values.
///
/// Equality ignores key order, like `IndexMap`; the order of values inside an entry matters.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UclObject {
    entries: IndexMap<String, Entry>,
}

impl UclObject {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            entries: IndexMap::with_capacity(capacity),
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
        self.entries.contains_key(key)
    }

    /// The first value of `key`, as libucl's `ucl_object_lookup` returns it.
    pub fn get(&self, key: &str) -> Option<&UclValue> {
        self.entries.get(key).map(Entry::first)
    }

    /// The first value of `key`, mutably.
    pub fn get_mut(&mut self, key: &str) -> Option<&mut UclValue> {
        self.entries.get_mut(key).map(Entry::first_mut)
    }

    /// Every value of `key` in insertion order; empty if the key is absent.
    pub fn get_all(&self, key: &str) -> Values<'_> {
        match self.entries.get(key) {
            Some(entry) => entry.values(),
            None => Values([].iter()),
        }
    }

    pub fn entry(&self, key: &str) -> Option<&Entry> {
        self.entries.get(key)
    }

    pub fn entry_mut(&mut self, key: &str) -> Option<&mut Entry> {
        self.entries.get_mut(key)
    }

    /// Sets `key` to a single value, replacing every existing value. An existing key keeps its
    /// position.
    pub fn insert(&mut self, key: impl Into<String>, value: UclValue) -> Option<Entry> {
        self.entries.insert(key.into(), Entry::new(value))
    }

    /// Sets `key` to `entry`, replacing an existing entry in place.
    pub fn insert_entry(&mut self, key: impl Into<String>, entry: Entry) -> Option<Entry> {
        self.entries.insert(key.into(), entry)
    }

    /// Adds a value to `key`: a new key, or one more value of an existing key (implicit array).
    pub fn append(&mut self, key: impl Into<String>, value: UclValue) {
        let key = key.into();
        match self.entries.get_mut(&key) {
            Some(entry) => entry.push(value),
            None => {
                self.entries.insert(key, Entry::new(value));
            }
        }
    }

    /// Removes `key`, keeping the order of the others.
    pub fn remove(&mut self, key: &str) -> Option<Entry> {
        self.entries.shift_remove(key)
    }

    /// Removes and returns the entry at `index`, keeping the order of the others.
    pub fn remove_index(&mut self, index: usize) -> Option<(String, Entry)> {
        self.entries.shift_remove_index(index)
    }

    /// Keys and entries in insertion order.
    pub fn iter(&self) -> indexmap::map::Iter<'_, String, Entry> {
        self.entries.iter()
    }

    /// Keys and entries in insertion order, entries mutable.
    pub fn iter_mut(&mut self) -> indexmap::map::IterMut<'_, String, Entry> {
        self.entries.iter_mut()
    }

    pub fn keys(&self) -> indexmap::map::Keys<'_, String, Entry> {
        self.entries.keys()
    }

    /// Entries in insertion order.
    pub fn entries(&self) -> indexmap::map::Values<'_, String, Entry> {
        self.entries.values()
    }

    /// Inserts `value` under `key`, resolving an existing key by `strategy` and `priority`,
    /// and applying `flags`. Behaviour is specified in `docs/spec/08-duplicates.md`; see
    /// [`UclObject::insert_slot_with_strategy`].
    pub fn insert_with_strategy(
        &mut self,
        key: impl Into<String>,
        value: UclValue,
        priority: u8,
        strategy: DuplicateStrategy,
        flags: ParserFlags,
    ) -> Result<(), DuplicateKeyError> {
        self.insert_slot_with_strategy(key, Slot::new(value, priority), strategy, flags)
    }

    /// [`UclObject::insert_with_strategy`] for a prepared slot.
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
        key: impl Into<String>,
        slot: Slot,
        strategy: DuplicateStrategy,
        flags: ParserFlags,
    ) -> Result<(), DuplicateKeyError> {
        let mut key = key.into();
        if flags.contains(ParserFlags::KEY_LOWERCASE) {
            key.make_ascii_lowercase();
        }
        let Some(entry) = self.entries.get_mut(&key) else {
            self.entries.insert(key, Entry::from_slot(slot));
            return Ok(());
        };
        match strategy {
            DuplicateStrategy::Append => entry.add_by_priority(&key, slot, true, flags),
            DuplicateStrategy::Merge => entry.merge(&key, slot, flags),
            DuplicateStrategy::Rewrite => {
                *entry = Entry::from_slot(slot);
                Ok(())
            }
            DuplicateStrategy::Error => Err(DuplicateKeyError { key }),
        }
    }
}

impl std::ops::Index<&str> for UclObject {
    type Output = UclValue;

    /// The first value of `key`. Panics if the key is absent.
    fn index(&self, key: &str) -> &UclValue {
        self.get(key)
            .unwrap_or_else(|| panic!("key '{key}' not found in UCL object"))
    }
}

impl IntoIterator for UclObject {
    type Item = (String, Entry);
    type IntoIter = indexmap::map::IntoIter<String, Entry>;

    fn into_iter(self) -> Self::IntoIter {
        self.entries.into_iter()
    }
}

impl<'a> IntoIterator for &'a UclObject {
    type Item = (&'a String, &'a Entry);
    type IntoIter = indexmap::map::Iter<'a, String, Entry>;

    fn into_iter(self) -> Self::IntoIter {
        self.entries.iter()
    }
}

impl<K: Into<String>> FromIterator<(K, UclValue)> for UclObject {
    /// Collects pairs with [`UclObject::append`]: a repeated key becomes an implicit array.
    fn from_iter<I: IntoIterator<Item = (K, UclValue)>>(iter: I) -> Self {
        let mut object = UclObject::new();
        for (k, v) in iter {
            object.append(k, v);
        }
        object
    }
}

impl<K: Into<String>> Extend<(K, UclValue)> for UclObject {
    fn extend<I: IntoIterator<Item = (K, UclValue)>>(&mut self, iter: I) {
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
            UclValue::Array(vec![int(1), int(2)]),
            0,
            DuplicateStrategy::Append,
        );
        insert(&mut obj, "k", int(3), 0, DuplicateStrategy::Append);
        assert_eq!(
            values(&obj, "k"),
            vec![UclValue::Array(vec![int(1), int(2)]), int(3)]
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
        assert_eq!(err.to_string(), "duplicate element for key 'k' found");
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
            UclValue::Array(vec![int(1)]),
            0,
            DuplicateStrategy::Merge,
        );
        insert(
            &mut obj,
            "arr",
            UclValue::Array(vec![int(2)]),
            0,
            DuplicateStrategy::Merge,
        );
        assert_eq!(obj.get("arr"), Some(&UclValue::Array(vec![int(1), int(2)])));
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
            vec![UclValue::Array(vec![int(1), int(2), int(3)])]
        );

        // A user-written array is not the collection array: it becomes its first element.
        let mut obj = UclObject::new();
        let written = UclValue::Array(vec![int(1), int(2)]);
        obj.insert_with_strategy("k", written.clone(), 0, DuplicateStrategy::Append, flags)
            .unwrap();
        obj.insert_with_strategy("k", int(3), 0, DuplicateStrategy::Append, flags)
            .unwrap();
        assert_eq!(
            values(&obj, "k"),
            vec![UclValue::Array(vec![written, int(3)])]
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

    fn insert_slot(obj: &mut UclObject, key: &str, slot: Slot, s: DuplicateStrategy) {
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
            UclValue::Array(vec![int(1)]),
            1,
            DuplicateStrategy::Merge,
        );
        insert(&mut obj, "b", int(2), 3, DuplicateStrategy::Merge);
        assert_eq!(values(&obj, "b"), vec![int(2)]);
        assert_eq!(priorities(&obj, "b"), vec![1]);
    }

    #[test]
    fn test_merge_container_type_mismatch_is_an_error() {
        let arr = UclValue::Array(vec![int(1)]);
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
            UclValue::Array(vec![int(1)]),
            3,
            DuplicateStrategy::Merge,
        );
        insert(
            &mut obj,
            "a",
            UclValue::Array(vec![int(2)]),
            1,
            DuplicateStrategy::Merge,
        );
        assert_eq!(
            values(&obj, "a"),
            vec![UclValue::Array(vec![int(1), int(2)])]
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
            vec![UclValue::Array(vec![int(2), int(4)])]
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
        for v in [int(1), int(2), UclValue::Array(vec![int(3)])] {
            obj.insert_with_strategy("b", v, 0, merge, flags).unwrap();
        }
        assert_eq!(
            values(&obj, "b"),
            vec![UclValue::Array(vec![int(1), int(2), int(3)])]
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
        assert_eq!(entry.into_value(), UclValue::Array(vec![int(1), int(2)]));
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
