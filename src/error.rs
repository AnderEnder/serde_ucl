//! The crate's error type, [`UclError`]; [`Position`], a position in the parser's input; and
//! [`DeserializeError`], a deserialization error with the value it is about.

use crate::parse::PathSegment;
use std::fmt;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// A position in the parser's input: 1-based line and column (in characters), 0-based byte
/// offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Position {
    /// Line number (1-based)
    pub line: usize,
    /// Column number (1-based)
    pub column: usize,
    /// Byte offset from start of input (0-based)
    pub offset: usize,
}

impl Position {
    /// The start of the input: line 1, column 1, offset 0.
    pub fn new() -> Self {
        Self {
            line: 1,
            column: 1,
            offset: 0,
        }
    }
}

impl Default for Position {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.line, self.column)
    }
}

/// Main error type for UCL parsing operations
#[derive(Debug, Error)]
pub enum UclError {
    /// Serde deserialization error
    #[error("Serde error: {0}")]
    Serde(#[from] SerdeError),

    /// I/O error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// A document the parser ([`crate::parse`]) rejects, or an include macro or `.load` that
    /// fails: the parser's error, with its [`kind`](crate::parse::Error::kind), its
    /// [`position`](crate::parse::Error::position) and, for an error in an included file, that
    /// [`file`](crate::parse::Error::file).
    #[error("Syntax error: {0}")]
    Syntax(#[source] crate::parse::Error),

    /// A silent stop of the parser (spec §9.4): libucl ends the parse at a
    /// `.try_include` that finds no usable file, or at an `.include` of a glob pattern that
    /// matches nothing, and keeps what it has parsed. The error's
    /// [`partial`](crate::parse::Error::partial) result holds that.
    #[error("Parsing stopped: {0}")]
    Stopped(#[source] crate::parse::Error),

    /// A value that does not fit the type it is deserialized into: the serde error, the path of
    /// the value from the root and, for a document the crate parsed, where the value was
    /// written ([`DeserializeError`]).
    #[error("Deserialization error: {0}")]
    Deserialize(DeserializeError),
}

impl UclError {
    /// The parser core's error, for [`UclError::Syntax`] and [`UclError::Stopped`]: its
    /// [`kind`](crate::parse::Error::kind), [`position`](crate::parse::Error::position) and
    /// [`file`](crate::parse::Error::file).
    pub fn parse_error(&self) -> Option<&crate::parse::Error> {
        match self {
            UclError::Syntax(e) | UclError::Stopped(e) => Some(e),
            _ => None,
        }
    }

    /// Where the error is: for [`UclError::Syntax`] and [`UclError::Stopped`], where the parser
    /// found it; for [`UclError::Deserialize`], where the value it is about was written, when
    /// the crate parsed the document ([`DeserializeError::position`]). The position is in the
    /// document given to the parser, or in the included file that [`UclError::file`] names.
    pub fn position(&self) -> Option<Position> {
        match self {
            UclError::Deserialize(e) => e.position(),
            _ => self.parse_error().map(crate::parse::Error::position),
        }
    }

    /// The included file (spec §9.4) that [`UclError::position`] is in, or `None` when it is in
    /// the document given to the parser, or there is no position.
    pub fn file(&self) -> Option<&Path> {
        match self {
            UclError::Deserialize(e) => e.file(),
            _ => self.parse_error().and_then(crate::parse::Error::file),
        }
    }

    /// A serde error of deserialization, at the document itself: [`UclError::Deserialize`] with
    /// an empty path. Any other error is returned as it is.
    #[cold]
    #[inline(never)]
    pub(crate) fn in_document(self) -> Self {
        match self {
            UclError::Serde(error) => UclError::Deserialize(DeserializeError::new(error)),
            other => other,
        }
    }

    /// The error of deserializing a value, as an error inside the container that holds it at
    /// `segment`.
    pub(crate) fn inside(self, segment: PathSegment) -> Self {
        self.map_deserialize(|e| e.path.insert(0, segment))
    }

    /// The error of deserializing the value of entry `key`, as an error inside the object that
    /// holds it.
    pub(crate) fn inside_entry(self, key: &str) -> Self {
        self.inside(PathSegment::Key {
            key: key.to_owned(),
            index: 0,
        })
    }

    /// The error of deserializing the values of entry `key`, which has several, read as a
    /// sequence or as a map from indices: the step `Index(n)` that the access over them put in
    /// front of the path becomes value `n` of the entry. Their values are reached only through
    /// that access, so a path that does not start with such a step is an error about the values
    /// as a whole, which is about the entry's first value.
    pub(crate) fn inside_values_of(self, key: &str) -> Self {
        self.map_deserialize(|e| match e.path.first() {
            Some(&PathSegment::Index(index)) => {
                e.path[0] = PathSegment::Key {
                    key: key.to_owned(),
                    index,
                };
            }
            _ => e.path.insert(
                0,
                PathSegment::Key {
                    key: key.to_owned(),
                    index: 0,
                },
            ),
        })
    }

    /// The error of deserializing the key of entry `key`, as an error inside the object that
    /// holds it.
    #[cold]
    #[inline(never)]
    pub(crate) fn at_key(self, key: &str) -> Self {
        self.inside_entry(key).map_deserialize(|e| {
            if e.path.len() == 1 {
                e.key = true;
            }
        })
    }

    /// Puts `step` in front of the path of this error of deserializing a value, in place, as
    /// the error goes up through the map or sequence that holds the value.
    #[cold]
    #[inline(never)]
    pub(crate) fn add_step(&mut self, step: Step<'_>) {
        let placeholder = UclError::Serde(SerdeError::Custom(String::new()));
        let error = std::mem::replace(self, placeholder);
        *self = match step {
            Step::Index(index) => error.inside(PathSegment::Index(index)),
            Step::Entry(key) => error.inside_entry(key),
            Step::Values(key) => error.inside_values_of(key),
            Step::Key(key) => error.at_key(key),
        };
    }

    fn map_deserialize(self, change: impl FnOnce(&mut Located)) -> Self {
        match self.in_document() {
            UclError::Deserialize(mut e) => {
                change(&mut e.0);
                UclError::Deserialize(e)
            }
            other => other,
        }
    }
}

/// A step from a map or a sequence to the value inside it that an error of deserialization is
/// about ([`UclError::add_step`]).
#[derive(Debug, Clone, Copy)]
pub(crate) enum Step<'a> {
    /// Element `n` of an array.
    Index(usize),
    /// The value of the entry with this key.
    Entry(&'a str),
    /// The values of the entry with this key, which has several, read as a sequence or as a map
    /// from indices ([`UclError::inside_values_of`]).
    Values(&'a str),
    /// The key of the entry with this key.
    Key(&'a str),
}

/// A deserialization error, with the value it is about: the error serde reported
/// ([`DeserializeError::error`]), the value's path from the root ([`DeserializeError::path`]),
/// and, when the crate parsed the document, where the value was written
/// ([`DeserializeError::position`], [`DeserializeError::file`]).
///
/// [`crate::from_str`], [`crate::from_slice`], [`crate::from_reader`], [`crate::from_file`], the
/// `from_str_with_*` functions and [`crate::UclDeserializer`] give the path and the position (see
/// [`crate::de`], *Errors*, for how and at what cost); [`crate::from_value`] gives neither. The
/// position is where the value's text starts: the
/// first byte of a number or an unquoted string, the quote of a quoted string, the `<<` of a
/// heredoc, the bracket of an object or array, and the name of an object made by a section name
/// (spec §3.4). A value that `.inherit` copied (§9.7) is where the copied value was written, a
/// container made by an include macro's `key` or `prefix` parameter (§9.4) and a value made by
/// `.load` (§9.6) where the macro is, and a value from an included file is in that file. An
/// error about a key rather than its value, such as an unknown field or a key that does not
/// parse as the map's key type, is where the key was written ([`DeserializeError::at_key`]).
/// The document itself is at its opening bracket, or at the start of the input.
///
/// ```
/// use serde::Deserialize;
///
/// #[derive(Debug, Deserialize)]
/// struct Server {
///     #[allow(dead_code)]
///     port: u16,
/// }
/// #[derive(Debug, Deserialize)]
/// struct Config {
///     #[allow(dead_code)]
///     server: Server,
/// }
///
/// let err = ucl_lexer::from_str::<Config>("server {\n    port = 80000\n}").unwrap_err();
/// let position = err.position().unwrap();
/// assert_eq!((position.line, position.column), (2, 12));
/// assert_eq!(
///     err.to_string(),
///     "Deserialization error: invalid value: integer `80000`, expected u16 at server.port \
///      (line 2, column 12)"
/// );
/// ```
#[derive(Debug)]
pub struct DeserializeError(Box<Located>);

#[derive(Debug)]
struct Located {
    error: SerdeError,
    path: Vec<PathSegment>,
    /// The error is about the key of the entry at `path`, not its value.
    key: bool,
    position: Option<Position>,
    file: Option<PathBuf>,
}

impl DeserializeError {
    fn new(error: SerdeError) -> Self {
        Self(Box::new(Located {
            error,
            path: Vec::new(),
            key: false,
            position: None,
            file: None,
        }))
    }

    /// The error serde reported.
    pub fn error(&self) -> &SerdeError {
        &self.0.error
    }

    /// The error serde reported, by value.
    pub fn into_error(self) -> SerdeError {
        self.0.error
    }

    /// The path from the root of the value the error is about; empty for the document itself,
    /// and for an error of [`crate::from_value`].
    /// A value of an entry is [`PathSegment::Key`] with the index of the value, which is not 0
    /// only for a key with several values (spec §8.2).
    pub fn path(&self) -> &[PathSegment] {
        &self.0.path
    }

    /// True if the error is about the key of the entry at [`DeserializeError::path`] rather than
    /// its value: an unknown field, or a key that the map's key type does not accept.
    pub fn at_key(&self) -> bool {
        self.0.key
    }

    /// Where the value, or with [`DeserializeError::at_key`] its key, was written, in the
    /// document given to the parser or in the included file [`DeserializeError::file`] names.
    /// `None` for [`crate::from_value`], which has no document, and when the document could not
    /// be read again to find it.
    pub fn position(&self) -> Option<Position> {
        self.0.position
    }

    /// The included file (spec §9.4) the value was written in, or `None` when it was written in
    /// the document given to the parser, or there is no position.
    pub fn file(&self) -> Option<&Path> {
        self.0.file.as_deref()
    }

    /// Whether the error is about a value, or a key, of the document.
    pub(crate) fn locate_request(&self) -> (&[PathSegment], bool) {
        (&self.0.path, self.0.key)
    }

    /// Records where the value was written.
    pub(crate) fn set_position(&mut self, position: Position, file: Option<PathBuf>) {
        self.0.position = Some(position);
        self.0.file = file;
    }
}

impl fmt::Display for DeserializeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.error)?;
        if !self.0.path.is_empty() {
            let what = if self.0.key { "at the key of" } else { "at" };
            write!(f, " {what} {}", PathText(&self.0.path))?;
        }
        if let Some(position) = self.0.position {
            write!(f, " (line {}, column {}", position.line, position.column)?;
            match &self.0.file {
                Some(file) => write!(f, " of {})", file.display())?,
                None => f.write_str(")")?,
            }
        }
        Ok(())
    }
}

impl std::error::Error for DeserializeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0.error)
    }
}

