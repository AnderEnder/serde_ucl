//! Saved comments and the values they attach to, under `SAVE_COMMENTS` (spec §12.5).
//!
//! Comments are saved in input order. Those not yet attached go *before* the next value that is
//! created. When a container closes with its bracket, and at the end of input, they go *after*
//! the value created most recently (the root, if none was). A value has one list of comments;
//! whether it is a *before* or an *after* list is fixed by the first comment attached, and later
//! ones are appended to it. Comments in the group before a value on a following line (§1.6) are
//! held back until that value has been created, so they attach to whatever comes after it
//! (QUESTIONS.md #17).
//!
//! Values are identified by their path from the root. When the duplicate rules of §8 move or
//! remove values that already have comments, the paths are updated: comments follow a value into
//! a `NO_IMPLICIT_ARRAYS` collection and disappear with a value that is replaced. Paths are
//! passed around as [`PathRef`]s, which share their prefixes, and the values that have comments
//! are kept as a tree of paths ([`CommentGroups`]), so that creating a value and attaching
//! comments to it cost the same at any depth. The paths of [`AttachedComments`] are written out
//! only when they are asked for.
//!
//! Comments are collected across input units (§9.4): pending comments may attach to a value of
//! an included file. Each unit's comments are turned into [`Comment`]s, text and position in
//! that unit's input, before the parser switches to another unit.

use super::tree::Children;
use super::{AttachedComments, Comment, CommentPlacement, PathSegment};
use crate::error::Position;
use std::cell::Cell;
use std::rc::Rc;

pub(crate) type ValuePath = Vec<PathSegment>;

/// A value's path from the root, as a list that shares its prefix with the path of the container
/// the value is in: extending a path by one segment costs the same at any depth.
/// [`PathRef::to_vec`] writes the path out.
#[derive(Debug, Clone, Default)]
pub(crate) struct PathRef(Option<Rc<PathNode>>);

#[derive(Debug)]
struct PathNode {
    parent: PathRef,
    segment: PathSegment,
    len: usize,
    /// The path's node in the [`CommentGroups`] of the parse, once looked up there
    /// ([`Notes::place_of`]).
    place: Cell<Option<usize>>,
}

impl Drop for PathNode {
    /// Drops the nodes of a long path one after the other rather than recursively.
    fn drop(&mut self) {
        let mut parent = self.parent.0.take();
        while let Some(node) = parent {
            match Rc::try_unwrap(node) {
                Ok(mut node) => parent = node.parent.0.take(),
                Err(_) => break,
            }
        }
    }
}

impl PathRef {
    /// The path of the root, which is empty.
    pub(crate) fn root() -> Self {
        Self(None)
    }

    /// The number of segments.
    pub(crate) fn len(&self) -> usize {
        self.0.as_ref().map_or(0, |node| node.len)
    }

    /// This path followed by `segment`.
    pub(crate) fn child(&self, segment: PathSegment) -> Self {
        Self(Some(Rc::new(PathNode {
            parent: self.clone(),
            segment,
            len: self.len() + 1,
            place: Cell::new(None),
        })))
    }

    /// This path followed by `segments`.
    pub(crate) fn join(&self, segments: impl IntoIterator<Item = PathSegment>) -> Self {
        segments
            .into_iter()
            .fold(self.clone(), |path, segment| path.child(segment))
    }

    /// The path written out, root first.
    pub(crate) fn to_vec(&self) -> ValuePath {
        let mut out = Vec::with_capacity(self.len());
        let mut node = self.0.as_deref();
        while let Some(n) = node {
            out.push(n.segment.clone());
            node = n.parent.0.as_deref();
        }
        out.reverse();
        out
    }
}

/// How far the current unit's input has been scanned for line numbers.
#[derive(Debug, Clone, Copy)]
pub(crate) struct LineCursor {
    line: usize,
    line_start: usize,
    scanned: usize,
}

impl LineCursor {
    fn new() -> Self {
        Self {
            line: 1,
            line_start: 0,
            scanned: 0,
        }
    }
}

/// The node of the root's path in [`CommentGroups`].
const ROOT: usize = 0;

/// The values that have saved comments, as a tree of their paths with one node per path
/// segment, and the comment groups attached to them. The paths of [`AttachedComments`] are
/// written out from the tree only when they are asked for ([`CommentGroups::attached`]).
#[derive(Debug, Clone)]
pub(crate) struct CommentGroups {
    /// `nodes[ROOT]` is the root's path.
    nodes: Vec<Place>,
    /// The groups in the order they were made; `None` for one dropped with its value.
    groups: Vec<Option<Group>>,
}

/// A node of [`CommentGroups`]: a value's path.
#[derive(Debug, Clone, Default)]
struct Place {
    /// The node of the path without its last segment, and that segment; `None` for the root.
    parent: Option<(usize, PathSegment)>,
    /// The nodes of the values inside the value at this path.
    children: Children,
    /// The group of comments attached to the value.
    group: Option<usize>,
    /// Taken out of the tree with a value that was replaced. A path that remembers this node
    /// looks its place up again ([`Notes::place_of`]).
    gone: bool,
}

