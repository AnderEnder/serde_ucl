//! Document<'t> structure: entries, keys, named sections and containers (spec §1–§4, §8, §11).
//!
//! The parser keeps an explicit stack of the containers that are open, so nesting depth never
//! grows the call stack. An object or array is inserted into its parent (by the duplicate rules of
//! §8) when its opening bracket is read. While it is open, its frame on the stack holds it, and an
//! empty container of its kind keeps its place in the parent; when it closes, it goes back to
//! that place. So later entries go straight into the container of the top frame, at the same cost
//! at any depth. Each stack frame records where its container goes in the container of the frame
//! below. The few readers that need the tree as parsed so far, open containers included
//! (`.inherit`, §9.7), use [`Core::value_at`] and [`Core::filled_copy`].
//!
//! Input units (§9.4): an included file is parsed by a [`Core`] of its own that takes over the
//! container stack, the tree and the saved comments of the unit that includes it, and hands them
//! back when the file ends. So an included file adds entries where the macro stands, may close
//! containers the including unit opened, and leaves section objects open for it (§9.4, *Where
//! the entries go*).

use super::comments::{CommentGroups, Notes, PathRef, ValuePath};
use super::error::position_at;
use super::facts::{self, NodeId, OutputFacts, Pos};
use super::include::Includes;
use super::number::{self, Number};
use super::string;
use super::vars::Expander;
use super::{Comment, Error, ErrorKind, MAX_NESTING, PathSegment, Uncertain};
use crate::emit::key_needs_quoting;
use crate::error::Position;
use crate::value::{
    DuplicateKeyError, DuplicateStrategy, Entry, KeyCopy, Object, ParserFlags, Placement, Slot,
    Str, UclValue, Value,
};
use std::cell::OnceCell;
use std::collections::HashMap;
use std::ops::RangeInclusive;
use std::path::Path;

/// The settings of one input unit.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Settings {
    pub(crate) flags: ParserFlags,
    pub(crate) priority: u8,
    pub(crate) strategy: DuplicateStrategy,
}

/// What the next input starts with, from where the input before it ended (spec §13.1).
#[derive(Debug)]
pub(crate) enum Boundary<'t> {
    /// No input has been read: the next one sets up the root (§1.1).
    Start,
    /// An entry may start.
    Entry,
    /// A value, or section names after a separator and a line break (§3.4), ended the input
    /// with no separator after it, or a later input held only whitespace while an entry could
    /// start (oracle runs, QUESTIONS.md #59): a line break, `;`, `,` or a comment must come
    /// before the next entry (§13.1, *Quirk*).
    Separator,
    /// A key and its separator ended the input before a line break, so the value comes from a
    /// following line (§1.6), also in a later input (oracle runs, QUESTIONS.md #59).
    Value(Box<PendingValue<'t>>),
    /// The root has closed: its brace, or an array root. Later inputs may hold only
    /// whitespace, `;`, `,` and comments (oracle runs, QUESTIONS.md #59).
    Closed,
    /// The first input had no bytes: the root is an empty object that later inputs cannot add
    /// to (§13.1, *Quirk*).
    Empty,
}

/// A key whose value comes from a following line, in a later input ([`Boundary::Value`]).
#[derive(Debug)]
pub(crate) struct PendingValue<'t> {
    key: Key<'t>,
    /// The settings of the input the key is in: the value is inserted with them (oracle runs).
    settings: Settings,
}

/// The state of a parse from one input to the next (spec §13.1): the tree, the open containers,
/// the saved comments and the output facts.
pub(crate) struct Document<'t> {
    root: Value<'t>,
    frames: Vec<Frame<'t>>,
    notes: Option<Notes>,
    facts: Option<OutputFacts>,
    uppercase_keys: bool,
    boundary: Boundary<'t>,
    /// How many macro argument documents this document is inside (§9.2): 0 for the parse of
    /// the parser's inputs.
    depth: usize,
    /// The root's priority: that of the first input, which creates the root (§13.2, *The root's
    /// priority*; §8.7).
    root_priority: u8,
}

/// A finished parse ([`Document::finish`]).
pub(crate) struct Finished<'t> {
    pub(crate) root: Value<'t>,
    /// The saved comments and the values they are attached to, when comments are saved.
    pub(crate) comments: Option<(Vec<Comment>, CommentGroups)>,
    pub(crate) facts: Option<OutputFacts>,
}

impl<'t> Document<'t> {
    /// A parse with no input read yet. With `save_comments`, comments are saved (§12.5); with
    /// `facts`, output facts are recorded there (§10.1), which may be set to record locations
    /// ([`OutputFacts::locating`]).
    pub(crate) fn new(save_comments: bool, facts: Option<OutputFacts>, depth: usize) -> Self {
        Self {
            root: Value::Object(Object::new()),
            frames: Vec::new(),
            notes: save_comments.then(Notes::new),
            facts,
            uppercase_keys: false,
            boundary: Boundary::Start,
            depth,
            root_priority: 0,
        }
    }

    /// Reads one input, `src`, with `settings` (§13.1). It goes on where the input before it
    /// ended: its entries go into the containers left open, the first input setting up the
    /// root. A zero-byte input after the first changes nothing.
    ///
    /// A silent stop (§9.4, §13.2) ends this input only: it is returned as an error, and the
    /// next input goes on with the containers the stopped one left open. Any other error leaves
    /// the document unusable.
    pub(crate) fn read(
        &mut self,
        src: &[u8],
        settings: Settings,
        expander: &mut Expander<'_>,
        includes: &mut Includes<'_>,
    ) -> Result<(), Error> {
        self.read_unit(src, None, settings, expander, includes)
    }

    /// [`Document::read`], for text that the keys and strings of the tree may borrow, where
    /// they appear in it as they are: without an escape, a variable reference replaced, or a
    /// change of case (spec §12.1).
    pub(crate) fn read_borrowed(
        &mut self,
        src: &'t [u8],
        settings: Settings,
        expander: &mut Expander<'_>,
        includes: &mut Includes<'_>,
    ) -> Result<(), Error> {
        let borrow = std::str::from_utf8(src).ok();
        self.read_unit(src, borrow, settings, expander, includes)
    }

    /// [`Document::read`], with `borrow`, `src` as text, when the tree may borrow from it.
    fn read_unit(
        &mut self,
        src: &[u8],
        borrow: Option<&'t str>,
        settings: Settings,
        expander: &mut Expander<'_>,
        includes: &mut Includes<'_>,
    ) -> Result<(), Error> {
        debug_assert!(borrow.is_none_or(|text| text.as_bytes() == src));
        if src.is_empty() && !matches!(self.boundary, Boundary::Start) {
            return Ok(());
        }
        let boundary = std::mem::replace(&mut self.boundary, Boundary::Entry);
        if matches!(boundary, Boundary::Start) {
            self.root_priority = settings.priority;
        }
        let unit = includes.new_unit();
        includes.open_units.push(unit);
        let mut core = Core {
            src,
            utf8: std::str::from_utf8(src).ok(),
            borrow,
            pos: 0,
            settings,
            expander,
            includes,
            notes: self.notes.take(),
            facts: self.facts.take(),
            uppercase_keys: self.uppercase_keys,
            root: std::mem::replace(&mut self.root, Value::Null),
            frames: std::mem::take(&mut self.frames),
            depth: self.depth,
            root_priority: self.root_priority,
            unit,
            role: Role::Input,
            unit_done: false,
            name_run: false,
            run_macro_end: None,
            outer_run: false,
            recent: None,
            first_key_shares: false,
            section_shares: false,
            bracket_scan: (usize::MAX, 0),
            pending: None,
            unseparated: false,
            ends: None,
            sizes: Sizes::default(),
        };
        let result = core.read_input(boundary);
        let result = match core.includes.pending_search_miss.take() {
            Some((pending_unit, error)) if pending_unit == unit => Err(error),
            pending => {
                core.includes.pending_search_miss = pending;
                result
            }
        };
        self.boundary = match &result {
            Ok(()) => core.boundary_at_end(),
            Err(_) => Boundary::Entry,
        };
        self.root = core.root;
        self.frames = core.frames;
        self.uppercase_keys = core.uppercase_keys;
        self.facts = core.facts;
        self.notes = core.notes;
        includes.open_units.pop();
        if let Some(notes) = &mut self.notes {
            // The next input's comments are counted from its own start.
            notes.suspend(src);
        }
        result
    }

    /// A copy of the root as parsed so far, with the containers that are still open and what
    /// they hold: the partial result of a silent stop in one of several inputs.
    pub(crate) fn snapshot(&self) -> Value<'t> {
        let mut root = self.root.clone();
        let mut containers: Vec<Option<Value<'t>>> = self
            .frames
            .iter()
            .map(|frame| match &frame.home {
                Home::Root => None,
                Home::Attached(_, value) | Home::Detached(value) => Some(value.clone()),
            })
            .collect();
        for index in (1..self.frames.len()).rev() {
            let Home::Attached(step, _) = &self.frames[index].home else {
                continue;
            };
            let value = containers[index]
                .take()
                .expect("an attached frame holds its container");
            let below = match containers[index - 1].as_mut() {
                Some(below) => below,
                None => &mut root,
            };
            if let Some(place) = step.try_enter(below) {
                *place = value;
            }
        }
        root
    }

    /// The comments saved so far, for a parse that failed.
    pub(crate) fn into_comments(self) -> Vec<Comment> {
        self.notes
            .map(|notes| notes.finish(b"").0)
            .unwrap_or_default()
    }

    /// Ends the parse after its last input: a key still waiting for its value on a following
    /// line gets `null` (§1.6), then every container closes. Pending comments attach as at the
    /// end of input (§12.5). An error there, such as a repeated key under the `error` strategy,
    /// is returned with the saved comments.
    pub(crate) fn finish(mut self) -> Result<Finished<'t>, Box<(Error, Vec<Comment>)>> {
        if let Boundary::Value(pending) = std::mem::replace(&mut self.boundary, Boundary::Entry) {
            let mut expander = Expander::new(super::vars::Variables::new(), None, false);
            let loader = super::MemoryLoader::new();
            let budget = super::include::Budget::new(None);
            let mut includes = Includes::new(&loader, Path::new("").into(), None, &budget, None);
            let mut core = Core {
                src: b"",
                utf8: Some(""),
                borrow: None,
                pos: 0,
                settings: pending.settings,
                expander: &mut expander,
                includes: &mut includes,
                notes: self.notes.take(),
                facts: self.facts.take(),
                uppercase_keys: self.uppercase_keys,
                root: std::mem::replace(&mut self.root, Value::Null),
                frames: std::mem::take(&mut self.frames),
                depth: self.depth,
                root_priority: self.root_priority,
                unit: 0,
                role: Role::Input,
                unit_done: false,
                name_run: false,
                run_macro_end: None,
                outer_run: false,
                recent: None,
                first_key_shares: false,
                section_shares: false,
                bracket_scan: (usize::MAX, 0),
                pending: None,
                unseparated: false,
                ends: None,
                sizes: Sizes::default(),
            };
            let result = core.null_value(pending.key);
            if let Some(notes) = &mut core.notes {
                notes.trailing();
            }
            self.root = core.root;
            self.frames = core.frames;
            self.facts = core.facts;
            self.notes = core.notes;
            if let Err(e) = result {
                return Err(Box::new((e, self.into_comments())));
            }
        }
        let mut core_frames = std::mem::take(&mut self.frames);
        while let Some(frame) = core_frames.pop() {
            if let Home::Attached(step, value) = frame.home {
                let below = match core_frames.last_mut().map(|f| &mut f.home) {
                    Some(Home::Attached(_, below) | Home::Detached(below)) => below,
                    Some(Home::Root) | None => &mut self.root,
                };
                *step.enter(below) = value;
            }
        }
        Ok(Finished {
            root: self.root,
            comments: self.notes.map(|notes| notes.finish(b"")),
            facts: self.facts,
        })
    }
}

/// Parses a macro's argument document (§9.2) that is `depth` argument documents deep. Its
/// comments are not saved.
pub(super) fn parse_nested(
    input: &[u8],
    settings: Settings,
    expander: &mut Expander<'_>,
    includes: &mut Includes<'_>,
    depth: usize,
) -> Result<UclValue, Error> {
    let mut document = Document::new(false, None, depth);
    document.read(input, settings, expander, includes)?;
    document
        .finish()
        .map(|finished| finished.root)
        .map_err(|failed| failed.0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Object,
    Array,
}

/// What closes an open container.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Close {
    /// The end of input: the root object written without braces.
    Eof,
    /// `}`.
    Brace,
    /// `]`.
    Bracket,
    /// An object created for a section name (§3.4). It closes together with the container
    /// above it, or quietly at the end of input (after a macro ignored there, §9.2).
    Section,
    /// A section-name object whose path ended in an ordinary key (§3.4, *Quirk*). It has no
    /// closing bracket. It closes when a container written with a bracket that was opened in it
    /// closes, together with every such object below it, or at the end of input.
    LeftOpen,
    /// An object whose opening brace an included file's leading `{` has taken over (§9.4,
    /// *Quirk: braces around an included file*). A `}` removes the brace; what the object does
    /// then is the [`Revert`].
    IncludedBrace(Revert),
    /// The object of the first name of an included file's first key, when the file's leading
    /// `{` took over the brace of the root, of an object written with braces, or of the object
    /// that `key` or `prefix` creates for the file (a [`Revert::Open`] brace): §9.4 gives it a
    /// brace of its own. A `}` closes it as [`Close::Brace`] does: as its own bracket, so it does
    /// not become the value created most recently (§12.5), and the object below it keeps the
    /// brace taken over. Like a left-open section object, it also closes when a container written
    /// with brackets that was opened in it closes, and then it counts as the value created most
    /// recently (§9.4, §12.5).
    NameBrace,
}

/// What an object whose brace an included file has taken over does when a `}` removes that
/// brace (oracle runs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Revert {
    /// It stays open as [`Close::Eof`]: the root, an object nested under a key, or an object
    /// written with a brace, which has lost its own brace, so only the end of input closes it.
    /// Also the object a file nested under a key goes into, when it holds a share of the brace
    /// of the object where the macro stands ([`Core::open_nest_target`]).
    Open,
    /// A section object ([`Close::Section`], [`Close::LeftOpen`]): it closes, with the section
    /// objects around it, as when a bracketed container opened in it closes. Also the object of
    /// a first name with a brace of its own ([`Close::NameBrace`]) whose brace a file included in
    /// it takes over: it closes then as a section object, and counts as the value created most
    /// recently (oracle runs, QUESTIONS.md #85).
    Section,
}

impl Close {
    /// Whether a bracket is still open for the container.
    fn has_bracket(self) -> bool {
        matches!(
            self,
            Close::Brace | Close::Bracket | Close::IncludedBrace(_) | Close::NameBrace
        )
    }

    fn is_section(self) -> bool {
        matches!(self, Close::Section | Close::LeftOpen)
    }

    /// Whether the container closes when a bracketed container opened in it closes (§3.4): a
    /// section object, also one whose brace an included file took over, which loses that brace
    /// then (oracle runs, QUESTIONS.md #40), and the object of the first name that §9.4 gives a
    /// brace of its own ([`Close::NameBrace`]).
    fn closes_with_inner(self) -> bool {
        self.is_section()
            || matches!(
                self,
                Close::IncludedBrace(Revert::Section) | Close::NameBrace
            )
    }
}

