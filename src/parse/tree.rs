//! The children of a node in a tree of value paths, by path segment: the output facts
//! ([`super::OutputFacts`]), the saved comments of a parse, and the comments an emitter writes
//! are kept in such trees, so that the node of a value is one step from the node of its
//! container.
//!
//! Most nodes have no children or a few, so a node keeps up to [`SMALL`] keyed children, and up
//! to [`SMALL`] element children, in a vector searched in order, and moves them to a hash map
//! when there are more (clean-room work item C11). A node without children allocates nothing.

use super::PathSegment;
use crate::value::KeyCopy;
use std::collections::HashMap;

/// The most keyed children, and the most element children, kept in a vector.
const SMALL: usize = 8;

/// A key as the children of a node are looked up by: a `str`, or a [`KeyCopy`], whose bytes are
/// compared without checking them again (clean-room work item C13).
pub(crate) trait KeyRef {
    fn bytes(&self) -> &[u8];
    fn to_key(&self) -> KeyCopy;
}

impl KeyRef for str {
    fn bytes(&self) -> &[u8] {
        self.as_bytes()
    }

    fn to_key(&self) -> KeyCopy {
        KeyCopy::from(self)
    }
}

impl KeyRef for String {
    fn bytes(&self) -> &[u8] {
        self.as_bytes()
    }

    fn to_key(&self) -> KeyCopy {
        KeyCopy::from(self)
    }
}

impl KeyRef for crate::value::Str<'_> {
    fn bytes(&self) -> &[u8] {
        self.as_bytes()
    }

    fn to_key(&self) -> KeyCopy {
        KeyCopy::from(self)
    }
}

impl KeyRef for KeyCopy {
    fn bytes(&self) -> &[u8] {
        self.as_bytes()
    }

    fn to_key(&self) -> KeyCopy {
        self.clone()
    }
}

/// A key of the map of a node with many keyed children, which is looked up by the key's bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ByteKey(KeyCopy);

impl std::hash::Hash for ByteKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.as_bytes().hash(state);
    }
}

