//! The children of a node in a tree of value paths, by path segment: the output facts
//! ([`super::OutputFacts`]), the saved comments of a parse, and the comments an emitter writes
//! are kept in such trees, so that the node of a value is one step from the node of its
//! container.

use super::PathSegment;
use std::collections::HashMap;

/// The children of a node, identified by the caller's node numbers.
#[derive(Debug, Clone, Default)]
pub(crate) struct Children {
    /// The node of value `index` of entry `key` is `keys[key][index]`.
    keys: HashMap<String, Vec<Option<usize>>>,
    /// The node of element `index`.
    elements: HashMap<usize, usize>,
}

impl Children {
    /// The node of value `index` of entry `key`.
    pub(crate) fn key(&self, key: &str, index: usize) -> Option<usize> {
        self.keys.get(key)?.get(index).copied().flatten()
    }

    /// The node of element `index`.
    pub(crate) fn element(&self, index: usize) -> Option<usize> {
        self.elements.get(&index).copied()
    }

    pub(crate) fn get(&self, segment: &PathSegment) -> Option<usize> {
        match segment {
            PathSegment::Key { key, index } => self.key(key, *index),
            PathSegment::Index(index) => self.element(*index),
        }
    }

    /// Makes `child` the node of `segment`.
    pub(crate) fn insert(&mut self, segment: &PathSegment, child: usize) {
        match segment {
            PathSegment::Key { key, index } => {
                let values = self.keys.entry(key.clone()).or_default();
                if values.len() <= *index {
                    values.resize(*index + 1, None);
                }
                values[*index] = Some(child);
            }
            PathSegment::Index(index) => {
                self.elements.insert(*index, child);
            }
        }
    }

    /// Removes the node of value `index` of entry `key`, and returns it.
    pub(crate) fn take_value(&mut self, key: &str, index: usize) -> Option<usize> {
        self.keys.get_mut(key)?.get_mut(index)?.take()
    }

    /// Removes the nodes of every value of entry `key`, and returns them with their indices.
    pub(crate) fn take_values(&mut self, key: &str) -> Vec<(usize, usize)> {
        self.keys.remove(key).map_or_else(Vec::new, |values| {
            values
                .into_iter()
                .enumerate()
                .filter_map(|(index, child)| Some((index, child?)))
                .collect()
        })
    }

    /// Every child with its segment.
    pub(crate) fn iter(&self) -> impl Iterator<Item = (PathSegment, usize)> + '_ {
        let keyed = self.keys.iter().flat_map(|(key, values)| {
            values.iter().enumerate().filter_map(move |(index, child)| {
                Some((
                    PathSegment::Key {
                        key: key.clone(),
                        index,
                    },
                    (*child)?,
                ))
            })
        });
        let elements = self
            .elements
            .iter()
            .map(|(&index, &child)| (PathSegment::Index(index), child));
        keyed.chain(elements)
    }

    /// Removes every child, and returns them.
    pub(crate) fn take_all(&mut self) -> Vec<usize> {
        let keyed = self.keys.drain().flat_map(|(_, values)| values).flatten();
        let mut all: Vec<usize> = keyed.collect();
        all.extend(self.elements.drain().map(|(_, child)| child));
        all
    }
}