/// How to reach a frame's container from the container of the frame below.
#[derive(Debug)]
enum Step {
    /// Value `slot` of the entry at position `entry`, whose key is `key`, a copy of the entry's
    /// ([`KeyCopy`]). The entry is found by its key; its position is for the output facts, which
    /// are kept by position (clean-room work item C13).
    Entry {
        entry: usize,
        key: KeyCopy,
        slot: usize,
    },
    /// Element `index` of the explicit array that is the first value of the entry at position
    /// `entry`, whose key is `key`: a repeat collected under `NO_IMPLICIT_ARRAYS` (§8.5).
    Collected {
        entry: usize,
        key: KeyCopy,
        index: usize,
    },
    /// Element `index` of an array.
    Element(usize),
    /// These steps in turn: a value reopened below the container of the frame below (§9.1,
    /// [`Core::reopen_recent`]).
    Path(Vec<Step>),
}

impl Step {
    fn segments(&self) -> Vec<PathSegment> {
        match self {
            Step::Entry { key, slot, .. } => vec![PathSegment::Key {
                key: key.to_string(),
                index: *slot,
            }],
            Step::Collected { key, index, .. } => vec![
                PathSegment::Key {
                    key: key.to_string(),
                    index: 0,
                },
                PathSegment::Index(*index),
            ],
            Step::Element(index) => vec![PathSegment::Index(*index)],
            Step::Path(steps) => steps.iter().flat_map(Step::segments).collect(),
        }
    }

    fn try_enter<'v, 't>(&self, value: &'v mut Value<'t>) -> Option<&'v mut Value<'t>> {
        match self {
            Step::Entry { entry, key, slot } => value
                .as_object_mut()
                .and_then(|o| o.entry_at_mut(*entry, key))
                .and_then(|e| e.value_at_mut(*slot)),
            Step::Collected { entry, key, index } => value
                .as_object_mut()
                .and_then(|o| o.entry_at_mut(*entry, key))
                .and_then(|e| e.value_at_mut(0))
                .and_then(Value::as_array_mut)
                .and_then(|a| a.get_mut(*index)),
            Step::Element(index) => value.as_array_mut().and_then(|a| a.get_mut(*index)),
            Step::Path(steps) => steps
                .iter()
                .try_fold(value, |value, step| step.try_enter(value)),
        }
    }

    fn enter<'v, 't>(&self, value: &'v mut Value<'t>) -> &'v mut Value<'t> {
        self.try_enter(value)
            .expect("an open container stays where it was inserted")
    }

    /// The steps of `path`, a value path inside `value`, in turn; `None` when `value` has no
    /// value there.
    fn resolve(value: &Value<'_>, path: &[PathSegment]) -> Option<Step> {
        let mut steps = Vec::with_capacity(path.len());
        let mut value = value;
        for segment in path {
            match segment {
                PathSegment::Key { key, index } => {
                    let object = value.as_object()?;
                    let entry = object.index_of(key)?;
                    let (name, found) = object.get_index(entry)?;
                    value = found.slots().get(*index)?.value();
                    steps.push(Step::Entry {
                        entry,
                        key: KeyCopy::from(name),
                        slot: *index,
                    });
                }
                PathSegment::Index(index) => {
                    value = value.as_array()?.get(*index)?;
                    steps.push(Step::Element(*index));
                }
            }
        }
        Some(Step::Path(steps))
    }

    /// The node in `facts` of the value this step leads to from the value at `node`, added with
    /// the nodes on the way if missing.
    fn descend_facts(&self, facts: &mut OutputFacts, node: NodeId) -> NodeId {
        match self {
            Step::Entry { entry, key, slot } => {
                facts.entry_child_or_insert(node, *entry, key, *slot)
            }
            Step::Collected { entry, key, index } => {
                let entry = facts.entry_child_or_insert(node, *entry, key, 0);
                facts.element_child_or_insert(entry, *index)
            }
            Step::Element(index) => facts.element_child_or_insert(node, *index),
            Step::Path(steps) => steps
                .iter()
                .fold(node, |node, step| step.descend_facts(facts, node)),
        }
    }
}

/// The value at `path` inside `value`.
pub(super) fn get<'v, 't>(value: &'v Value<'t>, path: &[PathSegment]) -> Option<&'v Value<'t>> {
    path.iter().try_fold(value, |value, segment| match segment {
        PathSegment::Key { key, index } => value
            .as_object()?
            .entry(key)?
            .slots()
            .get(*index)
            .map(Slot::value),
        PathSegment::Index(index) => value.as_array()?.get(*index),
    })
}

/// The offset after the `*/` that ends the block comment at `start` (§2.3), or `None` when it is
/// not terminated. Comments nest. A `"` that does not directly follow a `\` begins or ends a
/// quoted part, inside which `/*` and `*/` have no effect.
fn block_comment_end(src: &[u8], start: usize) -> Option<usize> {
    let mut i = start + 2;
    let mut depth = 1;
    let mut in_quotes = false;
    while depth > 0 {
        match src.get(i)? {
            b'"' => {
                if src[i - 1] != b'\\' {
                    in_quotes = !in_quotes;
                }
                i += 1;
            }
            b'/' if !in_quotes && src.get(i + 1) == Some(&b'*') => {
                depth += 1;
                i += 2;
            }
            b'*' if !in_quotes && src.get(i + 1) == Some(&b'/') => {
                depth -= 1;
                i += 2;
            }
            _ => i += 1,
        }
    }
    Some(i)
}

/// Whether `src[from..]` holds only whitespace and comments, and at least one comment.
fn only_comments(src: &[u8], from: usize) -> bool {
    let mut i = from;
    let mut comment = false;
    loop {
        match src.get(i) {
            None => return comment,
            Some(&b) if is_space(b) => i += 1,
            Some(b'#') => {
                comment = true;
                i = src[i..]
                    .iter()
                    .position(|&b| b == b'\n')
                    .map_or(src.len(), |n| i + n + 1);
            }
            Some(b'/') if src.get(i + 1) == Some(&b'*') => {
                comment = true;
                match block_comment_end(src, i) {
                    Some(end) => i = end,
                    None => return false,
                }
            }
            Some(_) => return false,
        }
    }
}

/// Where a frame's container lives.
#[derive(Debug)]
enum Home<'t> {
    /// It is the root value, [`Core::root`].
    Root,
    /// It belongs at the step inside the container of the frame below, where an empty container
    /// of its kind keeps its place while the frame holds it. It goes back there when it closes.
    Attached(Step, Value<'t>),
    /// It is parsed and then discarded, because a value with a higher priority exists (§8.3).
    Detached(Value<'t>),
}

#[derive(Debug)]
struct Frame<'t> {
    kind: Kind,
    close: Close,
    home: Home<'t>,
    /// No element or entry has been read into the container yet.
    fresh: bool,
    /// The container's path from the root, set when first needed ([`Core::frame_path`]); `None`
    /// in it for a container that is not part of the result.
    path: OnceCell<Option<PathRef>>,
    /// The container's node in the output facts, set when first needed ([`Core::facts_node`]).
    facts_node: OnceCell<Option<NodeId>>,
    /// For an object, the index of each key by its ASCII lowercase form, for finding keys
    /// regardless of case (§12.1): built when first needed, for the entries up to the count
    /// kept with it, and extended as entries are added ([`Core::find_current_key`]).
    lowercase_keys: Option<(usize, HashMap<String, usize>)>,
    /// The input unit that opened the container: 0 for the main document, `n` for a file
    /// included `n` levels deep (§9.4).
    unit: usize,
    /// A section object that was the innermost open object when a registered macro had text
    /// parsed in place: it no longer closes with a bracketed container that closes in it, nor
    /// when a `}` removes a brace taken over from it, for the rest of the parse (§13.2, *Quirk:
    /// text in place and a section object left open*; QUESTIONS.md #74, #76). The same holds
    /// for the object of a first name with a brace of its own ([`Close::NameBrace`]), except
    /// that its own `}` still closes it (oracle runs, QUESTIONS.md #85).
    stays_open: bool,
}

/// Where an object is in the saved comments and the output facts ([`Core::object_places`]).
struct Places {
    path: Option<PathRef>,
    node: Option<NodeId>,
}

/// An entry as it was before an insertion, for keeping saved comments with their values.
struct Before {
    /// Its number of values.
    len: usize,
    /// Its value is a `NO_IMPLICIT_ARRAYS` collection.
    collected: bool,
    /// The insertion replaces its first value in place (the `merge` scalar quirk).
    in_place: bool,
}

/// A key and where it starts.
#[derive(Debug)]
struct Key<'t> {
    name: Str<'t>,
    at: usize,
    /// The key was written in double quotes and contains a backslash escape or a byte that makes
    /// the output formats quote it (spec §10.1, fact 3).
    quoted: bool,
    /// [`key_needs_quoting`] of `name` as read, worked out once. A bare key never needs quoting.
    needs_quoting: bool,
    /// Set by [`Core::insert`] when it moved `name` into the entry it created, at this index of
    /// the current object; the name is then read from there ([`Core::key_name`]).
    stored: Option<usize>,
    /// The position in the current object of the entry [`Core::insert`] put the value in.
    entry: usize,
    /// For a key read in an earlier input ([`Boundary::Value`]), its position there: errors about
    /// the key are reported there.
    origin: Option<Position>,
}

/// Where a scalar value came from, for the output formats (spec §10.1, facts 1 and 2; §10.7).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Origin {
    /// Written in single quotes (§6.2).
    single_quoted: bool,
    /// A heredoc (§6.3).
    multiline: bool,
    /// An unquoted keyword of §4.5 (`true`, `null`, `nan`, `inf`, …), not a number written with
    /// digits. It is not recorded as a fact: it only decides the layout of a value that takes a
    /// container's place under `merge` (§10.7, *Quirk*).
    keyword: bool,
}

impl Origin {
    /// Whether fact 1 or 2 of §10.1 holds for the string, so that it is recorded.
    fn has_string_facts(self) -> bool {
        self.single_quoted || self.multiline
    }
}

/// The facts of a value that [`Core::insert`] placed, besides its key.
#[derive(Debug, Clone, Copy)]
struct PlacedFacts {
    origin: Origin,
    /// The value took the place of a container under `merge` (§8.4, *Quirk*).
    in_place: bool,
    normal_layout: bool,
}

/// The name of `key`: its own, or for a key that [`Core::insert`] moved into the object of the
/// top frame, that entry's ([`Key::stored`]). Takes the fields it reads, so that the caller may
/// borrow others mutably meanwhile.
fn stored_key_name<'a, 't>(
    frames: &'a [Frame<'t>],
    root: &'a Value<'t>,
    key: &'a Key<'t>,
) -> &'a str {
    let Some(index) = key.stored else {
        return &key.name;
    };
    let top = match &frames.last().expect("a container is open").home {
        Home::Root => root,
        Home::Attached(_, value) | Home::Detached(value) => value,
    };
    top.as_object()
        .and_then(|object| object.get_index(index))
        .map(|(name, _)| name.as_str())
        .expect("the entry that insert created")
}

/// Bytes that may start a bare key (§3.1).
fn is_key_start(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'/' || b == b'_' || b >= 0x80
}

/// Bytes that may continue a bare key (§3.1).
fn is_key_byte(b: u8) -> bool {
    is_key_start(b) || b == b'-' || b == b'.'
}

/// Whitespace between entries and before values (§2.1).
pub(super) fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0B | 0x0C)
}

/// The extent of an unquoted value ([`Core::unquoted_extent`]).
struct Extent {
    end: usize,
    /// The value has a backslash.
    backslash: bool,
    /// The value has a `$` that is not the byte after a backslash.
    dollar: bool,
}

/// The lengths of the keywords of §4.5: [`Core::unquoted`] calls [`keyword`] only for a value of
/// such a length. It must cover every word `keyword` accepts; `tests::keyword_lengths` checks
/// the words of §4.5.
const KEYWORD_LEN: RangeInclusive<usize> = 2..=5;

/// The keywords of §4.5, compared with the whole unquoted value. A word added here also goes in
/// `tests::KEYWORDS`, so that the test checks [`KEYWORD_LEN`] covers it.
fn keyword(raw: &[u8]) -> Option<UclValue> {
    let is = |word: &str| raw.eq_ignore_ascii_case(word.as_bytes());
    if is("true") || is("yes") || is("on") {
        Some(Value::Boolean(true))
    } else if is("false") || is("no") || is("off") {
        Some(Value::Boolean(false))
    } else {
        match raw {
            b"null" => Some(Value::Null),
            b"nan" => Some(Value::Float(f64::NAN)),
            b"inf" => Some(Value::Float(f64::INFINITY)),
            _ => None,
        }
    }
}

/// The parser state of one input unit. Macros are read and run in `super::macros`, and the
/// include macros and `.load` in `super::include`.
pub(super) struct Core<'s, 't, 'e, 'v, 'l> {
    pub(super) src: &'s [u8],
    /// `src` as text when all of it is valid UTF-8, checked once: keys and strings taken from
    /// it as written need no check of their own ([`Core::slice_string`]; clean-room work item
    /// C11).
    utf8: Option<&'s str>,
    /// `src` as text that the keys and strings of the tree may borrow ([`Core::tree_str`]): set
    /// only for the caller's own text in a parse that borrows from it (the serde entry points,
    /// clean-room work item C13), never for an included file or text parsed in place.
    borrow: Option<&'t str>,
    pub(super) pos: usize,
    /// The settings of the input unit; `.priority` changes its priority (§9.5).
    pub(super) settings: Settings,
    pub(super) expander: &'e mut Expander<'v>,
    /// The loader and the include state of the whole parse (§9.3, §9.4).
    pub(super) includes: &'e mut Includes<'l>,
    /// Saved comments and their attachment, when comments are saved (§12.5).
    notes: Option<Notes>,
    /// The output facts of the values created (spec §10.1), except in macro argument documents.
    pub(super) facts: Option<OutputFacts>,
    /// A key with an uppercase ASCII letter has been read under `KEY_LOWERCASE` (§12.1).
    pub(super) uppercase_keys: bool,
    pub(super) root: Value<'t>,
    frames: Vec<Frame<'t>>,
    /// How many macro argument documents this document is inside (§9.2): 0 for the main one.
    pub(super) depth: usize,
    /// The root's priority, that of the first input ([`Document<'t>`]); a context macro's handler
    /// receives it with the root (§13.2).
    pub(super) root_priority: u8,
    /// This input unit's identity, which the frames it opens record.
    unit: usize,
    /// Whether this unit is an input or an included file (or text parsed in place).
    role: Role,
    /// The input of this unit has ended.
    unit_done: bool,
    /// A name run (§9.1, *A macro directly after a name*): a macro ran directly after a name in
    /// this unit, and no key has been read in it since. The next key read counts as a word
    /// that follows a name.
    name_run: bool,
    /// Where the input continues after the last macro of this unit's name run.
    run_macro_end: Option<usize>,
    /// An including unit is in a name run, so the values this unit creates count for `recent`.
    outer_run: bool,
    /// The value created most recently (§12.5), kept while a name run is in effect in this unit
    /// or an including one: its path from the root, or `None` when it is not part of the result.
    recent: Option<PathRef>,
    /// This included unit's leading `{` took over a brace (§9.4), and no key has been read yet.
    first_key_shares: bool,
    /// The key being read is that first key: if it starts a section path, the object of its
    /// first name gets a share of the taken-over brace.
    section_shares: bool,
    /// The last scan of [`Core::line_has_bracket`]: from the first offset, the first LF, CR,
    /// `,`, `;`, `{` or `[` is at the second (the input's length if there is none).
    bracket_scan: (usize, usize),
    /// The input ended while a key waited for its value on a following line (§13.1).
    pending: Option<PendingValue<'t>>,
    /// The input ended right after a value, or after section names, with no separator
    /// ([`Boundary::Separator`]).
    unseparated: bool,
    /// How this input ends when that does not follow from the state at its end.
    ends: Option<Boundary<'t>>,
    /// The sizes of the containers that closed last, for sizing new ones ([`Sizes`]).
    sizes: Sizes,
}

