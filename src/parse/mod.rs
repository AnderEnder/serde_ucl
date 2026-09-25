//! The byte-oriented parser core (clean-room work item C2).
//!
//! It reads a UCL document from bytes and produces the [`crate::value`] model, following the
//! behaviour spec in `docs/spec/` (§01–§09, §11, §12). Keys and strings must be valid UTF-8; the
//! check happens where each key or string is materialised, and a violation is an
//! [`ErrorKind::InvalidUtf8`] error (a project divergence from libucl, spec §11.3). Comments may
//! hold any bytes.
//!
//! Macros (§09) are recognised where a key could start: `.priority` (§9.5), `.inherit` (§9.7),
//! `.include` and `.try_include` (§9.4) with their parameters, glob patterns and search paths,
//! and `.load` (§9.6) when the crate is built with its `load` feature (off by default). The
//! project never verifies signatures, so `.includes` and `sign=true` are rejected with
//! [`ErrorKind::Unsupported`], and it never fetches URLs. An unknown macro is
//! [`ErrorKind::UnknownMacro`]; a macro that §9.2 ignores at the end of input is ignored.
//!
//! Files come from a [`Loader`]. A parser's default loader is an empty [`MemoryLoader`], so a
//! default parser reads no files: an include macro or `.load` in its input finds nothing and
//! behaves as §9.4 and §9.6 describe for a missing file (project decision, WORKLIST C5 decision
//! 1). [`FsLoader`] (Cargo feature `fs`, on by default) reads the filesystem; set it with
//! [`Parser::set_loader`] or [`ParserBuilder::with_loader`]. Relative paths resolve against the
//! parser's base directory ([`Parser::set_base_dir`]); without one, against the directory of the
//! file given to [`Parser::parse_file`], or for a document given as bytes against the loader's
//! current directory. They never resolve against the directory of an included file (§9.3).
//!
//! Where libucl stops parsing silently at a `.try_include` that finds no usable file, or at an
//! `.include` of a glob pattern that matches nothing (§9.4), the parser returns an error of kind
//! [`ErrorKind::Stopped`], from which [`Error::partial`] gives the entries parsed before the
//! stop.
//!
//! The crate's serde entry points ([`crate::from_str`], [`crate::from_slice`],
//! [`crate::from_reader`], [`crate::from_file`]) parse with a default [`Parser`]. Build one with
//! other settings through its setters or [`ParserBuilder`], parse to a [`UclValue`], and
//! deserialize that with [`crate::from_value`].
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

mod builder;
mod comments;
mod core;
mod error;
pub(crate) mod facts;
mod glob;
mod include;
mod loader;
mod macros;
mod number;
mod string;
pub(crate) mod tree;
mod vars;

pub use builder::ParserBuilder;
pub use error::{Error, ErrorKind};
pub use facts::{OutputFacts, ValueFacts};
#[cfg(feature = "fs")]
pub use loader::FsLoader;
pub use loader::{FileKind, Loader, MemoryLoader};

use crate::error::Position;
use crate::value::{DuplicateStrategy, ParserFlags, UclValue};
use indexmap::IndexMap;
use std::cell::OnceCell;
use std::fmt;
use std::path::{Path, PathBuf};

/// The most containers (objects and arrays, the root included) that may be open at once
/// (spec §11.2).
pub const MAX_NESTING: usize = 1024;

/// The most input units (spec §9.4) that may be open at once, the main document included: 15
/// files may be included inside one another.
pub const MAX_INCLUDE_DEPTH: usize = 16;

/// The most macro argument documents (spec §9.2) that may be open inside one another, the
/// document holding the outermost macro included. libucl has no such limit; the core has one so
/// that nested arguments cannot exhaust the stack (QUESTIONS.md #26).
pub const MAX_ARGUMENT_DEPTH: usize = 64;

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
///
/// Paths are ordered segment by segment, so the paths inside a value follow its own path
/// directly.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
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
/// registered variables, an optional variable handler, the loader and base directory for the
/// include macros, and saved comments.
pub struct Parser {
    flags: ParserFlags,
    priority: u8,
    strategy: DuplicateStrategy,
    variables: IndexMap<String, String>,
    handler: Option<Box<VariableHandler>>,
    loader: Box<dyn Loader>,
    base_dir: Option<PathBuf>,
    comments: Vec<Comment>,
    attached: comments::CommentGroups,
    /// [`Parser::attached_comments`], written out from `attached` when first asked for.
    attached_paths: OnceCell<Vec<AttachedComments>>,
    facts: OutputFacts,
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
            .field("base_dir", &self.base_dir)
            .field("comments", &self.comments.len())
            .field("attached", &self.attached.len())
            .finish()
    }
}