impl std::borrow::Borrow<[u8]> for ByteKey {
    fn borrow(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

/// The children of a node, identified by the caller's node numbers.
#[derive(Debug, Clone, Default)]
pub(crate) struct Children {
    keys: Keyed,
    elements: Elements,
}

/// The nodes of the values of an object's entries.
#[derive(Debug, Clone)]
enum Keyed {
    /// The node of value `index` of entry `key` is `child` in an item `(key, index, child)`.
    Small(Vec<(KeyCopy, usize, usize)>),
    /// The node of value `index` of entry `key` is `map[key][index]`.
    Large(HashMap<ByteKey, Vec<Option<usize>>>),
}

impl Default for Keyed {
    fn default() -> Self {
        Keyed::Small(Vec::new())
    }
}

/// The nodes of an array's elements.
#[derive(Debug, Clone)]
enum Elements {
    /// The node of element `index` is `child` in an item `(index, child)`.
    Small(Vec<(usize, usize)>),
    Large(HashMap<usize, usize>),
}

impl Default for Elements {
    fn default() -> Self {
        Elements::Small(Vec::new())
    }
}

impl Children {
    /// The node of value `index` of entry `key`.
    pub(crate) fn key<K: KeyRef + ?Sized>(&self, key: &K, index: usize) -> Option<usize> {
        match &self.keys {
            Keyed::Small(items) => items
                .iter()
                .find(|(k, i, _)| *i == index && k.as_bytes() == key.bytes())
                .map(|&(_, _, child)| child),
            Keyed::Large(map) => map.get(key.bytes())?.get(index).copied().flatten(),
        }
    }

    /// The node of element `index`.
    pub(crate) fn element(&self, index: usize) -> Option<usize> {
        match &self.elements {
            Elements::Small(items) => items
                .iter()
                .find(|(i, _)| *i == index)
                .map(|&(_, child)| child),
            Elements::Large(map) => map.get(&index).copied(),
        }
    }

    pub(crate) fn get(&self, segment: &PathSegment) -> Option<usize> {
        match segment {
            PathSegment::Key { key, index } => self.key(key.as_str(), *index),
            PathSegment::Index(index) => self.element(*index),
        }
    }

    /// Makes `child` the node of `segment`.
    pub(crate) fn insert(&mut self, segment: &PathSegment, child: usize) {
        match segment {
            PathSegment::Key { key, index } => self.insert_key(key.as_str(), *index, child),
            PathSegment::Index(index) => self.insert_element(*index, child),
        }
    }

    /// Makes `child` the node of value `index` of entry `key`. The key is copied only when the
    /// value has no node yet.
    pub(crate) fn insert_key<K: KeyRef + ?Sized>(&mut self, key: &K, index: usize, child: usize) {
        match &mut self.keys {
            Keyed::Small(items) => {
                if let Some(item) = items
                    .iter_mut()
                    .find(|(k, i, _)| *i == index && k.as_bytes() == key.bytes())
                {
                    item.2 = child;
                } else if items.len() < SMALL {
                    items.push((key.to_key(), index, child));
                } else {
                    let mut map: HashMap<ByteKey, Vec<Option<usize>>> = HashMap::new();
                    for (key, index, child) in items.drain(..) {
                        set_value(map.entry(ByteKey(key)).or_default(), index, child);
                    }
                    set_value(map.entry(ByteKey(key.to_key())).or_default(), index, child);
                    self.keys = Keyed::Large(map);
                }
            }
            Keyed::Large(map) => {
                let values = match map.get_mut(key.bytes()) {
                    Some(values) => values,
                    None => map.entry(ByteKey(key.to_key())).or_default(),
                };
                set_value(values, index, child);
            }
        }
    }

    /// Makes `child` the node of element `index`.
    pub(crate) fn insert_element(&mut self, index: usize, child: usize) {
        match &mut self.elements {
            Elements::Small(items) => {
                if let Some(item) = items.iter_mut().find(|(i, _)| *i == index) {
                    item.1 = child;
                } else if items.len() < SMALL {
                    items.push((index, child));
                } else {
                    let mut map: HashMap<usize, usize> = items.drain(..).collect();
                    map.insert(index, child);
                    self.elements = Elements::Large(map);
                }
            }
            Elements::Large(map) => {
                map.insert(index, child);
            }
        }
    }

    /// Removes the node of value `index` of entry `key`, and returns it.
    pub(crate) fn take_value(&mut self, key: &str, index: usize) -> Option<usize> {
        match &mut self.keys {
            Keyed::Small(items) => {
                let at = items
                    .iter()
                    .position(|(k, i, _)| *i == index && k.as_bytes() == key.as_bytes())?;
                Some(items.remove(at).2)
            }
            Keyed::Large(map) => map.get_mut(key.as_bytes())?.get_mut(index)?.take(),
        }
    }

    /// Removes the nodes of every value of entry `key`, and returns them with their indices, in
    /// the order of the indices.
    pub(crate) fn take_values(&mut self, key: &str) -> Vec<(usize, usize)> {
        match &mut self.keys {
            Keyed::Small(items) => {
                let mut taken = Vec::new();
                items.retain(|(k, index, child)| {
                    let keep = k.as_bytes() != key.as_bytes();
                    if !keep {
                        taken.push((*index, *child));
                    }
                    keep
                });
                taken.sort_unstable();
                taken
            }
            Keyed::Large(map) => map.remove(key.as_bytes()).map_or_else(Vec::new, |values| {
                values
                    .into_iter()
                    .enumerate()
                    .filter_map(|(index, child)| Some((index, child?)))
                    .collect()
            }),
        }
    }

    /// Every child with its segment, in no particular order.
    #[cfg(test)]
    fn iter(&self) -> impl Iterator<Item = (PathSegment, usize)> + '_ {
        let mut all = Vec::new();
        match &self.keys {
            Keyed::Small(items) => all.extend(items.iter().map(|(key, index, child)| {
                let segment = PathSegment::Key {
                    key: key.to_string(),
                    index: *index,
                };
                (segment, *child)
            })),
            Keyed::Large(map) => {
                for (ByteKey(key), values) in map {
                    for (index, child) in values.iter().enumerate() {
                        if let Some(child) = child {
                            let segment = PathSegment::Key {
                                key: key.to_string(),
                                index,
                            };
                            all.push((segment, *child));
                        }
                    }
                }
            }
        }
        match &self.elements {
            Elements::Small(items) => all.extend(
                items
                    .iter()
                    .map(|&(index, child)| (PathSegment::Index(index), child)),
            ),
            Elements::Large(map) => all.extend(
                map.iter()
                    .map(|(&index, &child)| (PathSegment::Index(index), child)),
            ),
        }
        all.into_iter()
    }

    /// Removes every child, and returns them.
    pub(crate) fn take_all(&mut self) -> Vec<usize> {
        let mut all: Vec<usize> = match std::mem::take(&mut self.keys) {
            Keyed::Small(items) => items.into_iter().map(|(_, _, child)| child).collect(),
            Keyed::Large(map) => map.into_values().flatten().flatten().collect(),
        };
        match std::mem::take(&mut self.elements) {
            Elements::Small(items) => all.extend(items.into_iter().map(|(_, child)| child)),
            Elements::Large(map) => all.extend(map.into_values()),
        }
        all
    }
}

/// Makes `child` value `index` of `values`, the nodes of an entry's values.
fn set_value(values: &mut Vec<Option<usize>>, index: usize, child: usize) {
    if values.len() <= index {
        values.resize(index + 1, None);
    }
    values[index] = Some(child);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(key: &str, index: usize) -> PathSegment {
        PathSegment::Key {
            key: key.to_owned(),
            index,
        }
    }

    /// The same operations on a node with few children and on one with many give the same
    /// answers, before and after the node's children move to a map.
    #[test]
    fn small_and_large_forms_agree() {
        for extra in [0, SMALL * 3] {
            let mut children = Children::default();
            let mut next = 100;
            for i in 0..extra {
                children.insert(&key(&format!("k{i}"), 0), next);
                children.insert(&PathSegment::Index(1000 + i), next + 1);
                next += 2;
            }
            children.insert(&key("a", 2), 1);
            children.insert(&key("a", 0), 2);
            children.insert(&key("b", 0), 3);
            children.insert(&PathSegment::Index(5), 4);
            children.insert(&key("a", 0), 5);
            children.insert(&PathSegment::Index(5), 6);
            assert_eq!(children.key("a", 0), Some(5));
            assert_eq!(children.key("a", 1), None);
            assert_eq!(children.key("a", 2), Some(1));
            assert_eq!(children.get(&PathSegment::Index(5)), Some(6));
            assert_eq!(children.element(4), None);
            assert_eq!(children.iter().count(), 4 + 2 * extra);
            assert_eq!(children.take_value("a", 2), Some(1));
            assert_eq!(children.take_value("a", 2), None);
            children.insert(&key("a", 3), 7);
            assert_eq!(children.take_values("a"), vec![(0, 5), (3, 7)]);
            assert_eq!(children.take_values("a"), vec![]);
            assert_eq!(children.key("b", 0), Some(3));
            let mut all = children.take_all();
            all.sort_unstable();
            assert_eq!(all.len(), 2 + 2 * extra);
            assert!(all.contains(&3) && all.contains(&6));
            assert_eq!(children.iter().count(), 0);
        }
    }
}
