//! Facts remembered from parsing that the config and YAML output formats depend on (spec §10.1).
//!
//! The facts are kept in a tree with one node per value that has facts, or has such a value
//! inside it, so that the parser records a fact, and the emitter finds one, with one step from the
//! node of the value's container, at any depth. Only facts that differ from what the emitter
//! assumes for a value without facts are stored, so a document without single-quoted strings,
//! heredocs or escaped keys records nothing. When the duplicate rules of §8 move or replace
//! values, the parser updates the tree the same way it does the paths of comments.
//!
//! A node is found by the position of its value: value `slot` of the object's entry at position
//! `entry`, or element `index` of an array (clean-room work item C13, decision 1). The parser
//! never removes an entry, so positions do not change while it parses, and it records and finds
//! facts without comparing or copying keys. Each node of an entry's value also keeps the entry's
//! key, which the emitter compares with the key at that position of the object it writes: when
//! they differ, as when the caller removed an earlier entry after the parse, the value is
//! written as if it had no facts. [`OutputFacts::get`] and [`OutputFacts::iter`] give facts by key
//! path, as the saved comments are given ([`super::AttachedComments`]).
//!
//! The same tree can also record where each value was written ([`Location`]), for the
//! positions of deserialization errors. Only a parse run to find such a position records them
//! ([`OutputFacts::locating`]): then every value has a node, and the locations move with the
//! nodes when values are moved, replaced or copied, as the facts do.

use super::PathSegment;
use super::error::position_at;
use super::tree::KeyRef;
use crate::error::Position;
use crate::value::{KeyCopy, UclValue, Value};
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

/// A node of [`OutputFacts`], which stands for a value.
pub(crate) type NodeId = usize;

/// The node of the root.
pub(crate) const ROOT: NodeId = 0;

/// One step from a value to a value inside it, by position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Pos {
    /// Value `slot` of the entry at position `entry` of an object, whose key is `key`.
    Entry {
        entry: usize,
        key: KeyCopy,
        slot: usize,
    },
    /// Element `index` of an array.
    Element(usize),
}

impl Pos {
    fn segment(&self) -> PathSegment {
        match self {
            Pos::Entry { key, slot, .. } => PathSegment::Key {
                key: key.to_string(),
                index: *slot,
            },
            Pos::Element(index) => PathSegment::Index(*index),
        }
    }
}

/// The value at `path` inside `value`, by position.
pub(crate) fn get<'v, 't>(value: &'v Value<'t>, path: &[Pos]) -> Option<&'v Value<'t>> {
    path.iter().try_fold(value, |value, pos| match pos {
        Pos::Entry { entry, slot, .. } => value
            .as_object()?
            .get_index(*entry)?
            .1
            .slots()
            .get(*slot)
            .map(crate::value::Slot::value),
        Pos::Element(index) => value.as_array()?.get(*index),
    })
}

/// The positions of the value at the key path `path` inside `value`.
fn positions(value: &UclValue, path: &[PathSegment]) -> Option<Vec<Pos>> {
    let mut value = value;
    let mut out = Vec::with_capacity(path.len());
    for segment in path {
        match segment {
            PathSegment::Key { key, index } => {
                let object = value.as_object()?;
                let entry = object.index_of(key)?;
                let (name, found) = object.get_index(entry)?;
                value = found.slots().get(*index)?.value();
                out.push(Pos::Entry {
                    entry,
                    key: KeyCopy::from(name),
                    slot: *index,
                });
            }
            PathSegment::Index(index) => {
                value = value.as_array()?.get(*index)?;
                out.push(Pos::Element(*index));
            }
        }
    }
    Some(out)
}

/// The most children of each kind a node keeps in a vector, searched in order.
const SMALL: usize = 8;

#[derive(Debug, Clone, Default)]
struct Node {
    facts: ValueFacts,
    children: Children,
}