impl Parser {
    /// A parser with no flags, priority 0, the `append` strategy, no registered variables, no
    /// variable handler, no base directory, and an empty [`MemoryLoader`] as its loader, so that
    /// it reads no files (WORKLIST C5 decision 1). Set [`FsLoader`] with [`Parser::set_loader`]
    /// to read the filesystem.
    pub fn new() -> Self {
        let loader: Box<dyn Loader> = Box::new(MemoryLoader::new());
        Self {
            flags: ParserFlags::DEFAULT,
            priority: 0,
            strategy: DuplicateStrategy::Append,
            variables: IndexMap::new(),
            handler: None,
            loader,
            base_dir: None,
            comments: Vec::new(),
            attached: comments::CommentGroups::default(),
            attached_paths: OnceCell::new(),
            facts: OutputFacts::new(),
        }
    }

    /// A [`ParserBuilder`], which starts from the settings of [`Parser::new`].
    pub fn builder() -> ParserBuilder {
        ParserBuilder::new()
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

    /// Sets the priority of the document's values, kept modulo 16 (spec §8.3). A `.priority`
    /// macro changes it for the values after it (spec §9.5).
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

    /// Sets where [`Parser::parse_file`], `.include`, `.try_include` and `.load` read files from.
    /// The default is an empty [`MemoryLoader`], which holds no files.
    pub fn set_loader(&mut self, loader: impl Loader + 'static) -> &mut Self {
        self.loader = Box::new(loader);
        self
    }

    /// Sets the directory that relative paths resolve against: paths in the include macros and
    /// `.load` (spec §9.3), and a relative path given to [`Parser::parse_file`]. It is also
    /// `CURDIR` for a document given as bytes and for macro argument lists (spec §7.8, §9.2).
    ///
    /// Without one, relative paths in a file given to [`Parser::parse_file`], and in the files it
    /// includes, resolve against that file's directory, which is then also `CURDIR` in macro
    /// argument lists (WORKLIST C5 decision 1; libucl uses the process's working directory).
    /// Everything else uses the loader's [`Loader::current_dir`]: `/` for a [`MemoryLoader`]
    /// unless it was changed, the process's working directory for [`FsLoader`].
    pub fn set_base_dir(&mut self, dir: impl Into<PathBuf>) -> &mut Self {
        self.base_dir = Some(dir.into());
        self
    }

    /// The base directory set with [`Parser::set_base_dir`].
    pub fn base_dir(&self) -> Option<&Path> {
        self.base_dir.as_deref()
    }

    /// Comments saved by the last parse under [`ParserFlags::SAVE_COMMENTS`], in the order they
    /// were read, included files among them. A comment's position is in the input it was read
    /// from.
    pub fn comments(&self) -> &[Comment] {
        &self.comments
    }

    /// The values the saved comments of the last successful parse, or of a parse that stopped
    /// silently, are attached to. A comment attached to a value that a later repeat of its key
    /// replaced (spec §8) is in [`Parser::comments`] but in none of these.
    pub fn attached_comments(&self) -> &[AttachedComments] {
        self.attached_paths.get_or_init(|| self.attached.attached())
    }

    /// What the output formats need to know about the values of the last successful parse, or
    /// of a parse that stopped silently, beyond the values themselves (spec §10.1): which strings
    /// were single-quoted or heredocs, and how keys were written.
    pub fn output_facts(&self) -> &OutputFacts {
        &self.facts
    }

    /// An emitter for `format` that uses the output facts of the last parse, so that the value it
    /// returned is written as libucl writes it (spec §10). Saved comments are written only when
    /// asked for with [`crate::emit::Emitter::with_comments`].
    pub fn emitter(&self, format: crate::emit::Format) -> crate::emit::Emitter<'_> {
        crate::emit::Emitter::new(format).with_facts(&self.facts)
    }

