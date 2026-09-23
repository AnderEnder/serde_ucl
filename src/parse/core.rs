//! Document structure: entries, keys, named sections and containers (spec §1–§4, §8, §11).
//!
//! The parser keeps an explicit stack of the containers that are open, so nesting depth never
//! grows the call stack. Containers are filled in place: an object or array is inserted into its
//! parent (by the duplicate rules of §8) when its opening bracket is read, and later entries go
//! straight into it. Each stack frame records how to reach its container from the one below.
//!
//! Input units (§9.4): an included file is parsed by a [`Core`] of its own that takes over the
//! container stack, the tree and the saved comments of the unit that includes it, and hands them
//! back when the file ends. So an included file adds entries where the macro stands, may close
//! containers the including unit opened, and leaves section objects open for it (§9.4, *Where
//! the entries go*).

use super::comments::{Notes, ValuePath};
use super::error::position_at;
use super::facts::OutputFacts;
use super::include::Includes;
use super::macros::find_key;
use super::number::{self, Number};
use super::string;
use super::vars::Expander;
use super::{AttachedComments, Comment, Error, ErrorKind, MAX_NESTING, PathSegment};
use crate::emit::key_needs_quoting;
use crate::value::{
    DuplicateKeyError, DuplicateStrategy, Entry, ParserFlags, Placement, Slot, UclObject, UclValue,
};
use std::path::Path;

/// The settings of one input unit.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Settings {
    pub(crate) flags: ParserFlags,
    pub(crate) priority: u8,
    pub(crate) strategy: DuplicateStrategy,
}

/// Where saved comments go (spec §12.5).
pub(crate) struct CommentSink<'c> {
    pub(crate) comments: &'c mut Vec<Comment>,
    pub(crate) attached: &'c mut Vec<AttachedComments>,
}

/// Parses a whole document into its root value.
///
/// A silent stop (§9.4) is an [`ErrorKind::Stopped`] error that carries the root as parsed so
/// far; the saved comments and their attachments are kept then too.
///
/// When `facts` is given, the output facts of the result (spec §10.1) are recorded there, also
/// for a silent stop.
pub(crate) fn parse_document(
    input: &[u8],
    settings: Settings,
    expander: &mut Expander<'_>,
    includes: &mut Includes<'_>,
    sink: Option<CommentSink<'_>>,
    facts: Option<&mut OutputFacts>,
) -> Result<UclValue, Error> {
    parse_unit(input, settings, expander, includes, sink, facts, 0)
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
    parse_unit(input, settings, expander, includes, None, None, depth)
}

fn parse_unit(
    input: &[u8],
    settings: Settings,
    expander: &mut Expander<'_>,
    includes: &mut Includes<'_>,
    sink: Option<CommentSink<'_>>,
    facts_sink: Option<&mut OutputFacts>,
    depth: usize,
) -> Result<UclValue, Error> {
    let mut core = Core {
        src: input,
        pos: 0,
        settings,
        expander,
        includes,
        notes: sink.as_ref().map(|_| Notes::new()),
        facts: facts_sink.as_ref().map(|_| OutputFacts::new()),
        uppercase_keys: false,
        root: UclValue::Null,
        frames: Vec::new(),
        depth,
        unit: 0,
        unit_done: false,
        name_run: false,
        run_macro_end: None,
        outer_run: false,
        recent: None,
        first_key_shares: false,
        section_shares: false,
    };
    let result = core.run();
    let kept = result.as_ref().is_ok() || result.as_ref().is_err_and(Error::is_stopped);
    if let (Some(sink), Some(facts)) = (facts_sink, core.facts.take())
        && kept
    {
        *sink = facts;
    }
    if let (Some(sink), Some(notes)) = (sink, core.notes.take()) {
        let (comments, groups) = notes.finish(input);
        *sink.comments = comments;
        if result.as_ref().is_ok() || result.as_ref().is_err_and(Error::is_stopped) {
            *sink.attached = groups;
        }
    }
    match result {
        Ok(()) => Ok(core.root),
        Err(e) if e.is_stopped() => Err(e.with_partial(core.root)),
        Err(e) => Err(e),
    }
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
    /// objects around it, as when a bracketed container opened in it closes.
    Section,
}

impl Close {
    /// Whether a bracket is still open for the container.
    fn has_bracket(self) -> bool {
        matches!(
            self,
            Close::Brace | Close::Bracket | Close::IncludedBrace(_)
        )
    }

    fn is_section(self) -> bool {
        matches!(self, Close::Section | Close::LeftOpen)
    }

    /// Whether the container closes when a bracketed container opened in it closes (§3.4): a
    /// section object, also one whose brace an included file took over, which loses that brace
    /// then (oracle runs, QUESTIONS.md #40).
    fn closes_with_inner(self) -> bool {
        self.is_section() || self == Close::IncludedBrace(Revert::Section)
    }
}

/// How to reach a frame's container from the container of the frame below.
#[derive(Debug)]
enum Step {
    /// Value `slot` of entry `key`.
    Entry { key: String, slot: usize },
    /// Element `index` of the explicit array that is the first value of entry `key`: a repeat
    /// collected under `NO_IMPLICIT_ARRAYS` (§8.5).
    Collected { key: String, index: usize },
    /// Element `index` of an array.
    Element(usize),
    /// These steps in turn: a value reopened below the container of the frame below (§9.1,
    /// [`Core::reopen_recent`]).
    Path(Vec<Step>),
}

