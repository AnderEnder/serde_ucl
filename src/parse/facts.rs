//! Facts remembered from parsing that the config and YAML output formats depend on (spec §10.1).
//!
//! Values are identified by their path from the root, as saved comments are
//! ([`super::AttachedComments`]). The facts are kept in a tree of those paths, one node per path
//! segment, so that the parser records a fact, and the emitter finds one, with one step from the
//! node of the value's container, at any depth. Only facts that differ from what the emitter
//! assumes for a value without facts are stored, so a document without single-quoted strings,
//! heredocs or escaped keys records nothing. When the duplicate rules of §8 move or replace
//! values, the parser updates the tree the same way it does the paths of comments.

use super::PathSegment;
use super::tree::Children;

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
}

pub(crate) type ValuePath = Vec<PathSegment>;

impl Default for OutputFacts {
    fn default() -> Self {
        Self {
            nodes: vec![Node::default()],
            recorded: 0,
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
    /// `count` values are now the first elements of the array that is its only value.
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

    /// The facts of the value at `from` and of everything inside it, with paths relative to it.
    pub(crate) fn subtree(&self, from: &[PathSegment]) -> Vec<(ValuePath, ValueFacts)> {
        let Some(node) = self.node_at(from) else {
            return Vec::new();
        };
        let mut facts = Vec::new();
        let mut stack = vec![(node, Vec::new())];
        while let Some((node, path)) = stack.pop() {
            let n = &self.nodes[node];
            if !n.facts.is_default() {
                facts.push((path.clone(), n.facts.clone()));
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
    pub(crate) fn graft(&mut self, to: NodeId, facts: Vec<(ValuePath, ValueFacts)>) {
        for (rest, f) in facts {
            let node = self.descend_or_insert(to, &rest);
            self.set(node, f);
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