/// The nodes of the values inside a node's value: of an object's entries, and of an array's
/// elements. A node without children allocates nothing.
#[derive(Debug, Clone, Default)]
struct Children {
    entries: Entries,
    elements: Elements,
}

/// The nodes of the values of an object's entries.
#[derive(Debug, Clone)]
enum Entries {
    /// Up to [`SMALL`] nodes, in any order.
    Small(Vec<EntryChild>),
    /// The nodes of the values of the entry at position `n` are `by_entry[n]`.
    Large(Vec<Option<EntryChildren>>),
}

impl Default for Entries {
    fn default() -> Self {
        Entries::Small(Vec::new())
    }
}

/// The node `child` of value `slot` of the entry at position `entry`, whose key is `key`.
#[derive(Debug, Clone)]
struct EntryChild {
    entry: usize,
    slot: usize,
    child: NodeId,
    key: KeyCopy,
}

/// The nodes of the values of one entry, whose key is `key`: that of value `n` is `slots[n]`.
#[derive(Debug, Clone)]
struct EntryChildren {
    key: KeyCopy,
    slots: Vec<Option<NodeId>>,
}

/// The nodes of an array's elements.
#[derive(Debug, Clone)]
enum Elements {
    /// Up to [`SMALL`] nodes `(index, child)`, in any order.
    Small(Vec<(usize, NodeId)>),
    /// The node of element `n` is `by_index[n]`.
    Large(Vec<Option<NodeId>>),
}

impl Default for Elements {
    fn default() -> Self {
        Elements::Small(Vec::new())
    }
}

/// Makes `value` element `index` of `items`, growing it with `None`s.
fn set_at<T>(items: &mut Vec<Option<T>>, index: usize, value: T) {
    if items.len() <= index {
        items.resize_with(index + 1, || None);
    }
    items[index] = Some(value);
}

impl Children {
    /// The node of value `slot` of the entry at position `entry`, with the entry's key.
    fn entry(&self, entry: usize, slot: usize) -> Option<(NodeId, &KeyCopy)> {
        match &self.entries {
            Entries::Small(items) => items
                .iter()
                .find(|c| c.entry == entry && c.slot == slot)
                .map(|c| (c.child, &c.key)),
            Entries::Large(by_entry) => {
                let values = by_entry.get(entry)?.as_ref()?;
                Some((values.slots.get(slot).copied().flatten()?, &values.key))
            }
        }
    }

    /// The node of value `slot` of the entry whose key is `key`.
    fn entry_by_key(&self, key: &str, slot: usize) -> Option<NodeId> {
        match &self.entries {
            Entries::Small(items) => items
                .iter()
                .find(|c| c.slot == slot && c.key.as_bytes() == key.as_bytes())
                .map(|c| c.child),
            Entries::Large(by_entry) => by_entry
                .iter()
                .flatten()
                .find(|values| values.key.as_bytes() == key.as_bytes())
                .and_then(|values| values.slots.get(slot).copied().flatten()),
        }
    }

    /// Makes `child` the node of value `slot` of the entry at position `entry`, whose key is
    /// `key`. The key is copied only for an entry without a node yet.
    fn insert_entry<K: KeyRef + ?Sized>(
        &mut self,
        entry: usize,
        key: &K,
        slot: usize,
        child: NodeId,
    ) {
        match &mut self.entries {
            Entries::Small(items) => {
                if let Some(item) = items
                    .iter_mut()
                    .find(|c| c.entry == entry && c.slot == slot)
                {
                    item.child = child;
                } else if items.len() < SMALL {
                    items.push(EntryChild {
                        entry,
                        slot,
                        child,
                        key: key.to_key(),
                    });
                } else {
                    let mut by_entry: Vec<Option<EntryChildren>> = Vec::new();
                    for c in items.drain(..) {
                        Self::set_large(&mut by_entry, c.entry, &c.key, c.slot, c.child);
                    }
                    Self::set_large(&mut by_entry, entry, key, slot, child);
                    self.entries = Entries::Large(by_entry);
                }
            }
            Entries::Large(by_entry) => Self::set_large(by_entry, entry, key, slot, child),
        }
    }