/// A value path for messages: keys joined by `.`, in double quotes unless they hold only
/// letters, digits, `_`, `-` and `/`; `[n]` for element `n` of an array, or value `n` of a key
/// with several values.
struct PathText<'a>(&'a [PathSegment]);

impl fmt::Display for PathText<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (n, segment) in self.0.iter().enumerate() {
            match segment {
                PathSegment::Key { key, index } => {
                    if n > 0 {
                        f.write_str(".")?;
                    }
                    let plain = !key.is_empty()
                        && key
                            .chars()
                            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '/'));
                    if plain {
                        f.write_str(key)?;
                    } else {
                        write!(f, "{key:?}")?;
                    }
                    if *index > 0 {
                        write!(f, "[{index}]")?;
                    }
                }
                PathSegment::Index(index) => write!(f, "[{index}]")?,
            }
        }
        Ok(())
    }
}

impl From<crate::parse::Error> for UclError {
    /// A silent stop becomes [`UclError::Stopped`], any other error [`UclError::Syntax`].
    fn from(error: crate::parse::Error) -> Self {
        if error.is_stopped() {
            UclError::Stopped(error)
        } else {
            UclError::Syntax(error)
        }
    }
}

/// Serde integration errors
#[derive(Debug, Error)]
pub enum SerdeError {
    /// Custom serde error message
    #[error("{0}")]
    Custom(String),