/// The depths below which [`Sizes`] keeps the size of the container that closed last.
const SIZED_DEPTHS: usize = 32;

/// The most entries or elements a new container is given room for ([`Sizes`]).
const MAX_SIZE_HINT: u16 = 64;

/// For each kind of container and each of the first [`SIZED_DEPTHS`] depths, how many entries
/// or elements the container of that kind that closed last at that depth held, at most
/// [`MAX_SIZE_HINT`]. A new container at that depth is created with room for as many, so that
/// the repeated sections of a document do not grow their maps and vectors step by step
/// (clean-room work item C11; the C11 research notes adapt simd-json's containers created with
/// their exact length). Only capacity depends on it, never a result.
#[derive(Debug, Default)]
struct Sizes([[u16; SIZED_DEPTHS]; 2]);

impl Sizes {
    fn hint(&self, kind: Kind, depth: usize) -> usize {
        self.0[kind as usize]
            .get(depth)
            .map_or(0, |&n| usize::from(n))
    }

    fn record(&mut self, kind: Kind, depth: usize, len: usize) {
        if let Some(slot) = self.0[kind as usize].get_mut(depth) {
            *slot = u16::try_from(len)
                .unwrap_or(MAX_SIZE_HINT)
                .min(MAX_SIZE_HINT);
        }
    }
}

/// What kind of input unit a [`Core`] parses (§9.4, §13.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Role {
    /// An input given to the parser: the document, or one of several.
    Input,
    /// An included file, or text a registered macro parses in place (§13.2).
    Included,
}

/// The container an included file's contents go into when it is nested under a key (§9.4,
/// *Nesting under a key*).
#[derive(Debug, Clone)]
pub(super) struct NestTarget {
    pub(super) key: String,
    pub(super) array: bool,
    pub(super) priority: u8,
}

