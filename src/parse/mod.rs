//! The byte-oriented parser core (clean-room work item C2).
//!
//! It reads a UCL document from bytes and produces the [`crate::value`] model, following the
//! behaviour spec in `docs/spec/` (§01–§08, §11, §12). Keys and strings must be valid UTF-8; the
//! check happens where each key or string is materialised, and a violation is an
//! [`ErrorKind::InvalidUtf8`] error (a project divergence from libucl, spec §11.3). Comments may
//! hold any bytes.
//!
//! Macros (§09) are recognised where a key could start, and their syntax is checked. Until work
//! item C3, a known macro is rejected with [`ErrorKind::Unsupported`] and an unknown one with
//! [`ErrorKind::UnknownMacro`]; a macro that §9.2 ignores at the end of input is ignored.
//!
//! This core sits next to the existing parser, which stays the default for the crate's public
//! API until the cut-over (C5).
//!
//! ```
//! use ucl_lexer::parse::Parser;
//!
//! let mut parser = Parser::new();
//! parser.register_variable("HOST", "example.org");
//! let value = parser.parse(b"server { url = \"https://$HOST/\"; port = 8080 }").unwrap();
//! let server = value.as_object().unwrap()["server"].as_object().unwrap();
//! assert_eq!(server["url"].as_str(), Some("https://example.org/"));
//! assert_eq!(server["port"].as_integer(), Some(8080));
//! ```

mod comments;
mod core;
mod error;
mod number;
mod string;
mod vars;

pub use error::{Error, ErrorKind};

use crate::error::Position;
use crate::value::{DuplicateStrategy, ParserFlags, UclValue};
use indexmap::IndexMap;
use std::fmt;
use std::path::Path;

/// The most containers (objects and arrays, the root included) that may be open at once
/// (spec §11.2).
pub const MAX_NESTING: usize = 1024;

/// A comment saved under [`ParserFlags::SAVE_COMMENTS`] (spec §12.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comment {
    /// The comment as saved. A line comment runs from `#` up to, not including, its line feed; a
    /// carriage return before the line feed is kept. A block comment runs from `/*` through its
    /// `*/` and, a quirk of the format, the one byte after it when the input has one. Bytes that
    /// are not UTF-8 are replaced with U+FFFD.
    pub text: String,
    /// Where the comment starts.
    pub position: Position,
}

/// One step of the path from the root to a value.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PathSegment {
    /// Value `index` of the entry `key` of an object; an entry holds several values when a key
    /// repeats (spec §8.2).
    Key { key: String, index: usize },
    /// Element `index` of an array.
    Index(usize),
}

/// Whether a value's comments were attached before or after it (spec §12.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommentPlacement {
    /// Comments written before the value, or between its key and the value.
    Before,
    /// Comments at the end of a container or of the input, after the value created last.
    After,
}

/// The saved comments attached to one value (spec §12.5).
///
/// Comments that are not yet attached go before the next value that is created. When a
/// container closes with its bracket, and at the end of input, they go after the value created
/// most recently, or after the root if there is none. A value has a single list: its placement
/// is that of the first comment attached, and later comments are appended to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachedComments {
    /// The value, as a path from the root; empty for the root.
    pub path: Vec<PathSegment>,
    /// Where the comments were attached.
    pub placement: CommentPlacement,
    /// Indices into [`Parser::comments`], in input order.
    pub comments: Vec<usize>,
}

/// The variable handler's signature: given the name of a braced reference `${NAME}` whose name
/// is not registered, return its value, or `None` to leave the reference as written (spec §7.7).
pub type VariableHandler = dyn FnMut(&str) -> Option<String>;

/// Parser settings and state: flags, the input unit's priority and duplicate strategy,
/// registered variables, an optional variable handler, and saved comments.
pub struct Parser {
    flags: ParserFlags,
    priority: u8,
    strategy: DuplicateStrategy,
    variables: IndexMap<String, String>,
    handler: Option<Box<VariableHandler>>,
    comments: Vec<Comment>,
    attached: Vec<AttachedComments>,
}

impl Default for Parser {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for Parser {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Parser")
            .field("flags", &self.flags)
            .field("priority", &self.priority)
            .field("strategy", &self.strategy)
            .field("variables", &self.variables)
            .field("handler", &self.handler.is_some())
            .field("comments", &self.comments.len())
            .field("attached", &self.attached.len())
            .finish()
    }
}