    /// Serialization met a value that has no form that reads back as the same value (spec §10.8),
    /// such as an integer outside the 64-bit signed range, a subnormal float, the empty key or a
    /// root that is not an object or an array. See [`crate::ser`] for the full list.
    #[error("cannot serialize {0}")]
    Unrepresentable(String),

    /// Deserializing into, or serializing from, a type other than [`UclValue`](crate::UclValue)
    /// and [`UclObject`](crate::UclObject) met more than `limit`
    /// ([`MAX_SERDE_NESTING`](crate::de::MAX_SERDE_NESTING)) maps and sequences nested inside
    /// one another, the outermost included. serde reads and writes such types by recursion, one
    /// call per level, so the limit keeps a thread's stack from overflowing.
    #[error("more than {limit} maps and sequences nested inside one another")]
    TooDeep { limit: usize },
}

impl serde::de::Error for UclError {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        UclError::Serde(SerdeError::Custom(msg.to_string()))
    }
}

impl serde::ser::Error for UclError {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        UclError::Serde(SerdeError::Custom(msg.to_string()))
    }
}

impl serde::de::Error for SerdeError {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        SerdeError::Custom(msg.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_position_new() {
        let pos = Position::new();
        assert_eq!(pos.line, 1);
        assert_eq!(pos.column, 1);
        assert_eq!(pos.offset, 0);
        assert_eq!(Position::default(), pos);
    }

    #[test]
    fn test_position_display() {
        let pos = Position {
            line: 42,
            column: 13,
            offset: 100,
        };
        assert_eq!(format!("{pos}"), "42:13");
    }
}
