//! [`Map`], the entries of an [`Object`](super::Object): a vector searched in order while an
//! object is small, an [`IndexMap`] above [`SMALL`] keys (clean-room work item C13, item 3;
//! after halfbrown's `SizedHashMap` and the vector-backed objects of sonic-rs, C11 research
//! notes). A small object thus needs one allocation and no hashing, and a large one stays linear
//! to build and to look up (`tests/scaling.rs`).

use super::{Entry, Str};
use indexmap::IndexMap;
use indexmap::map::RawEntryApiV1;
use indexmap::map::raw_entry_v1::RawEntryMut;
use std::hash::BuildHasher;
use std::iter::FusedIterator;

/// The most keys an object keeps in a vector, searched in order. In parses of objects whose keys
/// have the same length and a common 32-byte prefix, the worst case of the search, it is faster
/// than the hash index up to 16 keys and as fast at 24 (C13 measurements); objects of the crate's
/// configuration benchmark have 9.
pub(crate) const SMALL: usize = 16;

/// The keys and entries of an object, in insertion order.
#[derive(Clone)]
pub(crate) enum Map<'a> {
    Small(Vec<(Str<'a>, Entry<'a>)>),
    Large(Box<IndexMap<Str<'a>, Entry<'a>>>),
}

impl Default for Map<'_> {
    fn default() -> Self {
        Map::Small(Vec::new())
    }
}

impl std::fmt::Debug for Map<'_> {
    /// As the `IndexMap` this replaces: `{key: entry, ...}`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_map().entries(self.iter()).finish()
    }
}

/// Where a key is in a map, or where it would go, found once for a lookup and the insertion that
/// follows it ([`Map::probe`], [`Map::push_probed`]).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Probe {
    /// The key's position, if it is in the map.
    pub(crate) index: Option<usize>,
    /// The key's hash, for a map with a hash index.
    hash: Option<u64>,
}

impl<'a> Map<'a> {
    pub(crate) fn with_capacity(capacity: usize) -> Self {
        if capacity <= SMALL {
            Map::Small(Vec::with_capacity(capacity))
        } else {
            Map::Large(Box::new(IndexMap::with_capacity(capacity)))
        }
    }

    pub(crate) fn len(&self) -> usize {
        match self {
            Map::Small(items) => items.len(),
            Map::Large(map) => map.len(),
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The position of `key`.
    pub(crate) fn get_index_of(&self, key: &str) -> Option<usize> {
        match self {
            Map::Small(items) => items.iter().position(|(k, _)| k.as_str() == key),
            Map::Large(map) => map.get_index_of(key),
        }
    }

    pub(crate) fn get(&self, key: &str) -> Option<&Entry<'a>> {
        let index = self.get_index_of(key)?;
        self.get_index(index).map(|(_, entry)| entry)
    }

    pub(crate) fn get_mut(&mut self, key: &str) -> Option<&mut Entry<'a>> {
        let index = self.get_index_of(key)?;
        self.get_index_mut(index).map(|(_, entry)| entry)
    }

    pub(crate) fn get_index(&self, index: usize) -> Option<(&Str<'a>, &Entry<'a>)> {
        match self {
            Map::Small(items) => items.get(index).map(|(k, e)| (k, e)),
            Map::Large(map) => map.get_index(index),
        }
    }

    pub(crate) fn get_index_mut(&mut self, index: usize) -> Option<(&Str<'a>, &mut Entry<'a>)> {
        match self {
            Map::Small(items) => items.get_mut(index).map(|(k, e)| (&*k, e)),
            Map::Large(map) => map.get_index_mut(index),
        }
    }

    /// Where `key` is, or would go, in the map: found by comparing keys in a small map, and by
    /// its hash, computed once, in a large one.
    pub(crate) fn probe(&self, key: &str) -> Probe {
        match self {
            Map::Small(items) => Probe {
                index: items.iter().position(|(k, _)| k.as_str() == key),
                hash: None,
            },
            Map::Large(map) => {
                let hash = map.hasher().hash_one(key);
                let index = map
                    .raw_entry_v1()
                    .index_from_hash(hash, |k| k.as_str() == key);
                Probe {
                    index,
                    hash: Some(hash),
                }
            }
        }
    }

