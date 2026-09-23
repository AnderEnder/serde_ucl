//! Output formats: JSON, compact JSON, the UCL config format and YAML (spec §10).
//!
//! An [`Emitter`] writes a [`UclValue`] in one [`Format`], byte for byte as libucl writes it,
//! quirks included. Besides the value itself, the config and YAML formats depend on facts
//! remembered from parsing (spec §10.1): whether a string was single-quoted or a heredoc, and how
//! each key was written. The parser records them ([`crate::parse::Parser::output_facts`]);
//! [`Parser::emitter`](crate::parse::Parser::emitter) gives an emitter that uses the facts of the
//! last parse. Without facts, strings use the JSON form and keys are quoted by
//! [`key_needs_quoting`].
//!
//! The config format can also write the comments saved under `SAVE_COMMENTS` (spec §10.10), but
//! only when asked with [`Emitter::with_comments`]; by default no comments are written. JSON and
//! YAML never contain comments.
//!
//! ```
//! use ucl_lexer::emit::Format;
//! use ucl_lexer::parse::Parser;
//!
//! let mut parser = Parser::new();
//! let value = parser.parse(b"name = 'web'\nports = [80, 443]\ntimeout = 1.5").unwrap();
//! let config = parser.emitter(Format::Config).emit(&value);
//! assert_eq!(config, "name = 'web';\nports [\n    80,\n    443,\n]\ntimeout = 1.500000;\n");
//! let json = parser.emitter(Format::JsonCompact).emit(&value);
//! assert_eq!(json, r#"{"name":"web","ports":[80,443],"timeout":1.500000}"#);
//! ```
//!
//! The default formats do not always read back as the same value; spec §10.8 lists where they
//! differ. Floats, for instance, keep at most six decimals. The serde functions of
//! [`crate::ser`] use the same layouts with forms that read back exactly, except that their JSON
//! output, which is valid JSON, writes a time as its seconds, which read back as a float.

mod config;
mod json;
mod number;
mod text;

pub use text::key_needs_quoting;

use crate::parse::{
    AttachedComments, Comment, CommentPlacement, OutputFacts, PathSegment, ValueFacts,
};
use crate::value::UclValue;
use std::collections::HashMap;

/// An output format of spec §10.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Format {
    /// JSON with one member or element per line, indented four spaces per level (§10.4).
    Json,
    /// JSON without whitespace (§10.4).
    JsonCompact,
    /// The UCL config format, which libucl reads back (§10.5).
    Config,
    /// libucl's YAML-like format (§10.6).
    Yaml,
}

/// How an [`Emitter`] writes scalars, keys and multi-value entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    /// As libucl writes them (spec §10.2–§10.7), quirks included.
    Libucl,
    /// In forms that read back as exactly the value written (spec §10.8), for serde
    /// serialization ([`crate::ser`]). Output facts and saved comments are not used. JSON and
    /// compact JSON are valid JSON (RFC 8259): a time is written as its number of seconds, and a
    /// NaN or infinite float or time has no form there (WORKLIST.md C4, decision 2).
    RoundTrip,
}

/// Writes values in one [`Format`], with optional output facts and saved comments.
#[derive(Debug, Clone, Copy)]
pub struct Emitter<'a> {
    format: Format,
    facts: Option<&'a OutputFacts>,
    comments: Option<(&'a [Comment], &'a [AttachedComments])>,
    mode: Mode,
}

impl<'a> Emitter<'a> {
    /// An emitter for `format`, without output facts or comments.
    pub fn new(format: Format) -> Self {
        Self {
            format,
            facts: None,
            comments: None,
            mode: Mode::Libucl,
        }
    }

    /// The emitter in round-trip mode: every value is written in a form that libucl and
    /// [`crate::parse`] read back as exactly that value (spec §10.8), in the layouts of the
    /// emitter's format; in JSON and compact JSON a time is written as its seconds and reads back
    /// as a float. Use [`Emitter::try_emit`]; see [`crate::ser`] for the forms and for the
    /// values that have none.
    pub(crate) fn round_trip(mut self) -> Self {
        self.mode = Mode::RoundTrip;
        self
    }

    /// The format this emitter writes.
    pub fn format(&self) -> Format {
        self.format
    }

    /// Uses `facts`, recorded when the value was parsed, for the forms of strings and keys
    /// (spec §10.1). The paths in `facts` must be those of the value given to
    /// [`Emitter::emit`].
    pub fn with_facts(mut self, facts: &'a OutputFacts) -> Self {
        self.facts = Some(facts);
        self
    }

    /// Writes the saved comments `attached` to each value, as libucl's config format does when
    /// the application passes them in (spec §10.10). `comments` and `attached` are
    /// [`Parser::comments`](crate::parse::Parser::comments) and
    /// [`Parser::attached_comments`](crate::parse::Parser::attached_comments) of the parse that
    /// produced the value. This has an effect in the config format only.
    pub fn with_comments(
        mut self,
        comments: &'a [Comment],
        attached: &'a [AttachedComments],
    ) -> Self {
        self.comments = Some((comments, attached));
        self
    }

