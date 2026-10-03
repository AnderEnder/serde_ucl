//! Several inputs into one parser (spec §13.1).
//!
//! [`Parser::inputs`](super::Parser::inputs) starts a parse that takes inputs in turn,
//! [`Inputs::add`] gives it the next one, and [`Inputs::finish`] ends it and returns the result.
//! [`Parser::parse`](super::Parser::parse) and [`Parser::parse_file`](super::Parser::parse_file)
//! are such a parse of a single input.

use super::comments::CommentGroups;
use super::core::{Document, Settings};
use super::include::{Budget, Includes, Read};
use super::loader::Loader;
use super::registered::MacroTable;
use super::vars::{self, Expander, Variables};
use super::{Comment, Error, ErrorKind, MAX_INCLUDE_DEPTH, OutputFacts};
use crate::error::Position;
use crate::value::{DuplicateStrategy, MAX_PRIORITY, ParserFlags, UclValue, Value};
use indexmap::IndexMap;
use std::borrow::Cow;
use std::path::{Path, PathBuf};

/// One input for [`Inputs::add`] (spec §13.1): a document given as bytes, or a file given by its
/// path, with the priority (§8.3) and duplicate strategy (§8.4) of its values.
///
/// Without [`Input::with_priority`] and [`Input::with_strategy`], an input takes the parser's
/// ([`super::Parser::set_priority`], [`super::Parser::set_strategy`]).
#[derive(Debug, Clone)]
pub struct Input<'a> {
    source: Source<'a>,
    priority: Option<u8>,
    strategy: Option<DuplicateStrategy>,
}

#[derive(Debug, Clone)]
enum Source<'a> {
    Bytes(&'a [u8]),
    File(PathBuf),
    /// A file already read through the parser's loader: its canonical path and its bytes.
    Read {
        canonical: PathBuf,
        bytes: &'a [u8],
    },
}

impl<'a> Input<'a> {
    /// A document given as bytes: text, a byte string or a buffer. It leaves the file
    /// variables `FILENAME` and `CURDIR` as they are, except as the first input, which defines
    /// them as [`super::Parser::parse`] does (spec §7.8).
    pub fn bytes<T: AsRef<[u8]> + ?Sized>(input: &'a T) -> Self {
        Self {
            source: Source::Bytes(input.as_ref()),
            priority: None,
            strategy: None,
        }
    }

    /// The file at `path`, read through the parser's loader as
    /// [`super::Parser::parse_file`] reads it. A relative `path` resolves against the parser's
    /// base directory, or without one the loader's current directory. The file sets `FILENAME`
    /// and `CURDIR` from its canonical path, also under `NO_FILEVARS`, and they keep those
    /// values for later inputs given as bytes (spec §13.1, *File variables and paths*).
    pub fn file(path: impl Into<PathBuf>) -> Input<'static> {
        Input {
            source: Source::File(path.into()),
            priority: None,
            strategy: None,
        }
    }

    /// The contents of the file whose canonical path is `canonical`, already read.
    pub(crate) fn read_file(canonical: PathBuf, bytes: &'a [u8]) -> Self {
        Self {
            source: Source::Read { canonical, bytes },
            priority: None,
            strategy: None,
        }
    }

    /// Sets the priority of the input's values, kept modulo 16 (spec §8.3). A `.priority`
    /// macro in the input changes it for the rest of that input only (§9.5).
    pub fn with_priority(mut self, priority: u8) -> Self {
        self.priority = Some(priority & MAX_PRIORITY);
        self
    }

    /// Sets how the input resolves the keys it repeats (spec §8.4). It applies when this input
    /// repeats a key, also one that an earlier input added; values that earlier inputs added
    /// are not treated again.
    pub fn with_strategy(mut self, strategy: DuplicateStrategy) -> Self {
        self.strategy = Some(strategy);
        self
    }
}

/// Where the parser's results go when a parse is finished.
struct Outputs<'p> {
    comments: &'p mut Vec<Comment>,
    attached: &'p mut CommentGroups,
    facts: &'p mut OutputFacts,
}