    /// Adds `key`, which [`Map::probe`] found absent, with `entry` at the end.
    pub(crate) fn push_probed(&mut self, probe: Probe, key: Str<'a>, entry: Entry<'a>) {
        debug_assert!(
            probe.index.is_none(),
            "push_probed of a key already present"
        );
        match self {
            Map::Small(items) if items.len() < SMALL => items.push((key, entry)),
            Map::Small(_) => {
                self.grow();
                self.insert(key, entry);
            }
            Map::Large(map) => {
                let hash = probe
                    .hash
                    .unwrap_or_else(|| map.hasher().hash_one(key.as_str()));
                match map
                    .raw_entry_mut_v1()
                    .from_hash(hash, |k| k.as_str() == key.as_str())
                {
                    RawEntryMut::Vacant(vacant) => {
                        vacant.insert_hashed_nocheck(hash, key, entry);
                    }
                    RawEntryMut::Occupied(mut occupied) => *occupied.get_mut() = entry,
                }
            }
        }
    }

    /// Moves a small map's entries into a hash index.
    fn grow(&mut self) {
        if let Map::Small(items) = self {
            let mut map = IndexMap::with_capacity(items.len() * 2);
            map.extend(items.drain(..));
            *self = Map::Large(Box::new(map));
        }
    }

    /// Sets `key` to `entry`, replacing an existing entry in place, and returns the entry that
    /// was there.
    pub(crate) fn insert(&mut self, key: Str<'a>, entry: Entry<'a>) -> Option<Entry<'a>> {
        let probe = self.probe(&key);
        if let Some(index) = probe.index {
            let (_, old) = self.get_index_mut(index).expect("just found");
            return Some(std::mem::replace(old, entry));
        }
        self.push_probed(probe, key, entry);
        None
    }

    /// Changes the key at `index` to `key`; fails when `key` is another key already present.
    pub(crate) fn replace_index(&mut self, index: usize, key: Str<'a>) -> Result<(), ()> {
        match self {
            Map::Small(items) => {
                if items
                    .iter()
                    .enumerate()
                    .any(|(i, (k, _))| i != index && *k == key)
                {
                    return Err(());
                }
                items[index].0 = key;
                Ok(())
            }
            Map::Large(map) => map.replace_index(index, key).map(drop).map_err(drop),
        }
    }

    pub(crate) fn shift_remove(&mut self, key: &str) -> Option<Entry<'a>> {
        let index = self.get_index_of(key)?;
        self.shift_remove_index(index).map(|(_, entry)| entry)
    }

    pub(crate) fn shift_remove_index(&mut self, index: usize) -> Option<(Str<'a>, Entry<'a>)> {
        match self {
            Map::Small(items) => (index < items.len()).then(|| items.remove(index)),
            Map::Large(map) => map.shift_remove_index(index),
        }
    }

    /// Adds `key`, which is not in the map, with `entry` at the end, without looking for it: for
    /// a copy of another object's entries.
    pub(crate) fn push_unique(&mut self, key: Str<'a>, entry: Entry<'a>) {
        match self {
            Map::Small(items) if items.len() < SMALL => items.push((key, entry)),
            Map::Small(_) => {
                self.grow();
                self.push_unique(key, entry);
            }
            Map::Large(map) => {
                map.insert(key, entry);
            }
        }
    }

    /// The entries, mutably, in insertion order.
    pub(crate) fn values_mut(&mut self) -> impl Iterator<Item = &mut Entry<'a>> {
        self.iter_mut().map(|(_, entry)| entry)
    }

    pub(crate) fn iter(&self) -> Iter<'_, 'a> {
        match self {
            Map::Small(items) => Iter::Small(items.iter()),
            Map::Large(map) => Iter::Large(map.iter()),
        }
    }

    pub(crate) fn iter_mut(&mut self) -> IterMut<'_, 'a> {
        match self {
            Map::Small(items) => IterMut::Small(items.iter_mut()),
            Map::Large(map) => IterMut::Large(map.iter_mut()),
        }
    }

    pub(crate) fn into_iter(self) -> IntoIter<'a> {
        match self {
            Map::Small(items) => IntoIter::Small(items.into_iter()),
            Map::Large(map) => IntoIter::Large(map.into_iter()),
        }
    }
}