    /// The text of `value` in the emitter's format.
    pub fn emit(&self, value: &UclValue) -> String {
        self.run(value).out
    }

    /// The text of `value` in round-trip mode, or a description of the first value that has no
    /// form that reads back exactly.
    pub(crate) fn try_emit(&self, value: &UclValue) -> Result<String, String> {
        let writer = self.run(value);
        match writer.error {
            Some(error) => Err(error),
            None => Ok(writer.out),
        }
    }

    fn run(&self, value: &UclValue) -> Writer<'a> {
        let exact = self.mode == Mode::RoundTrip;
        let comments = match (self.format, self.comments) {
            (Format::Config, Some((comments, attached))) if !exact => {
                comment_map(comments, attached)
            }
            _ => HashMap::new(),
        };
        let facts = self.facts.filter(|f| !f.is_empty() && !exact);
        let mut writer = Writer {
            out: String::new(),
            track: facts.is_some() || !comments.is_empty(),
            facts,
            comments,
            path: Vec::new(),
            mode: self.mode,
            json: exact && matches!(self.format, Format::Json | Format::JsonCompact),
            error: None,
        };
        if exact && !matches!(value, UclValue::Object(_) | UclValue::Array(_)) {
            writer.fail(format!(
                "a root {}: a UCL document is an object or an array (spec §1.1)",
                value.type_name()
            ));
        }
        if exact && nesting(value) > crate::parse::MAX_NESTING {
            writer.fail(format!(
                "a value nested more than {} containers deep, the root included (spec §11.2)",
                crate::parse::MAX_NESTING
            ));
            return writer;
        }
        match self.format {
            Format::Json => writer.json_root(value, json::Style::Json),
            Format::JsonCompact => writer.json_root(value, json::Style::Compact),
            Format::Yaml => writer.json_root(value, json::Style::Yaml),
            Format::Config => writer.config_root(value),
        }
        writer
    }
}

/// The most containers (objects and arrays) open at once in `value`, itself included.
fn nesting(value: &UclValue) -> usize {
    let mut deepest = 0;
    let mut stack = vec![(value, 1)];
    while let Some((value, depth)) = stack.pop() {
        match value {
            UclValue::Object(object) => {
                deepest = deepest.max(depth);
                stack.extend(
                    object
                        .entries()
                        .flat_map(|e| e.values())
                        .map(|v| (v, depth + 1)),
                );
            }
            UclValue::Array(items) => {
                deepest = deepest.max(depth);
                stack.extend(items.iter().map(|v| (v, depth + 1)));
            }
            _ => {}
        }
    }
    deepest
}

/// `value` as pretty JSON (spec §10.4).
pub fn to_json(value: &UclValue) -> String {
    Emitter::new(Format::Json).emit(value)
}

/// `value` as JSON without whitespace (spec §10.4).
pub fn to_json_compact(value: &UclValue) -> String {
    Emitter::new(Format::JsonCompact).emit(value)
}

/// `value` in the UCL config format (spec §10.5), without output facts: every string in the JSON
/// form, keys quoted by [`key_needs_quoting`].
pub fn to_config(value: &UclValue) -> String {
    Emitter::new(Format::Config).emit(value)
}

/// `value` in libucl's YAML format (spec §10.6), without output facts.
pub fn to_yaml(value: &UclValue) -> String {
    Emitter::new(Format::Yaml).emit(value)
}

type CommentMap<'a> = HashMap<&'a [PathSegment], (CommentPlacement, Vec<&'a str>)>;

fn comment_map<'a>(comments: &'a [Comment], attached: &'a [AttachedComments]) -> CommentMap<'a> {
    attached
        .iter()
        .map(|group| {
            let texts = group
                .comments
                .iter()
                .filter_map(|&i| comments.get(i).map(|c| c.text.as_str()))
                .collect();
            (group.path.as_slice(), (group.placement, texts))
        })
        .collect()
}

/// The state of one [`Emitter::emit`]: the output, and the path of the value being written
/// when facts or comments are looked up by path.
struct Writer<'a> {
    out: String,
    facts: Option<&'a OutputFacts>,
    comments: CommentMap<'a>,
    track: bool,
    path: Vec<PathSegment>,
    mode: Mode,
    /// Round-trip mode in JSON or compact JSON: numbers are JSON numbers.
    json: bool,
    /// In round-trip mode, the first value that has no exact form.
    error: Option<String>,
}

impl<'a> Writer<'a> {
    /// Records that a value has no exact form (round-trip mode). The first failure is kept.
    fn fail(&mut self, error: String) {
        if self.error.is_none() {
            self.error = Some(error);
        }
    }

