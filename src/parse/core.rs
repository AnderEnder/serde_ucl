//! Document structure: entries, keys, named sections and containers (spec §1–§4, §8, §11).
//!
//! The parser keeps an explicit stack of the containers that are open, so nesting depth never
//! grows the call stack. Containers are filled in place: an object or array is inserted into its
//! parent (by the duplicate rules of §8) when its opening bracket is read, and later entries go
//! straight into it. Each stack frame records how to reach its container from the one below.

use super::error::position_at;
use super::number::{self, Number};
use super::string;
use super::vars::Expander;
use super::{Comment, Error, ErrorKind, MAX_NESTING};
use crate::value::{
    DuplicateKeyError, DuplicateStrategy, ParserFlags, Placement, Slot, UclObject, UclValue,
};

/// The settings of one input unit.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Settings {
    pub(crate) flags: ParserFlags,
    pub(crate) priority: u8,
    pub(crate) strategy: DuplicateStrategy,
}

/// Parses a whole document into its root value.
pub(crate) fn parse_document(
    input: &[u8],
    settings: Settings,
    expander: &mut Expander<'_>,
    comments: Option<&mut Vec<Comment>>,
) -> Result<UclValue, Error> {
    let mut core = Core {
        src: input,
        pos: 0,
        settings,
        expander,
        comment_spans: comments.as_ref().map(|_| Vec::new()),
        root: UclValue::Null,
        frames: Vec::new(),
    };
    let result = core.run();
    if let (Some(out), Some(spans)) = (comments, core.comment_spans.take()) {
        *out = comments_at(input, &spans);
    }
    result.map(|()| core.root)
}

/// Converts comment byte ranges, in input order, to [`Comment`]s in one pass.
fn comments_at(src: &[u8], spans: &[(usize, usize)]) -> Vec<Comment> {
    let mut line = 1;
    let mut line_start = 0;
    let mut scanned = 0;
    spans
        .iter()
        .map(|&(start, end)| {
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
            Comment {
                text: String::from_utf8_lossy(&src[start..end]).into_owned(),
                position: crate::error::Position {
                    line,
                    column,
                    offset: start,
                },
            }
        })
        .collect()
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
    /// above it.
    Section,
    /// A section-name object whose path ended in an ordinary key (§3.4, *Quirk*). It has no
    /// closing bracket; only the end of input closes it.
    LeftOpen,
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
}

impl Step {
    fn enter<'v>(&self, value: &'v mut UclValue) -> &'v mut UclValue {
        let target = match self {
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
        };
        target.expect("an open container stays where it was inserted")
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

/// A key and where it starts.
struct Key {
    name: String,
    at: usize,
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
fn is_space(b: u8) -> bool {
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

/// The macro names of §9.2.
const MACROS: [&[u8]; 6] = [
    b"include",
    b"try_include",
    b"includes",
    b"priority",
    b"load",
    b"inherit",
];

struct Core<'s, 'e, 'v> {
    src: &'s [u8],
    pos: usize,
    settings: Settings,
    expander: &'e mut Expander<'v>,
    /// Byte ranges of skipped comments, when comments are saved (§12.5).
    comment_spans: Option<Vec<(usize, usize)>>,
    root: UclValue,
    frames: Vec<Frame>,
}

impl Core<'_, '_, '_> {
    // ----- bytes ---------------------------------------------------------------------------

    fn peek(&self) -> Option<u8> {
        self.src.get(self.pos).copied()
    }

    fn peek_at(&self, ahead: usize) -> Option<u8> {
        self.src.get(self.pos + ahead).copied()
    }

    fn at_block_comment(&self) -> bool {
        self.peek() == Some(b'/') && self.peek_at(1) == Some(b'*')
    }

    fn error(&self, kind: ErrorKind, at: usize) -> Error {
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
        if let Some(spans) = &mut self.comment_spans {
            spans.push((start, end));
        }
        self.pos = (end + 1).min(self.src.len());
    }