    /// The directory that relative paths in a document given as bytes, and a relative path given
    /// to [`Parser::parse_file`], resolve against: the base directory, or the loader's current
    /// directory.
    fn base(&self) -> PathBuf {
        match &self.base_dir {
            Some(dir) => dir.clone(),
            None => self
                .loader
                .current_dir()
                .unwrap_or_else(|_| PathBuf::from(".")),
        }
    }

    /// Parses a document given as bytes rather than read from a file.
    ///
    /// Unless [`ParserFlags::NO_FILEVARS`] is set, the file variables are defined as for such a
    /// document (spec §7.8): `FILENAME` is `undef` and `CURDIR` is the base directory
    /// ([`Parser::set_base_dir`]), or without one the loader's current directory, which is `/`
    /// for the default loader. Registered variables of the same names override them.
    pub fn parse(&mut self, input: &[u8]) -> Result<UclValue, Error> {
        let base = self.base();
        let filevars = (!self.flags.contains(ParserFlags::NO_FILEVARS))
            .then(|| ("undef".to_string(), base.to_string_lossy().into_owned()));
        self.run(input, base, filevars, None)
    }

    /// Reads the file at `path` through the loader and parses it. A relative `path` resolves
    /// against the base directory, or without one the loader's current directory. The default
    /// loader holds no files: set [`FsLoader`] to read the filesystem, or use
    /// [`crate::from_file`].
    ///
    /// Relative paths in the file's include macros resolve against the base directory, or
    /// without one against the directory of the file (its canonical path's parent), also inside
    /// included files (WORKLIST C5 decision 1).
    ///
    /// `FILENAME` is the file's canonical path and `CURDIR` its directory, whatever registered
    /// variables of those names say (spec §7.1), and also under [`ParserFlags::NO_FILEVARS`]
    /// (project decision; libucl's function for parsing a file does the same, spec §12.7).
    pub fn parse_file(&mut self, path: impl AsRef<Path>) -> Result<UclValue, Error> {
        let (canonical, input) = self.read_file(path.as_ref()).map_err(|(path, e)| {
            Error::new(
                ErrorKind::Io {
                    message: format!("{}: {e}", path.display()),
                },
                Position::new(),
            )
        })?;
        self.parse_read_file(canonical, &input)
    }

    /// The first half of [`Parser::parse_file`]: `path` resolved against the base directory, made
    /// canonical and read through the loader. On failure, the resolved path and the I/O error.
    pub(crate) fn read_file(
        &self,
        path: &Path,
    ) -> Result<(PathBuf, Vec<u8>), (PathBuf, std::io::Error)> {
        let path = self.base().join(path);
        let canonical = match self.loader.canonicalize(&path) {
            Ok(canonical) => canonical,
            Err(e) => return Err((path, e)),
        };
        match self.loader.read(&canonical) {
            Ok(input) => Ok((canonical, input)),
            Err(e) => Err((path, e)),
        }
    }

    /// The second half of [`Parser::parse_file`]: parses `input`, the contents of the file whose
    /// canonical path is `canonical`.
    pub(crate) fn parse_read_file(
        &mut self,
        canonical: PathBuf,
        input: &[u8],
    ) -> Result<UclValue, Error> {
        let dir = canonical
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default();
        let base = self.base_dir.clone().unwrap_or_else(|| dir.clone());
        let filevars = Some((
            canonical.to_string_lossy().into_owned(),
            dir.to_string_lossy().into_owned(),
        ));
        self.run(input, base, filevars, Some(canonical))
    }

