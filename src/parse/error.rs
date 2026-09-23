//! Errors of the byte-oriented parser core.
//!
//! The spec fixes only whether a document is rejected (`docs/spec/11-errors.md` §11.1). The kinds
//! and messages here are the project's own; every error carries the position where it was
//! detected.

use crate::error::Position;
use std::fmt;

/// A parse error: what went wrong and where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    kind: ErrorKind,
    position: Position,
}

impl Error {
    /// An error of `kind` at `position`.
    pub fn new(kind: ErrorKind, position: Position) -> Self {
        Self { kind, position }
    }

    /// What went wrong.
    pub fn kind(&self) -> &ErrorKind {
        &self.kind
    }

    /// Where the error was detected: 1-based line and column (in characters), 0-based byte offset.
    pub fn position(&self) -> Position {
        self.position
    }

    /// True for input the parser recognises but does not support yet, such as macros before
    /// work item C3. Such an error is not a rejection of the document by the format rules.
    pub fn is_unsupported(&self) -> bool {
        matches!(self.kind, ErrorKind::Unsupported { .. })
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} (line {}, column {})",
            self.kind, self.position.line, self.position.column
        )
    }
}

impl std::error::Error for Error {}

/// The kinds of parse error.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ErrorKind {
    /// An object's `{` has no matching `}`.
    UnterminatedObject,
    /// An array's `[` has no matching `]`.
    UnterminatedArray,
    /// A quoted string has no closing quote.
    UnterminatedString,
    /// A heredoc has no terminator line.
    UnterminatedHeredoc,
    /// A block comment has no matching `*/`.
    UnterminatedComment,
    /// A `#` that is the last byte of the input, directly after whitespace, where the first key of
    /// an unbraced root or a macro's value could start (spec §2.2, *Quirk*).
    HashAtEnd,
    /// A `}` or `]` that closes nothing, or the wrong kind of container.
    UnmatchedClose { found: char },
    /// A key could not start or continue here.
    InvalidKey { found: Option<char> },
    /// `""` as a key.
    EmptyKey,
    /// A key written in single quotes.
    SingleQuotedKey,
    /// Two separators between a key and its value, such as `a == b`.
    DoubleSeparator,
    /// A key without a value, or a value that is empty.
    MissingValue,
    /// A `,` or `;` where an entry or an array element must start.
    UnexpectedTerminator,
    /// Something other than whitespace, a comment, a terminator or a closing bracket directly
    /// after a quoted string or heredoc.
    MissingDelimiter { found: char },
    /// A raw control character inside a double-quoted string.
    ControlCharacter { byte: u8 },
    /// A `\u` escape without four hex digits in a double-quoted string.
    InvalidUnicodeEscape,
    /// An integer outside the 64-bit signed range, or a float that overflows or underflows.
    NumberOutOfRange,
    /// A key or string that is not valid UTF-8 (a project divergence from libucl, spec §11.3).
    InvalidUtf8,
    /// A repeated key that the duplicate strategy does not accept (spec §8).
    DuplicateKey { key: String },
    /// More containers open at once than the nesting limit allows (spec §11.2).
    NestingTooDeep { limit: usize },
    /// A macro name that is not known.
    UnknownMacro { name: String },
    /// A macro while macros are disabled (`ParserFlags::DISABLE_MACRO`).
    MacrosDisabled,
    /// A macro's `(` has no matching `)` (spec §9.2).
    UnterminatedArguments,
    /// Input the parser recognises but does not support.
    Unsupported { feature: String },
    /// The input file could not be read.
    Io { message: String },
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErrorKind::UnterminatedObject => f.write_str("object is not closed with '}'"),
            ErrorKind::UnterminatedArray => f.write_str("array is not closed with ']'"),
            ErrorKind::UnterminatedString => f.write_str("string has no closing quote"),
            ErrorKind::UnterminatedHeredoc => f.write_str("heredoc has no terminator line"),
            ErrorKind::UnterminatedComment => f.write_str("block comment is not closed"),
            ErrorKind::HashAtEnd => {
                f.write_str("a '#' after whitespace cannot be the last byte of the input here")
            }
            ErrorKind::UnmatchedClose { found } => {
                write!(f, "'{found}' does not close an open container")
            }
            ErrorKind::InvalidKey { found: Some(c) } => {
                write!(f, "{} is not allowed in or after a key", describe(*c))
            }
            ErrorKind::InvalidKey { found: None } => f.write_str("input ends inside a key"),
            ErrorKind::EmptyKey => f.write_str("a key cannot be empty"),
            ErrorKind::SingleQuotedKey => f.write_str("a key cannot be single-quoted"),
            ErrorKind::DoubleSeparator => f.write_str("more than one separator after a key"),
            ErrorKind::MissingValue => f.write_str("key has no value"),
            ErrorKind::UnexpectedTerminator => f.write_str("separator before any entry"),
            ErrorKind::MissingDelimiter { found } => write!(
                f,
                "{} directly after a quoted value; expected a separator",
                describe(*found)
            ),
            ErrorKind::ControlCharacter { byte } => {
                write!(f, "control character 0x{byte:02x} in a quoted string")
            }
            ErrorKind::InvalidUnicodeEscape => {
                f.write_str("'\\u' must be followed by four hex digits")
            }
            ErrorKind::NumberOutOfRange => f.write_str("number is out of range"),
            ErrorKind::InvalidUtf8 => f.write_str("text is not valid UTF-8"),
            ErrorKind::DuplicateKey { key } => {
                write!(f, "key '{key}' cannot take another value")
            }
            ErrorKind::NestingTooDeep { limit } => {
                write!(f, "more than {limit} containers are open")
            }
            ErrorKind::UnknownMacro { name } if name.is_empty() => {
                f.write_str("'.' must be followed by a macro name")
            }
            ErrorKind::UnknownMacro { name } => write!(f, "unknown macro '.{name}'"),
            ErrorKind::MacrosDisabled => f.write_str("macros are disabled"),
            ErrorKind::UnterminatedArguments => {
                f.write_str("macro arguments are not closed with ')'")
            }
            ErrorKind::Unsupported { feature } => write!(f, "{feature} is not supported yet"),
            ErrorKind::Io { message } => write!(f, "cannot read input: {message}"),
        }
    }
}

fn describe(c: char) -> String {
    match c {
        '\n' => "a line break".to_string(),
        '\r' => "a carriage return".to_string(),
        c if c.is_control() => format!("control character U+{:04X}", c as u32),
        c => format!("'{c}'"),
    }
}

/// The position of byte `offset` in `src`. Columns count UTF-8 characters.
pub(crate) fn position_at(src: &[u8], offset: usize) -> Position {
    let offset = offset.min(src.len());
    let before = &src[..offset];
    let line_start = before
        .iter()
        .rposition(|&b| b == b'\n')
        .map_or(0, |i| i + 1);
    let line = 1 + before.iter().filter(|&&b| b == b'\n').count();
    let column = 1 + before[line_start..]
        .iter()
        .filter(|&&b| (b & 0xC0) != 0x80)
        .count();
    Position {
        line,
        column,
        offset,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn position_counts_lines_and_characters() {
        let src = "a = 1\nbé = x".as_bytes();
        assert_eq!(
            position_at(src, 0),
            Position {
                line: 1,
                column: 1,
                offset: 0
            }
        );
        let x = src.iter().position(|&b| b == b'x').unwrap();
        let p = position_at(src, x);
        assert_eq!((p.line, p.column, p.offset), (2, 6, x));
    }
}