    fn set_large<K: KeyRef + ?Sized>(
        by_entry: &mut Vec<Option<EntryChildren>>,
        entry: usize,
        key: &K,
        slot: usize,
        child: NodeId,
    ) {
        if by_entry.len() <= entry {
            by_entry.resize_with(entry + 1, || None);
        }
        let values = by_entry[entry].get_or_insert_with(|| EntryChildren {
            key: key.to_key(),
            slots: Vec::new(),
        });
        set_at(&mut values.slots, slot, child);
    }

    /// The node of element `index`.
    fn element(&self, index: usize) -> Option<NodeId> {
        match &self.elements {
            Elements::Small(items) => items
                .iter()
                .find(|(i, _)| *i == index)
                .map(|&(_, child)| child),
            Elements::Large(by_index) => by_index.get(index).copied().flatten(),
        }
    }

    /// Makes `child` the node of element `index`.
    fn insert_element(&mut self, index: usize, child: NodeId) {
        match &mut self.elements {
            Elements::Small(items) => {
                if let Some(item) = items.iter_mut().find(|(i, _)| *i == index) {
                    item.1 = child;
                } else if items.len() < SMALL {
                    items.push((index, child));
                } else {
                    let mut by_index = Vec::new();
                    for (i, c) in items.drain(..) {
                        set_at(&mut by_index, i, c);
                    }
                    set_at(&mut by_index, index, child);
                    self.elements = Elements::Large(by_index);
                }
            }
            Elements::Large(by_index) => set_at(by_index, index, child),
        }
    }

    /// The node of the value at the key path segment `segment`.
    fn by_key(&self, segment: &PathSegment) -> Option<NodeId> {
        match segment {
            PathSegment::Key { key, index } => self.entry_by_key(key, *index),
            PathSegment::Index(index) => self.element(*index),
        }
    }

    fn get(&self, pos: &Pos) -> Option<NodeId> {
        match pos {
            Pos::Entry { entry, slot, .. } => self.entry(*entry, *slot).map(|(child, _)| child),
            Pos::Element(index) => self.element(*index),
        }
    }

    /// Removes the node of value `slot` of the entry at position `entry`, and returns it.
    fn take_value(&mut self, entry: usize, slot: usize) -> Option<NodeId> {
        match &mut self.entries {
            Entries::Small(items) => {
                let at = items
                    .iter()
                    .position(|c| c.entry == entry && c.slot == slot)?;
                Some(items.remove(at).child)
            }
            Entries::Large(by_entry) => by_entry
                .get_mut(entry)?
                .as_mut()?
                .slots
                .get_mut(slot)?
                .take(),
        }
    }

    /// Removes the nodes of every value of the entry at position `entry`, and returns them with
    /// their value indices, in the order of the indices.
    fn take_values(&mut self, entry: usize) -> Vec<(usize, NodeId)> {
        match &mut self.entries {
            Entries::Small(items) => {
                let mut taken = Vec::new();
                items.retain(|c| {
                    let keep = c.entry != entry;
                    if !keep {
                        taken.push((c.slot, c.child));
                    }
                    keep
                });
                taken.sort_unstable();
                taken
            }
            Entries::Large(by_entry) => match by_entry.get_mut(entry).and_then(Option::take) {
                Some(values) => values
                    .slots
                    .into_iter()
                    .enumerate()
                    .filter_map(|(slot, child)| Some((slot, child?)))
                    .collect(),
                None => Vec::new(),
            },
        }
    }