#[derive(Debug, Clone)]
struct Group {
    place: usize,
    placement: CommentPlacement,
    /// Indices into the saved comments.
    comments: Vec<usize>,
}

impl Default for CommentGroups {
    fn default() -> Self {
        Self {
            nodes: vec![Place::default()],
            groups: Vec::new(),
        }
    }
}

impl CommentGroups {
    /// The number of groups.
    pub(crate) fn len(&self) -> usize {
        self.groups.iter().flatten().count()
    }

    /// The groups with their paths written out, in the order they were made.
    pub(crate) fn attached(&self) -> Vec<AttachedComments> {
        self.groups
            .iter()
            .flatten()
            .map(|group| AttachedComments {
                path: self.path(group.place),
                placement: group.placement,
                comments: group.comments.clone(),
            })
            .collect()
    }

    fn path(&self, mut place: usize) -> ValuePath {
        let mut path = Vec::new();
        while let Some((parent, segment)) = &self.nodes[place].parent {
            path.push(segment.clone());
            place = *parent;
        }
        path.reverse();
        path
    }

    fn child_or_insert(&mut self, place: usize, segment: &PathSegment) -> usize {
        if let Some(child) = self.nodes[place].children.get(segment) {
            return child;
        }
        let child = self.nodes.len();
        self.nodes.push(Place {
            parent: Some((place, segment.clone())),
            ..Place::default()
        });
        self.nodes[place].children.insert(segment, child);
        child
    }

    /// Takes the value at `place`, already unlinked from its parent, and everything inside it out
    /// of the tree with their groups.
    fn drop_value(&mut self, place: usize) {
        let mut stack = vec![place];
        while let Some(node) = stack.pop() {
            let n = &mut self.nodes[node];
            if let Some(group) = n.group.take() {
                self.groups[group] = None;
            }
            n.gone = true;
            stack.extend(n.children.take_all());
        }
    }
}

#[derive(Debug)]
pub(crate) struct Notes {
    /// Saved comments of units the parser has switched away from, and of the current unit up
    /// to the last switch, in the order they were saved.
    done: Vec<Comment>,
    /// Byte ranges of the current unit's comments saved since the last switch.
    spans: Vec<(usize, usize)>,
    /// Where line counting stands in the current unit's input.
    cursor: LineCursor,
    /// Comments from this index on are not attached yet.
    pending_from: usize,
    /// Comments from this index on are held back from the next value created.
    held_from: Option<usize>,
    /// Held comments up to this index were read in an earlier input: they attach after the
    /// value they were held back for (§13.1; oracle runs, QUESTIONS.md #59).
    held_earlier: Option<usize>,
    groups: CommentGroups,
    /// The value created most recently. `None` when that value is not part of the result.
    last: Option<PathRef>,
}

impl Notes {
    pub(crate) fn new() -> Self {
        Self {
            done: Vec::new(),
            spans: Vec::new(),
            cursor: LineCursor::new(),
            pending_from: 0,
            held_from: None,
            held_earlier: None,
            groups: CommentGroups::default(),
            last: Some(PathRef::root()),
        }
    }

    pub(crate) fn save(&mut self, start: usize, end: usize) {
        self.spans.push((start, end));
    }

    /// The number of comments saved so far, in every unit.
    fn count(&self) -> usize {
        self.done.len() + self.spans.len()
    }

    /// Turns the current unit's saved ranges into comments of `src`, its input.
    fn flush(&mut self, src: &[u8]) {
        let LineCursor {
            mut line,
            mut line_start,
            mut scanned,
        } = self.cursor;
        for (start, end) in self.spans.drain(..) {
            for (i, &b) in src[scanned..start].iter().enumerate() {
                if b == b'\n' {
                    line += 1;
                    line_start = scanned + i + 1;
                }
            }
            scanned = start;
            let column = 1 + src[line_start..start]
                .iter()
                .filter(|&&b| (b & 0xC0) != 0x80)
                .count();
            self.done.push(Comment {
                text: String::from_utf8_lossy(&src[start..end]).into_owned(),
                position: Position {
                    line,
                    column,
                    offset: start,
                },
            });
        }
        self.cursor = LineCursor {
            line,
            line_start,
            scanned,
        };
    }

    /// The parser leaves the unit whose input is `src` for a new one, an included file.
    /// Returns what [`Notes::resume`] needs to come back.
    pub(crate) fn suspend(&mut self, src: &[u8]) -> LineCursor {
        self.flush(src);
        std::mem::replace(&mut self.cursor, LineCursor::new())
    }

    /// The unit whose input is `src` has ended; the parser returns to the unit it left.
    pub(crate) fn resume(&mut self, src: &[u8], cursor: LineCursor) {
        self.flush(src);
        self.cursor = cursor;
    }

