//! Facts remembered from parsing that the config and YAML output formats depend on (spec §10.1).
//!
//! Values are identified by their path from the root, as saved comments are
//! ([`super::AttachedComments`]). The facts are kept in a tree of those paths, one node per path
//! segment, so that the parser records a fact, and the emitter finds one, with one step from the
//! node of the value's container, at any depth. Only facts that differ from what the emitter
//! assumes for a value without facts are stored, so a document without single-quoted strings,
//! heredocs or escaped keys records nothing. When the duplicate rules of §8 move or replace
//! values, the parser updates the tree the same way it does the paths of comments.
//!
//! The same tree can also record where each value was written ([`Location`]), for the
//! positions of deserialization errors. Only a parse run to find such a position records them
//! ([`OutputFacts::locating`]): then every value has a node, and the locations move with the
//! nodes when values are moved, replaced or copied, as the facts do.

use super::PathSegment;
use super::error::position_at;
use super::tree::Children;
use crate::error::Position;
use std::path::{Path, PathBuf};

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

/// A node of [`OutputFacts`], which stands for a path from the root.
pub(crate) type NodeId = usize;

/// The node of the root's path, which is empty.
pub(crate) const ROOT: NodeId = 0;

#[derive(Debug, Clone, Default)]
struct Node {
    facts: ValueFacts,
    /// The nodes of the values of an object's entries, or of an array's elements, at this path.
    children: Children,
}

/// Where a value was written: in which input unit (0 for the document given to the parser, see
/// [`Locations`]), at which byte offset its text starts, and where the key of an entry's value
/// starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Location {
    unit: usize,
    value: usize,
    key: Option<usize>,
}

/// The locations of a parse that records them: where each node's value was written, by node,
/// the files included, in the order they were opened, after the document itself, and the unit
/// being parsed. Kept apart from the nodes, so that a parse that records no locations pays
/// nothing for them.
#[derive(Debug, Clone, Default)]
struct Locations {
    /// The location of node `n` is `at[n]`; nodes past the end have none.
    at: Vec<Option<Location>>,
    /// The canonical path and the bytes of each included file; unit `n` is `files[n - 1]`.
    files: Vec<(PathBuf, Vec<u8>)>,
    current: usize,
}

impl Locations {
    fn get(&self, node: NodeId) -> Option<Location> {
        self.at.get(node).copied().flatten()
    }

    fn set(&mut self, node: NodeId, at: Option<Location>) {
        if self.at.len() <= node {
            if at.is_none() {
                return;
            }
            self.at.resize(node + 1, None);
        }
        self.at[node] = at;
    }
}

/// The output facts of a parsed document, by value path: what [`crate::emit::Emitter`] needs to
/// write the document as libucl does (spec §10.1). [`super::Parser::output_facts`] gives those of
/// the last parse.
#[derive(Debug, Clone)]
pub struct OutputFacts {
    /// The tree of paths; `nodes[ROOT]` is the root. A node taken out of the tree stays in the
    /// vector, unused, until [`OutputFacts::clear`].
    nodes: Vec<Node>,
    /// The number of nodes in the tree whose facts are not the default.
    recorded: usize,
    /// Set when the locations of values are recorded.
    locations: Option<Box<Locations>>,
}

pub(crate) type ValuePath = Vec<PathSegment>;

/// The facts and locations of a value and of everything inside it, with paths relative to it
/// ([`OutputFacts::subtree`]).
pub(crate) type Subtree = Vec<(ValuePath, ValueFacts, Option<Location>)>;

impl Default for OutputFacts {
    fn default() -> Self {
        Self {
            nodes: vec![Node::default()],
            recorded: 0,
            locations: None,
        }
    }
}

impl PartialEq for OutputFacts {
    fn eq(&self, other: &Self) -> bool {
        self.iter().eq(other.iter())
    }
}

impl Eq for OutputFacts {}

impl OutputFacts {
    /// No facts.
    pub fn new() -> Self {
        Self::default()
    }

    /// True if no value has a fact recorded.
    pub fn is_empty(&self) -> bool {
        self.recorded == 0
    }

    /// The facts of the value at `path`, if any were recorded.
    pub fn get(&self, path: &[PathSegment]) -> Option<&ValueFacts> {
        self.facts_of(self.node_at(path)?)
    }

    /// Sets the facts of the value at `path`; default facts remove those recorded.
    pub fn insert(&mut self, path: Vec<PathSegment>, facts: ValueFacts) {
        let node = if facts.is_default() {
            self.node_at(&path)
        } else {
            Some(self.descend_or_insert(ROOT, &path))
        };
        if let Some(node) = node {
            self.set(node, facts);
        }
    }