    /// Every child with its position, in no particular order.
    fn iter(&self) -> Vec<(Pos, NodeId)> {
        let mut all = Vec::new();
        match &self.entries {
            Entries::Small(items) => all.extend(items.iter().map(|c| {
                let pos = Pos::Entry {
                    entry: c.entry,
                    key: c.key.clone(),
                    slot: c.slot,
                };
                (pos, c.child)
            })),
            Entries::Large(by_entry) => {
                for (entry, values) in by_entry.iter().enumerate() {
                    let Some(values) = values else { continue };
                    for (slot, child) in values.slots.iter().enumerate() {
                        if let Some(child) = child {
                            let pos = Pos::Entry {
                                entry,
                                key: values.key.clone(),
                                slot,
                            };
                            all.push((pos, *child));
                        }
                    }
                }
            }
        }
        match &self.elements {
            Elements::Small(items) => {
                all.extend(
                    items
                        .iter()
                        .map(|&(index, child)| (Pos::Element(index), child)),
                );
            }
            Elements::Large(by_index) => all.extend(
                by_index
                    .iter()
                    .enumerate()
                    .filter_map(|(index, child)| Some((Pos::Element(index), (*child)?))),
            ),
        }
        all
    }

    /// Removes every child, and returns them.
    fn take_all(&mut self) -> Vec<NodeId> {
        let mut all: Vec<NodeId> = match std::mem::take(&mut self.entries) {
            Entries::Small(items) => items.into_iter().map(|c| c.child).collect(),
            Entries::Large(by_entry) => by_entry
                .into_iter()
                .flatten()
                .flat_map(|values| values.slots.into_iter().flatten())
                .collect(),
        };
        match std::mem::take(&mut self.elements) {
            Elements::Small(items) => all.extend(items.into_iter().map(|(_, child)| child)),
            Elements::Large(by_index) => all.extend(by_index.into_iter().flatten()),
        }
        all
    }
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

/// The output facts of a parsed document: what [`crate::emit::Emitter`] needs to write the
/// document as libucl does (spec §10.1). [`super::Parser::output_facts`] gives those of the last
/// parse.
///
/// The facts of a value are found by its position in the parsed document, and those of an
/// entry's value hold its entry's key: an emitter writes a value whose position now holds
/// another key, because the value was changed after the parse, as if it had no facts.
#[derive(Debug, Clone, Default)]
pub struct OutputFacts {
    /// The tree; `nodes[ROOT]` is the root. A node taken out of the tree stays in the vector,
    /// unused, until [`OutputFacts::clear`]. Empty until the first node is needed
    /// ([`OutputFacts::nodes_mut`]), so that a parse that records nothing allocates nothing
    /// for its facts (clean-room work item C12); an empty vector stands for a root without
    /// facts or children.
    nodes: Vec<Node>,
    /// The number of nodes in the tree whose facts are not the default.
    recorded: usize,
    /// Set when the locations of values are recorded.
    locations: Option<Box<Locations>>,
}

/// The facts and locations of a value and of everything inside it, with positions relative to
/// it ([`OutputFacts::subtree`]).
pub(crate) type Subtree = Vec<(Vec<Pos>, ValueFacts, Option<Location>)>;

impl PartialEq for OutputFacts {
    fn eq(&self, other: &Self) -> bool {
        self.iter().eq(other.iter())
    }
}

impl Eq for OutputFacts {}

/// Opaque position in output facts, used by the read-only C adapter.
#[doc(hidden)]
#[derive(Clone, Copy)]
pub struct FactsCursor(Option<std::num::NonZeroUsize>);

impl FactsCursor {
    /// Facts of the root value.
    pub fn root() -> Self {
        Self(std::num::NonZeroUsize::new(ROOT + 1))
    }
    /// Facts of an object's slot at its existing entry position.
    pub fn entry(self, facts: &OutputFacts, entry: usize, key: &str, slot: usize) -> Self {
        Self(
            self.0
                .and_then(|node| facts.child_entry(node.get() - 1, entry, key, slot))
                .and_then(|node| std::num::NonZeroUsize::new(node + 1)),
        )
    }
    /// Facts of an array element.
    pub fn element(self, facts: &OutputFacts, index: usize) -> Self {
        Self(
            self.0
                .and_then(|node| facts.child_element(node.get() - 1, index))
                .and_then(|node| std::num::NonZeroUsize::new(node + 1)),
        )
    }
    /// Facts recorded for this value.
    pub fn get(self, facts: &OutputFacts) -> Option<&ValueFacts> {
        self.0.and_then(|node| facts.facts_of(node.get() - 1))
    }
    pub(crate) fn node(self) -> Option<usize> {
        self.0.map(|node| node.get() - 1)
    }
}

impl OutputFacts {
    /// No facts.
    pub fn new() -> Self {
        Self::default()
    }