    /// The saved comments and their attachments; `src` is the current unit's input.
    pub(crate) fn finish(mut self, src: &[u8]) -> (Vec<Comment>, CommentGroups) {
        self.flush(src);
        (self.done, self.groups)
    }

    /// Comments saved from now until the next value is created stay pending after it.
    pub(crate) fn hold(&mut self) {
        self.held_from = Some(self.count());
    }

    /// A value was created at `path` (`None`: it is not part of the result). Pending comments
    /// go before it, except those held back. Held comments read in an earlier input go after it.
    pub(crate) fn created(&mut self, path: Option<PathRef>) {
        let end = self.held_from.take().unwrap_or(self.count());
        self.attach(end, path.as_ref(), CommentPlacement::Before);
        if let Some(until) = self.held_earlier.take() {
            self.attach(until, path.as_ref(), CommentPlacement::After);
        }
        self.last = path;
    }

    /// An input ended while a key waits for its value on a following line: the pending
    /// comments wait for the value too. Those before the key go before it, as in one document;
    /// those held back for it go after it, while comments of the value's own input are held as
    /// in one document (oracle runs, QUESTIONS.md #59).
    pub(crate) fn wait_for_value(&mut self) {
        self.held_earlier = Some(self.count());
    }

    /// A container closed with its bracket, or the input ended: pending comments go after the
    /// value created most recently.
    pub(crate) fn trailing(&mut self) {
        let last = self.last.take();
        self.attach(self.count(), last.as_ref(), CommentPlacement::After);
        self.last = last;
    }

    /// Makes `path` the value created most recently, without attaching anything.
    pub(crate) fn set_last(&mut self, path: Option<PathRef>) {
        self.last = path;
    }

    /// The node of `path` in the tree, added with the nodes on the way if missing. Each node of
    /// `path` remembers its place, so a path that extends one looked up before costs one step.
    fn place_of(&mut self, path: &PathRef) -> usize {
        let mut unknown = Vec::new();
        let mut node = path.0.as_deref();
        let mut place = ROOT;
        while let Some(n) = node {
            if let Some(known) = n.place.get()
                && !self.groups.nodes[known].gone
            {
                place = known;
                break;
            }
            unknown.push(n);
            node = n.parent.0.as_deref();
        }
        for n in unknown.into_iter().rev() {
            place = self.groups.child_or_insert(place, &n.segment);
            n.place.set(Some(place));
        }
        place
    }

    /// Attaches the pending comments before index `end`.
    fn attach(&mut self, end: usize, path: Option<&PathRef>, placement: CommentPlacement) {
        let pending = self.pending_from..end;
        if pending.is_empty() {
            return;
        }
        self.pending_from = end;
        let Some(path) = path else {
            return;
        };
        let place = self.place_of(path);
        match self.groups.nodes[place].group {
            Some(group) => self.groups.groups[group]
                .as_mut()
                .expect("the group of a place is kept")
                .comments
                .extend(pending),
            None => {
                self.groups.nodes[place].group = Some(self.groups.groups.len());
                self.groups.groups.push(Some(Group {
                    place,
                    placement,
                    comments: pending.collect(),
                }));
            }
        }
    }

    /// Values of entry `key` in the object at `object` were replaced: value `slot`, or all of
    /// them. Their comments, and those of anything inside them, are dropped.
    pub(crate) fn replaced(&mut self, object: &PathRef, key: &str, slot: Option<usize>) {
        if self.groups.groups.is_empty() {
            return;
        }
        let object = self.place_of(object);
        let children = &mut self.groups.nodes[object].children;
        let hits: Vec<usize> = match slot {
            Some(slot) => children.take_value(key, slot).into_iter().collect(),
            None => children
                .take_values(key)
                .into_iter()
                .map(|(_, c)| c)
                .collect(),
        };
        for place in hits {
            self.groups.drop_value(place);
        }
    }

    /// Entry `key` of the object at `object` became a `NO_IMPLICIT_ARRAYS` collection: its first
    /// `count` values are now the first elements of the array that is its only value. Their
    /// comments go with them, and so does the value created most recently if it is one of them.
    pub(crate) fn collected(&mut self, object: &PathRef, key: &str, count: usize) {
        let object = self.place_of(object);
        if let Some(last) = self.last.clone() {
            // Its nodes remember their places, which move with the values.
            self.place_of(&last);
        }
        let moved: Vec<(usize, usize)> = (0..count)
            .filter_map(|index| {
                let child = self.groups.nodes[object].children.take_value(key, index)?;
                Some((index, child))
            })
            .collect();
        if moved.is_empty() {
            return;
        }
        let first = PathSegment::Key {
            key: key.to_owned(),
            index: 0,
        };
        let array = self.groups.nodes.len();
        self.groups.nodes.push(Place {
            parent: Some((object, first.clone())),
            ..Place::default()
        });
        self.groups.nodes[object].children.insert(&first, array);
        for (index, child) in moved {
            let element = PathSegment::Index(index);
            self.groups.nodes[array].children.insert(&element, child);
            self.groups.nodes[child].parent = Some((array, element));
        }
    }
}