    /// Every recorded path and its facts, in path order.
    pub fn iter(&self) -> impl Iterator<Item = (Vec<PathSegment>, &ValueFacts)> {
        let mut recorded = Vec::with_capacity(self.recorded);
        let mut stack = vec![(ROOT, Vec::new())];
        while let Some((node, path)) = stack.pop() {
            let n = &self.nodes[node];
            for (segment, child) in n.children.iter() {
                let mut path = path.clone();
                path.push(segment);
                stack.push((child, path));
            }
            if !n.facts.is_default() {
                recorded.push((path, &n.facts));
            }
        }
        recorded.sort_by(|a, b| a.0.cmp(&b.0));
        recorded.into_iter()
    }

    pub(crate) fn clear(&mut self) {
        *self = Self::default();
    }

    /// No facts, and the locations of values are to be recorded.
    pub(crate) fn locating() -> Self {
        Self {
            locations: Some(Box::default()),
            ..Self::default()
        }
    }

    /// Whether the locations of values are recorded.
    pub(crate) fn records_locations(&self) -> bool {
        self.locations.is_some()
    }

    /// Whether moving or replacing values must update the tree: some fact is recorded, or
    /// locations are.
    pub(crate) fn tracks_moves(&self) -> bool {
        self.recorded > 0 || self.locations.is_some()
    }

    /// Records that the value at `node` was written at byte `value` of the current input unit,
    /// with its key at byte `key`. Does nothing unless locations are recorded.
    pub(crate) fn locate(&mut self, node: NodeId, value: usize, key: Option<usize>) {
        if let Some(locations) = &mut self.locations {
            let at = Location {
                unit: locations.current,
                value,
                key,
            };
            locations.set(node, Some(at));
        }
    }

    /// The included file `file`, whose bytes are `src`, becomes the current input unit. Returns
    /// the unit it replaces, for [`OutputFacts::leave_unit`].
    pub(crate) fn enter_unit(&mut self, file: &Path, src: &[u8]) -> usize {
        let Some(locations) = &mut self.locations else {
            return 0;
        };
        locations.files.push((file.to_path_buf(), src.to_vec()));
        std::mem::replace(&mut locations.current, locations.files.len())
    }

    /// Goes back to the input unit `unit` after an included file.
    pub(crate) fn leave_unit(&mut self, unit: usize) {
        if let Some(locations) = &mut self.locations {
            locations.current = unit;
        }
    }

    /// Where the value at `path` was written, as a position in its input unit and that unit's
    /// file (`None` for the document, whose bytes are `document`); with `key`, where its key
    /// was written, if it has one. A value without a location of its own, such as one the
    /// recording parse did not produce, gets that of the nearest value around it that has one.
    pub(crate) fn position_of(
        &self,
        path: &[PathSegment],
        key: bool,
        document: &[u8],
    ) -> Option<(Position, Option<PathBuf>)> {
        let locations = self.locations.as_ref()?;
        let mut node = ROOT;
        let mut found = locations.get(ROOT).map(|at| (at, path.is_empty()));
        for (depth, segment) in path.iter().enumerate() {
            let Some(child) = self.nodes[node].children.get(segment) else {
                break;
            };
            node = child;
            if let Some(at) = locations.get(node) {
                found = Some((at, depth + 1 == path.len()));
            }
        }
        let (at, exact) = found?;
        let offset = match at.key {
            Some(key_at) if key && exact => key_at,
            _ => at.value,
        };
        match at.unit {
            0 => Some((position_at(document, offset), None)),
            unit => {
                let (file, src) = locations.files.get(unit - 1)?;
                Some((position_at(src, offset), Some(file.clone())))
            }
        }
    }

    /// The facts recorded at `node`, if any.
    pub(crate) fn facts_of(&self, node: NodeId) -> Option<&ValueFacts> {
        let facts = &self.nodes[node].facts;
        (!facts.is_default()).then_some(facts)
    }

    /// The node of value `index` of entry `key` of the object at `node`, if there is one.
    pub(crate) fn child_key(&self, node: NodeId, key: &str, index: usize) -> Option<NodeId> {
        self.nodes[node].children.key(key, index)
    }

    /// The node of element `index` of the array at `node`, if there is one.
    pub(crate) fn child_element(&self, node: NodeId, index: usize) -> Option<NodeId> {
        self.nodes[node].children.element(index)
    }

    /// The node of `path`, if there is one.
    fn node_at(&self, path: &[PathSegment]) -> Option<NodeId> {
        path.iter()
            .try_fold(ROOT, |node, segment| self.nodes[node].children.get(segment))
    }

