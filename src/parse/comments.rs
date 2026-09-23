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
//! a `NO_IMPLICIT_ARRAYS` collection and disappear with a value that is replaced.
//!
//! Comments are collected across input units (§9.4): pending comments may attach to a value of
//! an included file. Each unit's comments are turned into [`Comment`]s, text and position in
//! that unit's input, before the parser switches to another unit.

use super::{AttachedComments, Comment, CommentPlacement, PathSegment};
use crate::error::Position;
use std::collections::HashMap;

pub(crate) type ValuePath = Vec<PathSegment>;

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
    groups: Vec<AttachedComments>,
    by_path: HashMap<ValuePath, usize>,
    /// The value created most recently. `None` when that value is not part of the result.
    last: Option<ValuePath>,
}

impl Notes {
    pub(crate) fn new() -> Self {
        Self {
            done: Vec::new(),
            spans: Vec::new(),
            cursor: LineCursor::new(),
            pending_from: 0,
            held_from: None,
            groups: Vec::new(),
            by_path: HashMap::new(),
            last: Some(Vec::new()),
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
    pub(crate) fn finish(mut self, src: &[u8]) -> (Vec<Comment>, Vec<AttachedComments>) {
        self.flush(src);
        (self.done, self.groups)
    }

    /// Comments saved from now until the next value is created stay pending after it.
    pub(crate) fn hold(&mut self) {
        self.held_from = Some(self.count());
    }

    /// A value was created at `path` (`None`: it is not part of the result). Pending comments
    /// go before it, except those held back.
    pub(crate) fn created(&mut self, path: Option<ValuePath>) {
        let end = self.held_from.take().unwrap_or(self.count());
        self.attach(end, path.as_ref(), CommentPlacement::Before);
        self.last = path;
    }

    /// A container closed with its bracket, or the input ended: pending comments go after the
    /// value created most recently.
    pub(crate) fn trailing(&mut self) {
        let last = self.last.take();
        self.attach(self.count(), last.as_ref(), CommentPlacement::After);
        self.last = last;
    }

    /// Makes `path` the value created most recently, without attaching anything.
    pub(crate) fn set_last(&mut self, path: Option<ValuePath>) {
        self.last = path;
    }

    /// Attaches the pending comments before index `end`.
    fn attach(&mut self, end: usize, path: Option<&ValuePath>, placement: CommentPlacement) {
        let pending = self.pending_from..end;
        if pending.is_empty() {
            return;
        }
        self.pending_from = end;
        let Some(path) = path else {
            return;
        };
        match self.by_path.get(path) {
            Some(&group) => self.groups[group].comments.extend(pending),
            None => {
                self.by_path.insert(path.clone(), self.groups.len());
                self.groups.push(AttachedComments {
                    path: path.clone(),
                    placement,
                    comments: pending.collect(),
                });
            }
        }
    }

    /// Values of entry `key` in the object at `object` were replaced: value `slot`, or all of
    /// them. Their comments, and those of anything inside them, are dropped.
    pub(crate) fn replaced(&mut self, object: &[PathSegment], key: &str, slot: Option<usize>) {
        let n = object.len();
        let before = self.groups.len();
        self.groups.retain(|g| {
            let hit = g.path.len() > n
                && g.path[..n] == *object
                && matches!(&g.path[n], PathSegment::Key { key: k, index }
                    if k == key && slot.is_none_or(|s| s == *index));
            !hit
        });
        if self.groups.len() != before {
            self.reindex();
        }
    }

    /// Entry `key` of the object at `object` became a `NO_IMPLICIT_ARRAYS` collection: its first
    /// `count` values are now the first elements of the array that is its only value.
    pub(crate) fn collected(&mut self, object: &[PathSegment], key: &str, count: usize) {
        let n = object.len();
        let relocate = |path: &mut ValuePath| {
            if path.len() > n
                && path[..n] == *object
                && let PathSegment::Key { key: k, index } = &path[n]
                && k == key
                && *index < count
            {
                let element = PathSegment::Index(*index);
                path[n] = PathSegment::Key {
                    key: key.to_owned(),
                    index: 0,
                };
                path.insert(n + 1, element);
                return true;
            }
            false
        };
        let mut moved = false;
        for group in &mut self.groups {
            moved |= relocate(&mut group.path);
        }
        if let Some(last) = &mut self.last {
            relocate(last);
        }
        if moved {
            self.reindex();
        }
    }

    fn reindex(&mut self) {
        self.by_path = self
            .groups
            .iter()
            .enumerate()
            .map(|(i, g)| (g.path.clone(), i))
            .collect();
    }
}