impl Parser {
    /// A parser with no flags, priority 0 and the `append` strategy.
    pub fn new() -> Self {
        Self {
            flags: ParserFlags::DEFAULT,
            priority: 0,
            strategy: DuplicateStrategy::Append,
            variables: IndexMap::new(),
            handler: None,
            comments: Vec::new(),
            attached: Vec::new(),
        }
    }

    /// A parser with the given flags.
    pub fn with_flags(flags: ParserFlags) -> Self {
        let mut parser = Self::new();
        parser.flags = flags;
        parser
    }

    /// The parser flags.
    pub fn flags(&self) -> ParserFlags {
        self.flags
    }

    /// Replaces the parser flags.
    pub fn set_flags(&mut self, flags: ParserFlags) -> &mut Self {
        self.flags = flags;
        self
    }

    /// Sets the priority of the document's values, kept modulo 16 (spec §8.3).
    pub fn set_priority(&mut self, priority: u8) -> &mut Self {
        self.priority = priority & crate::value::MAX_PRIORITY;
        self
    }

    /// Sets how repeated keys of the document are resolved (spec §8.4).
    pub fn set_strategy(&mut self, strategy: DuplicateStrategy) -> &mut Self {
        self.strategy = strategy;
        self
    }

    /// Registers a variable for `$NAME` and `${NAME}` references (spec §7.1). Registering a name
    /// again changes its value but not its place in the lookup order.
    pub fn register_variable(
        &mut self,
        name: impl Into<String>,
        value: impl Into<String>,
    ) -> &mut Self {
        self.variables.insert(name.into(), value.into());
        self
    }

    /// Installs a handler for braced references to names that are not registered (spec §7.7).
    pub fn set_variable_handler(
        &mut self,
        handler: impl FnMut(&str) -> Option<String> + 'static,
    ) -> &mut Self {
        self.handler = Some(Box::new(handler));
        self
    }

    /// Comments saved by the last parse under [`ParserFlags::SAVE_COMMENTS`], in input order.
    pub fn comments(&self) -> &[Comment] {
        &self.comments
    }

    /// The values the saved comments of the last successful parse are attached to. A comment
    /// attached to a value that a later repeat of its key replaced (spec §8) is in
    /// [`Parser::comments`] but in none of these.
    pub fn attached_comments(&self) -> &[AttachedComments] {
        &self.attached
    }

    /// Parses a document given as bytes rather than read from a file.
    ///
    /// Unless [`ParserFlags::NO_FILEVARS`] is set, the file variables are defined as for such a
    /// document (spec §7.8): `FILENAME` is `undef` and `CURDIR` is the process's working
    /// directory. Registered variables of the same names override them.
    pub fn parse(&mut self, input: &[u8]) -> Result<UclValue, Error> {
        let filevars = (
            "undef".to_string(),
            std::env::current_dir()
                .map(|d| d.to_string_lossy().into_owned())
                .unwrap_or_default(),
        );
        self.run(input, filevars, false)
    }

    /// Reads and parses the file at `path`.
    ///
    /// Unless [`ParserFlags::NO_FILEVARS`] is set, `FILENAME` is the file's absolute path and
    /// `CURDIR` its directory, whatever registered variables of those names say (spec §7.1).
    pub fn parse_file(&mut self, path: impl AsRef<Path>) -> Result<UclValue, Error> {
        let path = path.as_ref();
        let io_error = |e: std::io::Error| {
            Error::new(
                ErrorKind::Io {
                    message: format!("{}: {e}", path.display()),
                },
                Position::new(),
            )
        };
        let input = std::fs::read(path).map_err(io_error)?;
        let absolute = std::fs::canonicalize(path)
            .or_else(|_| std::path::absolute(path))
            .map_err(io_error)?;
        let dir = absolute
            .parent()
            .map(|d| d.to_string_lossy().into_owned())
            .unwrap_or_default();
        let filevars = (absolute.to_string_lossy().into_owned(), dir);
        self.run(&input, filevars, true)
    }