/// A parse of several inputs into one result (spec §13.1), from [`super::Parser::inputs`].
///
/// Each input goes on where the one before it ended: its top-level entries go into the same
/// root, and a key it repeats follows the duplicate rules of §8 with the input's own priority
/// and strategy. Only the first input sets up the root. Variables, the variable handler,
/// registered macros, the loader and the search path apply to every input; `.priority` applies
/// to the rest of its own input only.
///
/// - [`Inputs::finish`] returns the result: the root with every container closed. The parser's
///   [`comments`](super::Parser::comments), [`attached_comments`](super::Parser::attached_comments),
///   [`output_facts`](super::Parser::output_facts) and [`emitter`](super::Parser::emitter) then
///   describe it. With no input added, the result is an empty object.
/// - A silent stop (spec §9.4, §13.2) ends the input that holds the macro, with every file it
///   was including, and nothing else: [`Inputs::add`] returns an error for which
///   [`Error::is_stopped`] is true, whose [`Error::partial`] is the root parsed so far, and the
///   next input goes on where the stopped one left off, in the objects it left open.
///   [`Inputs::finish`] still returns the result.
/// - Any other error in an input fails the parse: [`Inputs::add`] returns it, later calls
///   return it again without reading their input, and [`Inputs::finish`] returns it. An error's
///   position is in the input it was found in; an error found by [`Inputs::finish`] is in the
///   last input that held text. The parser's saved comments are then those read so far.
///
/// libucl's quirks at the joins are kept (WORKLIST C8b decision 1; spec §13.1): a parser takes at
/// most [`MAX_INCLUDE_DEPTH`] inputs, each of which counts as an open unit for the include
/// nesting limit for the rest of the parse; a zero-byte first input gives an empty root that
/// later inputs cannot add to; and the end of an input is not a separator, so a value that ends
/// one input must be followed by a line break, `;`, `,` or a comment at the start of the next
/// input with entries. Finer points at the joins that the spec does not state yet follow
/// libucl's observed behaviour (QUESTIONS.md #59): a value on a following line may come from the
/// next input, an input of whitespace alone between entries makes the next one need a separator
/// first, and an included file that stops silently stays open for the rest of the parse.
///
/// Relative include paths resolve against the parser's base directory in every input; without
/// one, an input given as a file resolves them against its own directory, and one given as
/// bytes against the loader's current directory (WORKLIST C8b decision 4). The input limit of
/// [`super::Parser::set_max_input_bytes`] counts every input and every file they read, together.
///
/// ```
/// use serde_ucl::parse::{Input, MemoryLoader, ParserBuilder};
/// use serde_ucl::DuplicateStrategy;
///
/// let mut files = MemoryLoader::new();
/// files.add_file("/usr/share/app/defaults.conf", "workers = 4\nlog { level = info }\n");
/// let mut parser = ParserBuilder::new().with_loader(files).build();
/// let mut inputs = parser.inputs();
/// inputs.add(Input::file("/usr/share/app/defaults.conf"))?;
/// // The user's settings: a higher priority wins over the defaults (§8.3), and objects merge.
/// inputs.add(
///     Input::bytes("workers = 8\nlog { file = /var/log/app.log }\n")
///         .with_priority(1)
///         .with_strategy(DuplicateStrategy::Merge),
/// )?;
/// let value = inputs.finish()?;
/// let root = value.as_object().unwrap();
/// assert_eq!(root["workers"].as_integer(), Some(8));
/// let log = root["log"].as_object().unwrap();
/// assert_eq!(log["level"].as_str(), Some("info"));
/// assert_eq!(log["file"].as_str(), Some("/var/log/app.log"));
/// # Ok::<(), serde_ucl::parse::Error>(())
/// ```
pub struct Inputs<'p> {
    reader: Reader<'p, 'static>,
}

impl std::fmt::Debug for Inputs<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Inputs")
            .field("inputs", &self.reader.includes.files.len())
            .field("failed", &self.reader.failed)
            .finish()
    }
}

/// A parse of inputs into a tree that may borrow from text that lives for `'t` ([`Value`]):
/// what [`Inputs`] is, with `'t` the lifetime of the text a parse for the serde entry points
/// borrows from (clean-room work item C13).
pub(crate) struct Reader<'p, 't> {
    flags: ParserFlags,
    priority: u8,
    strategy: DuplicateStrategy,
    variables: &'p IndexMap<String, String>,
    base_dir: Option<&'p Path>,
    loader_dir: Option<&'p Path>,
    expander: Expander<'p>,
    includes: Includes<'p>,
    /// `None` once the parse has failed.
    document: Option<Document<'t>>,
    failed: Option<Error>,
    observe: bool,
    observation: Option<super::CObservation>,
    out: Outputs<'p>,
}

