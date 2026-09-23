//! Facts remembered from parsing that the config and YAML output formats depend on (spec §10.1).
//!
//! Values are identified by their path from the root, as saved comments are
//! ([`super::AttachedComments`]). Only facts that differ from what the emitter assumes for a value
//! without facts are stored, so a document without single-quoted strings, heredocs or escaped
//! keys records nothing. When the duplicate rules of §8 move or replace values, the parser updates
//! the paths the same way it does for comments.

use super::PathSegment;
use std::collections::BTreeMap;
use std::ops::Bound;

/// What the output formats need to know about one value beyond the value itself (spec §10.1).
///
/// The fields describe the value at a path: `single_quoted` and `multiline` apply when it is a
/// string; `key_spelling` and `key_quoted` apply when it is a value of an object's entry.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ValueFacts {
    /// The string was written in single quotes (§6.2): fact 1.
    pub single_quoted: bool,
    /// The string came from a heredoc (§6.3) or from `.load` with `multiline=true` (§9.6):
    /// fact 2.
    pub multiline: bool,
    /// The key as written for this value, when it differs from the entry's key: under
    /// `KEY_LOWERCASE` the values of one entry can be written with different spellings (§12.1).
    pub key_spelling: Option<String>,
    /// Whether the key needs quoting (fact 3), when that differs from
    /// [`crate::emit::key_needs_quoting`] applied to the key's spelling.
    pub key_quoted: Option<bool>,
    /// A multi-value entry whose first value this is uses the normal layout in JSON and YAML
    /// rather than the inline one that the value's kind gives (spec §10.7). The parser sets it on
    /// a number written with digits (not the keywords `nan` and `inf`), a time or a boolean that
    /// took the place of a non-empty object or array under the `merge` strategy (spec §8.4 and
    /// §10.7, *Quirk*; QUESTIONS.md #50).
    pub normal_layout: bool,
}

impl ValueFacts {
    fn is_default(&self) -> bool {
        *self == ValueFacts::default()
    }
}

/// The output facts of a parsed document, by value path: what [`crate::emit::Emitter`] needs to
/// write the document as libucl does (spec §10.1). [`super::Parser::output_facts`] gives those of
/// the last parse.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OutputFacts {
    /// Ordered by path, so that the facts of a value and of everything inside it are one range.
    values: BTreeMap<Vec<PathSegment>, ValueFacts>,
}

pub(crate) type ValuePath = Vec<PathSegment>;

impl OutputFacts {
    /// No facts.
    pub fn new() -> Self {
        Self::default()
    }

    /// True if no value has a fact recorded.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// The facts of the value at `path`, if any were recorded.
    pub fn get(&self, path: &[PathSegment]) -> Option<&ValueFacts> {
        self.values.get(path)
    }

    /// Sets the facts of the value at `path`; default facts remove the entry.
    pub fn insert(&mut self, path: Vec<PathSegment>, facts: ValueFacts) {
        if facts.is_default() {
            self.values.remove(&path);
        } else {
            self.values.insert(path, facts);
        }
    }

    /// Every recorded path and its facts, in path order.
    pub fn iter(&self) -> impl Iterator<Item = (&[PathSegment], &ValueFacts)> {
        self.values.iter().map(|(p, f)| (p.as_slice(), f))
    }

    pub(crate) fn clear(&mut self) {
        self.values.clear();
    }

    /// Changes the facts of the value at `path` with `change`.
    pub(crate) fn update(&mut self, path: &[PathSegment], change: impl FnOnce(&mut ValueFacts)) {
        let mut facts = self.values.get(path).cloned().unwrap_or_default();
        change(&mut facts);
        self.insert(path.to_vec(), facts);
    }

    /// The recorded paths from `start` on, as long as `inside` holds for them.
    fn paths_from(
        &self,
        start: &[PathSegment],
        inside: impl Fn(&[PathSegment]) -> bool,
    ) -> Vec<ValuePath> {
        self.values
            .range::<[PathSegment], _>((Bound::Included(start), Bound::Unbounded))
            .map(|(p, _)| p)
            .take_while(|p| inside(p))
            .cloned()
            .collect()
    }

    /// The recorded paths of the values of entry `key` in the object at `object` whose index
    /// satisfies `index`, and of everything inside them.
    fn entry_paths(
        &self,
        object: &[PathSegment],
        key: &str,
        index: impl Fn(usize) -> bool,
    ) -> Vec<ValuePath> {
        let n = object.len();
        let mut start = object.to_vec();
        start.push(PathSegment::Key {
            key: key.to_owned(),
            index: 0,
        });
        let mut paths = self.paths_from(&start, |p| {
            p.len() > n
                && p[..n] == *object
                && matches!(&p[n], PathSegment::Key { key: k, .. } if k == key)
        });
        paths.retain(|p| matches!(&p[n], PathSegment::Key { index: i, .. } if index(*i)));
        paths
    }