    /// The node of `segment` below `node`, added if there is none.
    pub(crate) fn child_or_insert(&mut self, node: NodeId, segment: &PathSegment) -> NodeId {
        if let Some(child) = self.nodes[node].children.get(segment) {
            return child;
        }
        let child = self.nodes.len();
        self.nodes.push(Node::default());
        self.nodes[node].children.insert(segment, child);
        child
    }

    /// The node of `path` below `node`, added with the nodes on the way if missing.
    pub(crate) fn descend_or_insert(&mut self, node: NodeId, path: &[PathSegment]) -> NodeId {
        path.iter()
            .fold(node, |node, segment| self.child_or_insert(node, segment))
    }

    fn set(&mut self, node: NodeId, facts: ValueFacts) {
        let was = !self.nodes[node].facts.is_default();
        let now = !facts.is_default();
        self.recorded = self.recorded + usize::from(now) - usize::from(was);
        self.nodes[node].facts = facts;
    }

    /// Changes the facts of the value at `node` with `change`.
    pub(crate) fn update(&mut self, node: NodeId, change: impl FnOnce(&mut ValueFacts)) {
        let mut facts = self.nodes[node].facts.clone();
        change(&mut facts);
        self.set(node, facts);
    }

    /// Takes the node `node` and everything below it out of the tree.
    fn drop_subtree(&mut self, node: NodeId) {
        let mut stack = vec![node];
        while let Some(node) = stack.pop() {
            let n = &mut self.nodes[node];
            if !n.facts.is_default() {
                self.recorded -= 1;
            }
            stack.extend(n.children.take_all());
        }
    }

    /// Removes the facts of everything inside the value at `node`, but not its own.
    pub(crate) fn clear_below(&mut self, node: NodeId) {
        for child in self.nodes[node].children.take_all() {
            self.drop_subtree(child);
        }
    }

    /// Values of entry `key` in the object at `object` were replaced: value `slot`, or all of
    /// them. Their facts, and those of anything inside them, are dropped.
    pub(crate) fn replaced(&mut self, object: NodeId, key: &str, slot: Option<usize>) {
        let children = &mut self.nodes[object].children;
        let dropped: Vec<NodeId> = match slot {
            Some(slot) => children.take_value(key, slot).into_iter().collect(),
            None => children
                .take_values(key)
                .into_iter()
                .map(|(_, c)| c)
                .collect(),
        };
        for child in dropped {
            self.drop_subtree(child);
        }
    }

    /// Entry `key` of the object at `object` became a `NO_IMPLICIT_ARRAYS` collection: its first
    /// `count` values are now the first elements of the array that is its only value. The
    /// array's location is that of its first element.
    pub(crate) fn collected(&mut self, object: NodeId, key: &str, count: usize) {
        let moved: Vec<(usize, NodeId)> = (0..count)
            .filter_map(|index| {
                let child = self.nodes[object].children.take_value(key, index)?;
                Some((index, child))
            })
            .collect();
        if moved.is_empty() {
            return;
        }
        let array = self.nodes.len();
        self.nodes.push(Node::default());
        if let Some(locations) = &mut self.locations {
            let at = locations.get(moved[0].1);
            locations.set(array, at);
        }
        let first = PathSegment::Key {
            key: key.to_owned(),
            index: 0,
        };
        self.nodes[object].children.insert(&first, array);
        for (index, child) in moved {
            let element = PathSegment::Index(index);
            self.nodes[array].children.insert(&element, child);
        }
    }

    /// The facts and locations of the value at `from` and of everything inside it, with paths
    /// relative to it.
    pub(crate) fn subtree(&self, from: &[PathSegment]) -> Subtree {
        let Some(node) = self.node_at(from) else {
            return Vec::new();
        };
        let mut facts = Vec::new();
        let mut stack = vec![(node, Vec::new())];
        while let Some((node, path)) = stack.pop() {
            let n = &self.nodes[node];
            let at = self.locations.as_ref().and_then(|l| l.get(node));
            if !n.facts.is_default() || at.is_some() {
                facts.push((path.clone(), n.facts.clone(), at));
            }
            for (segment, child) in n.children.iter() {
                let mut path = path.clone();
                path.push(segment);
                stack.push((child, path));
            }
        }
        facts
    }