impl<'a> IntoIterator for Map<'a> {
    type Item = (Str<'a>, Entry<'a>);
    type IntoIter = IntoIter<'a>;

    fn into_iter(self) -> IntoIter<'a> {
        Map::into_iter(self)
    }
}

/// The keys and entries of an object, in insertion order.
#[derive(Debug, Clone)]
pub enum Iter<'v, 'a> {
    #[doc(hidden)]
    Small(std::slice::Iter<'v, (Str<'a>, Entry<'a>)>),
    #[doc(hidden)]
    Large(indexmap::map::Iter<'v, Str<'a>, Entry<'a>>),
}

impl<'v, 'a> Iterator for Iter<'v, 'a> {
    type Item = (&'v Str<'a>, &'v Entry<'a>);

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Iter::Small(items) => items.next().map(|(k, e)| (k, e)),
            Iter::Large(items) => items.next(),
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        match self {
            Iter::Small(items) => items.size_hint(),
            Iter::Large(items) => items.size_hint(),
        }
    }
}

impl DoubleEndedIterator for Iter<'_, '_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        match self {
            Iter::Small(items) => items.next_back().map(|(k, e)| (k, e)),
            Iter::Large(items) => items.next_back(),
        }
    }
}

impl ExactSizeIterator for Iter<'_, '_> {}

impl FusedIterator for Iter<'_, '_> {}

/// The keys and entries of an object, in insertion order, the entries mutable.
#[derive(Debug)]
pub enum IterMut<'v, 'a> {
    #[doc(hidden)]
    Small(std::slice::IterMut<'v, (Str<'a>, Entry<'a>)>),
    #[doc(hidden)]
    Large(indexmap::map::IterMut<'v, Str<'a>, Entry<'a>>),
}

impl<'v, 'a> Iterator for IterMut<'v, 'a> {
    type Item = (&'v Str<'a>, &'v mut Entry<'a>);

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            IterMut::Small(items) => items.next().map(|(k, e)| (&*k, e)),
            IterMut::Large(items) => items.next(),
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        match self {
            IterMut::Small(items) => items.size_hint(),
            IterMut::Large(items) => items.size_hint(),
        }
    }
}

impl DoubleEndedIterator for IterMut<'_, '_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        match self {
            IterMut::Small(items) => items.next_back().map(|(k, e)| (&*k, e)),
            IterMut::Large(items) => items.next_back(),
        }
    }
}

impl ExactSizeIterator for IterMut<'_, '_> {}

impl FusedIterator for IterMut<'_, '_> {}

/// The keys and entries of an object, by value, in insertion order.
#[derive(Debug)]
pub enum IntoIter<'a> {
    #[doc(hidden)]
    Small(std::vec::IntoIter<(Str<'a>, Entry<'a>)>),
    #[doc(hidden)]
    Large(indexmap::map::IntoIter<Str<'a>, Entry<'a>>),
}

impl<'a> Iterator for IntoIter<'a> {
    type Item = (Str<'a>, Entry<'a>);

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            IntoIter::Small(items) => items.next(),
            IntoIter::Large(items) => items.next(),
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        match self {
            IntoIter::Small(items) => items.size_hint(),
            IntoIter::Large(items) => items.size_hint(),
        }
    }
}

impl DoubleEndedIterator for IntoIter<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        match self {
            IntoIter::Small(items) => items.next_back(),
            IntoIter::Large(items) => items.next_back(),
        }
    }
}

impl ExactSizeIterator for IntoIter<'_> {}

impl FusedIterator for IntoIter<'_> {}