impl Step {
    fn segments(&self) -> Vec<PathSegment> {
        match self {
            Step::Entry { key, slot } => vec![PathSegment::Key {
                key: key.clone(),
                index: *slot,
            }],
            Step::Collected { key, index } => vec![
                PathSegment::Key {
                    key: key.clone(),
                    index: 0,
                },
                PathSegment::Index(*index),
            ],
            Step::Element(index) => vec![PathSegment::Index(*index)],
            Step::Path(steps) => steps.iter().flat_map(Step::segments).collect(),
        }
    }

    /// The step that follows `segment` of a value path.
    fn from_segment(segment: &PathSegment) -> Step {
        match segment {
            PathSegment::Key { key, index } => Step::Entry {
                key: key.clone(),
                slot: *index,
            },
            PathSegment::Index(index) => Step::Element(*index),
        }
    }

    fn try_enter<'v>(&self, value: &'v mut UclValue) -> Option<&'v mut UclValue> {
        match self {
            Step::Entry { key, slot } => value
                .as_object_mut()
                .and_then(|o| o.entry_mut(key))
                .and_then(|e| e.value_at_mut(*slot)),
            Step::Collected { key, index } => value
                .as_object_mut()
                .and_then(|o| o.entry_mut(key))
                .and_then(|e| e.value_at_mut(0))
                .and_then(UclValue::as_array_mut)
                .and_then(|a| a.get_mut(*index)),
            Step::Element(index) => value.as_array_mut().and_then(|a| a.get_mut(*index)),
            Step::Path(steps) => steps
                .iter()
                .try_fold(value, |value, step| step.try_enter(value)),
        }
    }

    fn enter<'v>(&self, value: &'v mut UclValue) -> &'v mut UclValue {
        self.try_enter(value)
            .expect("an open container stays where it was inserted")
    }
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
enum Home {
    /// It is the root value.
    Root,
    /// It is inside the container of the frame below.
    Attached(Step),
    /// It is parsed and then discarded, because a value with a higher priority exists (§8.3).
    Detached(UclValue),
}

#[derive(Debug)]
struct Frame {
    kind: Kind,
    close: Close,
    home: Home,
    /// No element or entry has been read into the container yet.
    fresh: bool,
    /// The container's path from the root, kept only when comments are saved; `None` also for a
    /// detached container.
    path: Option<ValuePath>,
    /// The input unit that opened the container: 0 for the main document, `n` for a file
    /// included `n` levels deep (§9.4).
    unit: usize,
}