/// The parser's settings and state that a parse of inputs borrows, from
/// [`super::Parser::inputs`].
pub(crate) struct Parts<'p> {
    pub(crate) flags: ParserFlags,
    pub(crate) priority: u8,
    pub(crate) strategy: DuplicateStrategy,
    pub(crate) variables: &'p IndexMap<String, String>,
    pub(crate) handler: Option<Box<vars::Handler<'p>>>,
    pub(crate) loader: &'p dyn Loader,
    /// The loader's current directory, when it is known without asking the loader: that of the
    /// parser's default loader, which never changes ([`input_base`]).
    pub(crate) loader_dir: Option<&'p Path>,
    pub(crate) base_dir: Option<&'p Path>,
    pub(crate) search_path: Option<Vec<String>>,
    pub(crate) max_input_bytes: Option<u64>,
    pub(crate) inherit_depth_limit: usize,
    pub(crate) macros: &'p MacroTable,
    pub(crate) uncertain: &'p std::cell::Cell<u8>,
    pub(crate) comments: &'p mut Vec<Comment>,
    pub(crate) attached: &'p mut CommentGroups,
    pub(crate) facts: &'p mut OutputFacts,
    /// Whether output facts are recorded (`Parser::skip_output_facts`).
    pub(crate) records_facts: bool,
    /// The parser's input budget, reset for the parse.
    pub(crate) budget: &'p mut Budget,
}

/// The directory relative paths of an input resolve against (WORKLIST C8b decision 4): the base
/// directory; without one, the directory of an input given as the file `file`, or for one given
/// as bytes the loader's current directory, which `loader_dir` is when it is known without
/// asking the loader (the parser's default loader's, which never changes). Borrowed where it can
/// be, so that a parse of bytes allocates nothing for it (clean-room work item C13).
pub(crate) fn input_base<'p>(
    base_dir: Option<&'p Path>,
    loader: &dyn Loader,
    loader_dir: Option<&'p Path>,
    file: Option<&Path>,
) -> Cow<'p, Path> {
    match (base_dir, file, loader_dir) {
        (Some(dir), _, _) => Cow::Borrowed(dir),
        (None, Some(file), _) => {
            Cow::Owned(file.parent().map(Path::to_path_buf).unwrap_or_default())
        }
        (None, None, Some(dir)) => Cow::Borrowed(dir),
        (None, None, None) => {
            Cow::Owned(loader.current_dir().unwrap_or_else(|_| PathBuf::from(".")))
        }
    }
}

/// `base` as the text of `CURDIR` (spec §7.8), borrowed when `base` is.
pub(crate) fn base_text<'p>(base: &Cow<'p, Path>) -> Cow<'p, str> {
    match base {
        Cow::Borrowed(path) => path.to_string_lossy(),
        Cow::Owned(path) => Cow::Owned(path.to_string_lossy().into_owned()),
    }
}

/// The variables of a parse whose first input is the file `file` (its canonical path) or, for
/// `None`, given as bytes, in lookup order (spec §7.1): the file variables first, then the
/// registered ones. For a file, `FILENAME` is its path and `CURDIR` its directory, whatever
/// registered variables of those names say; for bytes, `FILENAME` is `undef` and `CURDIR` is
/// `curdir`, the base directory's text, unless `NO_FILEVARS` is set, and registered variables of
/// those names override them. Names, `undef` and registered variables are borrowed.
pub(crate) fn first_variables<'p>(
    flags: ParserFlags,
    registered: &'p IndexMap<String, String>,
    file: Option<&Path>,
    curdir: Cow<'p, str>,
) -> Variables<'p> {
    let filevars: Option<(Cow<'p, str>, Cow<'p, str>)> = match file {
        Some(file) => Some((
            Cow::Owned(file.to_string_lossy().into_owned()),
            Cow::Owned(
                file.parent()
                    .map(|d| d.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            ),
        )),
        None => {
            (!flags.contains(ParserFlags::NO_FILEVARS)).then_some((Cow::Borrowed("undef"), curdir))
        }
    };
    // Built as a vector directly: the names are unique, so only a registered `FILENAME` or
    // `CURDIR` can meet a name already there, and takes its place as a map's insert would.
    let mut variables = Variables::with_capacity(registered.len() + 2);
    if let Some((filename, curdir)) = filevars {
        variables.push((Cow::Borrowed("FILENAME"), filename));
        variables.push((Cow::Borrowed("CURDIR"), curdir));
    }
    for (name, value) in registered {
        let is_filevar = name == "FILENAME" || name == "CURDIR";
        let existing = if is_filevar {
            variables.iter().position(|(n, _)| n == name)
        } else {
            None
        };
        match existing {
            Some(_) if file.is_some() => {}
            Some(at) => variables[at].1 = Cow::Borrowed(value),
            None => variables.push((Cow::Borrowed(name), Cow::Borrowed(value))),
        }
    }
    variables
}

impl<'p> Inputs<'p> {
    pub(crate) fn new(parts: Parts<'p>) -> Self {
        Self {
            reader: Reader::new(parts),
        }
    }