    /// Records `facts`, a [`OutputFacts::subtree`], below the value at `to`.
    pub(crate) fn graft(&mut self, to: NodeId, facts: Subtree) {
        for (rest, f, at) in facts {
            let node = self.descend_or_insert(to, &rest);
            self.set(node, f);
            if let (Some(locations), Some(_)) = (&mut self.locations, at) {
                locations.set(node, at);
            }
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
        let a = facts.child_or_insert(ROOT, &key("a", 0));
        assert!(facts.is_empty());
        facts.update(a, |f| f.single_quoted = true);
        facts.update(a, |f| f.key_quoted = Some(true));
        assert_eq!(facts.get(&[key("a", 0)]).unwrap().key_quoted, Some(true));
        facts.update(a, |f| f.single_quoted = false);
        facts.update(a, |f| f.key_quoted = None);
        assert!(facts.is_empty());
        assert_eq!(facts, OutputFacts::new());
    }

    #[test]
    fn locations() {
        let document = b"a = 1\nb {\n  c = x\n}";
        let mut facts = OutputFacts::locating();
        assert!(facts.tracks_moves() && facts.records_locations());
        facts.locate(ROOT, 0, None);
        let b = facts.child_or_insert(ROOT, &key("b", 0));
        facts.locate(b, 8, Some(6));
        let c = facts.child_or_insert(b, &key("c", 0));
        facts.locate(c, 16, Some(12));
        fn line_col(facts: &OutputFacts, path: &[PathSegment], on_key: bool) -> (usize, usize) {
            let (p, file) = facts
                .position_of(path, on_key, b"a = 1\nb {\n  c = x\n}")
                .unwrap();
            assert_eq!(file, None);
            (p.line, p.column)
        }
        assert_eq!(line_col(&facts, &[], false), (1, 1));
        assert_eq!(line_col(&facts, &[key("b", 0), key("c", 0)], false), (3, 7));
        assert_eq!(line_col(&facts, &[key("b", 0), key("c", 0)], true), (3, 3));
        // A value without a location of its own: the nearest one around it, never its key.
        assert_eq!(line_col(&facts, &[key("b", 0), key("d", 0)], true), (2, 3));
        assert_eq!(line_col(&facts, &[key("z", 0)], false), (1, 1));
        // A collection array is where its first value was; a graft carries locations.
        facts.collected(b, "c", 1);
        assert_eq!(line_col(&facts, &[key("b", 0), key("c", 0)], false), (3, 7));
        let copied = facts.subtree(&[key("b", 0)]);
        let e = facts.child_or_insert(ROOT, &key("e", 0));
        facts.graft(e, copied);
        let inner = [key("e", 0), key("c", 0), PathSegment::Index(0)];
        assert_eq!(line_col(&facts, &inner, false), (3, 7));
        // Locations of an included file are positions in that file.
        let before = facts.enter_unit(Path::new("/i.conf"), b"\n  k = v");
        let k = facts.child_or_insert(ROOT, &key("k", 0));
        facts.locate(k, 7, Some(3));
        facts.leave_unit(before);
        let (p, file) = facts.position_of(&[key("k", 0)], false, document).unwrap();
        assert_eq!(
            ((p.line, p.column), file),
            ((2, 7), Some(PathBuf::from("/i.conf")))
        );
        // Without locations, nothing is recorded and nothing is found.
        let mut plain = OutputFacts::new();
        assert!(!plain.tracks_moves());
        plain.locate(ROOT, 3, None);
        assert_eq!(plain.position_of(&[], false, document), None);
    }

    #[test]
    fn replaced_collected_and_grafted_paths() {
        let mut facts = OutputFacts::new();
        facts.insert(vec![key("o", 0), key("a", 0)], sq());
        facts.insert(vec![key("o", 0), key("a", 1), PathSegment::Index(0)], sq());
        facts.insert(vec![key("o", 0), key("b", 0)], sq());
        let o = facts.child_key(ROOT, "o", 0).unwrap();
        facts.collected(o, "a", 1);
        assert!(
            facts
                .get(&[key("o", 0), key("a", 0), PathSegment::Index(0)])
                .is_some()
        );
        facts.replaced(o, "a", Some(1));
        assert_eq!(facts.iter().count(), 2);
        let copied = facts.subtree(&[key("o", 0)]);
        let p = facts.child_or_insert(ROOT, &key("p", 0));
        facts.graft(p, copied);
        assert!(facts.get(&[key("p", 0), key("b", 0)]).is_some());
        facts.clear_below(o);
        assert_eq!(facts.iter().count(), 2);
        let paths: Vec<_> = facts.iter().map(|(path, _)| path).collect();
        assert_eq!(
            paths,
            [
                vec![key("p", 0), key("a", 0), PathSegment::Index(0)],
                vec![key("p", 0), key("b", 0)],
            ]
        );
        facts.replaced(p, "a", None);
        facts.replaced(p, "b", None);
        assert!(facts.is_empty());
    }
}