    fn run(
        &mut self,
        input: &[u8],
        (filename, curdir): (String, String),
        from_file: bool,
    ) -> Result<UclValue, Error> {
        // Lookup order (spec §7.1): the file variables first, then the registered ones.
        let mut variables: IndexMap<String, String> = IndexMap::new();
        if !self.flags.contains(ParserFlags::NO_FILEVARS) {
            variables.insert("FILENAME".to_string(), filename);
            variables.insert("CURDIR".to_string(), curdir);
        }
        for (name, value) in &self.variables {
            let is_filevar = name == "FILENAME" || name == "CURDIR";
            if from_file && is_filevar && variables.contains_key(name) {
                continue;
            }
            variables.insert(name.clone(), value.clone());
        }
        self.comments.clear();
        self.attached.clear();
        let settings = core::Settings {
            flags: self.flags,
            priority: self.priority,
            strategy: self.strategy,
        };
        let variables: Vec<(String, String)> = variables.into_iter().collect();
        let mut expander = vars::Expander::new(
            &variables,
            self.handler.as_deref_mut(),
            !self.flags.contains(ParserFlags::DISABLE_MACRO),
        );
        let sink = self
            .flags
            .contains(ParserFlags::SAVE_COMMENTS)
            .then_some(core::CommentSink {
                comments: &mut self.comments,
                attached: &mut self.attached,
            });
        core::parse_document(input, settings, &mut expander, sink)
    }
}