    /// Reads the next input (spec §13.1). It goes on where the input before it ended; the first
    /// input sets up the root (§1.1).
    ///
    /// An error for which [`Error::is_stopped`] is true is a silent stop: the input ended at the
    /// macro, [`Error::partial`] holds the root parsed so far, and the parse goes on with the
    /// next input. Any other error fails the parse: it is returned again by every later call and
    /// by [`Inputs::finish`]. Adding a seventeenth input fails with
    /// [`ErrorKind::TooManyInputs`]; a file that cannot be read with [`ErrorKind::Io`].
    pub fn add(&mut self, input: Input<'_>) -> Result<(), Error> {
        self.reader.add(input, None)
    }

    /// [`Inputs::add`], without the partial result of a silent stop.
    pub(crate) fn read(&mut self, input: Input<'_>) -> Result<(), Error> {
        self.reader.read(input, None)
    }

    /// Ends the parse and returns its result: the root, with every container closed (a key
    /// still waiting for its value on a following line gets `null`, §1.6). The parser's saved
    /// comments and output facts then describe it. After an error in an input, returns that
    /// error.
    pub fn finish(self) -> Result<UclValue, Error> {
        self.reader.finish()
    }
}

impl<'p, 't> Reader<'p, 't> {
    pub(crate) fn new(parts: Parts<'p>) -> Self {
        let Parts {
            flags,
            priority,
            strategy,
            variables,
            handler,
            loader,
            loader_dir,
            base_dir,
            search_path,
            max_input_bytes,
            inherit_depth_limit,
            macros,
            uncertain,
            comments,
            attached,
            facts,
            records_facts,
            budget,
        } = parts;
        comments.clear();
        *attached = CommentGroups::empty();
        facts.clear();
        macros.ran.set(false);
        let expander = Expander::new(
            Variables::new(),
            handler,
            !flags.contains(ParserFlags::DISABLE_MACRO),
        );
        *budget = Budget::new(max_input_bytes);
        let mut includes = Includes::new(
            loader,
            Cow::Borrowed(Path::new("")),
            search_path,
            budget,
            (!macros.is_empty()).then_some(macros),
        );
        includes.uncertain = Some(uncertain);
        includes.inherit_limit = inherit_depth_limit;
        let document = Document::new(
            flags.contains(ParserFlags::SAVE_COMMENTS),
            records_facts.then(OutputFacts::new),
            0,
        );
        Self {
            flags,
            priority,
            strategy,
            variables,
            base_dir,
            loader_dir,
            expander,
            includes,
            document: Some(document),
            failed: None,
            observe: false,
            observation: None,
            out: Outputs {
                comments,
                attached,
                facts,
            },
        }
    }

    pub(crate) fn observe(&mut self) {
        self.observe = true;
        if let Some(document) = &mut self.document {
            document.observe_cursor = true;
        }
    }

    pub(crate) fn finish_observed(mut self, error: Option<Error>) -> super::CObservation
    where
        't: 'static,
    {
        if let Some(mut observation) = self.observation.take() {
            observation.error = error;
            return observation;
        }
        let document = self.document.as_ref().expect("observed document");
        let cursor = document.cursor;
        let snapshot = (error.is_some() || document.observation_needs_snapshot())
            .then(|| document.snapshot().into_owned());
        let facts = document.observation_facts();
        let root = match self.finish() {
            Ok(root) => Some(root),
            Err(_) => snapshot,
        };
        super::CObservation {
            root,
            error,
            cursor,
            facts,
        }
    }

    /// [`Inputs::add`]; with `borrow`, the input's bytes, as text the tree may borrow from.
    pub(crate) fn add(&mut self, input: Input<'_>, borrow: Option<&'t [u8]>) -> Result<(), Error> {
        self.read(input, borrow).map_err(|e| {
            if e.is_stopped()
                && let Some(document) = &self.document
            {
                e.with_partial(document.snapshot().into_owned())
            } else {
                e
            }
        })
    }