/// The container of the top frame.
fn resolve<'v>(root: &'v mut UclValue, frames: &'v mut [Frame]) -> &'v mut UclValue {
    let base = frames
        .iter()
        .rposition(|f| !matches!(f.home, Home::Attached(_)))
        .expect("the root frame is at the bottom");
    let (below, above) = frames.split_at_mut(base + 1);
    let mut current = match &mut below[base].home {
        Home::Root => root,
        Home::Detached(value) => value,
        Home::Attached(_) => unreachable!(),
    };
    for frame in above.iter() {
        let Home::Attached(step) = &frame.home else {
            unreachable!("only the base frame is not attached")
        };
        current = step.enter(current);
    }
    current
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
struct Key {
    name: String,
    at: usize,
    /// The key was written in double quotes and contains a backslash escape or a byte that makes
    /// the output formats quote it (spec §10.1, fact 3).
    quoted: bool,
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

/// The keywords of §4.5, compared with the whole unquoted value.
fn keyword(raw: &[u8]) -> Option<UclValue> {
    let is = |word: &str| raw.eq_ignore_ascii_case(word.as_bytes());
    if is("true") || is("yes") || is("on") {
        Some(UclValue::Boolean(true))
    } else if is("false") || is("no") || is("off") {
        Some(UclValue::Boolean(false))
    } else {
        match raw {
            b"null" => Some(UclValue::Null),
            b"nan" => Some(UclValue::Float(f64::NAN)),
            b"inf" => Some(UclValue::Float(f64::INFINITY)),
            _ => None,
        }
    }
}

/// The parser state of one input unit. Macros are read and run in `super::macros`, and the
/// include macros and `.load` in `super::include`.
pub(super) struct Core<'s, 'e, 'v, 'l> {
    pub(super) src: &'s [u8],
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
    pub(super) root: UclValue,
    frames: Vec<Frame>,
    /// How many macro argument documents this document is inside (§9.2): 0 for the main one.
    pub(super) depth: usize,
    /// This input unit: 0 for the main document, `n` for a file included `n` levels deep.
    unit: usize,
    /// The input of this included unit has ended.
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
    recent: Option<ValuePath>,
    /// This included unit's leading `{` took over a brace (§9.4), and no key has been read yet.
    first_key_shares: bool,
    /// The key being read is that first key: if it starts a section path, the object of its
    /// first name gets a share of the taken-over brace.
    section_shares: bool,
}

/// The container an included file's contents go into when it is nested under a key (§9.4,
/// *Nesting under a key*).
#[derive(Debug, Clone)]
pub(super) struct NestTarget {
    pub(super) key: String,
    pub(super) array: bool,
    pub(super) priority: u8,
}

impl Core<'_, '_, '_, '_> {
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

    pub(super) fn error(&self, kind: ErrorKind, at: usize) -> Error {
        Error::new(kind, position_at(self.src, at))
    }

    fn found(&self, at: usize) -> Option<char> {
        self.src.get(at).map(|&b| char::from(b))
    }

    fn text(&self, bytes: Vec<u8>, at: usize) -> Result<String, Error> {
        String::from_utf8(bytes).map_err(|_| self.error(ErrorKind::InvalidUtf8, at))
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
    /// anywhere (§3.4).
    fn line_has_bracket(&self) -> bool {
        self.src[self.pos..]
            .iter()
            .take_while(|&&b| !matches!(b, b'\n' | b'\r' | b',' | b';'))
            .any(|&b| b == b'{' || b == b'[')
    }

    // ----- containers ----------------------------------------------------------------------

    fn top(&self) -> &Frame {
        self.frames.last().expect("a container is open")
    }

    /// The container of the top frame.
    pub(super) fn current(&mut self) -> &mut UclValue {
        resolve(&mut self.root, &mut self.frames)
    }

    fn push_frame(
        &mut self,
        kind: Kind,
        close: Close,
        home: Home,
        path: Option<ValuePath>,
        at: usize,
    ) -> Result<(), Error> {
        if self.frames.len() >= MAX_NESTING {
            return Err(self.error(ErrorKind::NestingTooDeep { limit: MAX_NESTING }, at));
        }
        self.frames.push(Frame {
            kind,
            close,
            home,
            fresh: true,
            path,
            unit: self.unit,
        });
        Ok(())
    }

    fn empty(kind: Kind) -> UclValue {
        match kind {
            Kind::Object => UclValue::Object(UclObject::new()),
            Kind::Array => UclValue::Array(Vec::new()),
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

    /// The path of the top container from the root, or `None` when it is not part of the
    /// result. Frames record their paths only when comments are saved; otherwise the path is
    /// built from the frames' steps.
    pub(super) fn top_path(&self) -> Option<ValuePath> {
        if self.notes.is_some() {
            return self.top().path.clone();
        }
        let mut path = Vec::new();
        for frame in &self.frames {
            match &frame.home {
                Home::Root => {}
                Home::Detached(_) => return None,
                Home::Attached(step) => path.extend(step.segments()),
            }
        }
        Some(path)
    }

    /// The path of a value just placed in the current object under `key`, when values need
    /// paths and the value is part of the result.
    fn placed_path(&self, key: &str, placement: Placement) -> Option<ValuePath> {
        if !self.wants_paths() {
            return None;
        }
        let step = match placement {
            Placement::Slot(slot) => Step::Entry {
                key: key.to_owned(),
                slot,
            },
            Placement::Collected(index) => Step::Collected {
                key: key.to_owned(),
                index,
            },
            Placement::Merged => Step::Entry {
                key: key.to_owned(),
                slot: 0,
            },
            Placement::Dropped => return None,
        };
        let mut path = self.top_path()?;
        path.extend(step.segments());
        Some(path)
    }

    /// A value was created at `path`: pending comments attach to it (§12.5), and it is the
    /// value created most recently.
    fn created(&mut self, path: Option<ValuePath>) {
        if self.tracking() {
            self.recent.clone_from(&path);
        }
        if let Some(notes) = &mut self.notes {
            notes.created(path);
        }
    }

    /// Makes `path` the value created most recently, without attaching comments.
    fn made_recent(&mut self, path: Option<ValuePath>) {
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
    /// for this value.
    fn insert(
        &mut self,
        key: &mut Key,
        value: UclValue,
        origin: Origin,
    ) -> Result<Placement, Error> {
        let written = self.match_key_case(key);
        let Settings {
            mut flags,
            priority,
            strategy,
        } = self.settings;
        // `read_key` has already lowercased the key where §12.1 asks for it.
        flags.remove(ParserFlags::KEY_LOWERCASE);
        let track = self.notes.is_some() || self.facts.as_ref().is_some_and(|f| !f.is_empty());
        let object = self
            .current()
            .as_object_mut()
            .expect("entries are parsed inside objects");
        let is_container = |v: &UclValue| v.is_object() || v.is_array();
        // Under `merge`, a scalar that follows a container takes its place in the entry (§8.4,
        // *Quirk*); the oracle keeps the container's comments on it, as for one value.
        let replaces_in_place = |e: &Entry| {
            strategy == DuplicateStrategy::Merge && is_container(e.first()) && !is_container(&value)
        };
        let (existed, in_place) = object
            .entry(&key.name)
            .map_or((false, false), |e| (true, replaces_in_place(e)));
        // A number written with digits, a time or a boolean that takes the place of a
        // non-empty container keeps the normal layout of a multi-value entry; the keywords `nan`
        // and `inf` follow their own kind, as strings and `null` do (spec §10.7, *Quirk*).
        let counts_as_container = match value {
            UclValue::Integer(_) | UclValue::Time(_) | UclValue::Boolean(_) => true,
            UclValue::Float(_) => !origin.keyword,
            _ => false,
        };
        let normal_layout = in_place
            && counts_as_container
            && object.entry(&key.name).is_some_and(|e| match e.first() {
                UclValue::Object(o) => !o.is_empty(),
                UclValue::Array(a) => !a.is_empty(),
                _ => false,
            });
        let was_collected = object
            .entry(&key.name)
            .is_some_and(|e| e.slots()[0].is_collected());
        let before = track
            .then(|| {
                object.entry(&key.name).map(|e| Before {
                    len: e.len(),
                    collected: e.slots()[0].is_collected(),
                    in_place,
                })
            })
            .flatten();
        let result = object.insert_slot_placed(
            key.name.as_str(),
            Slot::new(value, priority),
            strategy,
            flags,
        );
        let after = if track {
            object.entry(&key.name).map_or(0, Entry::len)
        } else {
            0
        };
        let placement = result.map_err(|DuplicateKeyError { key: name }| {
            self.error(ErrorKind::DuplicateKey { key: name }, key.at)
        })?;
        if track {
            self.values_moved(&key.name, before, placement, after);
        }
        if matches!(placement, Placement::Collected(_)) && !was_collected {
            self.collection_key(&key.name);
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
            self.record_facts(key, written.as_deref(), placement, facts);
        }
        Ok(placement)
    }

    /// Records the output facts of a value just placed under `key` (spec §10.1). `written` is
    /// the key as written for the value when it differs from `key`, the entry's key.
    fn record_facts(
        &mut self,
        key: &Key,
        written: Option<&str>,
        placement: Placement,
        placed: PlacedFacts,
    ) {
        let spelling = written.unwrap_or(&key.name);
        let key_spelling = (spelling != key.name).then(|| spelling.to_owned());
        let key_quoted = (key.quoted != key_needs_quoting(spelling)).then_some(key.quoted);
        let (step, keyed) = match placement {
            // A scalar that took a container's place under `merge` keeps the container's key
            // (oracle runs, QUESTIONS.md #50).
            Placement::Slot(slot) => (
                Step::Entry {
                    key: key.name.clone(),
                    slot,
                },
                !placed.in_place,
            ),
            Placement::Collected(index) => (
                Step::Collected {
                    key: key.name.clone(),
                    index,
                },
                false,
            ),
            Placement::Merged | Placement::Dropped => return,
        };
        let has_value_facts = placed.origin.has_string_facts() || placed.normal_layout;
        let has_key_facts = keyed && (key_spelling.is_some() || key_quoted.is_some());
        let stale = placed.in_place && self.facts.as_ref().is_some_and(|f| !f.is_empty());
        if !has_value_facts && !has_key_facts && !stale {
            return;
        }
        let Some(mut path) = self.top_path() else {
            return;
        };
        path.extend(step.segments());
        let facts = self.facts.as_mut().expect("facts are recorded");
        if placed.in_place {
            facts.clear_below(&path);
        }
        facts.update(&path, |f| {
            f.single_quoted = placed.origin.single_quoted;
            f.multiline = placed.origin.multiline;
            f.normal_layout = placed.normal_layout;
            if keyed {
                f.key_spelling = key_spelling;
                f.key_quoted = key_quoted;
            }
        });
    }

    /// Entry `key` of the current object has just become a `NO_IMPLICIT_ARRAYS` collection
    /// (§8.5). The array's key never needs quoting, whatever the keys of its values were, and is
    /// spelled as the entry's key, that of its first value (spec §10.1, *Quirk*).
    fn collection_key(&mut self, key: &str) {
        if self.facts.is_none() || !key_needs_quoting(key) {
            return;
        }
        let Some(mut path) = self.top_path() else {
            return;
        };
        path.push(PathSegment::Key {
            key: key.to_owned(),
            index: 0,
        });
        let facts = self.facts.as_mut().expect("checked above");
        facts.update(&path, |f| f.key_quoted = Some(false));
    }

    /// Values of entry `key` of the object at `object` were replaced: value `slot`, or all.
    fn replaced_values(&mut self, object: &[PathSegment], key: &str, slot: Option<usize>) {
        if let Some(notes) = &mut self.notes {
            notes.replaced(object, key, slot);
        }
        if let Some(facts) = &mut self.facts {
            facts.replaced(object, key, slot);
        }
    }

    /// Entry `key` of the object at `object` became a `NO_IMPLICIT_ARRAYS` collection of its
    /// first `count` values.
    fn collected_values(&mut self, object: &[PathSegment], key: &str, count: usize) {
        if let Some(notes) = &mut self.notes {
            notes.collected(object, key, count);
        }
        if let Some(facts) = &mut self.facts {
            facts.collected(object, key, count);
        }
    }

    /// Under `KEY_LOWERCASE`, keys are compared without regard to ASCII case, and an entry keeps
    /// the spelling of its first key. Keys are lowercased as they are read, so only escapes in
    /// quoted keys can leave uppercase letters (§12.1); until one does, exact comparison is
    /// enough.
    ///
    /// Returns the spelling of `key` as written when it was changed.
    fn match_key_case(&mut self, key: &mut Key) -> Option<String> {
        if !self.settings.flags.contains(ParserFlags::KEY_LOWERCASE) {
            return None;
        }
        if key.name.bytes().any(|b| b.is_ascii_uppercase()) {
            self.uppercase_keys = true;
        }
        if !self.uppercase_keys {
            return None;
        }
        let object = self
            .current()
            .as_object()
            .expect("entries are parsed inside objects");
        if object.contains_key(&key.name) {
            return None;
        }
        let existing = object
            .keys()
            .find(|k| k.eq_ignore_ascii_case(&key.name))?
            .clone();
        Some(std::mem::replace(&mut key.name, existing))
    }

    /// Keeps saved comments and output facts with their values when an insertion under `key`
    /// replaced or moved values that were already there (QUESTIONS.md #17). `after` is the
    /// entry's length now.
    fn values_moved(
        &mut self,
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
        let Some(object) = self.top_path() else {
            return;
        };
        match placement {
            Placement::Slot(slot) if slot == len && after == len + 1 => {}
            Placement::Slot(_) if in_place => {}
            Placement::Slot(_) if after == 1 => self.replaced_values(&object, key, None),
            Placement::Slot(slot) => self.replaced_values(&object, key, Some(slot)),
            Placement::Collected(_) if !collected => {
                // Only the first value goes into the collection (QUESTIONS.md #25).
                for slot in 1..len {
                    self.replaced_values(&object, key, Some(slot));
                }
                self.collected_values(&object, key, 1);
            }
            Placement::Collected(_) | Placement::Merged | Placement::Dropped => {}
        }
    }

    /// Opens a new object or array under `key` in the current object.
    fn open_in_object(&mut self, mut key: Key, kind: Kind, close: Close) -> Result<(), Error> {
        if self.frames.len() >= MAX_NESTING {
            return Err(self.error(ErrorKind::NestingTooDeep { limit: MAX_NESTING }, self.pos));
        }
        let placement = self.insert(&mut key, Self::empty(kind), Origin::default())?;
        let path = self.placed_path(&key.name, placement);
        let home = match placement {
            Placement::Slot(slot) => Home::Attached(Step::Entry {
                key: key.name,
                slot,
            }),
            Placement::Collected(index) => Home::Attached(Step::Collected {
                key: key.name,
                index,
            }),
            // Merged into the entry's first value, a container of the same kind (§8.4).
            Placement::Merged => Home::Attached(Step::Entry {
                key: key.name,
                slot: 0,
            }),
            Placement::Dropped => Home::Detached(Self::empty(kind)),
        };
        self.created(path.clone());
        self.push_frame(kind, close, home, path, key.at)
    }

    /// Opens a new object or array as the next element of the current array.
    fn open_in_array(&mut self, kind: Kind, close: Close) -> Result<(), Error> {
        let at = self.pos;
        if self.frames.len() >= MAX_NESTING {
            return Err(self.error(ErrorKind::NestingTooDeep { limit: MAX_NESTING }, at));
        }
        let index = self.push_element(Self::empty(kind));
        let path = self.element_path(index);
        self.created(path.clone());
        self.push_frame(kind, close, Home::Attached(Step::Element(index)), path, at)
    }

    /// Appends `value` to the current array and returns its index.
    fn push_element(&mut self, value: UclValue) -> usize {
        let array = self
            .current()
            .as_array_mut()
            .expect("elements are parsed inside arrays");
        array.push(value);
        array.len() - 1
    }

    fn element_path(&self, index: usize) -> Option<ValuePath> {
        if !self.wants_paths() {
            return None;
        }
        let mut path = self.top_path()?;
        path.push(PathSegment::Index(index));
        Some(path)
    }

    /// Closes the top container after its bracket, with the section objects around it (§3.4).
    fn close_container(&mut self) -> Result<(), Error> {
        if let Some(notes) = &mut self.notes {
            notes.trailing();
        }
        self.frames.pop();
        self.close_sections()
    }

    /// After a bracketed container has closed: the section objects around it close too (§3.4).
    /// Objects left open by a section path close with the first bracketed container that closes
    /// in them, back to the nearest object written with a bracket, or the root. Section objects
    /// of both kinds close together in any order, as the oracle does when an included file
    /// leaves one inside another, and so do those whose brace an included file took over
    /// ([`Close::closes_with_inner`]).
    fn close_sections(&mut self) -> Result<(), Error> {
        let mut outermost = None;
        while self
            .frames
            .last()
            .is_some_and(|f| f.close.closes_with_inner())
        {
            let path = if self.wants_paths() {
                self.top_path()
            } else {
                None
            };
            self.frames.pop();
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
        if close != Close::Eof {
            self.pos += 1;
        }
        self.root = Self::empty(kind);
        let path = self.notes.as_ref().map(|_| Vec::new());
        self.push_frame(kind, close, Home::Root, path, self.pos)?;
        if close == Close::Eof {
            // Where the first key of an unbraced root could start (§2.2, *Quirk*).
            self.skip_space_checking_hash(after_space)?;
        }
        while let Some(frame) = self.frames.last() {
            match frame.kind {
                Kind::Object => self.object_step()?,
                Kind::Array => self.array_step()?,
            }
        }
        Ok(())
    }

    // ----- included units (§9.4) ------------------------------------------------------------

    /// Parses `input`, an included file whose canonical path is `file`, as a new input unit with
    /// `settings`. Its entries go into the current container, and it continues with the
    /// container stack the file leaves behind.
    pub(super) fn parse_included(
        &mut self,
        input: &[u8],
        settings: Settings,
        file: &Path,
    ) -> Result<(), Error> {
        let cursor = self.notes.as_mut().map(|n| n.suspend(self.src));
        let outer_run = self.tracking();
        let mut inner = Core {
            src: input,
            pos: 0,
            settings,
            expander: &mut *self.expander,
            includes: &mut *self.includes,
            notes: self.notes.take(),
            facts: self.facts.take(),
            uppercase_keys: self.uppercase_keys,
            root: std::mem::replace(&mut self.root, UclValue::Null),
            frames: std::mem::take(&mut self.frames),
            depth: self.depth,
            unit: self.unit + 1,
            unit_done: false,
            name_run: false,
            run_macro_end: None,
            outer_run,
            recent: self.recent.take(),
            first_key_shares: false,
            section_shares: false,
        };
        let result = inner.run_included();
        self.root = inner.root;
        self.frames = inner.frames;
        self.uppercase_keys = inner.uppercase_keys;
        self.notes = inner.notes;
        self.facts = inner.facts;
        self.recent = inner.recent;
        if let (Some(notes), Some(cursor)) = (&mut self.notes, cursor) {
            notes.resume(input, cursor);
        }
        result.map_err(|e| e.in_file(file))
    }

    /// The start of an included unit follows §1.1, except that a `[` there is an error and a
    /// `{` takes over the brace of the object the entries go into (§9.4).
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
            Some(b'[') => return Err(self.error(ErrorKind::IncludeArrayRoot, self.pos)),
            Some(b'{') => {
                self.pos += 1;
                let frame = self.frames.last_mut().expect("a container is open");
                frame.close = Close::IncludedBrace(match frame.close {
                    Close::IncludedBrace(revert) => revert,
                    close if close.is_section() => Revert::Section,
                    _ => Revert::Open,
                });
                self.first_key_shares = true;
            }
            _ => self.skip_space_checking_hash(after_space)?,
        }
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

    /// The input of the main document has ended: the check at the end of a unit, then every
    /// container closes. Pending comments attach to the value created most recently (§12.5).
    fn end_document(&mut self) -> Result<(), Error> {
        self.unit_end_check()?;
        if let Some(notes) = &mut self.notes {
            notes.trailing();
        }
        self.frames.clear();
        Ok(())
    }

    /// The check at the end of a unit (§9.4): the open containers, from the innermost outward,
    /// up to the first that another unit opened. One that holds a bracket, its own or a taken-over
    /// one, is not closed, and that is an error. Containers below one that another unit opened
    /// are not checked, whatever that container is (oracle runs, QUESTIONS.md #41).
    fn unit_end_check(&self) -> Result<(), Error> {
        for frame in self.frames.iter().rev() {
            if frame.unit != self.unit {
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

    /// The number of open containers.
    pub(super) fn open_containers(&self) -> usize {
        self.frames.len()
    }

    /// Closes the containers above the first `len`, without any check: the end of a file
    /// nested under a key (§9.4).
    pub(super) fn close_containers_above(&mut self, len: usize) {
        self.frames.truncate(len);
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
        // `target="array"`, `prefix` and section objects, QUESTIONS.md #39).
        let inner_close = match self.top().close {
            Close::IncludedBrace(_) => Close::IncludedBrace(Revert::Open),
            _ => Close::Eof,
        };
        let object = self
            .current()
            .as_object_mut()
            .expect("macros are read inside objects");
        let found = object.index_of(&target.key).or_else(|| {
            key_lowercase
                .then(|| {
                    object
                        .keys()
                        .position(|k| k.eq_ignore_ascii_case(&target.key))
                })
                .flatten()
        });
        let empty_object = || UclValue::Object(UclObject::new());
        // Where the array frame (if any) and the object frame go.
        // The number of values of an entry whose first value moves into a new array.
        let mut moved_from = None;
        let (key, element) = match found {
            None => {
                let (value, element) = if target.array {
                    (UclValue::Array(vec![empty_object()]), Some(0))
                } else {
                    (empty_object(), None)
                };
                object.insert_entry(
                    target.key.clone(),
                    Entry::from_slot(Slot::new(value, target.priority)),
                );
                (target.key.clone(), element)
            }
            Some(index) => {
                let (name, entry) = object.get_index_mut(index).expect("the key was just found");
                let name = name.clone();
                let len = entry.len();
                let element = match (entry.first_mut(), target.array) {
                    (UclValue::Object(_), false) => None,
                    (_, false) => {
                        return Err(self.error(ErrorKind::IncludeTargetNotObject { key: name }, at));
                    }
                    (UclValue::Array(items), true) => {
                        items.push(empty_object());
                        Some(items.len() - 1)
                    }
                    (first, true) => {
                        moved_from = Some(len);
                        let first = std::mem::replace(first, UclValue::Null);
                        let array = UclValue::Array(vec![first, empty_object()]);
                        *entry = Entry::from_slot(Slot::collection(array));
                        Some(1)
                    }
                };
                (name, element)
            }
        };
        let tracked = self.notes.is_some() || self.facts.as_ref().is_some_and(|f| !f.is_empty());
        if let Some(len) = moved_from
            && tracked
            && let Some(object_path) = self.top_path()
        {
            // Saved comments and output facts go with the first value into the array, and those
            // of the other values are lost with them (§12.5; oracle runs).
            for slot in 1..len {
                self.replaced_values(&object_path, &key, Some(slot));
            }
            self.collected_values(&object_path, &key, 1);
        }
        if (found.is_none() || moved_from.is_some())
            && self.facts.is_some()
            && key_needs_quoting(&key)
            && let Some(mut path) = self.top_path()
        {
            // A key that `.include` creates never needs quoting (spec §10.1, *Quirk*); nor does
            // the key of an array it creates in place of the key's values (oracle runs,
            // QUESTIONS.md #51).
            path.push(PathSegment::Key {
                key: key.clone(),
                index: 0,
            });
            let facts = self.facts.as_mut().expect("checked above");
            facts.update(&path, |f| f.key_quoted = Some(false));
        }
        let entry_path = self.placed_path(&key, Placement::Slot(0));
        let entry_step = Step::Entry { key, slot: 0 };
        let path = match element {
            None => {
                self.push_frame(
                    Kind::Object,
                    inner_close,
                    Home::Attached(entry_step),
                    entry_path.clone(),
                    at,
                )?;
                entry_path
            }
            Some(index) => {
                self.push_frame(
                    Kind::Array,
                    Close::Eof,
                    Home::Attached(entry_step),
                    entry_path,
                    at,
                )?;
                let path = self.element_path(index);
                self.push_frame(
                    Kind::Object,
                    inner_close,
                    Home::Attached(Step::Element(index)),
                    path.clone(),
                    at,
                )?;
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

    /// The error of §2.2's *Quirk* for a `#` at the current position.
    fn check_hash_at_end(&self, after_space: bool) -> Result<(), Error> {
        if after_space && self.peek() == Some(b'#') && self.pos + 1 == self.src.len() {
            return Err(self.error(ErrorKind::HashAtEnd, self.pos));
        }
        Ok(())
    }

    /// Parses the next entry of the current object, or closes it.
    fn object_step(&mut self) -> Result<(), Error> {
        self.skip_space()?;
        let at = self.pos;
        match self.peek() {
            None if self
                .run_macro_end
                .take()
                .is_some_and(|end| only_comments(self.src, end)) =>
            {
                self.reopen_recent()
            }
            None if self.unit > 0 => self.end_included_unit(),
            None => self.end_document(),
            Some(b'}') if self.top().close == Close::Brace => {
                self.pos += 1;
                self.close_container()
            }
            Some(b'}') if matches!(self.top().close, Close::IncludedBrace(_)) => {
                // The brace came from an included file: it goes, and the object stays open
                // unless it is a section object (§9.4).
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
            Some(path) => {
                let Some(rest) = path.strip_prefix(top.as_slice()) else {
                    return Ok(());
                };
                if rest.is_empty() {
                    return Ok(());
                }
                let step = Step::Path(rest.iter().map(Step::from_segment).collect());
                if !step
                    .try_enter(self.current())
                    .is_some_and(|v| v.is_object())
                {
                    return Ok(());
                }
                let path = self.notes.is_some().then_some(path);
                (Home::Attached(step), path)
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
                None if self.unit > 0 => self.end_included_unit(),
                None => self.end_document(),
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
            None if self.unit > 0 => self.end_included_unit(),
            None => self.end_document(),
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
                self.open_in_array(Kind::Object, Close::Brace)
            }
            Some(b'[') => {
                self.pos += 1;
                self.open_in_array(Kind::Array, Close::Bracket)
            }
            _ => {
                let (value, quoted, origin) = self.scalar()?;
                let index = self.push_element(value);
                if origin.has_string_facts()
                    && self.facts.is_some()
                    && let Some(mut path) = self.top_path()
                {
                    path.push(PathSegment::Index(index));
                    let facts = self.facts.as_mut().expect("checked above");
                    facts.update(&path, |f| {
                        f.single_quoted = origin.single_quoted;
                        f.multiline = origin.multiline;
                    });
                }
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

    fn read_key(&mut self) -> Result<Key, Error> {
        let at = self.pos;
        let lowercase = self.settings.flags.contains(ParserFlags::KEY_LOWERCASE);
        let (bytes, quoted, escaped) = match self.peek() {
            Some(b'"') => {
                let (bytes, end) = string::double_quoted(self.src, at)?;
                if bytes.is_empty() {
                    return Err(self.error(ErrorKind::EmptyKey, at));
                }
                self.pos = end;
                let escaped = self.src[at + 1..end - 1].contains(&b'\\');
                if lowercase {
                    // §12.1, *Quirk*: the key is lowercased as written, then its escapes are
                    // decoded. A `\U` that becomes `\u` is decoded like an unquoted value's
                    // (§4.8), as the oracle does (QUESTIONS.md #20).
                    let mut raw = self.src[at + 1..end - 1].to_vec();
                    raw.make_ascii_lowercase();
                    (string::decode_unquoted(&raw).0, true, escaped)
                } else {
                    (bytes, true, escaped)
                }
            }
            Some(b'\'') => return Err(self.error(ErrorKind::SingleQuotedKey, at)),
            Some(b) if is_key_start(b) => {
                while self.peek().is_some_and(is_key_byte) {
                    self.pos += 1;
                }
                (self.src[at..self.pos].to_vec(), false, false)
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
        let mut name = self.text(bytes, at)?;
        if lowercase && !quoted {
            name.make_ascii_lowercase();
        }
        // Fact 3 of spec §10.1.
        let quoted = quoted && (escaped || key_needs_quoting(&name));
        Ok(Key { name, at, quoted })
    }

    /// Everything after a key: an optional separator, then the value, or section names (§1.2,
    /// §3.4).
    fn after_key(&mut self, key: Key) -> Result<(), Error> {
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
    fn section_path(&mut self, first: Key) -> Result<(), Error> {
        self.names(first, false, false)
    }

    /// The first key read in a name run (§9.1, *Quirk*). It counts as a word that follows a
    /// name: with a `=` or `:` after it, it is a name too and the names go on; without, it is
    /// tested as any key is.
    fn key_after_name_run(&mut self, word: Key) -> Result<(), Error> {
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
        self.recent = self.top_path();
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
        first: Key,
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
            // The first name of the unit's first key gets a share of a brace that the unit's
            // leading `{` took over (oracle runs, QUESTIONS.md #42).
            let close = if std::mem::take(&mut self.section_shares) {
                Close::IncludedBrace(Revert::Section)
            } else {
                Close::Section
            };
            self.open_in_object(name, Kind::Object, close)?;
            opened += 1;
            if after_break {
                // A line break after a separator that follows a name: the next name may come on
                // any later line, and if the input ends first, the objects close quietly
                // (QUESTIONS.md #22).
                self.skip_space_checking_hash(false)?;
                if self.peek().is_none() {
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
    fn entry_value(&mut self, key: Key) -> Result<(), Error> {
        match self.peek() {
            None => Err(self.error(ErrorKind::MissingValue, self.pos)),
            Some(b'\n' | b'\r' | 0x0B | 0x0C) => self.next_line_value(key),
            Some(_) => self.object_value(key),
        }
    }

    /// A value on a following line (§1.6, *Quirk*). Blank lines and whitespace are skipped, then
    /// one group of comments that follow each other directly. The value starts right after the
    /// group; at the end of input it is `null`.
    fn next_line_value(&mut self, mut key: Key) -> Result<(), Error> {
        if let Some(notes) = &mut self.notes {
            // The group's comments attach after the value is created (§12.5, QUESTIONS.md #17).
            notes.hold();
        }
        self.leading_comments()?;
        if self.peek().is_none() {
            if let Some(existing) = self.null_merges_into(&key) {
                // Under `merge`, the `null` that ends the input goes into an object or array
                // that is the entry's first value, adding nothing, instead of taking its place
                // as a scalar does (§8.4; oracle runs, QUESTIONS.md #44).
                let path = self.placed_path(&existing, Placement::Merged);
                self.created(path);
                return Ok(());
            }
            let placement = self.insert(&mut key, UclValue::Null, Origin::default())?;
            let path = self.placed_path(&key.name, placement);
            self.created(path);
            return Ok(());
        }
        self.object_value(key)
    }

    /// Under `merge`, the spelling of the entry `key` names when its first value is an object or
    /// an array (keys compared as [`Core::insert`] does).
    fn null_merges_into(&mut self, key: &Key) -> Option<String> {
        if self.settings.strategy != DuplicateStrategy::Merge {
            return None;
        }
        let ignore_case = self.settings.flags.contains(ParserFlags::KEY_LOWERCASE)
            && (self.uppercase_keys || key.name.bytes().any(|b| b.is_ascii_uppercase()));
        let object = self.current().as_object()?;
        let index = find_key(object, &key.name, ignore_case)?;
        let (name, entry) = object.get_index(index)?;
        (entry.first().is_object() || entry.first().is_array()).then(|| name.clone())
    }

    fn object_value(&mut self, mut key: Key) -> Result<(), Error> {
        match self.peek() {
            Some(b'{') => {
                self.pos += 1;
                self.open_in_object(key, Kind::Object, Close::Brace)
            }
            Some(b'[') => {
                self.pos += 1;
                self.open_in_object(key, Kind::Array, Close::Bracket)
            }
            _ => {
                let (value, quoted, origin) = self.scalar()?;
                let placement = self.insert(&mut key, value, origin)?;
                let path = self.placed_path(&key.name, placement);
                self.created(path);
                self.after_value(quoted)
            }
        }
    }

    /// A value that is not a container. Returns it, whether it was quoted (a string in double or
    /// single quotes, or a heredoc), and where a string came from.
    fn scalar(&mut self) -> Result<(UclValue, bool, Origin), Error> {
        let at = self.pos;
        match self.peek() {
            Some(b'"') => {
                let (bytes, end) = string::double_quoted(self.src, at)?;
                self.pos = end;
                let bytes = self.expander.expand(bytes);
                Ok((
                    UclValue::String(self.text(bytes, at)?),
                    true,
                    Origin::default(),
                ))
            }
            Some(b'\'') => {
                let (bytes, end) = string::single_quoted(self.src, at)?;
                self.pos = end;
                let origin = Origin {
                    single_quoted: true,
                    ..Origin::default()
                };
                Ok((UclValue::String(self.text(bytes, at)?), true, origin))
            }
            Some(b'<') if string::heredoc_opener(self.src, at).is_some() => {
                let (bytes, end) = string::heredoc(self.src, at)?;
                self.pos = end;
                let bytes = self.expander.expand(bytes);
                let origin = Origin {
                    multiline: true,
                    ..Origin::default()
                };
                Ok((UclValue::String(self.text(bytes, at)?), true, origin))
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
    fn unquoted(&mut self) -> Result<(UclValue, bool), Error> {
        let start = self.pos;
        if matches!(self.peek(), Some(b'0'..=b'9' | b'-')) {
            let no_time = self.settings.flags.contains(ParserFlags::NO_TIME);
            match number::scan(self.src, start, no_time) {
                Number::Value(value, end) => {
                    self.pos = end;
                    return Ok((value, false));
                }
                Number::OutOfRange => return Err(self.error(ErrorKind::NumberOutOfRange, start)),
                Number::NotNumber => {}
            }
        }
        let end = self.unquoted_extent(start);
        self.pos = end;
        let mut trimmed = end;
        while trimmed > start && matches!(self.src[trimmed - 1], b' ' | b'\t') {
            trimmed -= 1;
        }
        let raw = &self.src[start..trimmed];
        if raw.is_empty() {
            return Err(self.error(ErrorKind::MissingValue, start));
        }
        if let Some(value) = keyword(raw) {
            return Ok((value, true));
        }
        // Variables are expanded only if some `$` is not written as `\$` (§7.6, *Quirk*).
        let (bytes, expand) = string::decode_unquoted(raw);
        let bytes = if expand {
            self.expander.expand(bytes)
        } else {
            bytes
        };
        Ok((UclValue::String(self.text(bytes, start)?), false))
    }

    /// The end of the unquoted value starting at `start` (§4.1, §4.2): the first line break,
    /// NUL, `,`, `;`, `#`, `/*`, or `}` or `]` without a match earlier in the value. A backslash
    /// takes the byte after it into the value.
    fn unquoted_extent(&self, start: usize) -> usize {
        let src = self.src;
        let mut i = start;
        let mut braces = 0usize;
        let mut brackets = 0usize;
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
                b'\\' if i + 1 < src.len() => i += 1,
                _ => {}
            }
            i += 1;
        }
        i
    }
}