/// The keys of an object, in insertion order.
#[derive(Debug, Clone)]
pub struct Keys<'v, 'a>(pub(crate) Iter<'v, 'a>);

impl<'v, 'a> Iterator for Keys<'v, 'a> {
    type Item = &'v Str<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(|(k, _)| k)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}

impl DoubleEndedIterator for Keys<'_, '_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.0.next_back().map(|(k, _)| k)
    }
}

impl ExactSizeIterator for Keys<'_, '_> {}

impl FusedIterator for Keys<'_, '_> {}

/// The entries of an object, in insertion order.
#[derive(Debug, Clone)]
pub struct Entries<'v, 'a>(pub(crate) Iter<'v, 'a>);

impl<'v, 'a> Iterator for Entries<'v, 'a> {
    type Item = &'v Entry<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(|(_, e)| e)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}

impl DoubleEndedIterator for Entries<'_, '_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.0.next_back().map(|(_, e)| e)
    }
}

impl ExactSizeIterator for Entries<'_, '_> {}

impl FusedIterator for Entries<'_, '_> {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::{UclValue, Value};

    fn entry(i: i64) -> Entry<'static> {
        Entry::new(Value::Integer(i))
    }

    fn int(e: Option<&Entry<'_>>) -> Option<i64> {
        e.and_then(|e| e.first().as_integer())
    }

    /// The same operations give the same answers on a map that stays small and on one that
    /// grows past [`SMALL`] and moves its entries into a hash index.
    #[test]
    fn small_and_large_forms_agree() {
        for extra in [0, SMALL * 3] {
            let mut map = Map::default();
            for i in 0..extra {
                map.insert(Str::from(format!("k{i}")), entry(i as i64));
            }
            let probe = map.probe("a");
            assert_eq!(probe.index, None);
            map.push_probed(probe, Str::from("a"), entry(-1));
            assert_eq!(map.insert(Str::from("b"), entry(-2)), None);
            assert_eq!(
                int(map.insert(Str::from("a"), entry(-3)).as_ref()),
                Some(-1)
            );
            assert_eq!(map.len(), extra + 2);
            assert_eq!(map.get_index_of("a"), Some(extra));
            assert_eq!(map.probe("b").index, Some(extra + 1));
            assert_eq!(int(map.get("a")), Some(-3));
            assert_eq!(map.get("missing").map(Entry::len), None);
            // Keys keep their order through a rename and removals.
            assert!(map.replace_index(extra, Str::from("c")).is_ok());
            assert!(map.replace_index(extra, Str::from("b")).is_err());
            assert_eq!(map.get_index(extra).map(|(k, _)| k.as_str()), Some("c"));
            assert_eq!(int(map.shift_remove("c").as_ref()), Some(-3));
            assert_eq!(map.shift_remove("c").map(|e| e.len()), None);
            let keys: Vec<String> = map.iter().map(|(k, _)| k.to_string()).collect();
            assert_eq!(keys.last().map(String::as_str), Some("b"));
            assert_eq!(keys.len(), extra + 1);
            for (_, e) in map.iter_mut() {
                e.push(Value::Null);
            }
            assert!(map.values_mut().all(|e| e.len() == 2));
            let owned: Vec<(Str<'_>, Entry<'_>)> = map.clone().into_iter().collect();
            assert_eq!(owned.len(), extra + 1);
            assert_eq!(map.iter().next_back().map(|(k, _)| k.as_str()), Some("b"));
        }
    }

    #[test]
    fn objects_move_to_a_hash_index_past_small_keys() {
        let mut object = crate::value::UclObject::new();
        for i in 0..=SMALL {
            object.insert(format!("k{i}"), UclValue::Integer(i as i64));
            let large = matches!(object.entries, Map::Large(_));
            assert_eq!(large, i >= SMALL, "{i}");
        }
        assert_eq!(object.index_of(&format!("k{SMALL}")), Some(SMALL));
        assert_eq!(std::mem::size_of::<crate::value::UclObject>(), 24);
    }
}