    fn indent(&mut self, depth: usize) {
        for _ in 0..depth {
            self.out.push_str("    ");
        }
    }

    /// Enters value `index` of the entry `key`.
    fn enter_key(&mut self, key: &str, index: usize) {
        if self.track {
            self.path.push(PathSegment::Key {
                key: key.to_owned(),
                index,
            });
        }
    }

    /// Enters element `index` of an array.
    fn enter_index(&mut self, index: usize) {
        if self.track {
            self.path.push(PathSegment::Index(index));
        }
    }

    fn leave(&mut self) {
        if self.track {
            self.path.pop();
        }
    }

    /// The facts of the value being written.
    fn facts(&self) -> Option<&ValueFacts> {
        self.facts.and_then(|f| f.get(&self.path))
    }

    /// Writes the key of the entry value being written, whose entry is `entry_key`: its own
    /// spelling, bare or in the JSON form (spec §10.1). The empty key is written as nothing.
    fn write_key(&mut self, entry_key: &str) {
        if self.mode == Mode::RoundTrip {
            self.exact_key(entry_key, true);
            return;
        }
        let (spelling, quoted) = match self.facts() {
            Some(facts) => {
                let spelling = facts.key_spelling.as_deref().unwrap_or(entry_key);
                let quoted = facts
                    .key_quoted
                    .unwrap_or_else(|| key_needs_quoting(spelling));
                (spelling.to_owned(), quoted)
            }
            None => (entry_key.to_owned(), key_needs_quoting(entry_key)),
        };
        text::write_key(&mut self.out, &spelling, quoted);
    }

    /// The saved comments of the value being written that are attached with `placement`.
    fn comments(&self, placement: CommentPlacement) -> Vec<&'a str> {
        match self.comments.get(self.path.as_slice()) {
            Some((p, texts)) if *p == placement => texts.clone(),
            _ => Vec::new(),
        }
    }

    /// Round-trip mode: a key that reads back exactly (spec §10.8). Bare where `bare_allowed` and
    /// spec §3.1 allow it, otherwise double-quoted with the escapes of §6.1. The empty key has no
    /// form, since parsing rejects it (§3.2).
    fn exact_key(&mut self, key: &str, bare_allowed: bool) {
        if key.is_empty() {
            self.fail("the empty key, which parsing rejects (spec §3.2, §10.8)".to_owned());
        } else if bare_allowed && text::is_bare_key(key) {
            self.out.push_str(key);
        } else {
            text::write_escaped_string(&mut self.out, key);
        }
    }

    /// Round-trip mode: a scalar in a form that reads back exactly (spec §10.8). Strings of the
    /// config format may use single quotes (`config`); the other formats use double quotes. In
    /// JSON, a float and a time are JSON numbers, the time as its seconds, which read back as a
    /// float.
    fn exact_scalar(&mut self, value: &UclValue, config: bool) {
        let result = match value {
            UclValue::Integer(i) => {
                use std::fmt::Write;
                let _ = write!(self.out, "{i}");
                Ok(())
            }
            UclValue::Float(f) if self.json => {
                number::write_json_number(&mut self.out, *f, "float")
            }
            UclValue::Time(t) if self.json => number::write_json_number(&mut self.out, *t, "time"),
            UclValue::Float(f) => number::write_exact_float(&mut self.out, *f),
            UclValue::Time(t) => number::write_exact_time(&mut self.out, *t),
            UclValue::String(s) if config => text::write_exact_config_string(&mut self.out, s),
            UclValue::String(s) => text::write_exact_double_quoted(&mut self.out, s),
            UclValue::Boolean(b) => {
                self.out.push_str(if *b { "true" } else { "false" });
                Ok(())
            }
            UclValue::Null => {
                self.out.push_str("null");
                Ok(())
            }
            UclValue::Object(_) | UclValue::Array(_) => {
                unreachable!("containers are written by the format")
            }
        };
        if let Err(error) = result {
            self.fail(error);
        }
    }

    /// A scalar in the form every format shares (spec §10.2), strings in the JSON form.
    fn scalar(&mut self, value: &UclValue) {
        if self.mode == Mode::RoundTrip {
            self.exact_scalar(value, false);
            return;
        }
        match value {
            UclValue::Integer(i) => {
                use std::fmt::Write;
                let _ = write!(self.out, "{i}");
            }
            UclValue::Float(f) | UclValue::Time(f) => number::write_float(&mut self.out, *f),
            UclValue::String(s) => text::write_json_string(&mut self.out, s),
            UclValue::Boolean(b) => self.out.push_str(if *b { "true" } else { "false" }),
            UclValue::Null => self.out.push_str("null"),
            UclValue::Object(_) | UclValue::Array(_) => {
                unreachable!("containers are written by the format")
            }
        }
    }
}

#[cfg(test)]
mod tests;