    /// [`Inputs::read`]; with `borrow`, the bytes of `input`, given as bytes, as text that the
    /// tree may borrow from.
    pub(crate) fn read(&mut self, input: Input<'_>, borrow: Option<&'t [u8]>) -> Result<(), Error> {
        if let Some(error) = &self.failed {
            return Err(error.clone());
        }
        if self.includes.files.len() >= MAX_INCLUDE_DEPTH {
            let limit = MAX_INCLUDE_DEPTH;
            return self.fail(Error::new(
                ErrorKind::TooManyInputs { limit },
                Position::new(),
            ));
        }
        let settings = Settings {
            flags: self.flags,
            priority: input.priority.unwrap_or(self.priority),
            strategy: input.strategy.unwrap_or(self.strategy),
        };
        let too_large = |limit: Option<u64>| {
            Error::new(
                ErrorKind::InputTooLarge {
                    limit: limit.unwrap_or_default(),
                    path: None,
                },
                Position::new(),
            )
        };
        let (bytes, file): (Cow<'_, [u8]>, Option<PathBuf>) = match input.source {
            Source::Bytes(bytes) => {
                if !self.includes.budget.take(bytes.len()) {
                    return self.fail(too_large(self.includes.budget.limit()));
                }
                (Cow::Borrowed(bytes), None)
            }
            Source::Read { canonical, bytes } => {
                if !self.includes.budget.take(bytes.len()) {
                    return self.fail(too_large(self.includes.budget.limit()));
                }
                (Cow::Borrowed(bytes), Some(canonical))
            }
            Source::File(path) => {
                let loader = self.includes.loader;
                let path = input_base(self.base_dir, loader, self.loader_dir, None).join(path);
                let io = |e: std::io::Error| {
                    let message = format!("{}: {e}", path.display());
                    Error::new(ErrorKind::Io { message }, Position::new())
                };
                let canonical = match loader.canonicalize(&path) {
                    Ok(canonical) => canonical,
                    Err(e) => return self.fail(io(e)),
                };
                match self.includes.read(&canonical) {
                    Ok(Read::Bytes(bytes)) => (Cow::Owned(bytes), Some(canonical)),
                    Ok(Read::TooLarge { limit }) => return self.fail(too_large(Some(limit))),
                    Err(e) => return self.fail(io(e)),
                }
            }
        };
        let loader = self.includes.loader;
        let base = input_base(self.base_dir, loader, self.loader_dir, file.as_deref());
        if self.includes.files.is_empty() {
            let curdir = base_text(&base);
            let variables = first_variables(self.flags, self.variables, file.as_deref(), curdir);
            self.expander.set_variables(variables);
        } else if let Some(file) = &file {
            let curdir = file
                .parent()
                .map(|d| d.to_string_lossy().into_owned())
                .unwrap_or_default();
            self.expander
                .set_file_vars(file.to_string_lossy().into_owned(), curdir);
        }
        self.includes.base = base;
        // An input given as bytes goes on in the file of the input before it (oracle runs,
        // QUESTIONS.md #59).
        let current = file.or_else(|| self.includes.files.last().cloned().flatten());
        self.includes.files.push(current);
        let document = self.document.as_mut().expect("a parse that has not failed");
        let read = match borrow {
            Some(src) => {
                debug_assert!(*src == *bytes, "the text to borrow is the input's");
                document.read_borrowed(src, settings, &mut self.expander, &mut self.includes)
            }
            None => document.read(&bytes, settings, &mut self.expander, &mut self.includes),
        };
        match read {
            Ok(()) => Ok(()),
            Err(e) if e.is_stopped() => Err(e),
            Err(e) => self.fail(e),
        }
    }

    /// Fails the parse with `error`: the saved comments read so far go to the parser.
    fn fail(&mut self, error: Error) -> Result<(), Error> {
        if let Some(document) = self.document.take() {
            if self.observe {
                let partial = matches!(
                    error.kind(),
                    ErrorKind::UnterminatedObject
                        | ErrorKind::UnterminatedArray
                        | ErrorKind::FileNotFound { .. }
                        | ErrorKind::NotAFile { .. }
                );
                self.observation = Some(super::CObservation {
                    root: partial.then(|| document.snapshot().into_owned()),
                    cursor: if matches!(error.kind(), ErrorKind::Io { .. }) {
                        Position {
                            line: 0,
                            column: 0,
                            offset: 0,
                        }
                    } else {
                        document.cursor
                    },
                    error: None,
                    facts: document.observation_facts(),
                });
            }
            *self.out.comments = document.into_comments();
        }
        self.failed = Some(error.clone());
        Err(error)
    }

    /// [`Inputs::finish`].
    pub(crate) fn finish(mut self) -> Result<Value<'t>, Error> {
        if let Some(error) = self.failed.take() {
            return Err(error);
        }
        let document = self.document.take().expect("a parse that has not failed");
        let finished = match document.finish() {
            Ok(finished) => finished,
            Err(failed) => {
                let (error, comments) = *failed;
                *self.out.comments = comments;
                return Err(error);
            }
        };
        if let Some((comments, attached)) = finished.comments {
            *self.out.comments = comments;
            *self.out.attached = attached;
        }
        if let Some(facts) = finished.facts {
            *self.out.facts = facts;
        }
        Ok(finished.root)
    }
}