    /// `filevars` are `FILENAME` and `CURDIR`, if defined; `main` is the canonical path of a
    /// document read from a file.
    fn run(
        &mut self,
        input: &[u8],
        base: PathBuf,
        filevars: Option<(String, String)>,
        main: Option<PathBuf>,
    ) -> Result<UclValue, Error> {
        let from_file = main.is_some();
        // Lookup order (spec §7.1): the file variables first, then the registered ones.
        let mut variables: IndexMap<String, String> = IndexMap::new();
        if let Some((filename, curdir)) = filevars {
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
        self.attached = comments::CommentGroups::default();
        self.attached_paths = OnceCell::new();
        self.facts.clear();
        let settings = core::Settings {
            flags: self.flags,
            priority: self.priority,
            strategy: self.strategy,
        };
        let variables: Vec<(String, String)> = variables.into_iter().collect();
        let mut expander = vars::Expander::new(
            variables,
            self.handler.as_deref_mut(),
            !self.flags.contains(ParserFlags::DISABLE_MACRO),
        );
        let mut includes = include::Includes::new(&*self.loader, base, main);
        let sink = self
            .flags
            .contains(ParserFlags::SAVE_COMMENTS)
            .then_some(core::CommentSink {
                comments: &mut self.comments,
                attached: &mut self.attached,
            });
        core::parse_document(
            input,
            settings,
            &mut expander,
            &mut includes,
            sink,
            Some(&mut self.facts),
        )
    }
}

/// Parses `input` with default settings ([`Parser::new`]): no flags, no variables other than the
/// file variables of a document given as bytes (spec §7.8), and no file access.
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
    fn element_after_a_vt_or_ff_is_read_like_the_first() {
        // spec §1.5, *Quirk* (QUESTIONS.md #49); expected results from oracle runs.
        let s = |t: &str| UclValue::String(t.into());
        let i = UclValue::Integer;
        for (input, expected) in [
            (&b"[1, \x0b# d\n 2]"[..], vec![i(1), s(" 2")]),
            (b"[1, \x0b/* d */ 2]", vec![i(1), s(" 2")]),
            (b"[1, \x0c/* d */2]", vec![i(1), i(2)]),
            (b"[1, \x0b 2]", vec![i(1), i(2)]),
            (b"[1,\x0b/* d */\x0b 2]", vec![i(1), s("\x0b 2")]),
            (
                b"[{x=1}\x0b/* d */ 2]",
                vec![
                    UclValue::Object([("x", i(1))].into_iter().collect()),
                    s(" 2"),
                ],
            ),
            (b"[1\n\x0b# d\n]", vec![i(1)]),
            (b"[1\n\x0b]", vec![i(1)]),
            (b"[1\n/* d */\n]", vec![i(1)]),
        ] {
            assert_eq!(
                parse(input).unwrap(),
                UclValue::Array(expected),
                "{}",
                String::from_utf8_lossy(input)
            );
        }
        for bad in [&b"[1\n\x0b/* d */\n]"[..], b"[1\n\x0c/* d */\n]"] {
            assert_eq!(kind(bad), ErrorKind::MissingValue, "{bad:?}");
        }
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
        // A last-byte `#` there ends an empty VALUE, the empty path (QUESTIONS.md #23).
        assert!(
            matches!(kind(b".include\n#"), ErrorKind::FileNotFound { path } if path.is_empty())
        );
        assert_eq!(kind(b".priority\n#"), ErrorKind::InvalidPriority);
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
        assert!(parse(b".priority(p=\"(\") 1").is_ok());
        assert!(matches!(
            kind(b".include(t=\")\") \"no such file\""),
            ErrorKind::FileNotFound { .. }
        ));
        assert!(matches!(
            kind(b".include # c\n\n"),
            ErrorKind::FileNotFound { path } if path.is_empty()
        ));
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
    fn key_lowercase_replacement_takes_the_new_spelling() {
        // Oracle runs (QUESTIONS.md #27): a value that replaces all of an entry's values gives
        // the entry its key's spelling, in place; other repeats keep the first spelling.
        let keys = |input: &[u8], strategy: DuplicateStrategy, flags: ParserFlags| {
            let mut p = Parser::with_flags(ParserFlags::KEY_LOWERCASE | flags);
            p.set_strategy(strategy);
            let v = p.parse(input).unwrap();
            obj(&v).keys().cloned().collect::<Vec<_>>()
        };
        let (append, none) = (DuplicateStrategy::Append, ParserFlags::DEFAULT);
        let nia = ParserFlags::NO_IMPLICIT_ARRAYS;
        for (input, strategy, flags, expected) in [
            (
                &b"x = 1\na = 6\n.priority 3\n\"\\u0041\" = 3\ny = 1"[..],
                append,
                none,
                ["x", "A", "y"],
            ),
            (
                b"x = 1\na = 1\n\"\\u0041\" = 2\ny = 1",
                DuplicateStrategy::Rewrite,
                none,
                ["x", "A", "y"],
            ),
            (
                b"x = 1\na = 1\na = 2\n.priority 3\n\"\\u0041\" = 3\ny = 1",
                append,
                nia,
                ["x", "A", "y"],
            ),
            (
                b"x = 1\na = 1\n.priority 3\n\"\\u0041\" = 2\ny = 1",
                DuplicateStrategy::Merge,
                none,
                ["x", "A", "y"],
            ),
            (
                b"x = 1\n\"\\u0041\" = 1\n.priority 3\na = 2\ny = 1",
                append,
                none,
                ["x", "a", "y"],
            ),
            (
                b"x = 1\na = 1\n\"\\u0041\" = 2\ny = 1",
                append,
                none,
                ["x", "a", "y"],
            ),
            (
                b"x = 1\na = 1\n\"\\u0041\" = 2\ny = 1",
                append,
                nia,
                ["x", "a", "y"],
            ),
            (
                b"x = 1\n.priority 3\na = 1\n.priority 1\n\"\\u0041\" = 2\ny = 1",
                append,
                none,
                ["x", "a", "y"],
            ),
            (
                b"x = 1\na { b = 1 }\n\"\\u0041\" { c = 1 }\ny = 1",
                DuplicateStrategy::Merge,
                none,
                ["x", "a", "y"],
            ),
            (
                b"x = 1\na { b = 1 }\n\"\\u0041\" = 2\ny = 1",
                DuplicateStrategy::Merge,
                none,
                ["x", "a", "y"],
            ),
        ] {
            assert_eq!(
                keys(input, strategy, flags),
                expected,
                "{}",
                String::from_utf8_lossy(input)
            );
        }
        // An inherited value replaced by an explicit one.
        let v = Parser::with_flags(ParserFlags::KEY_LOWERCASE)
            .parse(b"d { a = 1 }\ne { .inherit \"d\"; \"\\u0041\" = 2 }")
            .unwrap();
        assert_eq!(obj(&obj(&v)["e"]).keys().collect::<Vec<_>>(), ["A"]);
        // The renamed entry keeps working as a container and for comments.
        let mut p = Parser::with_flags(ParserFlags::KEY_LOWERCASE | ParserFlags::SAVE_COMMENTS);
        let v = p
            .parse(b"a { x = 1 }\n.priority 3\n# c\n\"\\u0041\" { y = 2 # d\n}")
            .unwrap();
        assert_eq!(obj(&obj(&v)["A"]).keys().collect::<Vec<_>>(), ["y"]);
        let paths: Vec<_> = p
            .attached_comments()
            .iter()
            .map(|g| g.path.clone())
            .collect();
        let key = |k: &str| PathSegment::Key {
            key: k.into(),
            index: 0,
        };
        assert_eq!(paths, [vec![key("A")], vec![key("A"), key("y")]]);
    }

    #[test]
    fn macros_are_recognised() {
        let e = parse(b".includes \"x.conf\"").unwrap_err();
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
        // `CURDIR` of a document given as bytes: the default loader's current directory, `/`,
        // or the base directory (WORKLIST C5 decision 1).
        let v = parse(b"a = $CURDIR").unwrap();
        assert_eq!(obj(&v)["a"], UclValue::String("/".into()));
        let v = Parser::builder()
            .with_base_dir("/srv/app")
            .build()
            .parse(b"a = $CURDIR")
            .unwrap();
        assert_eq!(obj(&v)["a"], UclValue::String("/srv/app".into()));
    }

    #[cfg(feature = "fs")]
    #[test]
    fn file_variables_of_a_file_read_from_the_filesystem() {
        let dir = std::env::temp_dir().join(format!("ucl-parse-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("vars.conf");
        std::fs::write(&file, "f = \"$FILENAME\"\nd = \"$CURDIR\"\n").unwrap();
        let mut p = Parser::new();
        p.set_loader(FsLoader::new());
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
        // The default loader holds no files (WORKLIST C5 decision 1).
        assert!(matches!(
            Parser::new().parse_file(&file).unwrap_err().kind(),
            ErrorKind::Io { .. }
        ));
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(matches!(
            Parser::new()
                .set_loader(FsLoader::new())
                .parse_file(dir.join("missing"))
                .unwrap_err()
                .kind(),
            ErrorKind::Io { .. }
        ));
    }

    #[test]
    fn relative_paths_in_a_file_resolve_against_its_directory() {
        // WORKLIST C5 decision 1: without a base directory, relative include paths in a file
        // given to `parse_file`, and in the files it includes, resolve against that file's
        // directory, never against an included file's directory (§9.3).
        let mut loader = MemoryLoader::new();
        loader
            .add_file("/etc/app/main.conf", "a = 1\n.include \"sub/part.conf\"\n")
            .add_file("/etc/app/sub/part.conf", "b = 2\n.include \"leaf.conf\"\n")
            .add_file("/etc/app/leaf.conf", "c = 3\n")
            .add_file("/etc/app/sub/leaf.conf", "c = 4\n")
            .add_file("/srv/sub/part.conf", "b = 5\n")
            .set_current_dir("/srv");
        let mut p = Parser::new();
        p.set_loader(loader.clone());
        let v = p.parse_file("/etc/app/main.conf").unwrap();
        let keys: Vec<_> = obj(&v)
            .iter()
            .map(|(k, e)| (k.to_string(), e.first().clone()))
            .collect();
        assert_eq!(
            keys,
            [
                ("a".to_string(), UclValue::Integer(1)),
                ("b".to_string(), UclValue::Integer(2)),
                ("c".to_string(), UclValue::Integer(3)),
            ]
        );
        // A configured base directory wins.
        let mut p = Parser::new();
        p.set_loader(loader.clone()).set_base_dir("/srv");
        let v = p.parse_file("/etc/app/main.conf").unwrap();
        assert_eq!(obj(&v)["b"], UclValue::Integer(5));
        // A document given as bytes resolves against the loader's current directory.
        let mut p = Parser::new();
        p.set_loader(loader.clone());
        let v = p.parse(b".include \"sub/part.conf\"").unwrap();
        assert_eq!(obj(&v)["b"], UclValue::Integer(5));
        // The same directory is `CURDIR` in macro argument lists (spec §9.2).
        loader.add_file(
            "/etc/app/args.conf",
            ".include(key=\"$CURDIR\") \"leaf.conf\"\n",
        );
        let mut p = Parser::new();
        p.set_loader(loader.clone());
        let v = p.parse_file("/etc/app/args.conf").unwrap();
        assert_eq!(obj(&obj(&v)["/etc/app"])["c"], UclValue::Integer(3));
        let v = p
            .parse(b".include(key=\"$CURDIR\") \"/etc/app/leaf.conf\"")
            .unwrap();
        assert_eq!(obj(&obj(&v)["/srv"])["c"], UclValue::Integer(3));
    }

    #[test]
    fn the_default_loader_holds_no_files() {
        // WORKLIST C5 decision 1: an include macro in input parsed by a default parser finds no
        // file, as §9.4 describes for a missing file.
        let e = parse(b"a = 1\n.include \"/etc/hosts\"").unwrap_err();
        assert!(!e.is_stopped() && !e.is_unsupported(), "{e}");
        let e = parse(b"a = 1\n.try_include \"/etc/hosts\"\nb = 2").unwrap_err();
        assert!(e.is_stopped(), "{e}");
        assert_eq!(obj(e.partial().unwrap()).keys().collect::<Vec<_>>(), ["a"]);
    }

    #[test]
    fn errors_convert_into_ucl_error() {
        fn run(input: &[u8]) -> Result<UclValue, crate::UclError> {
            Ok(parse(input)?)
        }
        let err = run(b"a = ").unwrap_err();
        assert!(matches!(&err, crate::UclError::Syntax(e) if e.kind() == &ErrorKind::MissingValue));
        // A silent stop is an error of its own kind (project decision 4).
        let err = run(b"a = 1\n.try_include \"no such file\"\nb = 2").unwrap_err();
        let crate::UclError::Stopped(e) = &err else {
            panic!("{err:?}");
        };
        assert_eq!(obj(e.partial().unwrap()).keys().collect::<Vec<_>>(), ["a"]);
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
