//! The crate's error type, [`UclError`], and [`Position`], where in its input the parser found
//! an error.

use std::fmt;
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

    /// Where the parser found the error, for [`UclError::Syntax`] and [`UclError::Stopped`]: in
    /// the document given to the parser, or in the included file that
    /// [`crate::parse::Error::file`] names.
    pub fn position(&self) -> Option<Position> {
        self.parse_error().map(crate::parse::Error::position)
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
        assert_eq!(format!("{}", pos), "42:13");
    }
}