/// Parses `input` with default settings: no flags, no variables other than the file variables
/// of a document given as bytes (spec §7.8).
pub fn parse(input: &[u8]) -> Result<UclValue, Error> {
    Parser::new().parse(input)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::UclObject;

    fn obj(v: &UclValue) -> &UclObject {
        v.as_object().expect("object")
    }

    fn kind(input: &[u8]) -> ErrorKind {
        parse(input).expect_err("error").kind().clone()
    }

    #[test]
    fn nesting_limit_counts_the_root() {
        // spec §11.2: 1023 containers inside the root parse, 1024 are an error. The 1023 case has
        // no committed conformance case (spec README, B-20).
        for (open, close) in [("[", "]"), ("{a=", "}")] {
            let ok = format!("a = {}1{}", open.repeat(1023), close.repeat(1023));
            assert!(parse(ok.as_bytes()).is_ok(), "{open}: 1023 levels");
            let deep = format!("a = {}1{}", open.repeat(1024), close.repeat(1024));
            assert_eq!(
                kind(deep.as_bytes()),
                ErrorKind::NestingTooDeep { limit: MAX_NESTING }
            );
        }
        // Far deeper input fails the same way instead of exhausting the stack.
        assert!(matches!(
            kind("[".repeat(200_000).as_bytes()),
            ErrorKind::NestingTooDeep { .. }
        ));
    }

    #[test]
    fn root_bracket_only_after_whitespace_or_a_leading_comment_group() {
        // spec §1.1, *Quirk*
        for ok in [
            &b"\n\t [1]"[..],
            b"# c\n# d\n/* e */[1]",
            b"# c\r\n[1]",
            b"/* c */[1]",
        ] {
            assert!(
                parse(ok).unwrap().is_array(),
                "{}",
                String::from_utf8_lossy(ok)
            );
        }
        for bad in [
            &b"# header\n\n{\n \"a\": 1\n}"[..],
            b"# c\n [1]",
            b"\n# c\n{a = 1}",
            b"/* c */\n{a = 1}",
            b"/* c */ /* d */[1]",
        ] {
            assert!(
                matches!(kind(bad), ErrorKind::InvalidKey { .. }),
                "{}",
                String::from_utf8_lossy(bad)
            );
        }
        assert_eq!(obj(&parse(b"# header\n\na = 1").unwrap()).len(), 1);
    }

    #[test]
    fn hash_as_last_byte_after_leading_whitespace() {
        // spec §2.2, *Quirk*
        for bad in [
            &b"\n#"[..],
            b" #",
            b"\t#",
            b"\r#",
            b"\x0b#",
            b"# c\n #",
            b"/* c */\n#",
        ] {
            assert_eq!(kind(bad), ErrorKind::HashAtEnd, "{bad:?}");
        }
        for ok in [
            &b"#"[..],
            b"# c\n#",
            b"/* c */#",
            b"\n#\n",
            b"\n# ",
            b" # c\n#",
            b"a = 1\n #",
        ] {
            assert!(parse(ok).is_ok(), "{ok:?}");
        }
    }

    #[test]
    fn root_forms() {
        assert_eq!(parse(b"").unwrap(), UclValue::Object(UclObject::new()));
        assert_eq!(
            parse(b"  # only a comment\n").unwrap(),
            UclValue::Object(UclObject::new())
        );
        let v = parse(b"[1, 2] ignored").unwrap();
        assert_eq!(v.as_array().unwrap().len(), 2);
        let v = parse(b"{ a = 1 } b = 2").unwrap();
        assert_eq!(obj(&v).keys().collect::<Vec<_>>(), vec!["a"]);
        assert_eq!(kind(b"a = 1 }"), ErrorKind::UnmatchedClose { found: '}' });
        assert_eq!(kind(b"{ a = 1"), ErrorKind::UnterminatedObject);
        assert_eq!(kind(b"a = [1"), ErrorKind::UnterminatedArray);
    }

    #[test]
    fn sections_and_left_open_sections() {
        let v = parse(b"a b c { d = 1 }").unwrap();
        let d = &obj(&obj(&obj(&v)["a"])["b"])["c"];
        assert_eq!(obj(d)["d"], UclValue::Integer(1));
        // §3.4 quirk: a bracket inside a quoted word leaves the section objects open, so later
        // entries go into the innermost one.
        let v = parse(b"x = 1\nc \"x{y\" z\nd = 1").unwrap();
        let c = obj(&obj(&v)["c"]);
        assert_eq!(c.keys().collect::<Vec<_>>(), vec!["x{y", "d"]);
        assert_eq!(
            kind(b"k { c \"x{y\" z\n}"),
            ErrorKind::UnmatchedClose { found: '}' }
        );
        // They close with the next bracketed container that closes in them, all together.
        let keys = |v: &UclValue| obj(v).keys().cloned().collect::<Vec<_>>();
        let v = parse(b"x \"y{\" z\na { b = 1 }\nd = 1").unwrap();
        assert_eq!(keys(&v), ["x", "d"]);
        assert_eq!(keys(&obj(&v)["x"]), ["y{", "a"]);
        let v = parse(b"c \"x{y\" z\nd \"x{y\" w\ne = [1]\ng = 1").unwrap();
        assert_eq!(keys(&v), ["c", "g"]);
        let v = parse(b"k { c \"x{y\" z\na { b = 1 }\n}\nm = 1").unwrap();
        assert_eq!(keys(&v), ["k", "m"]);
        let v = parse(b"c \"x{y\" z\nd { e { f = 1 } g = 2 }\nh = 1").unwrap();
        assert_eq!(keys(&obj(&v)["c"]), ["x{y", "d"]);
        assert_eq!(
            kind(b"x \"y{\" z\na { b = 1 }\n}"),
            ErrorKind::UnmatchedClose { found: '}' }
        );
        // VT and FF between section names are whitespace (QUESTIONS.md #22).
        for input in [
            &b"a \x0cb { c = 1 }"[..],
            b"a \x0b\x0c /* x */ b { c = 1 }",
            b"a \x0cb = \x0cc { d = 1 }",
        ] {
            let v = parse(input).unwrap();
            assert!(obj(&obj(&v)["a"])["b"].is_object(), "{input:?}");
        }
        let v = parse(b"a \x0c# c\nb { c = 1 }").unwrap();
        assert_eq!(obj(&v)["a"].as_str(), Some("b { c = 1 }"));
        assert!(parse(b"a \x0c{ b = 1 }").is_err());
        // After a separator and a line break, the next name may come on a later line.
        let v = parse(b"c \"x{\" =\n# c\n d {}").unwrap();
        assert!(obj(&obj(&obj(&v)["c"])["x{"])["d"].is_object());
        let v = parse(b"c \"x{\" =\x0b").unwrap();
        assert!(obj(&obj(&obj(&v)["c"])["x{"]).is_empty());
        assert_eq!(kind(b"c \"x{\" = "), ErrorKind::MissingValue);
        assert_eq!(kind(b"c \"x{\" = # c\n"), ErrorKind::MissingValue);
        assert_eq!(kind(b"c \"x{\" =\n #"), ErrorKind::HashAtEnd);
    }

    #[test]
    fn first_array_element_after_a_comment_keeps_its_whitespace() {
        // Oracle behaviour, QUESTIONS.md #9.
        let v = parse(b"[ /* c */ 1, /* d */ 2]").unwrap();
        assert_eq!(
            v.as_array().unwrap(),
            &vec![UclValue::String(" 1".into()), UclValue::Integer(2)]
        );
        assert_eq!(parse(b"[ /* c */]").unwrap(), UclValue::Array(Vec::new()));
        assert_eq!(kind(b"[ /* c */ ]"), ErrorKind::MissingValue);
    }

    #[test]
    fn value_on_a_following_line() {
        let v = parse(b"a =\n\n  v\nb # c\n= 1\nc \n{ x = 1 }").unwrap();
        let o = obj(&v);
        assert_eq!(o["a"], UclValue::String("v".into()));
        assert_eq!(o["b"], UclValue::Integer(1));
        assert!(o["c"].is_object());
        assert_eq!(parse(b"a = \n").unwrap(), parse(b"a = null").unwrap());
        assert_eq!(kind(b"a"), ErrorKind::MissingValue);
        assert_eq!(
            kind(b"a\nb = 1"),
            ErrorKind::InvalidKey { found: Some('\n') }
        );
        assert_eq!(kind(b"a =\n/* c */\nb = 1"), ErrorKind::MissingValue);
    }

    #[test]
    fn errors_carry_positions() {
        let e = parse(b"a = 1\nb = \"open").unwrap_err();
        assert_eq!(e.kind(), &ErrorKind::UnterminatedString);
        assert_eq!((e.position().line, e.position().column), (2, 5));
        let e = parse(b"a = 1\n\"\" = 2").unwrap_err();
        assert_eq!(e.kind(), &ErrorKind::EmptyKey);
        assert_eq!(e.position().offset, 6);
        assert!(e.to_string().contains("line 2, column 1"));
    }

    #[test]
    fn utf8_is_required_in_keys_and_strings_only() {
        assert_eq!(kind(b"\xff = 1"), ErrorKind::InvalidUtf8);
        assert_eq!(kind(b"a = \"\xc3\""), ErrorKind::InvalidUtf8);
        assert_eq!(kind(b"a = x\xff"), ErrorKind::InvalidUtf8);
        assert_eq!(kind(br#"a = "\uD800""#), ErrorKind::InvalidUtf8);
        let v = parse(b"# \xff comment\na = 1\n/* \xfe */").unwrap();
        assert_eq!(obj(&v)["a"], UclValue::Integer(1));
    }

    #[test]
    fn macros_at_the_end_of_input() {
        // spec §9.2, *Quirk*; the finer points follow the oracle (QUESTIONS.md #18).
        for ok in [
            &b"a = 1\n.foo"[..],
            b".",
            b".foo;",
            b".foo#",
            b".priority\n",
            b".include # c\n",
            b".include\t# c",
            b".include /* d */",
            b".include\n# c\n# d\n",
            b".include\n# c\n#",
        ] {
            assert!(parse(ok).is_ok(), "{}", String::from_utf8_lossy(ok));
        }
        // Section objects around an ignored macro close quietly; braces do not.
        let v = parse(b"\"a\".51\"x{y\"z").unwrap();
        assert!(obj(&obj(&v)["a"]).is_empty());
        assert_eq!(kind(b"a { .foo"), ErrorKind::UnterminatedObject);
        assert_eq!(
            kind(b".foo\n"),
            ErrorKind::UnknownMacro { name: "foo".into() }
        );
        assert_eq!(
            kind(b"a .b {c = 1}"),
            ErrorKind::UnknownMacro { name: "b".into() }
        );
        assert_eq!(kind(b".include\n#"), ErrorKind::HashAtEnd);
        assert_eq!(kind(b".priority(priority=2)"), ErrorKind::MissingValue);
        assert_eq!(
            kind(b".priority(priority=2) # c\n"),
            ErrorKind::MissingValue
        );
        assert_eq!(
            kind(b".include(try=true \"x\"\nk = 1\n"),
            ErrorKind::UnterminatedArguments
        );
        // A `"` directly after a `\` does not end a quoted part (QUESTIONS.md #18).
        for input in [&br#".priority(p="a\") 1"#[..], br#".priority(p="a\\") 1"#] {
            assert_eq!(kind(input), ErrorKind::UnterminatedArguments, "{input:?}");
        }
        assert!(parse(b".priority(p=\"(\") 1").unwrap_err().is_unsupported());
        assert!(
            parse(b".include(p=\")\") \"x\"")
                .unwrap_err()
                .is_unsupported()
        );
        assert!(parse(b".include # c\n\n").unwrap_err().is_unsupported());
        assert_eq!(
            Parser::with_flags(ParserFlags::DISABLE_MACRO)
                .parse(b"a = 1\n.foo")
                .unwrap_err()
                .kind(),
            &ErrorKind::MacrosDisabled
        );
    }

    #[test]
    fn key_lowercase_before_escapes() {
        // spec §12.1, *Quirk*; `\UZZZZ` follows the oracle (QUESTIONS.md #20).
        let v = Parser::with_flags(ParserFlags::KEY_LOWERCASE)
            .parse(b"\"\\u0041\" = 1\n\"\\N\" = 2\n\"\\U0042\" = 3\n\"\\\\N\" = 4\n\"A\\tB\" = 5\n\"\\UZZZZ\" = 6\nC = 7")
            .unwrap();
        assert_eq!(
            obj(&v).keys().collect::<Vec<_>>(),
            ["A", "\n", "B", "\\n", "a\tb", "\0", "c"]
        );
        // Keys are compared without ASCII case; an entry keeps its first spelling.
        let v = Parser::with_flags(ParserFlags::KEY_LOWERCASE)
            .parse(b"\"\\u0041\\u0042\" = 1\nab = 2\n\"aB\" = 3\nC = 4\n\"\\u0043\" = 5")
            .unwrap();
        let o = obj(&v);
        assert_eq!(o.keys().collect::<Vec<_>>(), ["AB", "c"]);
        assert_eq!(
            (o.entry("AB").unwrap().len(), o.entry("c").unwrap().len()),
            (3, 2)
        );
        assert_eq!(
            Parser::with_flags(ParserFlags::KEY_LOWERCASE)
                .parse(b"\"\\uZZZZ\" = 1")
                .unwrap_err()
                .kind(),
            &ErrorKind::InvalidUnicodeEscape
        );
    }

    #[test]
    fn macros_are_recognised_but_not_supported() {
        let e = parse(b".include \"x.conf\"").unwrap_err();
        assert!(e.is_unsupported());
        assert_eq!(
            kind(b".unknown x"),
            ErrorKind::UnknownMacro {
                name: "unknown".into()
            }
        );
        assert_eq!(
            Parser::with_flags(ParserFlags::DISABLE_MACRO)
                .parse(b".include \"x\"")
                .unwrap_err()
                .kind(),
            &ErrorKind::MacrosDisabled
        );
        // Macros are not recognised in arrays or as values (§9.1).
        let v = parse(b"k = .include x\narr = [.include y]").unwrap();
        assert_eq!(obj(&v)["k"], UclValue::String(".include x".into()));
    }

    #[test]
    fn duplicate_strategies_and_priorities() {
        let v = parse(b"a = 1\na = 2").unwrap();
        assert_eq!(obj(&v).entry("a").unwrap().len(), 2);

        let mut p = Parser::new();
        p.set_strategy(DuplicateStrategy::Error);
        let e = p.parse(b"a = 1\nb { a = 2 }\na = 3").unwrap_err();
        assert_eq!(e.kind(), &ErrorKind::DuplicateKey { key: "a".into() });
        assert_eq!(e.position().line, 3);

        let mut p = Parser::new();
        p.set_strategy(DuplicateStrategy::Merge).set_priority(3);
        let v = p.parse(b"a { x = 1 }\na { y = 2 }").unwrap();
        let entry = obj(&v).entry("a").unwrap();
        assert_eq!(entry.len(), 1);
        assert_eq!(entry.slots()[0].priority(), 3);
        assert_eq!(obj(entry.first()).len(), 2);

        let v = Parser::with_flags(ParserFlags::NO_IMPLICIT_ARRAYS | ParserFlags::KEY_LOWERCASE)
            .parse(b"A { x = 1 }\na { y = 2 }")
            .unwrap();
        let items = obj(&v)["a"].as_array().unwrap();
        assert_eq!(items.len(), 2);
        assert!(items.iter().all(UclValue::is_object));
    }

    #[test]
    fn variables_file_variables_and_handler() {
        let mut p = Parser::new();
        p.register_variable("FILENAME", "mine");
        p.set_variable_handler(|name| (name == "H").then(|| "[h]".to_string()));
        let v = p.parse(b"a = \"$FILENAME ${H} ${X}\"").unwrap();
        assert_eq!(obj(&v)["a"], UclValue::String("mine [h] ${X}".into()));

        let v = parse(b"a = $FILENAME").unwrap();
        assert_eq!(obj(&v)["a"], UclValue::String("undef".into()));
        let v = Parser::with_flags(ParserFlags::NO_FILEVARS)
            .parse(b"a = $FILENAME")
            .unwrap();
        assert_eq!(obj(&v)["a"], UclValue::String("$FILENAME".into()));

        let dir = std::env::temp_dir().join(format!("ucl-parse-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("vars.conf");
        std::fs::write(&file, "f = \"$FILENAME\"\nd = \"$CURDIR\"\n").unwrap();
        let mut p = Parser::new();
        p.register_variable("FILENAME", "ignored for files");
        let v = p.parse_file(&file).unwrap();
        let expected = std::fs::canonicalize(&file).unwrap();
        assert_eq!(
            obj(&v)["f"].as_str(),
            Some(expected.to_string_lossy().as_ref())
        );
        assert_eq!(
            obj(&v)["d"].as_str(),
            Some(expected.parent().unwrap().to_string_lossy().as_ref())
        );
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(matches!(
            Parser::new()
                .parse_file(dir.join("missing"))
                .unwrap_err()
                .kind(),
            ErrorKind::Io { .. }
        ));
    }

    #[test]
    fn errors_convert_into_ucl_error() {
        fn run() -> Result<UclValue, crate::UclError> {
            Ok(parse(b"a = ")?)
        }
        let err = run().unwrap_err();
        assert!(matches!(&err, crate::UclError::Syntax(e) if e.kind() == &ErrorKind::MissingValue));
    }

    #[test]
    fn saved_comments_have_positions() {
        let mut p = Parser::with_flags(ParserFlags::SAVE_COMMENTS);
        let v = p.parse(b"# one\na = 1 /* two\n */\nb = 2 # three").unwrap();
        assert_eq!(obj(&v).len(), 2);
        let found: Vec<_> = p
            .comments()
            .iter()
            .map(|c| (c.text.as_str(), c.position.line, c.position.column))
            .collect();
        assert_eq!(
            found,
            vec![("# one", 1, 1), ("/* two\n */\n", 2, 7), ("# three", 4, 7)]
        );
        assert!(Parser::new().comments().is_empty());
        // No byte is added after a block comment that ends the input (spec §12.5, *Uncertain*).
        p.parse(b"a = 1 /* c */").unwrap();
        assert_eq!(p.comments()[0].text, "/* c */");
    }

    /// The attached comments of `input`, as `(path, "c" or "ca", texts)`.
    fn attached(
        input: &[u8],
        flags: ParserFlags,
        strategy: DuplicateStrategy,
    ) -> Vec<(String, &'static str, Vec<String>)> {
        let mut p = Parser::with_flags(flags | ParserFlags::SAVE_COMMENTS);
        p.set_strategy(strategy);
        p.parse(input).unwrap();
        let mut out: Vec<_> = p
            .attached_comments()
            .iter()
            .map(|g| {
                let path = g
                    .path
                    .iter()
                    .map(|s| match s {
                        PathSegment::Key { key, index } => format!("{key}[{index}]"),
                        PathSegment::Index(i) => format!("[{i}]"),
                    })
                    .collect::<Vec<_>>()
                    .join("/");
                let placement = match g.placement {
                    CommentPlacement::Before => "c",
                    CommentPlacement::After => "ca",
                };
                let texts = g
                    .comments
                    .iter()
                    .map(|&i| p.comments()[i].text.clone())
                    .collect();
                (path, placement, texts)
            })
            .collect();
        out.sort();
        out
    }

    fn row(
        path: &str,
        placement: &'static str,
        texts: &[&str],
    ) -> (String, &'static str, Vec<String>) {
        (
            path.to_string(),
            placement,
            texts.iter().map(|t| t.to_string()).collect(),
        )
    }

    #[test]
    fn comments_attach_to_values() {
        // spec §12.5; expected results from oracle runs (QUESTIONS.md #17).
        let none = ParserFlags::DEFAULT;
        let append = DuplicateStrategy::Append;
        assert_eq!(
            attached(
                b"# c1\n# c2\na = 1 # c3\nb = /* c4 */ 2\nc = [ 1, # c5\n 2 ]\n",
                none,
                append
            ),
            [
                row("a[0]", "c", &["# c1", "# c2"]),
                row("b[0]", "c", &["# c3", "/* c4 */ "]),
                row("c[0]/[1]", "c", &["# c5"]),
            ]
        );
        // After-comments join a value's list of before-comments.
        assert_eq!(
            attached(b"# c\na = 1 # d\n", none, append),
            [row("a[0]", "c", &["# c", "# d"])]
        );
        assert_eq!(
            attached(b"a = [ # c\n1 ] # d\n", none, append),
            [row("a[0]/[0]", "c", &["# c", "# d"])]
        );
        assert_eq!(
            attached(b"x { a = 1 # d\n} # e", none, append),
            [row("x[0]/a[0]", "ca", &["# d", "# e"])]
        );
        // Section objects and left-open objects: the outermost counts as the latest value.
        assert_eq!(
            attached(b"a b { c = 1 # z\n} # y", none, append),
            [
                row("a[0]", "ca", &["# y"]),
                row("a[0]/b[0]/c[0]", "ca", &["# z"])
            ]
        );
        assert_eq!(
            attached(b"x \"y{\" z\na { b = 1 }\n# c", none, append),
            [row("x[0]", "ca", &["# c"])]
        );
        assert_eq!(
            attached(b"# c\n{ }", none, append),
            [row("", "ca", &["# c"])]
        );
        assert!(attached(b"{ } # c", none, append).is_empty());
        // A `#` that is the last byte is saved only right after another comment.
        assert!(attached(b"a = 1\n#", none, append).is_empty());
        assert!(attached(b"a = [1];#", none, append).is_empty());
        assert_eq!(
            attached(b"a = 1\n# c\n#", none, append),
            [row("a[0]", "ca", &["# c", "#"])]
        );
        assert_eq!(attached(b"#", none, append), [row("", "ca", &["#"])]);
    }

    #[test]
    fn comments_follow_values_that_the_duplicate_rules_move() {
        // Expected results from oracle runs (QUESTIONS.md #17).
        let nia = ParserFlags::NO_IMPLICIT_ARRAYS;
        let none = ParserFlags::DEFAULT;
        assert_eq!(
            attached(b"# c\na = 1\na = 2\n# e", nia, DuplicateStrategy::Append),
            [
                row("a[0]/[0]", "c", &["# c"]),
                row("a[0]/[1]", "ca", &["# e"])
            ]
        );
        assert_eq!(
            attached(
                b"a = 1\n# c\na = 2\n# d\na = 3",
                nia,
                DuplicateStrategy::Append
            ),
            [
                row("a[0]/[1]", "c", &["# c"]),
                row("a[0]/[2]", "c", &["# d"])
            ]
        );
        assert_eq!(
            attached(b"# c\na = 1\n# d\na = 2", none, DuplicateStrategy::Rewrite),
            [row("a[0]", "c", &["# d"])]
        );
        assert_eq!(
            attached(
                b"# c\na { x = 1 }\n# d\na { y = 2 # e\n}",
                none,
                DuplicateStrategy::Merge
            ),
            [
                row("a[0]", "c", &["# c", "# d"]),
                row("a[0]/y[0]", "ca", &["# e"])
            ]
        );
        assert_eq!(
            attached(b"a { x = 1 }\na = 5\n# e", none, DuplicateStrategy::Merge),
            [row("a[0]", "ca", &["# e"])]
        );
        // The scalar that takes a container's place under `merge` keeps its comments.
        assert_eq!(
            attached(
                b"# c\nk { m = 1 }\n# d\nk = 1",
                none,
                DuplicateStrategy::Merge
            ),
            [row("k[0]", "c", &["# c", "# d"])]
        );
        // Comments before a value on a following line attach after it is created.
        assert_eq!(
            attached(b"c =\n#z\nb\nd = 1", none, DuplicateStrategy::Append),
            [row("d[0]", "c", &["#z"])]
        );
        assert_eq!(
            attached(b"# p\na =\n# c\nv", none, DuplicateStrategy::Append),
            [row("a[0]", "c", &["# p", "# c"])]
        );
        assert_eq!(
            attached(b"a =\n# c\n", none, DuplicateStrategy::Append),
            [row("a[0]", "ca", &["# c"])]
        );
    }
}