    /// Removes the facts of everything inside the value at `path`, but not its own.
    pub(crate) fn clear_below(&mut self, path: &[PathSegment]) {
        let n = path.len();
        for p in self.paths_from(path, |p| p.len() >= n && p[..n] == *path) {
            if p.len() > n {
                self.values.remove(&p);
            }
        }
    }

    /// Values of entry `key` in the object at `object` were replaced: value `slot`, or all of
    /// them. Their facts, and those of anything inside them, are dropped.
    pub(crate) fn replaced(&mut self, object: &[PathSegment], key: &str, slot: Option<usize>) {
        for p in self.entry_paths(object, key, |i| slot.is_none_or(|s| s == i)) {
            self.values.remove(&p);
        }
    }

    /// Entry `key` of the object at `object` became a `NO_IMPLICIT_ARRAYS` collection: its first
    /// `count` values are now the first elements of the array that is its only value.
    pub(crate) fn collected(&mut self, object: &[PathSegment], key: &str, count: usize) {
        let n = object.len();
        // All are taken out before any is put back: a new path can be an old one of the first
        // value, when that value is an array.
        let moved: Vec<(ValuePath, ValueFacts)> = self
            .entry_paths(object, key, |i| i < count)
            .into_iter()
            .map(|p| {
                let facts = self.values.remove(&p).expect("the path was just found");
                (p, facts)
            })
            .collect();
        for (old, facts) in moved {
            let PathSegment::Key { index, .. } = &old[n] else {
                unreachable!("a value of the entry")
            };
            let mut new = old[..n].to_vec();
            new.push(PathSegment::Key {
                key: key.to_owned(),
                index: 0,
            });
            new.push(PathSegment::Index(*index));
            new.extend_from_slice(&old[n + 1..]);
            self.values.insert(new, facts);
        }
    }

    /// The facts of the value at `from` and of everything inside it, with paths relative to it.
    pub(crate) fn subtree(&self, from: &[PathSegment]) -> Vec<(ValuePath, ValueFacts)> {
        let n = from.len();
        self.values
            .range::<[PathSegment], _>((Bound::Included(from), Bound::Unbounded))
            .take_while(|(p, _)| p.len() >= n && p[..n] == *from)
            .map(|(p, f)| (p[n..].to_vec(), f.clone()))
            .collect()
    }

    /// Records `facts`, a [`OutputFacts::subtree`], below the value at `to`.
    pub(crate) fn graft(&mut self, to: &[PathSegment], facts: Vec<(ValuePath, ValueFacts)>) {
        for (rest, f) in facts {
            let mut path = to.to_vec();
            path.extend(rest);
            self.insert(path, f);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(k: &str, index: usize) -> PathSegment {
        PathSegment::Key {
            key: k.into(),
            index,
        }
    }

    fn sq() -> ValueFacts {
        ValueFacts {
            single_quoted: true,
            ..ValueFacts::default()
        }
    }

    #[test]
    fn default_facts_are_not_stored() {
        let mut facts = OutputFacts::new();
        facts.insert(vec![key("a", 0)], ValueFacts::default());
        assert!(facts.is_empty());
        facts.update(&[key("a", 0)], |f| f.single_quoted = true);
        facts.update(&[key("a", 0)], |f| f.key_quoted = Some(true));
        assert_eq!(facts.get(&[key("a", 0)]).unwrap().key_quoted, Some(true));
        facts.update(&[key("a", 0)], |f| f.single_quoted = false);
        facts.update(&[key("a", 0)], |f| f.key_quoted = None);
        assert!(facts.is_empty());
    }

    #[test]
    fn replaced_collected_and_grafted_paths() {
        let mut facts = OutputFacts::new();
        facts.insert(vec![key("o", 0), key("a", 0)], sq());
        facts.insert(vec![key("o", 0), key("a", 1), PathSegment::Index(0)], sq());
        facts.insert(vec![key("o", 0), key("b", 0)], sq());
        facts.collected(&[key("o", 0)], "a", 1);
        assert!(
            facts
                .get(&[key("o", 0), key("a", 0), PathSegment::Index(0)])
                .is_some()
        );
        facts.replaced(&[key("o", 0)], "a", Some(1));
        assert_eq!(facts.iter().count(), 2);
        let copied = facts.subtree(&[key("o", 0)]);
        facts.graft(&[key("p", 0)], copied);
        assert!(facts.get(&[key("p", 0), key("b", 0)]).is_some());
        facts.clear_below(&[key("o", 0)]);
        assert_eq!(facts.iter().count(), 2);
    }
}