    /// Node `node`; `None` only for the root before any node was added.
    fn node(&self, node: NodeId) -> Option<&Node> {
        self.nodes.get(node)
    }

    /// The nodes, with the root added if there is none yet.
    fn nodes_mut(&mut self) -> &mut Vec<Node> {
        if self.nodes.is_empty() {
            self.nodes.push(Node::default());
        }
        &mut self.nodes
    }

    /// True if no value has a fact recorded.
    pub fn is_empty(&self) -> bool {
        self.recorded == 0
    }

    /// The facts of the value at `path`, if any were recorded: the path of the value in the
    /// parsed document, by key.
    pub fn get(&self, path: &[PathSegment]) -> Option<&ValueFacts> {
        self.facts_of(self.node_by_keys(path)?)
    }

    /// Sets the facts of the value at `path` in `root`, the value the facts are for; default
    /// facts remove those recorded. Returns false, and changes nothing, when `root` has no value
    /// at `path`.
    pub fn insert(&mut self, root: &UclValue, path: &[PathSegment], facts: ValueFacts) -> bool {
        let Some(path) = positions(root, path) else {
            return false;
        };
        let node = if facts.is_default() {
            self.node_at(&path)
        } else {
            Some(self.descend_or_insert(ROOT, &path))
        };
        if let Some(node) = node {
            self.set(node, facts);
        }
        true
    }