    /// Skips a `/* … */` comment. Comments nest, and a `*/` between double quotes does not end
    /// one (§2.3).
    fn skip_block_comment(&mut self) -> Result<(), Error> {
        let start = self.pos;
        let src = self.src;
        let mut i = start + 2;
        let mut depth = 1;
        let mut in_quotes = false;
        while depth > 0 {
            match src.get(i) {
                None => return Err(self.error(ErrorKind::UnterminatedComment, start)),
                Some(b'"') => {
                    in_quotes = !in_quotes;
                    i += 1;
                }
                Some(b'\\') if in_quotes => i += 2,
                Some(b'/') if !in_quotes && src.get(i + 1) == Some(&b'*') => {
                    depth += 1;
                    i += 2;
                }
                Some(b'*') if !in_quotes && src.get(i + 1) == Some(&b'/') => {
                    depth -= 1;
                    i += 2;
                }
                Some(_) => i += 1,
            }
        }
        if let Some(spans) = &mut self.comment_spans {
            spans.push((start, i));
        }
        self.pos = i;
        Ok(())
    }

    /// Skips whitespace, line breaks and comments: before an entry, an array element or the
    /// root value.
    fn skip_space(&mut self) -> Result<(), Error> {
        loop {
            match self.peek() {
                Some(b) if is_space(b) => self.pos += 1,
                Some(b'#') => self.skip_line_comment(),
                _ if self.at_block_comment() => self.skip_block_comment()?,
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

    fn current(&mut self) -> &mut UclValue {
        resolve(&mut self.root, &mut self.frames)
    }

    fn push_frame(&mut self, kind: Kind, close: Close, home: Home, at: usize) -> Result<(), Error> {
        if self.frames.len() >= MAX_NESTING {
            return Err(self.error(ErrorKind::NestingTooDeep { limit: MAX_NESTING }, at));
        }
        self.frames.push(Frame {
            kind,
            close,
            home,
            fresh: true,
        });
        Ok(())
    }

    fn empty(kind: Kind) -> UclValue {
        match kind {
            Kind::Object => UclValue::Object(UclObject::new()),
            Kind::Array => UclValue::Array(Vec::new()),
        }
    }

    /// Inserts `value` under `key` in the current object, by the duplicate rules of §8.
    fn insert(&mut self, key: &Key, value: UclValue) -> Result<Placement, Error> {
        let Settings {
            flags,
            priority,
            strategy,
        } = self.settings;
        let object = self
            .current()
            .as_object_mut()
            .expect("entries are parsed inside objects");
        object
            .insert_slot_placed(
                key.name.as_str(),
                Slot::new(value, priority),
                strategy,
                flags,
            )
            .map_err(|DuplicateKeyError { key: name }| {
                self.error(ErrorKind::DuplicateKey { key: name }, key.at)
            })
    }

    /// Opens a new object or array under `key` in the current object.
    fn open_in_object(&mut self, key: Key, kind: Kind, close: Close) -> Result<(), Error> {
        if self.frames.len() >= MAX_NESTING {
            return Err(self.error(ErrorKind::NestingTooDeep { limit: MAX_NESTING }, self.pos));
        }
        let placement = self.insert(&key, Self::empty(kind))?;
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
        self.push_frame(kind, close, home, key.at)
    }

    /// Opens a new object or array as the next element of the current array.
    fn open_in_array(&mut self, kind: Kind, close: Close) -> Result<(), Error> {
        let at = self.pos;
        if self.frames.len() >= MAX_NESTING {
            return Err(self.error(ErrorKind::NestingTooDeep { limit: MAX_NESTING }, at));
        }
        let array = self
            .current()
            .as_array_mut()
            .expect("elements are parsed inside arrays");
        array.push(Self::empty(kind));
        let index = array.len() - 1;
        self.push_frame(kind, close, Home::Attached(Step::Element(index)), at)
    }

    /// Closes the top container after its bracket, with the section objects around it.
    fn close_container(&mut self) -> Result<(), Error> {
        self.frames.pop();
        while self
            .frames
            .last()
            .is_some_and(|f| f.close == Close::Section)
        {
            self.frames.pop();
        }
        if self.frames.is_empty() {
            // The root's closing bracket: the rest of the input is ignored (§1.1, *Quirk*).
            return Ok(());
        }
        self.after_value(false)
    }

    // ----- document (§1) -------------------------------------------------------------------

    fn run(&mut self) -> Result<(), Error> {
        self.skip_space()?;
        let (kind, close) = match self.peek() {
            Some(b'[') => (Kind::Array, Close::Bracket),
            Some(b'{') => (Kind::Object, Close::Brace),
            _ => (Kind::Object, Close::Eof),
        };
        if close != Close::Eof {
            self.pos += 1;
        }
        self.root = Self::empty(kind);
        self.push_frame(kind, close, Home::Root, self.pos)?;
        while let Some(frame) = self.frames.last() {
            match frame.kind {
                Kind::Object => self.object_step()?,
                Kind::Array => self.array_step()?,
            }
        }
        Ok(())
    }

    /// Parses the next entry of the current object, or closes it.
    fn object_step(&mut self) -> Result<(), Error> {
        self.skip_space()?;
        let at = self.pos;
        match self.peek() {
            None => match self.top().close {
                Close::Eof | Close::LeftOpen => {
                    self.frames.pop();
                    Ok(())
                }
                _ => Err(self.error(ErrorKind::UnterminatedObject, at)),
            },
            Some(b'}') if self.top().close == Close::Brace => {
                self.pos += 1;
                self.close_container()
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
                self.after_key(key)
            }
        }
    }

    /// Parses the next element of the current array, or closes it.
    fn array_step(&mut self) -> Result<(), Error> {
        let frame = self.frames.last_mut().expect("a container is open");
        if std::mem::replace(&mut frame.fresh, false) && self.leading_comments()? {
            // The first element starts right after a group of comments that follow each
            // other directly, whitespace included (QUESTIONS.md #9).
            return match self.peek() {
                None => Err(self.error(ErrorKind::UnterminatedArray, self.pos)),
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
            None => Err(self.error(ErrorKind::UnterminatedArray, at)),
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
                let (value, quoted) = self.scalar()?;
                self.current()
                    .as_array_mut()
                    .expect("elements are parsed inside arrays")
                    .push(value);
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
        loop {
            self.skip_blanks();
            match self.peek() {
                Some(b'\n' | b'\r' | 0 | b',' | b';') => self.pos += 1,
                Some(b'#') => self.skip_line_comment(),
                _ if self.at_block_comment() => self.skip_block_comment()?,
                _ => return Ok(()),
            }
        }
    }

    // ----- keys and sections (§3) ----------------------------------------------------------

    fn read_key(&mut self) -> Result<Key, Error> {
        let at = self.pos;
        let (bytes, quoted) = match self.peek() {
            Some(b'"') => {
                let (bytes, end) = string::double_quoted(self.src, at)?;
                if bytes.is_empty() {
                    return Err(self.error(ErrorKind::EmptyKey, at));
                }
                self.pos = end;
                (bytes, true)
            }
            Some(b'\'') => return Err(self.error(ErrorKind::SingleQuotedKey, at)),
            Some(b) if is_key_start(b) => {
                while self.peek().is_some_and(is_key_byte) {
                    self.pos += 1;
                }
                (self.src[at..self.pos].to_vec(), false)
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
        if self.settings.flags.contains(ParserFlags::KEY_LOWERCASE) {
            name.make_ascii_lowercase();
        }
        Ok(Key { name, at })
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
        let mut name = first;
        let mut opened = 0;
        let mut after_separator = false;
        loop {
            match self.peek() {
                Some(b'{' | b'[') if after_separator => {
                    return Err(self.error(ErrorKind::MissingValue, self.pos));
                }
                Some(b'{' | b'[') => return self.entry_value(name),
                _ => {}
            }
            self.open_in_object(name, Kind::Object, Close::Section)?;
            opened += 1;
            let word = match self.peek() {
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
            after_separator = matches!(self.peek(), Some(b'=' | b':'));
            if after_separator {
                self.pos += 1;
                self.skip_inline()?;
            } else if !matches!(self.peek(), Some(b'{' | b'[')) && !self.line_has_bracket() {
                // An ordinary key. The section objects have no closing bracket, so they stay
                // open until the end of input (§3.4, *Quirk*).
                let n = self.frames.len();
                for frame in &mut self.frames[n - opened..] {
                    frame.close = Close::LeftOpen;
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
    fn next_line_value(&mut self, key: Key) -> Result<(), Error> {
        self.leading_comments()?;
        if self.peek().is_none() {
            self.insert(&key, UclValue::Null)?;
            return Ok(());
        }
        self.object_value(key)
    }

    fn object_value(&mut self, key: Key) -> Result<(), Error> {
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
                let (value, quoted) = self.scalar()?;
                self.insert(&key, value)?;
                self.after_value(quoted)
            }
        }
    }

    /// A value that is not a container. Returns it and whether it was quoted (a string in
    /// double or single quotes, or a heredoc).
    fn scalar(&mut self) -> Result<(UclValue, bool), Error> {
        let at = self.pos;
        match self.peek() {
            Some(b'"') => {
                let (bytes, end) = string::double_quoted(self.src, at)?;
                self.pos = end;
                let bytes = self.expander.expand(bytes, None);
                Ok((UclValue::String(self.text(bytes, at)?), true))
            }
            Some(b'\'') => {
                let (bytes, end) = string::single_quoted(self.src, at)?;
                self.pos = end;
                Ok((UclValue::String(self.text(bytes, at)?), true))
            }
            Some(b'<') if string::heredoc_opener(self.src, at).is_some() => {
                let (bytes, end) = string::heredoc(self.src, at)?;
                self.pos = end;
                let bytes = self.expander.expand(bytes, None);
                Ok((UclValue::String(self.text(bytes, at)?), true))
            }
            _ => Ok((self.unquoted()?, false)),
        }
    }

    /// An unquoted value (§4): a number, a keyword or a string.
    fn unquoted(&mut self) -> Result<UclValue, Error> {
        let start = self.pos;
        if matches!(self.peek(), Some(b'0'..=b'9' | b'-')) {
            let no_time = self.settings.flags.contains(ParserFlags::NO_TIME);
            match number::scan(self.src, start, no_time) {
                Number::Value(value, end) => {
                    self.pos = end;
                    return Ok(value);
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
            return Ok(value);
        }
        let (bytes, protected) = string::decode_unquoted(raw);
        let bytes = self.expander.expand(bytes, Some(&protected));
        Ok(UclValue::String(self.text(bytes, start)?))
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

    // ----- macros (§9) ---------------------------------------------------------------------

    /// A `.` where a key could start is a macro (§9.1). Its name is checked; known macros are
    /// not supported until work item C3.
    fn macro_entry(&mut self) -> Result<(), Error> {
        let at = self.pos;
        if self.settings.flags.contains(ParserFlags::DISABLE_MACRO) {
            return Err(self.error(ErrorKind::MacrosDisabled, at));
        }
        let name_start = at + 1;
        let name_len = self.src[name_start..]
            .iter()
            .take_while(|&&b| !is_space(b) && b != b'(')
            .count();
        let name = &self.src[name_start..name_start + name_len];
        let name_text = String::from_utf8_lossy(name).into_owned();
        if MACROS.contains(&name) {
            Err(self.error(
                ErrorKind::Unsupported {
                    feature: format!("the macro .{name_text}"),
                },
                at,
            ))
        } else {
            Err(self.error(ErrorKind::UnknownMacro { name: name_text }, at))
        }
    }
}