impl<'s, 't> Core<'s, 't, '_, '_, '_> {
    // ----- bytes ---------------------------------------------------------------------------

    pub(super) fn peek(&self) -> Option<u8> {
        self.src.get(self.pos).copied()
    }

    fn peek_at(&self, ahead: usize) -> Option<u8> {
        self.src.get(self.pos + ahead).copied()
    }

    fn at_block_comment(&self) -> bool {
        self.peek() == Some(b'/') && self.peek_at(1) == Some(b'*')
    }

    #[cold]
    #[inline(never)]
    pub(super) fn error(&self, kind: ErrorKind, at: usize) -> Error {
        Error::new(kind, position_at(self.src, at))
    }

    /// An error about `key`: where it starts, in this input or, for a key read in an earlier
    /// input, in that one.
    #[cold]
    #[inline(never)]
    fn key_error(&self, key: &Key<'t>, kind: ErrorKind) -> Error {
        match key.origin {
            Some(position) => Error::new(kind, position),
            None => self.error(kind, key.at),
        }
    }

    fn found(&self, at: usize) -> Option<char> {
        self.src.get(at).map(|&b| char::from(b))
    }

    fn text(&self, bytes: Vec<u8>, at: usize) -> Result<String, Error> {
        String::from_utf8(bytes).map_err(|_| self.error(ErrorKind::InvalidUtf8, at))
    }

    /// `src[from..to]` as a string, for a token that starts at `at`: [`Core::text`] of those
    /// bytes, without checking them again when the whole input unit is valid UTF-8.
    /// `src[from..to]` as a string of the tree, for a token that starts at `at`: borrowed where
    /// the tree may borrow this unit's text ([`Core::borrow`]), a copy otherwise.
    fn tree_str(&self, from: usize, to: usize, at: usize) -> Result<Str<'t>, Error> {
        if let Some(text) = self.borrow.and_then(|text| text.get(from..to)) {
            return Ok(Str::borrowed(text));
        }
        self.slice_string(from, to, at).map(Str::from)
    }

    /// `src[from..to]`, a string in which variables are expanded (§7), as a string of the tree:
    /// as [`Core::tree_str`] when no reference is replaced, a copy of the expansion otherwise.
    fn expanded_str(&mut self, from: usize, to: usize, at: usize) -> Result<Str<'t>, Error> {
        match self.expander.expand_bytes(&self.src[from..to]) {
            Some(bytes) => self.text(bytes, at).map(Str::from),
            None => self.tree_str(from, to, at),
        }
    }

    fn slice_string(&self, from: usize, to: usize, at: usize) -> Result<String, Error> {
        match self.utf8.and_then(|text| text.get(from..to)) {
            Some(text) => Ok(text.to_owned()),
            None => self.text(self.src[from..to].to_vec(), at),
        }
    }

    // ----- whitespace and comments (§2) ----------------------------------------------------

    /// Skips a `#` comment and the line break that ends it.
    fn skip_line_comment(&mut self) {
        let start = self.pos;
        let end = self.src[start..]
            .iter()
            .position(|&b| b == b'\n')
            .map_or(self.src.len(), |n| start + n);
        if let Some(notes) = &mut self.notes {
            notes.save(start, end);
        }
        self.pos = (end + 1).min(self.src.len());
    }

    /// Skips a `/* … */` comment (§2.3; [`block_comment_end`]).
    ///
    /// The saved text includes the byte after `*/` when there is one (§12.5, *Quirk*). At the
    /// end of input libucl's saved byte is undefined; nothing is added then.
    fn skip_block_comment(&mut self) -> Result<(), Error> {
        let start = self.pos;
        let src = self.src;
        let i = block_comment_end(src, start)
            .ok_or_else(|| self.error(ErrorKind::UnterminatedComment, start))?;
        if let Some(notes) = &mut self.notes {
            notes.save(start, (i + 1).min(src.len()));
        }
        self.pos = i;
        Ok(())
    }

    /// Skips whitespace, line breaks and comments: before an entry, an array element or the
    /// root value.
    fn skip_space(&mut self) -> Result<(), Error> {
        let mut after_comment = false;
        loop {
            match self.peek() {
                Some(b) if is_space(b) => {
                    self.pos += 1;
                    after_comment = false;
                }
                Some(b'#') => {
                    self.skip_hash_comment(after_comment);
                    after_comment = true;
                }
                _ if self.at_block_comment() => {
                    self.skip_block_comment()?;
                    after_comment = true;
                }
                _ => return Ok(()),
            }
        }
    }

    /// Skips a `#` comment between entries or after a value. A `#` that is the last byte of the
    /// input is not saved there unless it directly follows another comment (§12.5; QUESTIONS.md
    /// #17).
    fn skip_hash_comment(&mut self, after_comment: bool) {
        if !after_comment && self.pos + 1 == self.src.len() {
            self.pos += 1;
        } else {
            self.skip_line_comment();
        }
    }

    /// Skips spaces, tabs, VT, FF and comments before a section name (§3.4): VT and FF, which
    /// end a line before a value (§1.6), are ordinary whitespace between names. A line comment
    /// takes its line break. A `#` that is the last byte of the input directly after such
    /// whitespace is an error there (§2.2, *Quirk*; oracle runs).
    fn skip_before_name(&mut self) -> Result<(), Error> {
        let mut after_space = false;
        loop {
            match self.peek() {
                Some(b' ' | b'\t' | 0x0B | 0x0C) => {
                    self.pos += 1;
                    after_space = true;
                }
                Some(b'#') => {
                    self.check_hash_at_end(after_space)?;
                    self.skip_line_comment();
                    after_space = false;
                }
                _ if self.at_block_comment() => {
                    self.skip_block_comment()?;
                    after_space = false;
                }
                _ => return Ok(()),
            }
        }
    }

    /// Skips spaces, tabs and comments on the current line, between a key, its separator and
    /// its value. A `#` comment takes its line break with it (§1.6).
    fn skip_inline(&mut self) -> Result<(), Error> {
        loop {
            match self.peek() {
                Some(b' ' | b'\t') => self.pos += 1,
                Some(b'#') => self.skip_line_comment(),
                _ if self.at_block_comment() => self.skip_block_comment()?,
                _ => return Ok(()),
            }
        }
    }

    fn skip_blanks(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t')) {
            self.pos += 1;
        }
    }

    /// True if the rest of the current line, up to LF, CR, `,` or `;`, holds `{` or `[`
    /// anywhere (§3.4): the first of these six bytes from the current position is a bracket.
    ///
    /// The scan's result holds for every position up to the byte it found, so the names of a
    /// long section path on one line are not scanned again for each name.
    fn line_has_bracket(&mut self) -> bool {
        let (from, found) = self.bracket_scan;
        let found = if from <= self.pos && self.pos <= found {
            found
        } else {
            let found = self.src[self.pos..]
                .iter()
                .position(|&b| matches!(b, b'\n' | b'\r' | b',' | b';' | b'{' | b'['))
                .map_or(self.src.len(), |n| self.pos + n);
            self.bracket_scan = (self.pos, found);
            found
        };
        matches!(self.src.get(found), Some(b'{' | b'['))
    }

    // ----- containers ----------------------------------------------------------------------

    fn top(&self) -> &Frame<'t> {
        self.frames.last().expect("a container is open")
    }

    /// The container of the top frame.
    pub(super) fn current(&mut self) -> &mut Value<'t> {
        match &mut self.frames.last_mut().expect("a container is open").home {
            Home::Root => &mut self.root,
            Home::Attached(_, value) | Home::Detached(value) => value,
        }
    }

    /// The container of frame `index`.
    fn container(&self, index: usize) -> &Value<'t> {
        match &self.frames[index].home {
            Home::Root => &self.root,
            Home::Attached(_, value) | Home::Detached(value) => value,
        }
    }

    /// Fails when `value`, added to the current object, would be nested more than `limit`
    /// containers deep, the root included. Only macros add values that hold containers without
    /// opening them: `.inherit` (§9.7), up to its configured limit, and registered macros
    /// (§13.2), up to [`MAX_NESTING`], as deep as containers can be open (§11.2).
    pub(super) fn check_nesting(
        &self,
        value: &Value<'t>,
        limit: usize,
        at: usize,
    ) -> Result<(), Error> {
        if self.frames.len() + crate::value::nesting(value) > limit {
            return Err(self.error(ErrorKind::NestingTooDeep { limit }, at));
        }
        Ok(())
    }

    /// Pushes a frame for a container. `path` is its path from the root when the caller has it
    /// already; otherwise [`Core::frame_path`] works it out when it is needed.
    fn push_frame(
        &mut self,
        kind: Kind,
        close: Close,
        home: Home<'t>,
        path: Option<PathRef>,
        at: usize,
    ) -> Result<(), Error> {
        if self.frames.len() >= MAX_NESTING {
            return Err(self.error(ErrorKind::NestingTooDeep { limit: MAX_NESTING }, at));
        }
        let cell = OnceCell::new();
        if let Some(path) = path {
            let _ = cell.set(Some(path));
        }
        self.frames.push(Frame {
            kind,
            close,
            home,
            fresh: true,
            path: cell,
            facts_node: OnceCell::new(),
            lowercase_keys: None,
            unit: self.unit,
            stays_open: false,
        });
        Ok(())
    }

    /// Text is about to be parsed in place (§13.2): when the innermost open object is a section
    /// object, or the object of a first name with a brace of its own ([`Close::NameBrace`]), it
    /// stays open for the rest of the parse ([`Frame::stays_open`]).
    pub(super) fn keep_section_open(&mut self) {
        if let Some(frame) = self.frames.last_mut()
            && frame.kind == Kind::Object
            && frame.close.closes_with_inner()
        {
            frame.stays_open = true;
        }
    }

    /// Takes the container at `step` inside the current container out of the tree for a new
    /// frame, leaving an empty container of its kind in its place.
    fn take_out(&mut self, step: Step, kind: Kind) -> Home<'t> {
        let value = std::mem::replace(step.enter(self.current()), Self::empty(kind));
        Home::Attached(step, value)
    }

    /// Pops the top frame. An attached container goes back to its place in the container of the
    /// frame below.
    fn pop_frame(&mut self) {
        let frame = self.frames.pop().expect("a container is open");
        let len = match &frame.home {
            Home::Root => 0,
            Home::Attached(_, value) | Home::Detached(value) => match value {
                Value::Object(object) => object.len(),
                Value::Array(items) => items.len(),
                _ => 0,
            },
        };
        self.sizes.record(frame.kind, self.frames.len(), len);
        if let Home::Attached(step, value) = frame.home {
            let place = step.enter(self.current());
            debug_assert!(
                matches!(place, Value::Object(o) if o.is_empty())
                    || matches!(place, Value::Array(a) if a.is_empty()),
                "only the top frame's container changes"
            );
            *place = value;
        }
    }

    /// The position of `name` in the current object: the key itself or, with `ignore_case`, a key
    /// that differs from it only in ASCII case (§12.1), as `macros::find_key` finds it. Keys are
    /// only ever added to an object while it is parsed, or renamed to a spelling that differs
    /// only in case, so the frame's index of lowercase keys stays valid as it is extended.
    pub(super) fn find_current_key(&mut self, name: &str, ignore_case: bool) -> Option<usize> {
        let frame = self.frames.last_mut().expect("a container is open");
        let object = match &frame.home {
            Home::Root => &self.root,
            Home::Attached(_, value) | Home::Detached(value) => value,
        }
        .as_object()?;
        if let Some(index) = object.index_of(name) {
            return Some(index);
        }
        if !ignore_case {
            return None;
        }
        let (indexed, keys) = frame.lowercase_keys.get_or_insert_with(Default::default);
        debug_assert!(*indexed <= object.len(), "entries are never removed");
        for index in *indexed..object.len() {
            let (key, _) = object.get_index(index).expect("in range");
            keys.entry(key.to_ascii_lowercase()).or_insert(index);
        }
        *indexed = object.len();
        keys.get(&name.to_ascii_lowercase()).copied()
    }

    /// Where the value at `path` from the root is while containers are open: inside the
    /// container of the returned frame, at the returned rest of the path.
    fn locate<'p>(&self, path: &'p [PathSegment]) -> (usize, &'p [PathSegment]) {
        let mut frame = 0;
        let mut rest = path;
        while let Some(Frame {
            home: Home::Attached(step, _),
            ..
        }) = self.frames.get(frame + 1)
        {
            match rest.strip_prefix(step.segments().as_slice()) {
                Some(inner) => {
                    frame += 1;
                    rest = inner;
                }
                None => break,
            }
        }
        (frame, rest)
    }

    /// The value at `path` from the root as parsed so far. An open container is found in its
    /// frame, not at the empty container that keeps its place; containers open inside it are
    /// still empty in what is returned (see [`Core::filled_copy`]).
    pub(super) fn value_at(&self, path: &[PathSegment]) -> Option<&Value<'t>> {
        if self.frames.is_empty() {
            return get(&self.root, path);
        }
        let (frame, rest) = self.locate(path);
        get(self.container(frame), rest)
    }

    /// A copy of the value at `path` from the root as parsed so far, with the containers that
    /// are open inside it, as they are now (§9.7, *Quirk*).
    pub(super) fn filled_copy(&self, path: &[PathSegment]) -> Option<Value<'t>> {
        if self.frames.is_empty() {
            return get(&self.root, path).cloned();
        }
        let (frame, rest) = self.locate(path);
        let mut copy = get(self.container(frame), rest)?.clone();
        // The next frame's container, if it lies inside the value, and the chain of attached
        // frames above it.
        let Some(Frame {
            home: Home::Attached(step, _),
            ..
        }) = self.frames.get(frame + 1)
        else {
            return Some(copy);
        };
        let segments = step.segments();
        let Some(inner) = segments.strip_prefix(rest) else {
            return Some(copy);
        };
        let mut top = frame + 1;
        while matches!(
            self.frames.get(top + 1),
            Some(Frame {
                home: Home::Attached(..),
                ..
            })
        ) {
            top += 1;
        }
        let mut filled = self.container(top).clone();
        for below in (frame + 1..top).rev() {
            let mut outer = self.container(below).clone();
            let Home::Attached(step, _) = &self.frames[below + 1].home else {
                unreachable!("the frames of the chain are attached")
            };
            *step.enter(&mut outer) = filled;
            filled = outer;
        }
        if inner.is_empty() {
            copy = filled;
        } else {
            let step = Step::resolve(&copy, inner).expect("the open container is in the copy");
            *step.enter(&mut copy) = filled;
        }
        Some(copy)
    }

    fn empty(kind: Kind) -> Value<'t> {
        match kind {
            Kind::Object => Value::Object(Object::new()),
            Kind::Array => Value::Array(Vec::new().into()),
        }
    }

    /// An empty container of `kind` for a frame that is about to be pushed, with room for as
    /// many entries or elements as the one that closed last at its depth ([`Sizes`]).
    fn sized(&self, kind: Kind) -> Value<'t> {
        let capacity = self.sizes.hint(kind, self.frames.len());
        match kind {
            Kind::Object => Value::Object(Object::with_capacity(capacity)),
            Kind::Array => Value::Array(Vec::with_capacity(capacity).into()),
        }
    }

    /// A name run is in effect here or in an including unit: the value created most recently is
    /// kept (§9.1).
    fn tracking(&self) -> bool {
        self.name_run || self.outer_run
    }

    /// Values are identified by their paths: for saved comments, or for a name run.
    fn wants_paths(&self) -> bool {
        self.notes.is_some() || self.tracking()
    }

    /// The path from the root of frame `index`'s container, or `None` when it is not part of the
    /// result. Each frame's path is worked out once, from the path of the frame below and its
    /// step, when it is first needed.
    fn frame_path(&self, index: usize) -> Option<PathRef> {
        let mut known = index;
        while self.frames[known].path.get().is_none() {
            match &self.frames[known].home {
                Home::Root => {
                    let _ = self.frames[known].path.set(Some(PathRef::root()));
                }
                Home::Detached(_) => {
                    let _ = self.frames[known].path.set(None);
                }
                Home::Attached(..) => known -= 1,
            }
        }
        for above in known + 1..=index {
            let Home::Attached(step, _) = &self.frames[above].home else {
                unreachable!("a frame whose path is not known yet is attached")
            };
            let path = self.frames[above - 1]
                .path
                .get()
                .expect("set just before")
                .as_ref()
                .map(|below| below.join(step.segments()));
            let _ = self.frames[above].path.set(path);
        }
        self.frames[index].path.get().cloned().flatten()
    }

    /// The node of the top container in the output facts, or `None` when facts are not recorded
    /// or the container is not part of the result. Each frame's node is found or added once, from
    /// the node of the frame below and its step, when it is first needed.
    pub(super) fn facts_node(&mut self) -> Option<NodeId> {
        let facts = self.facts.as_mut()?;
        let index = self.frames.len() - 1;
        let mut known = index;
        while self.frames[known].facts_node.get().is_none() {
            match &self.frames[known].home {
                Home::Root => {
                    let _ = self.frames[known].facts_node.set(Some(facts::ROOT));
                }
                Home::Detached(_) => {
                    let _ = self.frames[known].facts_node.set(None);
                }
                Home::Attached(..) => known -= 1,
            }
        }
        for above in known + 1..=index {
            let Home::Attached(step, _) = &self.frames[above].home else {
                unreachable!("a frame whose node is not known yet is attached")
            };
            let node = self.frames[above - 1]
                .facts_node
                .get()
                .expect("set just before")
                .map(|below| step.descend_facts(facts, below));
            let _ = self.frames[above].facts_node.set(node);
        }
        self.frames[index].facts_node.get().copied().flatten()
    }

    /// The node in the output facts of the value at `path` in the top container, added if
    /// missing; `None` as for [`Core::facts_node`].
    pub(super) fn facts_node_below(&mut self, path: &[Pos]) -> Option<NodeId> {
        let object = self.facts_node()?;
        let facts = self.facts.as_mut().expect("facts are recorded");
        Some(facts.descend_or_insert(object, path))
    }

    /// The path of the top container from the root, or `None` when it is not part of the
    /// result.
    fn top_node(&self) -> Option<PathRef> {
        self.frame_path(self.frames.len() - 1)
    }

    /// [`Core::top_node`] written out.
    pub(super) fn top_path(&self) -> Option<ValuePath> {
        self.top_node().map(|path| path.to_vec())
    }

    /// The name of `key`, which [`Core::insert`] may have moved into the current object.
    fn key_name<'k>(&'k self, key: &'k Key<'t>) -> &'k str {
        stored_key_name(&self.frames, &self.root, key)
    }

    /// [`Core::placed_path`] for `key`, just inserted.
    fn placed_key_path(&self, key: &Key<'t>, placement: Placement) -> Option<PathRef> {
        if !self.wants_paths() {
            return None;
        }
        self.placed_path(self.key_name(key), placement)
    }

    /// The path of a value just placed in the current object under `key`, when values need
    /// paths and the value is part of the result.
    fn placed_path(&self, key: &str, placement: Placement) -> Option<PathRef> {
        if !self.wants_paths() {
            return None;
        }
        let entry = |index| PathSegment::Key {
            key: key.to_owned(),
            index,
        };
        let segments = match placement {
            Placement::Slot(slot) => vec![entry(slot)],
            Placement::Collected(index) => vec![entry(0), PathSegment::Index(index)],
            Placement::Merged => vec![entry(0)],
            Placement::Dropped => return None,
        };
        Some(self.top_node()?.join(segments))
    }

    /// A value was created at `path`: pending comments attach to it (§12.5), and it is the
    /// value created most recently.
    fn created(&mut self, path: Option<PathRef>) {
        if self.tracking() {
            self.recent.clone_from(&path);
        }
        if let Some(notes) = &mut self.notes {
            notes.created(path);
        }
    }

    /// Makes `path` the value created most recently, without attaching comments.
    fn made_recent(&mut self, path: Option<PathRef>) {
        if self.tracking() {
            self.recent.clone_from(&path);
        }
        if let Some(notes) = &mut self.notes {
            notes.set_last(path);
        }
    }

    /// Inserts `value` under `key` in the current object, by the duplicate rules of §8.
    ///
    /// Under `KEY_LOWERCASE`, `key` is first given the spelling of an existing key that differs
    /// from it only in ASCII case (§12.1; QUESTIONS.md #20). If the new value then replaces all
    /// of the entry's values, the entry takes the spelling of `key` as written, in its place
    /// (QUESTIONS.md #27).
    ///
    /// Then the value's output facts are recorded (spec §10.1): `origin`, and the key as written
    /// for this value; and, when locations are recorded, `at`, where the value was written.
    fn insert(
        &mut self,
        key: &mut Key<'t>,
        value: Value<'t>,
        origin: Origin,
        at: usize,
    ) -> Result<Placement, Error> {
        let written = self.match_key_case(key);
        let Settings {
            mut flags,
            priority,
            strategy,
        } = self.settings;
        // `read_key` has already lowercased the key where §12.1 asks for it.
        flags.remove(ParserFlags::KEY_LOWERCASE);
        let track =
            self.notes.is_some() || self.facts.as_ref().is_some_and(OutputFacts::tracks_moves);
        let object = self
            .current()
            .as_object_mut()
            .expect("entries are parsed inside objects");
        // The key is hashed once, and its entry, if any, found by position from then on.
        let probe = object.probe(&key.name);
        let index = probe.index;
        let entry = index.map(|index| object.get_index(index).expect("just found").1);
        let is_container = |v: &Value<'t>| v.is_object() || v.is_array();
        // Under `merge`, a scalar that follows a container takes its place in the entry (§8.4,
        // *Quirk*); the oracle keeps the container's comments on it, as for one value.
        let existed = entry.is_some();
        let in_place = entry.is_some_and(|e| {
            strategy == DuplicateStrategy::Merge && is_container(e.first()) && !is_container(&value)
        });
        // A number written with digits, a time or a boolean that takes the place of a
        // non-empty container keeps the normal layout of a multi-value entry; the keywords `nan`
        // and `inf` follow their own kind, as strings and `null` do (spec §10.7, *Quirk*).
        let counts_as_container = match value {
            Value::Integer(_) | Value::Time(_) | Value::Boolean(_) => true,
            Value::Float(_) => !origin.keyword,
            _ => false,
        };
        let normal_layout = in_place
            && counts_as_container
            && entry.is_some_and(|e| match e.first() {
                Value::Object(o) => !o.is_empty(),
                Value::Array(a) => !a.is_empty(),
                _ => false,
            });
        let was_collected = entry.is_some_and(|e| e.slots()[0].is_collected());
        let before = entry.filter(|_| track).map(|e| Before {
            len: e.len(),
            collected: e.slots()[0].is_collected(),
            in_place,
        });
        let slot = Slot::new(value, priority);
        let result = match index {
            // A new key is moved into the object rather than copied; its name is read from
            // there from now on ([`Key::stored`]).
            None => {
                key.stored = Some(object.len());
                let name = std::mem::take(&mut key.name);
                object.push_probed(probe, name, Entry::from_slot(slot));
                Ok(Placement::Slot(0))
            }
            Some(_) => object.insert_slot_at(index, key.name.as_str(), slot, strategy, flags),
        };
        // A new key, which cannot fail, is the object's last entry.
        key.entry = index.unwrap_or(object.len() - 1);
        let after = if track {
            object.get_index(key.entry).map_or(0, |(_, e)| e.len())
        } else {
            0
        };
        let placement = result.map_err(|DuplicateKeyError { key: name }| {
            self.key_error(key, ErrorKind::DuplicateKey { key: name })
        })?;
        if track {
            self.values_moved(key.entry, &key.name, before, placement, after);
        }
        if matches!(placement, Placement::Collected(_)) && !was_collected {
            self.collection_key(key.entry, &key.name);
        }
        if let Some(written) = &written
            && existed
            && placement == Placement::Slot(0)
            && !in_place
        {
            let object = self
                .current()
                .as_object_mut()
                .expect("entries are parsed inside objects");
            if object.rename_key(&key.name, written.as_str()) {
                key.name.clone_from(written);
            }
        }
        if self.facts.is_some() {
            let facts = PlacedFacts {
                origin,
                in_place,
                normal_layout,
            };
            self.record_facts(key, written.as_deref(), placement, facts, at);
        }
        Ok(placement)
    }

    /// Records the output facts of a value just placed under `key` (spec §10.1). `written` is
    /// the key as written for the value when it differs from `key`, the entry's key. When
    /// locations are recorded, the value's is `at`, and its key's where `key` starts.
    fn record_facts(
        &mut self,
        key: &Key<'t>,
        written: Option<&str>,
        placement: Placement,
        placed: PlacedFacts,
        at: usize,
    ) {
        let (slot, collected, keyed) = match placement {
            // A scalar that took a container's place under `merge` keeps the container's key
            // (oracle runs, QUESTIONS.md #50).
            Placement::Slot(slot) => (slot, false, !placed.in_place),
            Placement::Collected(index) => (index, true, false),
            Placement::Merged | Placement::Dropped => return,
        };
        // The key's facts are only recorded for a keyed value. A key written with another
        // spelling than the entry's was already there, so its name was not moved.
        let (key_spelling, key_quoted) = match written {
            _ if !keyed => (None, None),
            Some(spelling) => (
                (spelling != key.name).then(|| spelling.to_owned()),
                (key.quoted != key_needs_quoting(spelling)).then_some(key.quoted),
            ),
            None => (
                None,
                (key.quoted != key.needs_quoting).then_some(key.quoted),
            ),
        };
        let has_value_facts = placed.origin.has_string_facts() || placed.normal_layout;
        let has_key_facts = keyed && (key_spelling.is_some() || key_quoted.is_some());
        let stale = placed.in_place && self.facts.as_ref().is_some_and(OutputFacts::tracks_moves);
        let locating = self
            .facts
            .as_ref()
            .is_some_and(OutputFacts::records_locations);
        if !has_value_facts && !has_key_facts && !stale && !locating {
            return;
        }
        let Some(object) = self.facts_node() else {
            return;
        };
        let name = stored_key_name(&self.frames, &self.root, key);
        let facts = self.facts.as_mut().expect("facts are recorded");
        let node = if collected {
            let entry = facts.entry_child_or_insert(object, key.entry, name, 0);
            facts.element_child_or_insert(entry, slot)
        } else {
            facts.entry_child_or_insert(object, key.entry, name, slot)
        };
        if placed.in_place {
            facts.clear_below(node);
        }
        facts.update(node, |f| {
            f.single_quoted = placed.origin.single_quoted;
            f.multiline = placed.origin.multiline;
            f.normal_layout = placed.normal_layout;
            if keyed {
                f.key_spelling = key_spelling;
                f.key_quoted = key_quoted;
            }
        });
        facts.locate(node, at, Some(key.at));
    }

    /// The entry at position `entry`, whose key is `key`, of the current object has just become
    /// a `NO_IMPLICIT_ARRAYS` collection (§8.5). The array's key never needs quoting, whatever
    /// the keys of its values were, and is spelled as the entry's key, that of its first value
    /// (spec §10.1, *Quirk*).
    fn collection_key(&mut self, entry: usize, key: &str) {
        if self.facts.is_none() || !key_needs_quoting(key) {
            return;
        }
        let pos = Pos::Entry {
            entry,
            key: KeyCopy::from(key),
            slot: 0,
        };
        let Some(node) = self.facts_node_below(&[pos]) else {
            return;
        };
        let facts = self.facts.as_mut().expect("checked above");
        facts.update(node, |f| f.key_quoted = Some(false));
    }

    /// Where the current object is for saved comments and output facts: its path when comments
    /// are saved, and its node when facts are recorded. Both are `None` when it is not part of
    /// the result.
    fn object_places(&mut self) -> Places {
        let path = if self.notes.is_some() {
            self.top_node()
        } else {
            None
        };
        Places {
            path,
            node: self.facts_node(),
        }
    }

    /// Values of the entry at position `entry`, whose key is `key`, of the object at `object`
    /// were replaced: value `slot`, or all.
    fn replaced_values(&mut self, object: &Places, entry: usize, key: &str, slot: Option<usize>) {
        if let (Some(notes), Some(path)) = (&mut self.notes, &object.path) {
            notes.replaced(path, key, slot);
        }
        if let (Some(facts), Some(node)) = (&mut self.facts, object.node) {
            facts.replaced(node, entry, slot);
        }
    }

    /// The entry at position `entry`, whose key is `key`, of the object at `object` became a
    /// `NO_IMPLICIT_ARRAYS` collection of its first `count` values.
    fn collected_values(&mut self, object: &Places, entry: usize, key: &str, count: usize) {
        if let (Some(notes), Some(path)) = (&mut self.notes, &object.path) {
            notes.collected(path, key, count);
        }
        if let (Some(facts), Some(node)) = (&mut self.facts, object.node) {
            facts.collected(node, entry, key, count);
        }
    }

    /// Under `KEY_LOWERCASE`, keys are compared without regard to ASCII case, and an entry keeps
    /// the spelling of its first key. Keys are lowercased as they are read, so only escapes in
    /// quoted keys can leave uppercase letters (§12.1); until one does, exact comparison is
    /// enough.
    ///
    /// Returns the spelling of `key` as written when it was changed.
    fn match_key_case(&mut self, key: &mut Key<'t>) -> Option<Str<'t>> {
        if !self.settings.flags.contains(ParserFlags::KEY_LOWERCASE) {
            return None;
        }
        if key.name.bytes().any(|b| b.is_ascii_uppercase()) {
            self.uppercase_keys = true;
        }
        if !self.uppercase_keys {
            return None;
        }
        let index = self.find_current_key(&key.name, true)?;
        let (existing, _) = self
            .current()
            .as_object()
            .and_then(|object| object.get_index(index))
            .expect("the key was just found");
        if *existing == key.name {
            return None;
        }
        let existing = existing.clone();
        Some(std::mem::replace(&mut key.name, existing))
    }

    /// Keeps saved comments and output facts with their values when an insertion under `key`
    /// replaced or moved values that were already there (QUESTIONS.md #17). `after` is the
    /// entry's length now.
    fn values_moved(
        &mut self,
        entry: usize,
        key: &str,
        before: Option<Before>,
        placement: Placement,
        after: usize,
    ) {
        let Some(Before {
            len,
            collected,
            in_place,
        }) = before
        else {
            return;
        };
        // The object's path is written out only when values moved.
        match placement {
            Placement::Slot(slot) if slot == len && after == len + 1 => {}
            Placement::Slot(_) if in_place => {}
            Placement::Slot(slot) => {
                let object = self.object_places();
                let slot = (after != 1).then_some(slot);
                self.replaced_values(&object, entry, key, slot);
            }
            Placement::Collected(_) if !collected => {
                let object = self.object_places();
                // Only the first value goes into the collection (QUESTIONS.md #25).
                for slot in 1..len {
                    self.replaced_values(&object, entry, key, Some(slot));
                }
                self.collected_values(&object, entry, key, 1);
            }
            Placement::Collected(_) | Placement::Merged | Placement::Dropped => {}
        }
    }

    /// Opens a new object or array under `key` in the current object. `at` is where it was
    /// written: its bracket, or the name of a section (§3.4).
    fn open_in_object(
        &mut self,
        mut key: Key<'t>,
        kind: Kind,
        close: Close,
        at: usize,
    ) -> Result<(), Error> {
        if self.frames.len() >= MAX_NESTING {
            return Err(self.error(ErrorKind::NestingTooDeep { limit: MAX_NESTING }, self.pos));
        }
        let container = self.sized(kind);
        let placement = self.insert(&mut key, container, Origin::default(), at)?;
        let path = self.placed_key_path(&key, placement);
        let name = match key.stored {
            Some(_) => KeyCopy::from(self.key_name(&key)),
            None => KeyCopy::from(&key.name),
        };
        let entry = key.entry;
        let home = match placement {
            Placement::Slot(slot) => {
                let step = Step::Entry {
                    entry,
                    key: name,
                    slot,
                };
                self.take_out(step, kind)
            }
            Placement::Collected(index) => {
                let step = Step::Collected {
                    entry,
                    key: name,
                    index,
                };
                self.take_out(step, kind)
            }
            // Merged into the entry's first value, a container of the same kind (§8.4).
            Placement::Merged => {
                let step = Step::Entry {
                    entry,
                    key: name,
                    slot: 0,
                };
                self.take_out(step, kind)
            }
            Placement::Dropped => Home::Detached(Self::empty(kind)),
        };
        self.created(path.clone());
        self.push_frame(kind, close, home, path, key.at)
    }

    /// Opens a new object or array as the next element of the current array, written with its
    /// bracket at `bracket`.
    fn open_in_array(&mut self, kind: Kind, close: Close, bracket: usize) -> Result<(), Error> {
        let at = self.pos;
        if self.frames.len() >= MAX_NESTING {
            return Err(self.error(ErrorKind::NestingTooDeep { limit: MAX_NESTING }, at));
        }
        let index = self.push_element(Self::empty(kind));
        self.element_facts(index, Origin::default(), bracket);
        let path = self.element_path(index);
        self.created(path.clone());
        let home = Home::Attached(Step::Element(index), self.sized(kind));
        self.push_frame(kind, close, home, path, at)
    }

    /// Appends `value` to the current array and returns its index.
    fn push_element(&mut self, value: Value<'t>) -> usize {
        let array = self
            .current()
            .as_array_mut()
            .expect("elements are parsed inside arrays");
        array.push(value);
        array.len() - 1
    }

    /// Records the output facts of element `index` of the current array, written at `at`: a
    /// string's origin (spec §10.1) and, when locations are recorded, `at`.
    fn element_facts(&mut self, index: usize, origin: Origin, at: usize) {
        let Some(facts) = &self.facts else {
            return;
        };
        if !origin.has_string_facts() && !facts.records_locations() {
            return;
        }
        let Some(node) = self.facts_node_below(&[Pos::Element(index)]) else {
            return;
        };
        let facts = self.facts.as_mut().expect("checked above");
        if origin.has_string_facts() {
            facts.update(node, |f| {
                f.single_quoted = origin.single_quoted;
                f.multiline = origin.multiline;
            });
        }
        facts.locate(node, at, None);
    }

    fn element_path(&self, index: usize) -> Option<PathRef> {
        if !self.wants_paths() {
            return None;
        }
        Some(self.top_node()?.child(PathSegment::Index(index)))
    }

    /// Closes the top container after its bracket, with the section objects around it (§3.4).
    fn close_container(&mut self) -> Result<(), Error> {
        if let Some(notes) = &mut self.notes {
            notes.trailing();
        }
        self.pop_frame();
        self.close_sections()
    }

    /// After a bracketed container has closed: the section objects around it close too (§3.4).
    /// Objects left open by a section path close with the first bracketed container that closes
    /// in them, back to the nearest object written with a bracket, or the root. Section objects
    /// of both kinds close together in any order, as the oracle does when an included file
    /// leaves one inside another, and so do those whose brace an included file took over
    /// ([`Close::closes_with_inner`]). A section object that stays open after text parsed in
    /// place does not close, and so neither do the section objects around it
    /// ([`Frame::stays_open`], §13.2).
    fn close_sections(&mut self) -> Result<(), Error> {
        let mut outermost = None;
        while self
            .frames
            .last()
            .is_some_and(|f| f.close.closes_with_inner() && !f.stays_open)
        {
            let path = if self.wants_paths() {
                self.top_node()
            } else {
                None
            };
            self.pop_frame();
            outermost = Some(path);
        }
        if let Some(path) = outermost {
            // The outermost of the objects that closed with the bracket counts as the value
            // created most recently (§12.5).
            self.made_recent(path);
        }
        if self.frames.is_empty() {
            // The root's closing bracket: the rest of the input is ignored (§1.1, *Quirk*).
            return Ok(());
        }
        self.after_value(false)
    }

    // ----- document (§1) -------------------------------------------------------------------

    /// Reads this input, which goes on where the input before it ended (spec §13.1).
    fn read_input(&mut self, boundary: Boundary<'t>) -> Result<(), Error> {
        match boundary {
            Boundary::Start if self.src.is_empty() => {
                // §13.1, *Quirk*: a zero-byte first input gives an empty object root that later
                // inputs cannot add to.
                self.root = Self::empty(Kind::Object);
                if let Some(facts) = &mut self.facts {
                    facts.locate(facts::ROOT, 0, None);
                }
                self.ends = Some(Boundary::Empty);
                return Ok(());
            }
            Boundary::Start => return self.run(),
            Boundary::Empty => return self.after_empty_first_input(),
            Boundary::Closed => return self.after_closed_root(),
            Boundary::Entry if self.src.iter().all(|&b| is_space(b)) => {
                // A later input of whitespace alone, where an entry could start, makes the next
                // one need a separator first (oracle runs, QUESTIONS.md #59). Its end attaches
                // pending comments as the end of any input does.
                self.pos = self.src.len();
                self.ends = Some(Boundary::Separator);
                if let Some(notes) = &mut self.notes {
                    notes.trailing();
                }
                return Ok(());
            }
            // As at the start of a document, a `#` that is the last byte after whitespace is an
            // error (§2.2); the end of the input before counts as whitespace (oracle runs).
            Boundary::Entry => self.skip_space_checking_hash(true)?,
            Boundary::Separator => self.after_previous_value()?,
            Boundary::Value(pending) => self.resume_value(*pending)?,
        }
        self.steps()
    }

    /// How this input ended, for the next one.
    fn boundary_at_end(&mut self) -> Boundary<'t> {
        if let Some(ends) = self.ends.take() {
            return ends;
        }
        if self.frames.is_empty() {
            Boundary::Closed
        } else if let Some(pending) = self.pending.take() {
            Boundary::Value(Box::new(pending))
        } else if self.unseparated {
            Boundary::Separator
        } else {
            Boundary::Entry
        }
    }

    /// Parses entries and elements until the input ends or the root closes.
    fn steps(&mut self) -> Result<(), Error> {
        while !self.unit_done
            && let Some(frame) = self.frames.last()
        {
            match frame.kind {
                Kind::Object => self.object_step()?,
                Kind::Array => self.array_step()?,
            }
        }
        Ok(())
    }

    /// A later input after a zero-byte first input (§13.1, *Quirk*): only a group of comments
    /// at its very start (§1.1), then whitespace, may be in it.
    fn after_empty_first_input(&mut self) -> Result<(), Error> {
        self.comment_group()?;
        while self.peek().is_some_and(is_space) {
            self.pos += 1;
        }
        self.ends = Some(Boundary::Empty);
        if self.pos < self.src.len() {
            return Err(self.error(ErrorKind::AfterRoot, self.pos));
        }
        // Its comments attach after the root, as at the end of input (§12.5; oracle runs).
        if let Some(notes) = &mut self.notes {
            notes.trailing();
        }
        Ok(())
    }

    /// A later input after the root has closed (§13.1, *The root*; the details are from oracle
    /// runs, QUESTIONS.md #59). After spaces and tabs it must hold nothing, or begin with a line
    /// break, `;`, `,` or a comment: then, after any mix of these, the rest of the input is
    /// ignored, as after the root's closing bracket (§1.1), unless it begins with `}` or `]`.
    fn after_closed_root(&mut self) -> Result<(), Error> {
        self.ends = Some(Boundary::Closed);
        self.skip_blanks();
        match self.peek() {
            None => return Ok(()),
            Some(b'\n' | b'\r' | 0 | b',' | b';' | b'#') => {}
            _ if self.at_block_comment() => {}
            Some(_) => return Err(self.error(ErrorKind::AfterRoot, self.pos)),
        }
        self.after_value(false)?;
        if let Some(c @ (b'}' | b']')) = self.peek() {
            let found = char::from(c);
            return Err(self.error(ErrorKind::UnmatchedClose { found }, self.pos));
        }
        self.pos = self.src.len();
        Ok(())
    }

    /// The start of an input after one that a value ended without a separator (§13.1,
    /// *Quirk*): a line break, `;`, `,`, a comment or a closing bracket must come first, after
    /// spaces and tabs, as after a quoted value (§1.3).
    fn after_previous_value(&mut self) -> Result<(), Error> {
        self.skip_blanks();
        match self.peek() {
            None | Some(b'\n' | b'\r' | 0 | b',' | b';' | b'#' | b'}' | b']') => {}
            _ if self.at_block_comment() => {}
            Some(c) => {
                let found = char::from(c);
                return Err(self.error(ErrorKind::UnseparatedInput { found }, self.pos));
            }
        }
        self.after_value(false)
    }

    /// The value of a key that ended the input before, on a following line of this one (§1.6;
    /// oracle runs, QUESTIONS.md #59). It is inserted with the settings of the key's input; what
    /// an object or array value holds gets this input's.
    fn resume_value(&mut self, pending: PendingValue<'t>) -> Result<(), Error> {
        // An input that is a single `#` is an error there (oracle runs).
        self.check_hash_at_end(true)?;
        self.leading_comments()?;
        if self.peek().is_none() {
            self.pending = Some(pending);
            return Ok(());
        }
        let settings = std::mem::replace(&mut self.settings, pending.settings);
        let result = self.object_value(pending.key);
        self.settings = settings;
        result
    }

    /// The first input: the root (§1.1), then its entries or elements.
    fn run(&mut self) -> Result<(), Error> {
        // §1.1: a leading bracket starts the root only after whitespace alone, or directly after
        // a group of comments at the very start of the input.
        let mut after_space = false;
        while self.peek().is_some_and(is_space) {
            self.pos += 1;
            after_space = true;
        }
        if !after_space {
            self.comment_group()?;
        }
        let (kind, close) = match self.peek() {
            Some(b'[') => (Kind::Array, Close::Bracket),
            Some(b'{') => (Kind::Object, Close::Brace),
            _ => (Kind::Object, Close::Eof),
        };
        let root_at = if close == Close::Eof { 0 } else { self.pos };
        if close != Close::Eof {
            self.pos += 1;
        }
        self.root = Self::empty(kind);
        if let Some(facts) = &mut self.facts {
            facts.locate(facts::ROOT, root_at, None);
        }
        self.push_frame(kind, close, Home::Root, Some(PathRef::root()), self.pos)?;
        if close == Close::Eof {
            // Where the first key of an unbraced root could start (§2.2, *Quirk*).
            self.skip_space_checking_hash(after_space)?;
        }
        self.steps()
    }

    // ----- included units (§9.4) ------------------------------------------------------------

    /// Parses `input`, an included file whose canonical path is `file`, or text a registered
    /// macro parses in place (`file` is `None`), as a new input unit with `settings`. Its
    /// entries go into the current container, and it continues with the container stack the
    /// unit leaves behind.
    pub(super) fn parse_included(
        &mut self,
        input: &[u8],
        settings: Settings,
        file: Option<&Path>,
    ) -> Result<(), Error> {
        let cursor = self.notes.as_mut().map(|n| n.suspend(self.src));
        let outer_run = self.tracking();
        let outer_unit = match (&mut self.facts, file) {
            (Some(facts), Some(file)) => Some(facts.enter_unit(file, input)),
            _ => None,
        };
        let unit = self.includes.new_unit();
        self.includes.open_units.push(unit);
        let mut inner = Core {
            src: input,
            utf8: std::str::from_utf8(input).ok(),
            borrow: None,
            pos: 0,
            settings,
            expander: &mut *self.expander,
            includes: &mut *self.includes,
            notes: self.notes.take(),
            facts: self.facts.take(),
            uppercase_keys: self.uppercase_keys,
            root: std::mem::replace(&mut self.root, Value::Null),
            frames: std::mem::take(&mut self.frames),
            depth: self.depth,
            root_priority: self.root_priority,
            unit,
            role: Role::Included,
            unit_done: false,
            name_run: false,
            run_macro_end: None,
            outer_run,
            recent: self.recent.take(),
            first_key_shares: false,
            section_shares: false,
            bracket_scan: (usize::MAX, 0),
            pending: None,
            unseparated: false,
            ends: None,
            sizes: Sizes::default(),
        };
        let result = inner.run_included();
        inner.includes.open_units.pop();
        self.root = inner.root;
        self.frames = inner.frames;
        self.uppercase_keys = inner.uppercase_keys;
        self.notes = inner.notes;
        self.facts = inner.facts;
        if let (Some(facts), Some(unit)) = (&mut self.facts, outer_unit) {
            facts.leave_unit(unit);
        }
        self.recent = inner.recent;
        if let (Some(notes), Some(cursor)) = (&mut self.notes, cursor) {
            notes.resume(input, cursor);
        }
        match file {
            Some(file) => result.map_err(|e| e.in_file(file)),
            None => result,
        }
    }

    /// The start of an included unit follows §1.1, except that a `[` there is an error and a
    /// `{` takes over the brace of the object the entries go into (§9.4).
    ///
    /// A `{` or `[` there that is the last byte of the unit, where §1.1 lets a bracketed root
    /// start (after whitespace alone, or directly after a comment group at the start of the
    /// unit), takes nothing over and is not checked: the unit adds nothing (§9.4, *Quirk: a file
    /// that ends right after its leading bracket*; §13.2; QUESTIONS.md #70).
    fn run_included(&mut self) -> Result<(), Error> {
        let mut after_space = false;
        while self.peek().is_some_and(is_space) {
            self.pos += 1;
            after_space = true;
        }
        if !after_space {
            self.comment_group()?;
        }
        match self.peek() {
            Some(b'[' | b'{') if self.pos + 1 == self.src.len() => {
                self.pos += 1;
                return self.end_included_unit();
            }
            // Other text in place that starts with `[` is uncertain in libucl (§13.2); the
            // project reports the error there, as for an included file (§9.4).
            Some(b'[') => return Err(self.error(ErrorKind::IncludeArrayRoot, self.pos)),
            Some(b'{') => {
                self.pos += 1;
                let frame = self.frames.last_mut().expect("a container is open");
                frame.close = Close::IncludedBrace(match frame.close {
                    Close::IncludedBrace(revert) => revert,
                    close if close.is_section() => Revert::Section,
                    Close::NameBrace => Revert::Section,
                    _ => Revert::Open,
                });
                self.first_key_shares = true;
            }
            _ => self.skip_space_checking_hash(after_space)?,
        }
        self.steps()
    }

    /// The input of an included unit has ended: the check at the end of a unit, after which the
    /// containers stay open for the including unit (§9.4). Pending comments attach as at the end
    /// of input, except after a file of no bytes at all, where they stay pending (oracle runs,
    /// QUESTIONS.md #43).
    fn end_included_unit(&mut self) -> Result<(), Error> {
        self.unit_end_check()?;
        if let Some(notes) = &mut self.notes
            && !self.src.is_empty()
        {
            notes.trailing();
        }
        self.unit_done = true;
        Ok(())
    }

    /// An input has ended: the check at the end of a unit (§9.4), for the containers it opened
    /// (§13.1, *Brackets*). The containers stay open for a later input; after the last one,
    /// [`Document::finish`] closes them. Pending comments attach to the value created most
    /// recently (§12.5), unless a key waits for its value on a following line: then they wait
    /// with it.
    fn end_input(&mut self) -> Result<(), Error> {
        self.unit_end_check()?;
        if let Some(notes) = &mut self.notes {
            if self.pending.is_some() {
                notes.wait_for_value();
            } else {
                notes.trailing();
            }
        }
        self.unit_done = true;
        Ok(())
    }

    /// The check at the end of a unit (§9.4): the open containers, from the innermost outward,
    /// up to the first that another unit opened. One that holds a bracket, its own or a taken-over
    /// one, is not closed, and that is an error. Containers below one that another unit opened
    /// are not checked, whatever that container is (oracle runs, QUESTIONS.md #41).
    fn unit_end_check(&self) -> Result<(), Error> {
        for (index, frame) in self.frames.iter().enumerate().rev() {
            if frame.unit != self.unit {
                self.note_ended_unit_containers(index);
                break;
            }
            if frame.close.has_bracket() {
                let kind = match frame.kind {
                    Kind::Object => ErrorKind::UnterminatedObject,
                    Kind::Array => ErrorKind::UnterminatedArray,
                };
                return Err(self.error(kind, self.pos));
            }
        }
        Ok(())
    }

    /// Where the check at the end of an included file stops at frame `index`, opened by a unit
    /// that has ended: whether libucl counts such containers as this file's is uncertain (spec
    /// §9.4, *Where the entries go*). It matters only when one of them, from that frame inward
    /// to the first container of a unit still open, holds a bracket.
    fn note_ended_unit_containers(&self, index: usize) {
        let open = &self.includes.open_units;
        if self.role == Role::Included
            && self.frames[..=index]
                .iter()
                .rev()
                .take_while(|frame| !open.contains(&frame.unit))
                .any(|frame| frame.close.has_bracket())
        {
            self.includes.reached(Uncertain::EndedUnitContainer);
        }
    }

    /// The number of open containers.
    pub(super) fn open_containers(&self) -> usize {
        self.frames.len()
    }

    /// Closes the containers above the first `len`, without any check: the end of a file
    /// nested under a key (§9.4).
    pub(super) fn close_containers_above(&mut self, len: usize) {
        while self.frames.len() > len {
            self.pop_frame();
        }
    }

    /// Opens the container that an included file nested under a key goes into, in the current
    /// object (§9.4, *Nesting under a key*):
    ///
    /// | `target` | key absent | first value an object | first value an array | anything else |
    /// | --- | --- | --- | --- | --- |
    /// | object | new object | that object | error | error |
    /// | array | new array of one new object | array of that value and a new object | a new object appended | array of that value and a new object |
    ///
    /// The key is not lowercased under `KEY_LOWERCASE`, but found ignoring ASCII case. New
    /// containers have the include's priority, except that an array that takes the place of the
    /// key's values has priority 0 and collects later repeats under `NO_IMPLICIT_ARRAYS`, as a
    /// collection does; the key's other values are dropped then. Such containers are not values
    /// created for §12.5, but the one the contents go into counts as the most recent value.
    pub(super) fn open_nest_target(&mut self, target: &NestTarget, at: usize) -> Result<(), Error> {
        if self.frames.len() + 2 > MAX_NESTING {
            return Err(self.error(ErrorKind::NestingTooDeep { limit: MAX_NESTING }, at));
        }
        let key_lowercase = self.settings.flags.contains(ParserFlags::KEY_LOWERCASE);
        if key_lowercase && target.key.bytes().any(|b| b.is_ascii_uppercase()) {
            self.uppercase_keys = true;
        }
        // When the object where the macro stands holds a brace taken over from an included
        // file, the object the contents go into gets a share of it: a `}` in the file uses up
        // that share and closes nothing (spec §9.4, *Nesting under a key*; oracle runs for
        // `target="array"`, `prefix` and section objects, QUESTIONS.md #39). The brace of its
        // own that §9.4 gives a first name ([`Close::NameBrace`]) is shared the same way. §12.5
        // calls it the object's own bracket, and for an object that holds only its own bracket
        // §9.4 leaves the result undefined (the oracle crashes) and lets the implementation
        // choose. Under an object written with braces the key's object gets no share, so a `}`
        // in the file is an error there; this brace is shared all the same, which keeps 0.6.0's
        // result, from when the name's object held a share of the brace taken over
        // (QUESTIONS.md #85).
        let inner_close = match self.top().close {
            Close::IncludedBrace(_) | Close::NameBrace => Close::IncludedBrace(Revert::Open),
            _ => Close::Eof,
        };
        let found = self.find_current_key(&target.key, key_lowercase);
        let object = self
            .current()
            .as_object_mut()
            .expect("macros are read inside objects");
        let empty_object = || Value::Object(Object::new());
        // Where the array frame (if any) and the object frame go.
        // The number of values of an entry whose first value moves into a new array.
        let mut moved_from = None;
        let entry = found.unwrap_or(object.len());
        let (key, element) = match found {
            None => {
                let (value, element) = if target.array {
                    (Value::Array(vec![empty_object()].into()), Some(0))
                } else {
                    (empty_object(), None)
                };
                let key = Str::from(target.key.as_str());
                object.insert_entry(
                    key.clone(),
                    Entry::from_slot(Slot::new(value, target.priority)),
                );
                (key, element)
            }
            Some(index) => {
                let (name, entry) = object.get_index_mut(index).expect("the key was just found");
                let name = name.clone();
                let len = entry.len();
                let element = match (entry.first_mut(), target.array) {
                    (Value::Object(_), false) => None,
                    (_, false) => {
                        let key = name.to_string();
                        return Err(self.error(ErrorKind::IncludeTargetNotObject { key }, at));
                    }
                    (Value::Array(items), true) => {
                        items.push(empty_object());
                        Some(items.len() - 1)
                    }
                    (first, true) => {
                        moved_from = Some(len);
                        let first = std::mem::replace(first, Value::Null);
                        let array = Value::Array(vec![first, empty_object()].into());
                        *entry = Entry::from_slot(Slot::collection(array));
                        Some(1)
                    }
                };
                (name, element)
            }
        };
        let tracked =
            self.notes.is_some() || self.facts.as_ref().is_some_and(OutputFacts::tracks_moves);
        if let Some(len) = moved_from
            && tracked
        {
            // Saved comments and output facts go with the first value into the array, and those
            // of the other values are lost with them (§12.5; oracle runs).
            let object = self.object_places();
            for slot in 1..len {
                self.replaced_values(&object, entry, &key, Some(slot));
            }
            self.collected_values(&object, entry, &key, 1);
        }
        let entry_pos = Pos::Entry {
            entry,
            key: KeyCopy::from(&key),
            slot: 0,
        };
        if (found.is_none() || moved_from.is_some())
            && self.facts.is_some()
            && key_needs_quoting(&key)
            && let Some(node) = self.facts_node_below(std::slice::from_ref(&entry_pos))
        {
            // A key that `.include` creates never needs quoting (spec §10.1, *Quirk*); nor does
            // the key of an array it creates in place of the key's values (oracle runs,
            // QUESTIONS.md #51).
            let facts = self.facts.as_mut().expect("checked above");
            facts.update(node, |f| f.key_quoted = Some(false));
        }
        if self
            .facts
            .as_ref()
            .is_some_and(OutputFacts::records_locations)
        {
            // The containers the macro creates were written where the macro was.
            if found.is_none()
                && let Some(node) = self.facts_node_below(std::slice::from_ref(&entry_pos))
            {
                let facts = self.facts.as_mut().expect("checked above");
                facts.locate(node, at, Some(at));
            }
            if let Some(index) = element
                && let Some(node) = self.facts_node_below(&[entry_pos, Pos::Element(index)])
            {
                let facts = self.facts.as_mut().expect("checked above");
                facts.locate(node, at, None);
            }
        }
        let entry_path = self.placed_path(&key, Placement::Slot(0));
        let entry_step = Step::Entry {
            entry,
            key: KeyCopy::from(&key),
            slot: 0,
        };
        let path = match element {
            None => {
                let home = self.take_out(entry_step, Kind::Object);
                self.push_frame(Kind::Object, inner_close, home, entry_path.clone(), at)?;
                entry_path
            }
            Some(index) => {
                let home = self.take_out(entry_step, Kind::Array);
                self.push_frame(Kind::Array, Close::Eof, home, entry_path, at)?;
                let path = self.element_path(index);
                let home = self.take_out(Step::Element(index), Kind::Object);
                self.push_frame(Kind::Object, inner_close, home, path.clone(), at)?;
                path
            }
        };
        self.made_recent(path);
        Ok(())
    }

    /// Skips whitespace and comments. A `#` that is the last byte of the input is an error when
    /// whitespace comes directly before it (§2.2, *Quirk*); `after_space` tells whether the byte
    /// before the current position is such whitespace. This applies where the first key of an
    /// unbraced root could start, and in the places of QUESTIONS.md #18 and #22.
    fn skip_space_checking_hash(&mut self, mut after_space: bool) -> Result<(), Error> {
        loop {
            match self.peek() {
                Some(b) if is_space(b) => {
                    self.pos += 1;
                    after_space = true;
                }
                Some(b'#') => {
                    self.check_hash_at_end(after_space)?;
                    self.skip_line_comment();
                    after_space = false;
                }
                _ if self.at_block_comment() => {
                    self.skip_block_comment()?;
                    after_space = false;
                }
                _ => return Ok(()),
            }
        }
    }

    /// Looks ahead, consuming nothing, through whitespace and comments for a `#` that is the
    /// last byte of the input with whitespace directly before it: the error of §2.2's *Quirk*,
    /// which holds after a macro as where the first key of the root could start, so also after
    /// comments (`.priority 1⏎# c⏎ #`; oracle runs, C8c). The byte at the current position is not
    /// whitespace. An unterminated block comment is left for the next skip to report.
    pub(super) fn check_hash_at_end_ahead(&self) -> Result<(), Error> {
        let src = self.src;
        let mut at = self.pos;
        let mut after_space = false;
        loop {
            match src.get(at) {
                Some(&b) if is_space(b) => {
                    at += 1;
                    after_space = true;
                }
                Some(b'#') if after_space && at + 1 == src.len() => {
                    return Err(self.error(ErrorKind::HashAtEnd, at));
                }
                Some(b'#') => {
                    // The comment takes its line break (§2.2).
                    at = src[at..]
                        .iter()
                        .position(|&b| b == b'\n')
                        .map_or(src.len(), |n| at + n + 1);
                    after_space = false;
                }
                Some(b'/') if src.get(at + 1) == Some(&b'*') => match block_comment_end(src, at) {
                    Some(end) => {
                        at = end;
                        after_space = false;
                    }
                    None => return Ok(()),
                },
                _ => return Ok(()),
            }
        }
    }

    /// The error of §2.2's *Quirk* for a `#` at the current position.
    fn check_hash_at_end(&self, after_space: bool) -> Result<(), Error> {
        if after_space && self.peek() == Some(b'#') && self.pos + 1 == self.src.len() {
            return Err(self.error(ErrorKind::HashAtEnd, self.pos));
        }
        Ok(())
    }

    /// Parses the next entry of the current object, or closes it.
    fn object_step(&mut self) -> Result<(), Error> {
        if matches!(self.peek(), Some(0x0B | 0x0C)) {
            // After an entry, a VT or FF ends the skipping of [`Core::after_value`]: from there
            // on the place is where the first key of the root would start, so a last-byte `#`
            // is an error unless it directly follows a comment (§2.2, *Quirk: VT and FF after an
            // entry*).
            self.skip_space_checking_hash(true)?;
        } else {
            self.skip_space()?;
        }
        let at = self.pos;
        match self.peek() {
            None if self
                .run_macro_end
                .take()
                .is_some_and(|end| only_comments(self.src, end)) =>
            {
                self.reopen_recent()
            }
            None if self.role == Role::Included => self.end_included_unit(),
            None => self.end_input(),
            Some(b'}') if self.top().close == Close::Brace => {
                let n = self.frames.len();
                if self.role == Role::Included
                    && self.top().unit != self.unit
                    && n >= 2
                    && self.frames[n - 2].kind == Kind::Array
                {
                    // §9.4, *Where the entries go*: uncertain in libucl.
                    self.includes.reached(Uncertain::ClosedArrayElement);
                }
                self.pos += 1;
                self.close_container()
            }
            Some(b'}') if self.top().close == Close::NameBrace => {
                // The brace §9.4 gives the first name of an included file's first key is the
                // object's own: the `}` closes it as the bracket of an object written with
                // braces, so the value created most recently stays what it was (§12.5). Text
                // parsed in place does not keep it open: §13.2 stops only its closing by a
                // bracketed container inside it and by a `}` that removes a brace taken over
                // (oracle runs, QUESTIONS.md #85).
                self.pos += 1;
                self.close_container()
            }
            Some(b'}') if matches!(self.top().close, Close::IncludedBrace(_)) => {
                // The brace came from an included file: it goes, and the object stays open
                // unless it is a section object (§9.4) that has not been kept open by text
                // parsed in place (§13.2, see `close_sections`).
                self.pos += 1;
                if let Some(notes) = &mut self.notes {
                    notes.trailing();
                }
                let frame = self.frames.last_mut().expect("a container is open");
                if frame.close == Close::IncludedBrace(Revert::Section) {
                    frame.close = Close::Section;
                    return self.close_sections();
                }
                frame.close = Close::Eof;
                self.after_value(false)
            }
            Some(c @ (b'}' | b']')) => Err(self.error(
                ErrorKind::UnmatchedClose {
                    found: char::from(c),
                },
                at,
            )),
            Some(b',' | b';') => Err(self.error(ErrorKind::UnexpectedTerminator, at)),
            Some(b'.') => self.macro_entry(),
            Some(_) => {
                let key = self.read_key()?;
                // The unit's first key, while the brace its leading `{` took over is still held.
                let first_shares = std::mem::take(&mut self.first_key_shares)
                    && matches!(self.top().close, Close::IncludedBrace(_));
                if std::mem::take(&mut self.name_run) {
                    self.run_macro_end = None;
                    return self.key_after_name_run(key);
                }
                self.section_shares = first_shares;
                let result = self.after_key(key);
                self.section_shares = false;
                result
            }
        }
    }

    /// The end of a unit, where only whitespace and comments, at least one, followed the last
    /// macro of a name run (§9.1, *Quirk*; that a comment is needed and that any macro of the
    /// run counts are from oracle runs, QUESTIONS.md #34, #35). The value created most recently
    /// is opened once more as an object left open by a section path (§3.4), by this unit:
    ///
    /// - a value that is not part of the result gives an object that is discarded too
    ///   (QUESTIONS.md #36);
    /// - an object inside the current container is reopened where it is;
    /// - the current container itself, or a container below it, stays as it is;
    /// - a value that is not an object is left alone. The spec leaves this undefined (libucl
    ///   crashes, fails or turns the value into an object); this is the project's choice.
    fn reopen_recent(&mut self) -> Result<(), Error> {
        let at = self.pos;
        let Some(top) = self.top_path() else {
            // The current container is discarded, and so is everything that goes into it.
            return Ok(());
        };
        let (home, path) = match self.recent.clone() {
            None => (Home::Detached(Self::empty(Kind::Object)), None),
            Some(recent) => {
                let path = recent.to_vec();
                let Some(rest) = path.strip_prefix(top.as_slice()) else {
                    return Ok(());
                };
                if rest.is_empty() {
                    return Ok(());
                }
                let step = Step::resolve(self.current(), rest);
                let Some(step) = step.filter(|step| {
                    step.try_enter(self.current())
                        .is_some_and(|v| v.is_object())
                }) else {
                    self.includes.reached(Uncertain::ReopenedNotObject);
                    return Ok(());
                };
                (self.take_out(step, Kind::Object), Some(recent))
            }
        };
        self.push_frame(Kind::Object, Close::LeftOpen, home, path, at)
    }

    /// Parses the next element of the current array, or closes it.
    fn array_step(&mut self) -> Result<(), Error> {
        let frame = self.frames.last_mut().expect("a container is open");
        let fresh = std::mem::replace(&mut frame.fresh, false);
        // After an element, a VT or FF stops the skipping of separators and comments, and the
        // next element is read like the first one (§1.5, *Quirk*; QUESTIONS.md #49).
        let after_vt_ff = !fresh && matches!(self.peek(), Some(0x0B | 0x0C));
        if (fresh || after_vt_ff) && self.leading_comments()? {
            // The element starts right after a group of comments that follow each other
            // directly, whitespace included (§1.5, *Quirk*).
            return match self.peek() {
                None if self.role == Role::Included => self.end_included_unit(),
                None => self.end_input(),
                Some(b']') => {
                    self.pos += 1;
                    self.close_container()
                }
                Some(_) => self.element(),
            };
        }
        self.skip_space()?;
        let at = self.pos;
        match self.peek() {
            None if self.role == Role::Included => self.end_included_unit(),
            None => self.end_input(),
            Some(b']') => {
                self.pos += 1;
                self.close_container()
            }
            Some(b'}') => Err(self.error(ErrorKind::UnmatchedClose { found: '}' }, at)),
            Some(b',' | b';') => Err(self.error(ErrorKind::UnexpectedTerminator, at)),
            Some(_) => self.element(),
        }
    }

    /// Skips whitespace, then a group of comments that follow each other directly. Returns
    /// whether there was a comment.
    fn leading_comments(&mut self) -> Result<bool, Error> {
        while self.peek().is_some_and(is_space) {
            self.pos += 1;
        }
        self.comment_group()
    }

    /// Skips comments that follow each other directly, with nothing between them; a line
    /// comment includes its line break. Returns whether there was a comment.
    pub(super) fn comment_group(&mut self) -> Result<bool, Error> {
        let mut comment = false;
        loop {
            match self.peek() {
                Some(b'#') => self.skip_line_comment(),
                _ if self.at_block_comment() => self.skip_block_comment()?,
                _ => return Ok(comment),
            }
            comment = true;
        }
    }

    /// An array element at the current position.
    fn element(&mut self) -> Result<(), Error> {
        match self.peek() {
            Some(b'{') => {
                self.pos += 1;
                self.open_in_array(Kind::Object, Close::Brace, self.pos - 1)
            }
            Some(b'[') => {
                self.pos += 1;
                self.open_in_array(Kind::Array, Close::Bracket, self.pos - 1)
            }
            _ => {
                let at = self.pos;
                let (value, quoted, origin) = self.scalar()?;
                let index = self.push_element(value);
                self.element_facts(index, origin, at);
                let path = self.element_path(index);
                self.created(path);
                self.after_value(quoted)
            }
        }
    }

    /// After a value: a quoted string or heredoc must be followed by whitespace, a comment, a
    /// terminator or a closing bracket (§1.3). Then any mix of spaces, terminators and comments
    /// is skipped.
    fn after_value(&mut self, quoted: bool) -> Result<(), Error> {
        if quoted {
            self.skip_blanks();
            match self.peek() {
                None | Some(b'\n' | b'\r' | 0 | b',' | b';' | b'#' | b'}' | b']') => {}
                _ if self.at_block_comment() => {}
                Some(c) => {
                    return Err(self.error(
                        ErrorKind::MissingDelimiter {
                            found: char::from(c),
                        },
                        self.pos,
                    ));
                }
            }
        }
        let mut after_comment = false;
        loop {
            if self.peek().is_some_and(|b| b == b' ' || b == b'\t') {
                self.skip_blanks();
                after_comment = false;
            }
            match self.peek() {
                Some(b'\n' | b'\r' | 0 | b',' | b';') => {
                    self.pos += 1;
                    after_comment = false;
                }
                Some(b'#') => {
                    self.skip_hash_comment(after_comment);
                    after_comment = true;
                }
                _ if self.at_block_comment() => {
                    self.skip_block_comment()?;
                    after_comment = true;
                }
                _ => return Ok(()),
            }
        }
    }

    // ----- keys and sections (§3) ----------------------------------------------------------

    fn read_key(&mut self) -> Result<Key<'t>, Error> {
        let at = self.pos;
        let lowercase = self.settings.flags.contains(ParserFlags::KEY_LOWERCASE);
        // Whether a quoted key without escapes needs quoting, as its reading saw.
        let mut plain_needs_quoting = false;
        // The key's bytes: decoded, or `None` for those of `src[from..to]` as written.
        let (bytes, (from, to), quoted, escaped) = match self.peek() {
            Some(b'"') => {
                let read = string::double_quoted_scan(self.src, at)?;
                let end = read.end;
                let empty = match &read.bytes {
                    Some(bytes) => bytes.is_empty(),
                    None => end - 1 == at + 1,
                };
                if empty {
                    return Err(self.error(ErrorKind::EmptyKey, at));
                }
                self.pos = end;
                plain_needs_quoting = read.needs_quoting;
                let escaped = read.escaped();
                if lowercase {
                    // §12.1, *Quirk*: the key is lowercased as written, then its escapes are
                    // decoded. A `\U` that becomes `\u` is decoded like an unquoted value's
                    // (§4.8), as the oracle does (QUESTIONS.md #20).
                    let mut raw = self.src[at + 1..end - 1].to_vec();
                    raw.make_ascii_lowercase();
                    let bytes = Some(string::decode_unquoted(&raw).0);
                    (bytes, (0, 0), true, escaped)
                } else {
                    (read.bytes, (at + 1, end - 1), true, escaped)
                }
            }
            Some(b'\'') => return Err(self.error(ErrorKind::SingleQuotedKey, at)),
            Some(b) if is_key_start(b) => {
                while self.peek().is_some_and(is_key_byte) {
                    self.pos += 1;
                }
                (None, (at, self.pos), false, false)
            }
            _ => {
                return Err(self.error(
                    ErrorKind::InvalidKey {
                        found: self.found(at),
                    },
                    at,
                ));
            }
        };
        if !quoted {
            // A bare key ends at a space, a tab, `=` or `:` (§3.1).
            if !matches!(self.peek(), Some(b' ' | b'\t' | b'=' | b':')) {
                let found = self.found(self.pos);
                return Err(match found {
                    None => self.error(ErrorKind::MissingValue, self.pos),
                    Some(_) => self.error(ErrorKind::InvalidKey { found }, self.pos),
                });
            }
        }
        let name = match bytes {
            Some(bytes) => Str::from(self.text(bytes, at)?),
            // A key with an uppercase letter to lowercase does not appear as it is.
            None if lowercase
                && !quoted
                && self.src[from..to].iter().any(u8::is_ascii_uppercase) =>
            {
                let mut name = self.slice_string(from, to, at)?;
                name.make_ascii_lowercase();
                Str::from(name)
            }
            None => self.tree_str(from, to, at)?,
        };
        // Fact 3 of spec §10.1. The bytes of a bare key are none of those that need quoting. A
        // quoted key without escapes is its bytes as written, lowercasing aside, which changes
        // no byte that needs quoting.
        let needs_quoting = match (quoted, escaped) {
            (false, _) => false,
            (true, false) => plain_needs_quoting,
            (true, true) => key_needs_quoting(&name),
        };
        debug_assert_eq!(needs_quoting, quoted && key_needs_quoting(&name));
        debug_assert!(
            quoted || !key_needs_quoting(&name),
            "a bare key never needs quoting"
        );
        debug_assert!(!name.is_empty(), "keys are never empty");
        let quoted = quoted && (escaped || needs_quoting);
        Ok(Key {
            name,
            at,
            quoted,
            needs_quoting,
            origin: None,
            stored: None,
            entry: 0,
        })
    }

    /// Everything after a key: an optional separator, then the value, or section names (§1.2,
    /// §3.4).
    fn after_key(&mut self, key: Key<'t>) -> Result<(), Error> {
        self.skip_inline()?;
        if matches!(self.peek(), Some(b'=' | b':')) {
            self.pos += 1;
            self.skip_inline()?;
            if matches!(self.peek(), Some(b'=' | b':')) {
                return Err(self.error(ErrorKind::DoubleSeparator, self.pos));
            }
            return self.entry_value(key);
        }
        match self.peek() {
            Some(b'{' | b'[') => self.entry_value(key),
            _ if self.line_has_bracket() => self.section_path(key),
            _ => self.entry_value(key),
        }
    }

    /// A key followed by section names: each name gets a nested object, and the last one holds
    /// the object or array that follows (§3.4).
    fn section_path(&mut self, first: Key<'t>) -> Result<(), Error> {
        self.names(first, false, false)
    }

    /// The first key read in a name run (§9.1, *Quirk*). It counts as a word that follows a
    /// name: with a `=` or `:` after it, it is a name too and the names go on; without, it is
    /// tested as any key is.
    fn key_after_name_run(&mut self, word: Key<'t>) -> Result<(), Error> {
        self.skip_inline()?;
        if !matches!(self.peek(), Some(b'=' | b':')) {
            return self.after_key(word);
        }
        let (after_separator, after_break) = self.name_separator()?;
        self.names(word, after_separator, after_break)
    }

    /// A macro has run, and what follows it has been skipped. During a name run it is the run's
    /// last macro so far (§9.1).
    ///
    /// When a file the macro included has closed the braced root of the document, no container
    /// is open. The rest of that file is ignored, as after the root's closing bracket (§1.1), but
    /// here only whitespace and `;` may follow, up to the end of the unit; a comment too is an
    /// error (oracle runs, QUESTIONS.md #47).
    pub(super) fn macro_ran(&mut self) -> Result<(), Error> {
        if self.frames.is_empty() && self.pos < self.src.len() {
            return Err(self.error(ErrorKind::AfterRootClosedByInclude, self.pos));
        }
        if self.name_run {
            self.run_macro_end = Some(self.pos);
        }
        Ok(())
    }

    /// A macro directly after a name (§9.1): it runs inside the name's object, which stays open
    /// as a section object, and starts a name run. The name's object is the value created most
    /// recently.
    fn macro_after_name(&mut self) -> Result<(), Error> {
        self.name_run = true;
        self.recent = self.top_node();
        self.macro_entry()
    }

    /// After a word that follows a name: skips a `=` or `:` there, which makes the word a name
    /// (§3.4, *Quirk*). Returns whether there was one, and whether a line break follows it after
    /// spaces and comments on its line.
    fn name_separator(&mut self) -> Result<(bool, bool), Error> {
        if !matches!(self.peek(), Some(b'=' | b':')) {
            return Ok((false, false));
        }
        self.pos += 1;
        self.skip_inline()?;
        match self.peek() {
            None => Err(self.error(ErrorKind::MissingValue, self.pos)),
            Some(b'\n' | b'\r' | 0x0B | 0x0C) => Ok((true, true)),
            Some(_) => Ok((true, false)),
        }
    }

    /// Section names from `first` on (§3.4). `after_separator`: a `=` or `:` followed `first`;
    /// `after_break`: a line break followed that separator.
    fn names(
        &mut self,
        first: Key<'t>,
        mut after_separator: bool,
        mut after_break: bool,
    ) -> Result<(), Error> {
        let mut name = first;
        let mut opened = 0;
        loop {
            match self.peek() {
                Some(b'{' | b'[') if after_separator => {
                    return Err(self.error(ErrorKind::MissingValue, self.pos));
                }
                Some(b'{' | b'[') => return self.entry_value(name),
                _ => {}
            }
            // The first name of the unit's first key, while the brace that the unit's leading
            // `{` took over is held (§9.4, *Quirk*; QUESTIONS.md #42, #83). When that is the
            // brace of the root, of an object written with braces or of the object `key` or
            // `prefix` creates, the name's object gets a brace of its own. When it is a section
            // object's, the name gets no brace (§9.4): the object shares the brace taken over
            // instead, so that the file's `}` closes the two together and the section object,
            // the outermost, counts as the value created most recently (§12.5).
            let close = if std::mem::take(&mut self.section_shares) {
                match self.top().close {
                    Close::IncludedBrace(Revert::Open) => Close::NameBrace,
                    Close::IncludedBrace(Revert::Section) => Close::IncludedBrace(Revert::Section),
                    // `section_shares` is set only while the top frame holds a brace taken over,
                    // and no frame is pushed before the first name.
                    close => unreachable!("a first name shares a brace taken over, not {close:?}"),
                }
            } else {
                Close::Section
            };
            let at = name.at;
            self.open_in_object(name, Kind::Object, close, at)?;
            opened += 1;
            if after_break {
                // A line break after a separator that follows a name: the next name may come on
                // any later line, and if the input ends first, the objects close quietly
                // (QUESTIONS.md #22).
                self.skip_space_checking_hash(false)?;
                if self.peek().is_none() {
                    // A later input needs a separator first (oracle runs, QUESTIONS.md #59).
                    self.unseparated = true;
                    return Ok(());
                }
            }
            self.skip_before_name()?;
            if matches!(self.peek(), None | Some(b'\n' | b'\r')) {
                // The bracket that made this word a name was in a comment after a VT or FF
                // (§3.4). As after a separator and a line break, the next name may come on any
                // later line, and if the input ends first the objects are kept (oracle runs,
                // QUESTIONS.md #46).
                self.skip_space_checking_hash(false)?;
                if self.peek().is_none() {
                    self.unseparated = true;
                    return Ok(());
                }
            }
            let word = match self.peek() {
                // A key position inside the new object, so `.` starts a macro (§9.1).
                Some(b'.') => return self.macro_after_name(),
                Some(b'"' | b'\'') => self.read_key()?,
                Some(b) if is_key_start(b) => self.read_key()?,
                _ => {
                    let at = self.pos;
                    return Err(match self.found(at) {
                        None => self.error(ErrorKind::MissingValue, at),
                        found => self.error(ErrorKind::InvalidKey { found }, at),
                    });
                }
            };
            self.skip_inline()?;
            // A separator after a word that follows a name is ignored, and the word is a name.
            (after_separator, after_break) = self.name_separator()?;
            if !after_separator
                && !matches!(self.peek(), Some(b'{' | b'['))
                && !self.line_has_bracket()
            {
                // An ordinary key. The section objects have no closing bracket, so they stay
                // open (§3.4, *Quirk*; see `Close::LeftOpen`).
                let n = self.frames.len();
                for frame in &mut self.frames[n - opened..] {
                    if frame.close == Close::Section {
                        frame.close = Close::LeftOpen;
                    }
                }
                return self.entry_value(word);
            }
            name = word;
        }
    }

    // ----- values (§1.6, §4, §6) -----------------------------------------------------------

    /// The value of an entry, at the position after the key, the separator if any, and the
    /// spaces and comments on that line.
    fn entry_value(&mut self, key: Key<'t>) -> Result<(), Error> {
        match self.peek() {
            None => Err(self.error(ErrorKind::MissingValue, self.pos)),
            Some(b'\n' | b'\r' | 0x0B | 0x0C) => self.next_line_value(key),
            Some(_) => self.object_value(key),
        }
    }

    /// A value on a following line (§1.6, *Quirk*). Blank lines and whitespace are skipped, then
    /// one group of comments that follow each other directly. The value starts right after the
    /// group; at the end of input it is `null`.
    ///
    /// At the end of an input given to the parser, the value may still come from a later input
    /// (oracle runs, QUESTIONS.md #59): the key waits for it, and [`Document::finish`] gives it
    /// `null` if none does.
    fn next_line_value(&mut self, mut key: Key<'t>) -> Result<(), Error> {
        if let Some(notes) = &mut self.notes {
            // The group's comments attach after the value is created (§12.5, QUESTIONS.md #17).
            notes.hold();
        }
        self.leading_comments()?;
        if self.peek().is_none() {
            if self.role == Role::Input {
                if key.origin.is_none() {
                    key.origin = Some(position_at(self.src, key.at));
                }
                self.pending = Some(PendingValue {
                    key,
                    settings: self.settings,
                });
                return Ok(());
            }
            return self.null_value(key);
        }
        self.object_value(key)
    }

    /// `null` for `key`, whose value on a following line did not come before the end of input.
    fn null_value(&mut self, mut key: Key<'t>) -> Result<(), Error> {
        if let Some(existing) = self.null_merges_into(&key) {
            // Under `merge`, the `null` that ends the input goes into an object or array that
            // is the entry's first value, adding nothing, instead of taking its place as a
            // scalar does (§8.4; oracle runs, QUESTIONS.md #44).
            let path = self.placed_path(&existing, Placement::Merged);
            self.created(path);
            return Ok(());
        }
        let at = key.at;
        let placement = self.insert(&mut key, Value::Null, Origin::default(), at)?;
        let path = self.placed_key_path(&key, placement);
        self.created(path);
        Ok(())
    }

    /// Under `merge`, the spelling of the entry `key` names when its first value is an object or
    /// an array (keys compared as [`Core::insert`] does).
    fn null_merges_into(&mut self, key: &Key<'t>) -> Option<String> {
        if self.settings.strategy != DuplicateStrategy::Merge {
            return None;
        }
        let ignore_case = self.settings.flags.contains(ParserFlags::KEY_LOWERCASE)
            && (self.uppercase_keys || key.name.bytes().any(|b| b.is_ascii_uppercase()));
        let index = self.find_current_key(&key.name, ignore_case)?;
        let (name, entry) = self.current().as_object()?.get_index(index)?;
        (entry.first().is_object() || entry.first().is_array()).then(|| name.to_string())
    }

    fn object_value(&mut self, mut key: Key<'t>) -> Result<(), Error> {
        match self.peek() {
            Some(b'{') => {
                self.pos += 1;
                self.open_in_object(key, Kind::Object, Close::Brace, self.pos - 1)
            }
            Some(b'[') => {
                self.pos += 1;
                self.open_in_object(key, Kind::Array, Close::Bracket, self.pos - 1)
            }
            _ => {
                let at = self.pos;
                let (value, quoted, origin) = self.scalar()?;
                // A value whose text reaches the end of the input, an unquoted one with any
                // spaces and tabs after it, needs a separator at the start of the next input;
                // a quoted one ends at its closing quote (§13.1, *Quirk*; oracle runs,
                // QUESTIONS.md #59).
                let rest = &self.src[self.pos..];
                let unseparated = if quoted {
                    rest.is_empty()
                } else {
                    rest.iter().all(|&b| matches!(b, b' ' | b'\t'))
                };
                let placement = self.insert(&mut key, value, origin, at)?;
                let path = self.placed_key_path(&key, placement);
                self.created(path);
                self.after_value(quoted)?;
                if unseparated {
                    self.unseparated = true;
                }
                Ok(())
            }
        }
    }

    /// A value that is not a container. Returns it, whether it was quoted (a string in double or
    /// single quotes, or a heredoc), and where a string came from.
    fn scalar(&mut self) -> Result<(Value<'t>, bool, Origin), Error> {
        let at = self.pos;
        match self.peek() {
            Some(b'"') => {
                let read = string::double_quoted_scan(self.src, at)?;
                let end = read.end;
                self.pos = end;
                // Only a `$` as written, or one an escape made, can start a reference.
                let text = match read.bytes {
                    Some(bytes) => {
                        let bytes = self.expander.expand(bytes);
                        Str::from(self.text(bytes, at)?)
                    }
                    None if read.dollar => self.expanded_str(at + 1, end - 1, at)?,
                    None => self.tree_str(at + 1, end - 1, at)?,
                };
                Ok((Value::String(text), true, Origin::default()))
            }
            Some(b'\'') => {
                let (bytes, end) = string::single_quoted_scan(self.src, at)?;
                self.pos = end;
                let origin = Origin {
                    single_quoted: true,
                    ..Origin::default()
                };
                let text = match bytes {
                    Some(bytes) => Str::from(self.text(bytes, at)?),
                    None => self.tree_str(at + 1, end - 1, at)?,
                };
                Ok((Value::String(text), true, origin))
            }
            Some(b'<') if string::heredoc_opener(self.src, at).is_some() => {
                let heredoc = string::heredoc(self.src, at)?;
                self.pos = heredoc.end;
                let (from, to) = (heredoc.content.start, heredoc.content.end);
                let text = if heredoc.expand {
                    self.expanded_str(from, to, at)?
                } else {
                    self.tree_str(from, to, at)?
                };
                let origin = Origin {
                    multiline: true,
                    ..Origin::default()
                };
                Ok((Value::String(text), true, origin))
            }
            Some(b'<') if string::heredoc_opener_cut_by_end(self.src, at) => {
                // `<<` and uppercase letters up to the end of the unit (§6.3, *Quirk*).
                Err(self.error(ErrorKind::UnterminatedHeredoc, at))
            }
            _ => {
                let (value, keyword) = self.unquoted()?;
                let origin = Origin {
                    keyword,
                    ..Origin::default()
                };
                Ok((value, false, origin))
            }
        }
    }

    /// An unquoted value (§4): a number, a keyword or a string. Returns it, and whether it is a
    /// keyword (§4.5).
    fn unquoted(&mut self) -> Result<(Value<'t>, bool), Error> {
        let start = self.pos;
        if matches!(self.peek(), Some(b'0'..=b'9' | b'-')) {
            let no_time = self.settings.flags.contains(ParserFlags::NO_TIME);
            match number::scan(self.src, self.utf8, start, no_time) {
                Number::Value(value, end) => {
                    self.pos = end;
                    return Ok((value, false));
                }
                Number::OutOfRange => return Err(self.error(ErrorKind::NumberOutOfRange, start)),
                Number::Text => {}
            }
        }
        let Extent {
            end,
            backslash,
            dollar,
        } = self.unquoted_extent(start);
        self.pos = end;
        let mut trimmed = end;
        while trimmed > start && matches!(self.src[trimmed - 1], b' ' | b'\t') {
            trimmed -= 1;
        }
        let raw = &self.src[start..trimmed];
        if raw.is_empty() {
            return Err(self.error(ErrorKind::MissingValue, start));
        }
        // Other lengths cannot be a keyword.
        if KEYWORD_LEN.contains(&raw.len())
            && let Some(value) = keyword(raw)
        {
            return Ok((value, true));
        }
        // Variables are expanded only if some `$` is not written as `\$` (§7.6, *Quirk*). Without
        // a backslash there is nothing to decode, and every `$` counts.
        if !backslash {
            // The value as written, unless a variable reference is replaced.
            let text = if dollar {
                self.expanded_str(start, trimmed, start)?
            } else {
                self.tree_str(start, trimmed, start)?
            };
            return Ok((Value::String(text), false));
        }
        let (bytes, expand) = string::decode_unquoted(raw);
        let bytes = if expand {
            self.expander.expand(bytes)
        } else {
            bytes
        };
        Ok((Value::String(Str::from(self.text(bytes, start)?)), false))
    }

    /// The end of the unquoted value starting at `start` (§4.1, §4.2): the first line break,
    /// NUL, `,`, `;`, `#`, `/*`, or `}` or `]` without a match earlier in the value. A backslash
    /// takes the byte after it into the value. Also whether the value has a backslash, and a `$`
    /// that does not follow one, so that the caller need not read it again for them.
    fn unquoted_extent(&self, start: usize) -> Extent {
        let src = self.src;
        let mut i = start;
        let mut braces = 0usize;
        let mut brackets = 0usize;
        let mut backslash = false;
        let mut dollar = false;
        while let Some(&b) = src.get(i) {
            match b {
                b'\n' | b'\r' | 0 | b',' | b';' | b'#' => break,
                b'/' if src.get(i + 1) == Some(&b'*') => break,
                b'{' => braces += 1,
                b'[' => brackets += 1,
                b'}' if braces == 0 => break,
                b']' if brackets == 0 => break,
                b'}' => braces -= 1,
                b']' => brackets -= 1,
                b'\\' => {
                    backslash = true;
                    if i + 1 < src.len() {
                        i += 1;
                    }
                }
                b'$' => dollar = true,
                _ => {}
            }
            i += 1;
        }
        Extent {
            end: i,
            backslash,
            dollar,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The keywords of §4.5, as `keyword` accepts them.
    const KEYWORDS: [&str; 9] = [
        "true", "yes", "on", "false", "no", "off", "null", "nan", "inf",
    ];

    /// [`Core::unquoted`] skips [`keyword`] for a value whose length is outside [`KEYWORD_LEN`],
    /// so a keyword outside it would never match.
    #[test]
    fn keyword_lengths() {
        for word in KEYWORDS {
            assert!(
                keyword(word.as_bytes()).is_some(),
                "{word:?} is not a keyword"
            );
            assert!(
                KEYWORD_LEN.contains(&word.len()),
                "{word:?} is outside KEYWORD_LEN {KEYWORD_LEN:?}"
            );
        }
    }
}