    /// Every recorded path and its facts, in path order.
    pub fn iter(&self) -> impl Iterator<Item = (Vec<PathSegment>, &ValueFacts)> {
        let mut recorded = Vec::with_capacity(self.recorded);
        let mut stack = vec![(ROOT, Vec::new())];
        while let Some((node, path)) = stack.pop() {
            let Some(n) = self.node(node) else {
                continue;
            };
            for (pos, child) in n.children.iter() {
                let mut path = path.clone();
                path.push(pos.segment());
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

    /// Where the value at the key path `path` was written, as a position in its input unit and
    /// that unit's file (`None` for the document, whose bytes are `document`); with `key`, where
    /// its key was written, if it has one. A value without a location of its own, such as one
    /// the recording parse did not produce, gets that of the nearest value around it that has
    /// one.
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
            let Some(child) = self.node(node).and_then(|n| n.children.by_key(segment)) else {
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
        let facts = &self.node(node)?.facts;
        (!facts.is_default()).then_some(facts)
    }

    /// The node of value `slot` of the entry at position `entry` of the object at `node`, if
    /// there is one and its entry's key is `key`: the emitter's lookup, which gives nothing for
    /// a value whose object was changed after the parse so that another key is at `entry`.
    pub(crate) fn child_entry<K: KeyRef + ?Sized>(
        &self,
        node: NodeId,
        entry: usize,
        key: &K,
        slot: usize,
    ) -> Option<NodeId> {
        let (child, stored) = self.node(node)?.children.entry(entry, slot)?;
        (stored.as_bytes() == key.bytes()).then_some(child)
    }

    /// The node of element `index` of the array at `node`, if there is one.
    pub(crate) fn child_element(&self, node: NodeId, index: usize) -> Option<NodeId> {
        self.node(node)?.children.element(index)
    }

    /// The node of the value at the key path `path`, if there is one.
    pub(crate) fn node_by_keys(&self, path: &[PathSegment]) -> Option<NodeId> {
        path.iter().try_fold(ROOT, |node, segment| {
            self.node(node)?.children.by_key(segment)
        })
    }

    /// The node of `path`, if there is one.
    pub(crate) fn node_at(&self, path: &[Pos]) -> Option<NodeId> {
        path.iter()
            .try_fold(ROOT, |node, pos| self.node(node)?.children.get(pos))
    }

    /// The node of `pos` below `node`, added if there is none.
    pub(crate) fn child_or_insert(&mut self, node: NodeId, pos: &Pos) -> NodeId {
        match pos {
            Pos::Entry { entry, key, slot } => self.entry_child_or_insert(node, *entry, key, *slot),
            Pos::Element(index) => self.element_child_or_insert(node, *index),
        }
    }

    /// The node of value `slot` of the entry at position `entry`, whose key is `key`, of the
    /// object at `node`, added if there is none. The key is copied only for a new node.
    pub(crate) fn entry_child_or_insert<K: KeyRef + ?Sized>(
        &mut self,
        node: NodeId,
        entry: usize,
        key: &K,
        slot: usize,
    ) -> NodeId {
        let nodes = self.nodes_mut();
        if let Some((child, _)) = nodes[node].children.entry(entry, slot) {
            return child;
        }
        let child = nodes.len();
        nodes.push(Node::default());
        nodes[node].children.insert_entry(entry, key, slot, child);
        child
    }

    /// The node of element `index` of the array at `node`, added if there is none.
    pub(crate) fn element_child_or_insert(&mut self, node: NodeId, index: usize) -> NodeId {
        let nodes = self.nodes_mut();
        if let Some(child) = nodes[node].children.element(index) {
            return child;
        }
        let child = nodes.len();
        nodes.push(Node::default());
        nodes[node].children.insert_element(index, child);
        child
    }

    /// The node of `path` below `node`, added with the nodes on the way if missing.
    pub(crate) fn descend_or_insert(&mut self, node: NodeId, path: &[Pos]) -> NodeId {
        path.iter()
            .fold(node, |node, pos| self.child_or_insert(node, pos))
    }

    fn set(&mut self, node: NodeId, facts: ValueFacts) {
        let nodes = self.nodes_mut();
        let was = !nodes[node].facts.is_default();
        let now = !facts.is_default();
        nodes[node].facts = facts;
        self.recorded = self.recorded + usize::from(now) - usize::from(was);
    }

    /// Changes the facts of the value at `node` with `change`.
    pub(crate) fn update(&mut self, node: NodeId, change: impl FnOnce(&mut ValueFacts)) {
        let mut facts = self.nodes_mut()[node].facts.clone();
        change(&mut facts);
        self.set(node, facts);
    }

    /// Takes the node `node` and everything below it out of the tree.
    fn drop_subtree(&mut self, node: NodeId) {
        self.nodes_mut();
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
        for child in self.nodes_mut()[node].children.take_all() {
            self.drop_subtree(child);
        }
    }

    /// Values of the entry at position `entry` in the object at `object` were replaced: value
    /// `slot`, or all of them. Their facts, and those of anything inside them, are dropped.
    pub(crate) fn replaced(&mut self, object: NodeId, entry: usize, slot: Option<usize>) {
        let children = &mut self.nodes_mut()[object].children;
        let dropped: Vec<NodeId> = match slot {
            Some(slot) => children.take_value(entry, slot).into_iter().collect(),
            None => children
                .take_values(entry)
                .into_iter()
                .map(|(_, c)| c)
                .collect(),
        };
        for child in dropped {
            self.drop_subtree(child);
        }
    }

    /// The entry at position `entry`, whose key is `key`, of the object at `object` became a
    /// `NO_IMPLICIT_ARRAYS` collection: its first `count` values are now the first elements of
    /// the array that is its only value. The array's location is that of its first element.
    pub(crate) fn collected<K: KeyRef + ?Sized>(
        &mut self,
        object: NodeId,
        entry: usize,
        key: &K,
        count: usize,
    ) {
        let moved: Vec<(usize, NodeId)> = (0..count)
            .filter_map(|index| {
                let child = self.nodes_mut()[object].children.take_value(entry, index)?;
                Some((index, child))
            })
            .collect();
        if moved.is_empty() {
            return;
        }
        let nodes = self.nodes_mut();
        let array = nodes.len();
        nodes.push(Node::default());
        if let Some(locations) = &mut self.locations {
            let at = locations.get(moved[0].1);
            locations.set(array, at);
        }
        self.nodes[object]
            .children
            .insert_entry(entry, key, 0, array);
        for (index, child) in moved {
            self.nodes[array].children.insert_element(index, child);
        }
    }

    /// The facts and locations of the value at `node` and of everything inside it, with
    /// positions relative to it.
    pub(crate) fn subtree(&self, node: NodeId) -> Subtree {
        let mut facts = Vec::new();
        let mut stack = vec![(node, Vec::new())];
        while let Some((node, path)) = stack.pop() {
            let at = self.locations.as_ref().and_then(|l| l.get(node));
            let Some(n) = self.node(node) else {
                if at.is_some() {
                    facts.push((path, ValueFacts::default(), at));
                }
                continue;
            };
            if !n.facts.is_default() || at.is_some() {
                facts.push((path.clone(), n.facts.clone(), at));
            }
            for (pos, child) in n.children.iter() {
                let mut path = path.clone();
                path.push(pos);
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
    use crate::value::UclObject;

    fn key(k: &str, index: usize) -> PathSegment {
        PathSegment::Key {
            key: k.into(),
            index,
        }
    }

    fn entry(entry: usize, k: &str, slot: usize) -> Pos {
        Pos::Entry {
            entry,
            key: KeyCopy::from(k),
            slot,
        }
    }

    fn sq() -> ValueFacts {
        ValueFacts {
            single_quoted: true,
            ..ValueFacts::default()
        }
    }

    /// `o { a = x; a = [y]; b = z }`, and the root `{ o = ..., p = { a = ..., b = ... } }`.
    fn document() -> UclValue {
        let mut o = UclObject::new();
        o.append("a", UclValue::String("x".into()));
        o.append(
            "a",
            UclValue::Array(vec![UclValue::String("y".into())].into()),
        );
        o.append("b", UclValue::String("z".into()));
        let mut root = UclObject::new();
        root.insert("o", UclValue::Object(o.clone()));
        root.insert("p", UclValue::Object(o));
        UclValue::Object(root)
    }

    #[test]
    fn default_facts_are_not_stored() {
        let root = document();
        let mut facts = OutputFacts::new();
        assert!(facts.insert(&root, &[key("o", 0)], ValueFacts::default()));
        assert!(facts.is_empty());
        let a = facts.child_or_insert(ROOT, &entry(0, "o", 0));
        assert!(facts.is_empty());
        facts.update(a, |f| f.single_quoted = true);
        facts.update(a, |f| f.key_quoted = Some(true));
        assert_eq!(facts.get(&[key("o", 0)]).unwrap().key_quoted, Some(true));
        facts.update(a, |f| f.single_quoted = false);
        facts.update(a, |f| f.key_quoted = None);
        assert!(facts.is_empty());
        assert_eq!(facts, OutputFacts::new());
        // A path the value does not have.
        assert!(!facts.insert(&root, &[key("q", 0)], sq()));
        assert!(!facts.insert(&root, &[key("o", 0), key("a", 2)], sq()));
        assert!(facts.is_empty());
    }

    #[test]
    fn found_by_position_and_checked_against_the_key() {
        let root = document();
        let mut facts = OutputFacts::new();
        assert!(facts.insert(&root, &[key("p", 0), key("b", 0)], sq()));
        let p = facts.child_entry(ROOT, 1, "p", 0).unwrap();
        let b = facts.child_entry(p, 1, "b", 0).unwrap();
        assert_eq!(facts.facts_of(b), Some(&sq()));
        // Another key at the position, or the key at another position: no facts.
        assert_eq!(facts.child_entry(p, 1, "a", 0), None);
        assert_eq!(facts.child_entry(p, 0, "b", 0), None);
        assert_eq!(facts.child_entry(ROOT, 1, "o", 0), None);
        assert_eq!(facts.get(&[key("p", 0), key("b", 0)]), Some(&sq()));
        assert_eq!(facts.get(&[key("o", 0), key("b", 0)]), None);
    }

    #[test]
    fn many_children_by_position() {
        let mut facts = OutputFacts::new();
        let mut nodes = Vec::new();
        for i in 0..3 * SMALL {
            let pos = entry(2 * i, &format!("k{i}"), i % 2);
            nodes.push(facts.child_or_insert(ROOT, &pos));
            facts.element_child_or_insert(nodes[i], 5 * i);
        }
        for (i, &node) in nodes.iter().enumerate() {
            assert_eq!(
                facts.child_entry(ROOT, 2 * i, &format!("k{i}"), i % 2),
                Some(node)
            );
            assert_eq!(facts.child_entry(ROOT, 2 * i, "other", i % 2), None);
            assert_eq!(
                facts.child_entry(ROOT, 2 * i + 1, &format!("k{i}"), i % 2),
                None
            );
            assert!(facts.child_element(node, 5 * i).is_some());
            assert_eq!(facts.child_element(node, 5 * i + 1), None);
            let by_key = facts.node_by_keys(&[key(&format!("k{i}"), i % 2)]);
            assert_eq!(by_key, Some(node));
        }
        facts.replaced(ROOT, 4, None);
        assert_eq!(facts.child_entry(ROOT, 4, "k2", 0), None);
        assert_eq!(facts.child_entry(ROOT, 6, "k3", 1), Some(nodes[3]));
    }

    #[test]
    fn locations() {
        let document = b"a = 1\nb {\n  c = x\n}";
        let mut facts = OutputFacts::locating();
        assert!(facts.tracks_moves() && facts.records_locations());
        facts.locate(ROOT, 0, None);
        let b = facts.child_or_insert(ROOT, &entry(1, "b", 0));
        facts.locate(b, 8, Some(6));
        let c = facts.child_or_insert(b, &entry(0, "c", 0));
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
        facts.collected(b, 0, &KeyCopy::from("c"), 1);
        assert_eq!(line_col(&facts, &[key("b", 0), key("c", 0)], false), (3, 7));
        let copied = facts.subtree(b);
        let e = facts.child_or_insert(ROOT, &entry(2, "e", 0));
        facts.graft(e, copied);
        let inner = [key("e", 0), key("c", 0), PathSegment::Index(0)];
        assert_eq!(line_col(&facts, &inner, false), (3, 7));
        // Locations of an included file are positions in that file.
        let before = facts.enter_unit(Path::new("/i.conf"), b"\n  k = v");
        let k = facts.child_or_insert(ROOT, &entry(3, "k", 0));
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
    fn replaced_collected_and_grafted_values() {
        let root = document();
        let mut facts = OutputFacts::new();
        facts.insert(&root, &[key("o", 0), key("a", 0)], sq());
        facts.insert(
            &root,
            &[key("o", 0), key("a", 1), PathSegment::Index(0)],
            sq(),
        );
        facts.insert(&root, &[key("o", 0), key("b", 0)], sq());
        let o = facts.child_entry(ROOT, 0, "o", 0).unwrap();
        facts.collected(o, 0, &KeyCopy::from("a"), 1);
        assert!(
            facts
                .get(&[key("o", 0), key("a", 0), PathSegment::Index(0)])
                .is_some()
        );
        facts.replaced(o, 0, Some(1));
        assert_eq!(facts.iter().count(), 2);
        let copied = facts.subtree(o);
        let p = facts.child_or_insert(ROOT, &entry(1, "p", 0));
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
        facts.replaced(p, 0, None);
        facts.replaced(p, 1, None);
        assert!(facts.is_empty());
    }
}
