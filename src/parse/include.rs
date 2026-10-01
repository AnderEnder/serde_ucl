//! `.include`, `.try_include`, `.includes` (spec §9.3, §9.4) and `.load` (§9.6): file paths,
//! glob patterns, search paths, and the input units that included files become.
//!
//! Paths are used as written. A relative path resolves against the base directory
//! ([`Includes::base`]): the parser's base directory; without one, the directory of the file
//! given to `Parser::parse_file` (WORKLIST C5 decision 1), or for a document given as bytes the
//! loader's current directory. That holds inside included files too; `${CURDIR}` gives paths
//! relative to the including file. The base directory is also `CURDIR` in macro argument lists
//! (spec §9.2).
//!
//! How a file that cannot be included is handled (oracle runs; spec §9.4, *Missing and unusable
//! files*). `try` is the `try` parameter, true by default for `.try_include`:
//!
//! | The file | `.include` | `.include(try=true)` | `.try_include` |
//! | --- | --- | --- | --- |
//! | does not exist | error | skipped | stops |
//! | is not a regular file | error | skipped | stops (an error with `try=false`) |
//! | holds the macro | error | error | stops |
//!
//! "Stops" is a silent stop: the macro ends the parse without an error in libucl, and the
//! crate reports [`ErrorKind::Stopped`] with the partial result. Inside a glob expansion or a
//! search path it only means "not usable here" ([`Outcome::Unusable`]): with `try`, a glob
//! goes on with the next match, and a search path goes on with the next directory. A silent
//! stop from inside an included file always ends the whole parse (the oracle crashes when it
//! does not; QUESTIONS.md #30).

use super::core::{Core, NestTarget, Settings};
use super::glob;
use super::loader::{FileKind, Loader};
use super::macros::{MacroCall, MacroKind, priority_bits};
use super::registered::MacroTable;
use super::{Error, ErrorKind, MAX_INCLUDE_DEPTH};
use crate::value::{DuplicateStrategy, UclValue};
use smallvec::SmallVec;
use std::borrow::Cow;
use std::cell::Cell;
use std::io;
use std::path::{Path, PathBuf};

/// The input limit of one parse ([`super::Parser::set_max_input_bytes`]) and the bytes read so
/// far, the document included. The include state of the document and those of its macro
/// argument documents share it, so that files read from argument documents count too.
#[derive(Debug)]
pub(crate) struct Budget {
    limit: Option<u64>,
    used: Cell<u64>,
}

impl Budget {
    /// A budget of `limit` bytes, none of them used yet.
    pub(crate) fn new(limit: Option<u64>) -> Self {
        Self {
            limit,
            used: Cell::new(0),
        }
    }

    /// Counts an input of `len` bytes given as bytes; `false` if it goes over the limit.
    pub(crate) fn take(&self, len: usize) -> bool {
        let used = self.used.get().saturating_add(len as u64);
        self.used.set(used);
        self.limit.is_none_or(|limit| used <= limit)
    }

    pub(crate) fn limit(&self) -> Option<u64> {
        self.limit
    }
}

/// A file read through [`Includes::read`].
pub(crate) enum Read {
    Bytes(Vec<u8>),
    /// Its bytes would take the parse past its input limit.
    TooLarge {
        limit: u64,
    },
}

/// The include state of one parse, shared by its inputs (spec §13.1).
pub(crate) struct Includes<'l> {
    pub(crate) loader: &'l dyn Loader,
    /// The directory relative paths resolve against, and `CURDIR` of a document given as bytes
    /// (project decision 6). It is set for each input (WORKLIST C8b decision 4).
    pub(crate) base: Cow<'l, Path>,
    /// The `path` list in effect: set by any include macro, it stays for the rest of the parse
    /// (spec §9.4, *Signatures, URLs and search paths*). It starts as the parser's search path
    /// ([`super::Parser::set_search_path`]).
    search: Option<Vec<String>>,
    /// A first-directory miss that §9.4 allows a later skip in this input to recover: a skipped
    /// URL include, or a `.load` with `try=true` that reads nothing.
    pub(crate) pending_search_miss: Option<(usize, Error)>,
    /// The parser's search path, which macro argument documents start with.
    default_search: Option<Vec<String>>,
    /// The file of each open input unit, the inputs first: an included file's canonical path;
    /// for an input given as a file, its path; for one given as bytes, and for text a registered
    /// macro parses in place, the file of the unit before it, if any (oracle runs, QUESTIONS.md
    /// #59). An include of the last one's file includes itself (§9.4). Inputs stay open for the
    /// rest of the parse (spec §13.1, *How many inputs*), and so do included files that stop
    /// silently. The first two are kept inline, so that a parse of one document allocates
    /// nothing for them (clean-room work item C12), as are the first four of `open_units`.
    pub(crate) files: SmallVec<[Option<PathBuf>; 2]>,
    pub(crate) budget: &'l Budget,
    /// The macros the application registered (spec §13.2); `None` in macro argument documents,
    /// which know only the built-in macros.
    pub(crate) macros: Option<&'l MacroTable>,
    /// The number of input units opened so far, which gives each its own identity.
    units: usize,
    /// The input units being parsed, outermost first; the others have ended (spec §9.4).
    pub(crate) open_units: SmallVec<[usize; 4]>,
    /// Where the parse records the [`super::Uncertain`] rules it reaches.
    pub(crate) uncertain: Option<&'l Cell<u8>>,
    /// How deep a copy made by `.inherit` may nest a value, the root included
    /// ([`super::Parser::set_inherit_depth_limit`]).
    pub(crate) inherit_limit: usize,
}

impl<'l> Includes<'l> {
    /// The state for a parse with no input read yet, with the parser's search path `search`.
    pub(crate) fn new(
        loader: &'l dyn Loader,
        base: Cow<'l, Path>,
        search: Option<Vec<String>>,
        budget: &'l Budget,
        macros: Option<&'l MacroTable>,
    ) -> Self {
        Self {
            loader,
            base,
            search: search.clone(),
            pending_search_miss: None,
            default_search: search,
            files: SmallVec::new(),
            budget,
            macros,
            units: 0,
            open_units: SmallVec::new(),
            uncertain: None,
            inherit_limit: super::DEFAULT_INHERIT_DEPTH_LIMIT,
        }
    }

    /// Records that the parse reached `rule`, which the spec leaves uncertain.
    pub(crate) fn reached(&self, rule: super::Uncertain) {
        if let Some(cell) = self.uncertain {
            cell.set(cell.get() | rule.bit());
        }
    }

    /// The state for a macro argument document, which is parsed as if a new parser with the same
    /// settings were given it as bytes (spec §9.2): the parser's search path is in effect there,
    /// not a `path` list of the document that holds the macro, and no registered macro is known
    /// (§13.2).
    pub(crate) fn for_arguments(&self) -> Includes<'l> {
        let mut includes = Includes::new(
            self.loader,
            self.base.clone(),
            self.default_search.clone(),
            self.budget,
            None,
        );
        includes.files.push(None);
        includes.uncertain = self.uncertain;
        includes.inherit_limit = self.inherit_limit;
        includes
    }

    /// An identity for a new input unit, distinct from those of the units opened before.
    pub(crate) fn new_unit(&mut self) -> usize {
        self.units += 1;
        self.units
    }

    /// Reads the file at `path` through the loader, and counts its bytes against the input
    /// limit. Without a limit, the loader reads it whole.
    pub(crate) fn read(&self, path: &Path) -> io::Result<Read> {
        let Some(limit) = self.budget.limit else {
            return self.loader.read(path).map(Read::Bytes);
        };
        let left = limit.saturating_sub(self.budget.used.get());
        let bytes = self.loader.read_limited(path, left)?;
        let len = bytes.len() as u64;
        if len > left {
            return Ok(Read::TooLarge { limit });
        }
        self.budget.used.set(self.budget.used.get() + len);
        Ok(Read::Bytes(bytes))
    }

    fn resolve(&self, path: &str) -> PathBuf {
        let path = Path::new(path);
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.base.join(path)
        }
    }
}

/// What trying one file or pattern came to.
enum Outcome {
    /// Included, or skipped under `try`.
    Done,
    /// Not usable here, which `.try_include` does not make an error: the macro then stops the
    /// parse. Holds the error that the case is otherwise.
    Unusable(ErrorKind),
}

/// An include macro's parameters.
struct Request {
    /// `.try_include`.
    soft: bool,
    /// The `try` parameter, true by default for `.try_include`.
    try_: bool,
    glob: bool,
    /// With `glob=true`, the VALUE holds a `*` or `?` after its first NUL byte, which makes the
    /// path before the NUL a pattern when no search path is in effect (spec §9.4, *Quirk: a NUL
    /// byte in a pattern*).
    wildcard_after_nul: bool,
    prefix: bool,
    key: Option<String>,
    array: bool,
    /// The included unit's flags, priority and strategy.
    settings: Settings,
    /// Where the macro's value starts, for errors.
    at: usize,
}

/// The key `prefix=true` nests a file under: the base name of its canonical path, without a
/// final `.conf` or `.ucl` (spec §9.4).
fn prefix_key(path: &Path) -> String {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    match name
        .strip_suffix(".conf")
        .or_else(|| name.strip_suffix(".ucl"))
    {
        Some(stem) => stem.to_owned(),
        None => name,
    }
}

fn duplicate_strategy(name: Option<&str>) -> DuplicateStrategy {
    match name {
        Some("merge") => DuplicateStrategy::Merge,
        Some("rewrite") => DuplicateStrategy::Rewrite,
        Some("error") => DuplicateStrategy::Error,
        _ => DuplicateStrategy::Append,
    }
}

impl<'t> Core<'_, 't, '_, '_, '_> {
    /// The path a macro's value names: the value up to its first NUL byte, if it has one (spec
    /// §9.2, *Quirk: a NUL byte in VALUE*; §9.3). It must be UTF-8.
    fn macro_path(&self, call: &MacroCall) -> Result<String, Error> {
        let path = call.value.split(|&b| b == 0).next().unwrap_or_default();
        String::from_utf8(path.to_vec())
            .map_err(|_| self.error(ErrorKind::InvalidUtf8, call.value_at))
    }

    /// `.include`, `.try_include` and `.includes` (spec §9.4).
    pub(super) fn include_macro(&mut self, call: &MacroCall) -> Result<(), Error> {
        let params = call.args.resolve(call.kind.parameters());
        let unsupported = |feature: &str| ErrorKind::Unsupported {
            feature: feature.to_owned(),
        };
        if call.kind == MacroKind::Includes {
            return Err(self.error(
                unsupported("the macro .includes, which verifies signatures,"),
                call.at,
            ));
        }
        if params.bool("sign") == Some(true) {
            return Err(self.error(unsupported("signature checking (sign=true)"), call.at));
        }
        let path = self.macro_path(call)?;
        if let Some(dirs) = params.array("path") {
            // Each entry ends at its first NUL byte, as string parameters do (§9.2).
            let dirs = dirs
                .iter()
                .filter_map(UclValue::as_str)
                .map(|dir| super::macros::before_nul(dir).to_owned());
            self.includes.search = Some(dirs.collect());
        }
        let soft = call.kind == MacroKind::TryInclude;
        let try_ = params.bool("try").unwrap_or(soft);
        if params.bool("url") == Some(true) && path.contains("://") {
            // URLs are never fetched (project decision 2). As in a libucl built without URL
            // support, such an include is decided before the search path, globs and nesting
            // under a key: skipped with `try`, which is not a silent stop, an error without it
            // (spec §9.4, *Signatures, URLs and search paths*). Its `path` list still takes
            // effect for later includes (oracle runs, QUESTIONS.md #37).
            return if try_ {
                self.skipped_after_search_miss();
                Ok(())
            } else {
                Err(self.error(ErrorKind::UrlNotSupported { path }, call.value_at))
            };
        }
        let glob = params.bool("glob").unwrap_or(false);
        let after_nul = call.value.splitn(2, |&b| b == 0).nth(1).unwrap_or_default();
        let request = Request {
            soft,
            try_,
            glob,
            wildcard_after_nul: glob && after_nul.iter().any(|&b| matches!(b, b'*' | b'?')),
            prefix: params.bool("prefix").unwrap_or(false),
            key: params.string("key").map(str::to_owned),
            array: params
                .string("target")
                .is_some_and(|t| t.eq_ignore_ascii_case("array")),
            settings: Settings {
                flags: self.settings.flags,
                priority: params.int("priority").map_or(0, priority_bits),
                strategy: duplicate_strategy(params.string("duplicate")),
            },
            at: call.value_at,
        };
        let outcome = match self.includes.search.clone() {
            None => self.include_path(&path, request.wildcard_after_nul, &request)?,
            Some(dirs) if !soft && !try_ && !glob && self.includes.open_units.len() == 1 => {
                self.include_searched_with_later_url(&dirs, &path, &request)?
            }
            Some(dirs) => self.include_searched(&dirs, &path, &request)?,
        };
        match outcome {
            Outcome::Done => Ok(()),
            Outcome::Unusable(_) => Err(self.error(ErrorKind::Stopped { path }, call.at)),
        }
    }

    /// A macro of this input skipped its file: a skipped URL include, or a `.load` with
    /// `try=true` that reads nothing (spec §9.4, *Quirk: a later skipped URL include or
    /// `.load`*; §9.6). The first-directory misses of the input before it pass; a later miss
    /// needs a later skip of its own.
    fn skipped_after_search_miss(&mut self) {
        if self
            .includes
            .pending_search_miss
            .as_ref()
            .is_some_and(|(unit, _)| Some(unit) == self.includes.open_units.last())
        {
            self.includes.pending_search_miss = None;
        }
    }

    /// §9.4, *Quirk: a later skipped URL include or `.load`*: a later skip can recover a plain
    /// include's missing first directory when another directory has the file
    /// ([`Core::skipped_after_search_miss`]). The first error is kept until the input reaches
    /// such a skip, and is the input's error if it reaches none.
    fn include_searched_with_later_url(
        &mut self,
        dirs: &[String],
        path: &str,
        request: &Request,
    ) -> Result<Outcome, Error> {
        let Some((first, rest)) = dirs.split_first() else {
            return self.include_searched(dirs, path, request);
        };
        let first_path = format!("{first}/{path}");
        match self.include_path(&first_path, false, request) {
            Ok(outcome) => Ok(outcome),
            Err(miss) if matches!(miss.kind(), ErrorKind::FileNotFound { .. }) => {
                for dir in rest {
                    let later_path = format!("{dir}/{path}");
                    let candidate = self.includes.resolve(&later_path);
                    if self.includes.loader.kind(&candidate) != Some(FileKind::File) {
                        continue;
                    }
                    let unit = *self
                        .includes
                        .open_units
                        .last()
                        .expect("an input unit is open");
                    self.includes
                        .pending_search_miss
                        .get_or_insert((unit, miss));
                    self.include_path(&later_path, false, request)?;
                    return Ok(Outcome::Done);
                }
                Err(miss)
            }
            Err(error) => Err(error),
        }
    }

    /// With a search path, `DIR/PATH` for each directory in order. Without `glob`, the first
    /// directory where the file is included or skipped ends the search; with `glob`, every
    /// directory is expanded. The last directory's outcome decides: not usable there is an
    /// error, not a silent stop, and so is an empty list.
    fn include_searched(
        &mut self,
        dirs: &[String],
        path: &str,
        request: &Request,
    ) -> Result<Outcome, Error> {
        let mut last = Outcome::Unusable(ErrorKind::FileNotFound {
            path: path.to_owned(),
        });
        for dir in dirs {
            // The path is cut at its NUL before a wildcard is looked for (§9.4).
            last = self.include_path(&format!("{dir}/{path}"), false, request)?;
            if matches!(last, Outcome::Done) && !request.glob {
                break;
            }
        }
        match last {
            Outcome::Done => Ok(Outcome::Done),
            Outcome::Unusable(kind) => Err(self.error(kind, request.at)),
        }
    }

    /// One path, expanded when it is a glob pattern: with `glob=true`, when it holds a wildcard,
    /// or when `wildcard_after_nul` says that the VALUE held one after the NUL that ended the
    /// path (spec §9.4, *Quirk: a NUL byte in a pattern*).
    fn include_path(
        &mut self,
        path: &str,
        wildcard_after_nul: bool,
        request: &Request,
    ) -> Result<Outcome, Error> {
        if !(request.glob && (wildcard_after_nul || glob::has_wildcard(path))) {
            if path.is_empty() {
                // The empty path names nothing, not the base directory.
                return self.unusable_missing(path, request);
            }
            let named = path.trim_end_matches('/');
            if named.len() < path.len()
                && !named.is_empty()
                && self.includes.loader.kind(&self.includes.resolve(named)) == Some(FileKind::File)
            {
                // A file's name followed by `/`: uncertain (§9.4, *Globs*).
                self.includes.reached(super::Uncertain::TrailingSlash);
            }
            let candidate = self.includes.resolve(path);
            return self.include_candidate(&candidate, path, request, request.key.clone());
        }
        // The empty pattern matches nothing.
        let expansion = if path.is_empty() {
            glob::Expansion::default()
        } else {
            glob::expand(self.includes.loader, &self.includes.base, path)
        };
        if expansion.left_out_link {
            self.includes.reached(super::Uncertain::TrailingSlash);
        }
        let matches = expansion.paths;
        let Some(first) = matches.first() else {
            // Nothing matches: skipped with `try`, otherwise a silent stop (spec §9.4, *Quirk*).
            return Ok(if request.try_ {
                Outcome::Done
            } else {
                Outcome::Unusable(ErrorKind::FileNotFound {
                    path: path.to_owned(),
                })
            });
        };
        // With `prefix`, the first match names the key for all of them (spec §9.4, *Quirk*).
        let key = request.key.clone().or_else(|| {
            request.prefix.then(|| {
                let first = self.includes.loader.canonicalize(first);
                prefix_key(first.as_deref().unwrap_or(&matches[0]))
            })
        });
        // `.try_include(try=false)` skips a match that is the including file too, but fails when
        // it included no match at all (spec §9.4, *Globs*).
        let mut included = false;
        let mut skipped_self = None;
        for candidate in &matches {
            let shown = candidate.to_string_lossy().into_owned();
            match self.include_candidate(candidate, &shown, request, key.clone())? {
                Outcome::Done => included = true,
                Outcome::Unusable(_) if request.try_ => {}
                Outcome::Unusable(kind @ ErrorKind::IncludeSelf { .. }) if request.soft => {
                    skipped_self = Some(kind);
                }
                Outcome::Unusable(kind) => return Err(self.error(kind, request.at)),
            }
        }
        match skipped_self {
            Some(kind) if !included => Err(self.error(kind, request.at)),
            _ => Ok(Outcome::Done),
        }
    }

    fn unusable_missing(&self, shown: &str, request: &Request) -> Result<Outcome, Error> {
        let kind = ErrorKind::FileNotFound {
            path: shown.to_owned(),
        };
        if request.soft {
            Ok(Outcome::Unusable(kind))
        } else if request.try_ {
            Ok(Outcome::Done)
        } else {
            Err(self.error(kind, request.at))
        }
    }

    /// One file: checked, then parsed as a new input unit. `shown` is the path for messages.
    fn include_candidate(
        &mut self,
        candidate: &Path,
        shown: &str,
        request: &Request,
        key: Option<String>,
    ) -> Result<Outcome, Error> {
        let loader = self.includes.loader;
        let canonical = if request.try_ {
            loader.canonicalize_optional(candidate)
        } else {
            loader.canonicalize(candidate)
        };
        let Ok(canonical) = canonical else {
            return self.unusable_missing(shown, request);
        };
        let not_a_file = || ErrorKind::NotAFile {
            path: shown.to_owned(),
        };
        let unusable = |this: &Self, kind: ErrorKind| {
            if !request.try_ {
                Err(this.error(kind, request.at))
            } else if request.soft {
                Ok(Outcome::Unusable(kind))
            } else {
                Ok(Outcome::Done)
            }
        };
        match loader.kind(&canonical) {
            Some(FileKind::File) => {}
            None => return self.unusable_missing(shown, request),
            Some(_) => return unusable(self, not_a_file()),
        }
        if self.includes.files.last().and_then(Option::as_ref) == Some(&canonical) {
            let kind = ErrorKind::IncludeSelf {
                path: shown.to_owned(),
            };
            return if request.soft {
                Ok(Outcome::Unusable(kind))
            } else {
                Err(self.error(kind, request.at))
            };
        }
        if self.includes.files.len() >= MAX_INCLUDE_DEPTH {
            return Err(self.error(
                ErrorKind::IncludeTooDeep {
                    limit: MAX_INCLUDE_DEPTH,
                },
                request.at,
            ));
        }
        let bytes = match self.includes.read(&canonical) {
            Ok(Read::Bytes(bytes)) => bytes,
            // Not softened by `try` or `.try_include`: the limit is not about the file being
            // missing or unusable.
            Ok(Read::TooLarge { limit }) => {
                let path = Some(shown.to_owned());
                return Err(self.error(ErrorKind::InputTooLarge { limit, path }, request.at));
            }
            // A regular file that cannot be read is an error even for optional includes
            // (spec §9.4, *Missing and unusable files*). `try` only skips missing files and
            // non-regular paths.
            Err(_) => return Err(self.error(not_a_file(), request.at)),
        };
        let key = key.or_else(|| request.prefix.then(|| prefix_key(&canonical)));
        self.include_unit(&bytes, &canonical, request, key)?;
        Ok(Outcome::Done)
    }

    /// Parses an included file's bytes, nested under `key` when there is one. `FILENAME` and
    /// `CURDIR` name the file while it is parsed (spec §9.4).
    fn include_unit(
        &mut self,
        bytes: &[u8],
        canonical: &Path,
        request: &Request,
        key: Option<String>,
    ) -> Result<(), Error> {
        let open = self.open_containers();
        if let Some(key) = key.clone() {
            let target = NestTarget {
                key,
                array: request.array,
                priority: request.settings.priority,
            };
            self.open_nest_target(&target, request.at)?;
        }
        let filename = canonical.to_string_lossy().into_owned();
        let curdir = canonical
            .parent()
            .map(|d| d.to_string_lossy().into_owned())
            .unwrap_or_default();
        let saved = self.expander.enter_file(filename, curdir);
        self.includes.files.push(Some(canonical.to_path_buf()));
        let result = self.parse_included(bytes, request.settings, Some(canonical));
        // A file that stops silently stays open for the rest of the parse, with its file
        // variables, which matters to later inputs (oracle runs, QUESTIONS.md #59).
        if !result.as_ref().is_err_and(Error::is_stopped) {
            self.includes.files.pop();
            self.expander.leave_file(saved);
        }
        result?;
        if key.is_some() {
            // The containers of the key, and whatever the file left open in them, are done.
            self.close_containers_above(open);
        }
        Ok(())
    }

    /// `.load` (spec §9.6): a file's contents as one value under `key` of the current object.
    #[cfg(feature = "load")]
    pub(super) fn load_macro(&mut self, call: &MacroCall) -> Result<(), Error> {
        use crate::value::{Entry, ParserFlags, Slot};

        let params = call.args.resolve(MacroKind::Load.parameters());
        let try_ = params.bool("try").unwrap_or(false);
        let Some(key) = params.string("key").filter(|k| !k.is_empty()) else {
            return Err(self.error(ErrorKind::LoadKeyMissing, call.at));
        };
        let key = key.to_owned();
        let path = self.macro_path(call)?;
        // An empty VALUE is an error even with `try` (spec §9.6). A VALUE that starts with a NUL
        // byte is not empty: it names the empty path as a file, which is missing (§9.2,
        // QUESTIONS.md #71).
        if call.value.is_empty() {
            return Err(self.error(ErrorKind::FileNotFound { path }, call.value_at));
        }
        let loader = self.includes.loader;
        let not_found = || ErrorKind::FileNotFound { path: path.clone() };
        let not_a_file = || ErrorKind::NotAFile { path: path.clone() };
        // The path is used as written (§9.6), not resolved first as an include path is (§9.3):
        // the loader looks it up as it stands, so with `FsLoader` a regular file followed by `/`
        // names nothing (oracle runs, C9).
        let written = self.includes.resolve(&path);
        let read = match loader.kind(&written) {
            _ if path.is_empty() => Err(not_found()),
            None => Err(not_found()),
            Some(FileKind::File) => self.includes.read(&written).map_err(|_| not_a_file()),
            Some(_) => Err(not_a_file()),
        };
        let bytes = match read {
            Ok(Read::Bytes(bytes)) => bytes,
            // `try` does not soften the input limit.
            Ok(Read::TooLarge { limit }) => {
                let path = Some(path.clone());
                return Err(self.error(ErrorKind::InputTooLarge { limit, path }, call.value_at));
            }
            Err(_) if try_ => {
                // A missing or unusable file read nothing: an earlier first-directory miss of
                // `.include` passes (§9.4, *Quirk: a later skipped URL include or `.load`*). A
                // file that is read, an empty one included, does not count (QUESTIONS.md #86).
                self.skipped_after_search_miss();
                return Ok(());
            }
            Err(kind) => return Err(self.error(kind, call.value_at)),
        };
        let key_lowercase = self.settings.flags.contains(ParserFlags::KEY_LOWERCASE);
        let exists = self.find_current_key(&key, key_lowercase).is_some();
        if exists {
            return Err(self.error(ErrorKind::LoadKeyExists { key }, call.at));
        }
        let target = params.string("target").unwrap_or("string");
        let value = if target.eq_ignore_ascii_case("string") {
            if bytes.is_empty() {
                // An empty file inserts nothing (spec §9.6, *Quirk*).
                return Ok(());
            }
            let mut text = bytes;
            if params.bool("trim") == Some(true) {
                text = trim(&text).to_vec();
            }
            if params.bool("escape") == Some(true) {
                text = escape(&text);
            }
            let text = String::from_utf8(text)
                .map_err(|_| self.error(ErrorKind::InvalidUtf8, call.value_at))?;
            UclValue::String(text.into())
        } else if target.eq_ignore_ascii_case("int") {
            UclValue::Integer(leading_integer(&bytes))
        } else {
            return Ok(());
        };
        let priority = params.int("priority").map_or(0, priority_bits);
        if key_lowercase && key.bytes().any(|b| b.is_ascii_uppercase()) {
            self.uppercase_keys = true;
        }
        // The string counts as a heredoc for config output (§9.6, §10.5). The key follows the
        // emitter's default rule for quoting (§10.1), so it needs no fact.
        let multiline = value.is_string() && params.bool("multiline") == Some(true);
        let object = self
            .current()
            .as_object_mut()
            .expect("macros are read inside objects");
        object.insert_entry(key.clone(), Entry::from_slot(Slot::new(value, priority)));
        let entry = object.index_of(&key).expect("the entry was just added");
        let locating = self
            .facts
            .as_ref()
            .is_some_and(super::OutputFacts::records_locations);
        let pos = super::facts::Pos::Entry {
            entry,
            key: crate::value::KeyCopy::from(key),
            slot: 0,
        };
        if (multiline || locating)
            && let Some(node) = self.facts_node_below(&[pos])
        {
            let facts = self.facts.as_mut().expect("checked above");
            if multiline {
                facts.update(node, |f| f.multiline = true);
            }
            facts.locate(node, call.value_at, Some(call.at));
        }
        Ok(())
    }
}

/// Space, TAB, LF, CR, VT and FF.
#[cfg(feature = "load")]
fn is_load_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0B | 0x0C)
}

/// `bytes` without leading and trailing whitespace (`.load` with `trim=true`).
#[cfg(feature = "load")]
fn trim(bytes: &[u8]) -> &[u8] {
    let start = bytes.iter().take_while(|&&b| is_load_space(b)).count();
    let rest = &bytes[start..];
    let end = rest.len() - rest.iter().rev().take_while(|&&b| is_load_space(b)).count();
    &rest[..end]
}

/// `bytes` escaped for `.load` with `escape=true` (spec §9.6 table).
#[cfg(feature = "load")]
fn escape(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    for &b in bytes {
        match b {
            b'"' => out.extend_from_slice(b"\\\""),
            b'\\' => out.extend_from_slice(b"\\\\"),
            b'\n' => out.extend_from_slice(b"\\n"),
            b'\r' => out.extend_from_slice(b"\\r"),
            b'\t' => out.extend_from_slice(b"\\t"),
            0x08 => out.extend_from_slice(b"\\b"),
            0x0C => out.extend_from_slice(b"\\f"),
            0 => out.extend_from_slice(b"\\u0000"),
            0x0B => out.extend_from_slice(b"\\u000B"),
            b => out.push(b),
        }
    }
    out
}

/// The integer `.load` with `target="int"` reads (spec §9.6): optional leading whitespace, an
/// optional sign, then as many decimal digits as follow; the rest is ignored. No digits is 0,
/// and a number outside the 64-bit range is limited to the nearest end.
#[cfg(feature = "load")]
fn leading_integer(bytes: &[u8]) -> i64 {
    let start = bytes.iter().take_while(|&&b| is_load_space(b)).count();
    let (negative, digits) = match &bytes[start..] {
        [b'-', rest @ ..] => (true, rest),
        [b'+', rest @ ..] => (false, rest),
        rest => (false, rest),
    };
    let limit = i128::from(i64::MAX) + 1;
    let magnitude = digits
        .iter()
        .take_while(|b| b.is_ascii_digit())
        .fold(0i128, |n, &d| (n * 10 + i128::from(d - b'0')).min(limit));
    let n = if negative { -magnitude } else { magnitude };
    n.clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64
}

#[cfg(all(test, feature = "load"))]
mod load_tests {
    use super::*;

    #[test]
    fn load_helpers() {
        assert_eq!(trim(b"\x0b\x0c\t \r\nx y\r\n\x0b\x0c \t"), b"x y");
        assert_eq!(trim(b" \t\n "), b"");
        assert_eq!(
            escape(b"a\tb\x08c\x0cd\re\x0bf\"g\\h\x00i j\n"),
            b"a\\tb\\bc\\fd\\re\\u000Bf\\\"g\\\\h\\u0000i j\\n".to_vec()
        );
        for (text, n) in [
            (&b"42\n"[..], 42),
            (b"42abc", 42),
            (b"\n\t +17 rest", 17),
            (b"abc", 0),
            (b"", 0),
            (b"12\x0034", 12),
            (b"-99999999999999999999", i64::MIN),
            (b"99999999999999999999", i64::MAX),
            (b"- 1", 0),
        ] {
            assert_eq!(leading_integer(text), n, "{text:?}");
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::parse::{
        CommentPlacement, Error, ErrorKind, MAX_INCLUDE_DEPTH, MemoryLoader, Parser, PathSegment,
    };
    use crate::value::{DuplicateStrategy, ParserFlags, UclObject, UclValue};
    use std::path::Path;

    /// A parser over an in-memory tree with the base directory `/c`.
    fn parser(files: &[(&str, &str)], flags: ParserFlags) -> Parser {
        let mut loader = MemoryLoader::new();
        loader.add_dir("/c/dir");
        for (path, text) in files {
            loader.add_file(path, *text);
        }
        let mut p = Parser::with_flags(flags);
        p.set_loader(loader).set_base_dir("/c");
        p
    }

    fn run(files: &[(&str, &str)], input: &str) -> Result<UclValue, Error> {
        parser(files, ParserFlags::DEFAULT).parse(input.as_bytes())
    }

    fn obj(v: &UclValue) -> &UclObject {
        v.as_object().expect("object")
    }

    fn keys(v: &UclValue) -> Vec<String> {
        obj(v).keys().map(|k| k.to_string()).collect()
    }

    const A: (&str, &str) = ("/c/files/a.inc", "x = 1\ny = \"inc\"\n");

    #[test]
    fn uncertain_rules_reached_are_recorded() {
        use crate::parse::Uncertain;
        let files = [
            ("/c/elem.inc", "a = [ {\n.include \"sep.inc\""),
            ("/c/sep.inc", "x \"y{\" = \n"),
            ("/c/close.inc", "a = 1 }\n"),
            ("/c/left_open.inc", "x \"y{\" z"),
            ("/c/v.inc", "v = 1"),
            ("/c/reopen_int.inc", "\"s\".include {v.inc} # c"),
        ];
        let reached = |input: &str| {
            let mut p = parser(&files, ParserFlags::DEFAULT);
            let _ = p.parse(input.as_bytes());
            p.uncertain_reached()
        };
        // §9.4: the check at the end of close.inc stops at containers of ended units, the
        // array element among them (a fuzz finding, C9).
        assert_eq!(
            reached(".include \"elem.inc\"x {.include \"close.inc\""),
            [Uncertain::EndedUnitContainer]
        );
        // Section objects of an ended unit without a bracket are defined (§9.4).
        assert_eq!(reached("a {\n.include \"left_open.inc\"\nk = 1"), []);
        // §9.4: a `}` in an included file closes an array element of the including unit.
        assert_eq!(
            reached("a = [ { .include \"close.inc\"\n]"),
            [Uncertain::ClosedArrayElement]
        );
        // §9.1: the value created most recently is not an object.
        assert_eq!(
            reached(".include \"reopen_int.inc\"\nk = 1"),
            [Uncertain::ReopenedNotObject]
        );
        assert_eq!(reached("a = 1"), []);
    }

    #[cfg(feature = "fs")]
    #[test]
    fn a_trailing_slash_after_a_file_is_uncertain() {
        // spec §9.4, *Globs*, **Uncertain** (QUESTIONS.md #73), with the conformance fixtures:
        // `files/v4/link.inc` is a symbolic link to `../c.conf`.
        use crate::parse::{FsLoader, Uncertain};
        let dir =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/conformance/cases/spec/09-macros");
        let reached = |input: &str| {
            let mut p = Parser::new();
            p.set_loader(FsLoader::new()).set_base_dir(&dir);
            let _ = p.parse(input.as_bytes());
            p.uncertain_reached()
        };
        for input in [
            ".include(glob=true, try=true) \"files/v4/*/\"",
            ".include(glob=true) \"files/v4/l*/\"",
            ".include \"files/c.conf/\"",
            ".try_include \"files/v4/link.inc//\"",
        ] {
            assert_eq!(reached(input), [Uncertain::TrailingSlash], "{input}");
        }
        // A regular file left out by a pattern, and a directory, are defined.
        for input in [
            ".include(glob=true) \"files/v4/g/a.in?/\"",
            ".include(glob=true, try=true) \"files/v4/g/*\"",
            ".try_include \"files/v4/dir/\"",
            ".include \"files/c.conf\"",
        ] {
            assert_eq!(reached(input), [], "{input}");
        }
    }

    #[test]
    fn a_unit_that_is_only_its_leading_bracket_adds_nothing() {
        // spec §9.4, *Quirk: a file that ends right after its leading bracket*; §13.2: where
        // §1.1 lets a bracketed root start, after whitespace alone or directly after a leading
        // comment group (QUESTIONS.md #70).
        let files = [
            ("/c/b.inc", "{"),
            ("/c/nb.inc", "\n{"),
            ("/c/k.inc", "["),
            ("/c/sk.inc", " \t["),
            ("/c/cb.inc", "# c\n{"),
            ("/c/bb.inc", "/* c */{"),
            ("/c/ck.inc", "/* c */["),
            ("/c/lk.inc", "# c\n["),
            ("/c/sp.inc", "{ "),
            ("/c/ncb.inc", "\n# c\n{"),
            ("/c/csb.inc", "# c\n  {"),
        ];
        for file in [
            "b.inc", "nb.inc", "k.inc", "sk.inc", "cb.inc", "bb.inc", "ck.inc", "lk.inc",
        ] {
            let v = run(&files, &format!("a = 1\n.include \"{file}\"\nb = 2")).unwrap();
            assert_eq!(keys(&v), ["a", "b"], "{file}");
            // Nothing is taken over: the object keeps its own brace.
            let v = run(
                &files,
                &format!("x {{ .include \"{file}\"\nb = 2 }}\nc = 3"),
            )
            .unwrap();
            assert_eq!(keys(&v), ["x", "c"], "{file}");
            assert_eq!(keys(&obj(&v)["x"]), ["b"], "{file}");
        }
        assert!(run(&files, "a = 1\n.include \"b.inc\"\nb = 2\n}").is_err());
        // With any byte after the `{`, the brace is taken over (§9.4).
        let v = run(&files, "x { .include \"sp.inc\"\nb = 2 }\nc = 3").unwrap();
        assert_eq!(keys(&v), ["x"]);
        assert_eq!(keys(&obj(&v)["x"]), ["b", "c"]);
        // A `{` where no root can start is an error, as in the main document.
        for file in ["ncb.inc", "csb.inc"] {
            assert!(
                run(&files, &format!("a = 1\n.include \"{file}\"\nb = 2")).is_err(),
                "{file}"
            );
        }
        // Under a key, the key's object stays empty.
        let v = run(&files, ".include(key=\"k\") \"b.inc\"\nb = 2").unwrap();
        assert_eq!(keys(&v), ["k", "b"]);
        assert!(obj(&obj(&v)["k"]).is_empty());
        // Text parsed in place follows the same rule; other text that starts with `[` is an
        // error (the project's choice where libucl is uncertain, §13.2).
        let mut p = parser(&files, ParserFlags::DEFAULT);
        p.register_macro("emit", |call| {
            let text = call.value().to_vec();
            call.parse(text)
        });
        let v = p
            .parse(b"a = 1\n.emit \"{\"\nb = 2\no { .emit \"{\" }\nc = 3\n.emit \" [\"")
            .unwrap();
        assert_eq!(keys(&v), ["a", "b", "o", "c"]);
        assert!(obj(&obj(&v)["o"]).is_empty());
        for text in ["[1]", "[]", "[1", "[ "] {
            let e = p
                .parse(format!("a = 1\n.emit \"{text}\"\nb = 2").as_bytes())
                .unwrap_err();
            assert_eq!(e.kind(), &ErrorKind::IncludeArrayRoot, "{text}");
        }
    }

    #[test]
    fn a_nul_byte_ends_a_path() {
        // spec §9.2, *Quirk: a NUL byte in VALUE*; §9.3. Only a braced VALUE can hold one.
        let files = [A, ("/c/text.txt", "hello")];
        for input in [".include {files/a.inc\0zzz}", ".include {files/a.inc\0}"] {
            assert_eq!(keys(&run(&files, input).unwrap()), ["x", "y"], "{input:?}");
        }
        // An empty path before the NUL names no file.
        assert!(matches!(
            run(&files, "a = 1\n.include {\0}\nb = 2").unwrap_err().kind(),
            ErrorKind::FileNotFound { path } if path.is_empty()
        ));
        let e = run(&files, "a = 1\n.try_include {\0}\nb = 2").unwrap_err();
        assert!(e.is_stopped(), "{e}");
        assert_eq!(keys(e.partial().unwrap()), ["a"]);
        #[cfg(feature = "load")]
        {
            let v = run(&files, ".load(key=\"k\") {text.txt\0zz}").unwrap();
            assert_eq!(obj(&v)["k"].as_str(), Some("hello"));
            // An empty VALUE is an error even with `try` (§9.6); a VALUE that starts with a NUL
            // names the empty path as a file, which is missing: skipped with `try`
            // (QUESTIONS.md #71).
            for input in [".load(key=\"k\") {\0zz}", ".load(key=\"k\", try=true) \"\""] {
                assert!(
                    matches!(
                        run(&files, input).unwrap_err().kind(),
                        ErrorKind::FileNotFound { path } if path.is_empty()
                    ),
                    "{input:?}"
                );
            }
            let v = run(&files, ".load(key=\"k\", try=true) {\0zz}\nb = 2").unwrap();
            assert_eq!(keys(&v), ["b"]);
        }
    }

    #[test]
    fn a_wildcard_after_a_nul_byte_makes_a_pattern() {
        // spec §9.4, *Quirk: a NUL byte in a pattern* (QUESTIONS.md #72): the wildcard is looked
        // for in the whole VALUE, and the pattern is the part before the NUL.
        let files = [A];
        let v = run(&files, "a = 1\n.include(glob=true) {files/a.inc\0*}\nb = 2").unwrap();
        assert_eq!(keys(&v), ["a", "x", "y", "b"]);
        for input in [
            "a = 1\n.include(glob=true) {files/nomatch\0*}\nb = 2",
            "a = 1\n.include(glob=true) {\0*}\nb = 2",
        ] {
            let e = run(&files, input).unwrap_err();
            assert!(e.is_stopped(), "{input:?}: {e}");
            assert_eq!(keys(e.partial().unwrap()), ["a"], "{input:?}");
        }
        let v = run(
            &files,
            "a = 1\n.include(glob=true, try=true) {files/nomatch\0?}\nb = 2",
        );
        assert_eq!(keys(&v.unwrap()), ["a", "b"]);
        // Without `glob`, or with no wildcard anywhere, the path is a plain path.
        for input in [
            "a = 1\n.include {files/nomatch\0*}\nb = 2",
            "a = 1\n.include(glob=true) {files/nomatch\0}\nb = 2",
        ] {
            let e = run(&files, input).unwrap_err();
            assert!(
                matches!(e.kind(), ErrorKind::FileNotFound { .. }),
                "{input:?}: {e}"
            );
        }
        // A search path cuts the path at the NUL before the wildcard is looked for.
        let e = run(
            &files,
            "a = 1\n.include(glob=true, path=[\".\"]) {files/a.in\0?}\nb = 2",
        )
        .unwrap_err();
        assert!(matches!(e.kind(), ErrorKind::FileNotFound { .. }), "{e}");
    }

    #[test]
    fn string_parameters_end_at_a_nul_byte() {
        // spec §9.2, *Quirk: a NUL byte in a string parameter* (QUESTIONS.md #78).
        let files = [A, ("/c/text.txt", "hello"), ("/c/num.txt", "42")];
        let v = run(&files, ".include(key=\"s\\u0000t\") \"files/a.inc\"").unwrap();
        assert_eq!(keys(&v), ["s"]);
        let v = run(
            &files,
            ".include(key=\"s\\u0000t\", prefix=true) \"files/a.inc\"",
        )
        .unwrap();
        assert_eq!(keys(&v), ["s"]);
        let v = run(&files, ".include(key=\"\\u0000t\") \"files/a.inc\"").unwrap();
        assert_eq!(keys(&v), [""]);
        let v = run(&files, ".include(path=[\"files\\u0000zz\"]) \"a.inc\"").unwrap();
        assert_eq!(keys(&v), ["x", "y"]);
        let v = run(
            &files,
            "x = 1\n.include(duplicate=\"rewrite\\u0000zz\") \"files/a.inc\"",
        )
        .unwrap();
        assert_eq!(obj(&v).entry("x").unwrap().len(), 1);
        let v = run(
            &files,
            "x = 5\n.include(key=\"x\", target=\"array\\u0000q\") \"files/a.inc\"",
        )
        .unwrap();
        assert_eq!(obj(&v)["x"].as_array().map(|a| a.len()), Some(2));
        #[cfg(feature = "load")]
        {
            let v = run(&files, ".load(key=\"s\\u0000t\") \"text.txt\"").unwrap();
            assert_eq!(keys(&v), ["s"]);
            let e = run(&files, ".load(key=\"\\u0000t\") \"text.txt\"").unwrap_err();
            assert_eq!(e.kind(), &ErrorKind::LoadKeyMissing);
            let v = run(
                &files,
                ".load(key=\"k\", target=\"int\\u0000z\") \"num.txt\"",
            )
            .unwrap();
            assert_eq!(obj(&v)["k"], UclValue::Integer(42));
        }
    }

    #[test]
    fn included_entries_go_where_the_macro_stands() {
        let v = run(
            &[A],
            "k = 0\ns { .include \"files/a.inc\" }\n.include {files/a.inc}",
        )
        .unwrap();
        assert_eq!(keys(&v), ["k", "s", "x", "y"]);
        assert_eq!(keys(&obj(&v)["s"]), ["x", "y"]);
        // The unit's own priority and strategy (§9.4).
        let v = run(
            &[A],
            ".priority 5\nx = 0\n.include(priority=2) \"files/a.inc\"",
        )
        .unwrap();
        assert_eq!(obj(&v).entry("x").unwrap().slots()[0].priority(), 5);
        let v = run(
            &[A],
            "x = 0\n.include(priority=17, duplicate=\"rewrite\") \"files/a.inc\"",
        )
        .unwrap();
        let x = obj(&v).entry("x").unwrap();
        assert_eq!((x.len(), x.slots()[0].priority()), (1, 1));
    }

    #[test]
    fn file_variables_and_relative_paths() {
        let files = [
            (
                "/c/sub/one.inc",
                "f = \"$FILENAME\"\nd = \"${CURDIR}\"\n.include \"files/a.inc\"\n",
            ),
            A,
        ];
        let v = run(
            &files,
            ".include \"sub/one.inc\"\ng = \"$FILENAME $CURDIR\"",
        )
        .unwrap();
        let o = obj(&v);
        assert_eq!(o["f"].as_str(), Some("/c/sub/one.inc"));
        assert_eq!(o["d"].as_str(), Some("/c/sub"));
        // Relative to the base directory, not to the including file (§9.3); restored after.
        assert_eq!(o["x"], UclValue::Integer(1));
        assert_eq!(o["g"].as_str(), Some("undef /c"));
        // Under NO_FILEVARS the included file defines them, and they stay (§12.7, *Quirk*).
        let v = parser(&files, ParserFlags::NO_FILEVARS)
            .parse(b"a = \"$FILENAME\"\n.include \"sub/one.inc\"\ng = \"$CURDIR\"")
            .unwrap();
        assert_eq!(obj(&v)["a"].as_str(), Some("$FILENAME"));
        assert_eq!(obj(&v)["g"].as_str(), Some("/c/sub"));
    }

    #[test]
    fn parse_file_reads_through_the_loader() {
        // Decision 5: a file parsed by path defines FILENAME and CURDIR whatever the flag says.
        let files = [(
            "/c/main.conf",
            "n = \"$FILENAME\"\n.include \"main.conf\"\n",
        )];
        for flags in [ParserFlags::DEFAULT, ParserFlags::NO_FILEVARS] {
            let e = parser(&files, flags).parse_file("main.conf").unwrap_err();
            assert!(matches!(e.kind(), ErrorKind::IncludeSelf { .. }), "{e}");
        }
        let files = [("/c/main.conf", "n = \"$FILENAME\"\n")];
        let v = parser(&files, ParserFlags::NO_FILEVARS)
            .parse_file("/c/./main.conf")
            .unwrap();
        assert_eq!(obj(&v)["n"].as_str(), Some("/c/main.conf"));
        let e = parser(&files, ParserFlags::DEFAULT)
            .parse_file("missing.conf")
            .unwrap_err();
        assert!(matches!(e.kind(), ErrorKind::Io { .. }));
    }

    #[test]
    fn missing_and_unusable_files() {
        // Oracle runs (spec §9.4 table; module docs).
        let files = [
            A,
            (
                "/c/self.inc",
                "s = 1\n.try_include \"${CURDIR}/self.inc\"\nt = 2\n",
            ),
        ];
        let stopped = |input: &str| {
            let e = run(&files, input).unwrap_err();
            assert!(e.is_stopped(), "{input:?}: {e}");
            keys(e.partial().unwrap())
        };
        let failed = |input: &str| {
            let e = run(&files, input).unwrap_err();
            assert!(!e.is_stopped(), "{input:?}");
            e.kind().clone()
        };
        let keys_of = |input: &str| keys(&run(&files, input).unwrap());
        for input in [
            "a = 1\n.try_include \"nope\"\nk = 1",
            "a = 1\n.try_include(try=false) \"nope\"\nk = 1",
            "a = 1\n.try_include \"\"\nk = 1",
            "a = 1\n.try_include \"dir\"\nk = 1",
            "a = 1\n.try_include()#",
        ] {
            assert_eq!(stopped(input), ["a"], "{input:?}");
        }
        assert!(matches!(
            failed(".include \"nope\""),
            ErrorKind::FileNotFound { .. }
        ));
        assert!(matches!(
            failed(".include \"dir\""),
            ErrorKind::NotAFile { .. }
        ));
        assert!(matches!(
            failed(".try_include(try=false) \"dir\""),
            ErrorKind::NotAFile { .. }
        ));
        assert_eq!(keys_of(".include(try=true) \"nope\"\nk = 1"), ["k"]);
        assert_eq!(keys_of(".include(try=true) \"dir\"\nk = 1"), ["k"]);
        assert_eq!(keys_of(".include(try=true) \"\"\nk = 1"), ["k"]);
        // A stop inside an included file ends the whole parse there.
        let e = run(&files, ".include \"self.inc\"\nk = 1").unwrap_err();
        assert!(e.is_stopped());
        assert_eq!(e.file(), Some(Path::new("/c/self.inc")));
        assert_eq!(e.position().line, 2);
        assert_eq!(keys(e.partial().unwrap()), ["s"]);
        let files = [("/c/self.inc", ".include(try=true) \"${CURDIR}/self.inc\"\n")];
        let e = run(&files, ".include \"self.inc\"").unwrap_err();
        assert!(matches!(e.kind(), ErrorKind::IncludeSelf { .. }));
    }

    #[test]
    fn nesting_limit() {
        let mut files = Vec::new();
        for i in 1..=20 {
            files.push((
                format!("/c/{i}.inc"),
                format!("l{i} = 1\n.include \"${{CURDIR}}/{}.inc\"\n", i + 1),
            ));
        }
        files.push(("/c/21.inc".into(), "end = 1\n".into()));
        let files: Vec<(&str, &str)> = files
            .iter()
            .map(|(p, t)| (p.as_str(), t.as_str()))
            .collect();
        // 15 nested includes are fine, 16 are not (§9.4).
        let v = run(&files, ".include \"7.inc\"").unwrap();
        assert_eq!(obj(&v).len(), 15);
        let e = run(&files, ".include \"6.inc\"").unwrap_err();
        assert_eq!(
            e.kind(),
            &ErrorKind::IncludeTooDeep {
                limit: MAX_INCLUDE_DEPTH
            }
        );
        assert_eq!(e.file(), Some(Path::new("/c/20.inc")));
    }

    #[test]
    fn errors_in_included_files_name_the_file() {
        let files = [("/c/bad.inc", "a = 1\nb = \"open")];
        let e = run(&files, "x = 1\n.include \"bad.inc\"").unwrap_err();
        assert_eq!(e.kind(), &ErrorKind::UnterminatedString);
        assert_eq!(e.file(), Some(Path::new("/c/bad.inc")));
        assert_eq!((e.position().line, e.position().column), (2, 5));
        assert!(e.to_string().ends_with("of /c/bad.inc)"), "{e}");
        let e = run(&[], "x = 1\n.include \"nope.inc\"").unwrap_err();
        assert_eq!(e.file(), None);
        assert_eq!(e.position().line, 2);
    }

    #[test]
    fn braces_around_included_files() {
        // spec §9.4, *Quirk: braces around an included file*; section objects from oracle runs.
        let files = [
            ("/c/braced.inc", "{ a = 1 }\n"),
            ("/c/open.inc", "{ a = 1\n"),
            ("/c/close.inc", "a = 1 }\n"),
            ("/c/left_open.inc", "x \"y{\" z\n"),
            ("/c/array.inc", "[1]\n"),
            ("/c/unclosed.inc", "b {\n"),
        ];
        let ok = |input: &str| run(&files, input).unwrap();
        let err = |input: &str| run(&files, input).unwrap_err().kind().clone();
        assert_eq!(keys(&ok(".include \"braced.inc\"\nq = 1")), ["a", "q"]);
        assert!(matches!(
            err("x { .include \"braced.inc\" }"),
            ErrorKind::UnmatchedClose { .. }
        ));
        assert_eq!(
            keys(&ok(".include \"open.inc\"\nq = 1\n}\nr = 2")),
            ["a", "q", "r"]
        );
        assert_eq!(
            err(".include \"open.inc\"\nq = 1"),
            ErrorKind::UnterminatedObject
        );
        let v = ok("x { .include \"open.inc\"\n}\nq = 1");
        assert_eq!(keys(&obj(&v)["x"]), ["a", "q"]);
        let v = ok("x { .include \"close.inc\"\nq = 1");
        assert_eq!(keys(&v), ["x", "q"]);
        assert!(matches!(
            err(".include \"close.inc\""),
            ErrorKind::UnmatchedClose { .. }
        ));
        assert_eq!(
            err(".include \"unclosed.inc\"\n}"),
            ErrorKind::UnterminatedObject
        );
        assert_eq!(err(".include \"array.inc\""), ErrorKind::IncludeArrayRoot);
        // Section objects left open by a file stay open for the including unit.
        let v = ok(".include \"left_open.inc\"\nk = 1");
        assert_eq!(keys(&obj(&v)["x"]), ["y{", "k"]);
        // A section object whose brace a file took over closes at its `}`, and also when a
        // bracketed container opened in it closes, which takes the brace with it (oracle runs).
        let v = ok("x \"y{\" z\n.include \"braced.inc\"\nq = 1");
        assert_eq!(keys(&v), ["x", "q"]);
        let v = ok("x \"y{\" z\n.include \"open.inc\"\nm = [1]\nn = 1");
        assert_eq!(keys(&v), ["x", "n"]);
        assert_eq!(keys(&obj(&v)["x"]), ["y{", "a", "m"]);
        assert!(matches!(
            err("x \"y{\" z\n.include \"open.inc\"\nm { }\n}"),
            ErrorKind::UnmatchedClose { .. }
        ));
        assert_eq!(
            err("x \"y{\" z\n.include \"open.inc\"\nq = 1"),
            ErrorKind::UnterminatedObject
        );
        // At the end of input, section objects from an included file end the parse without
        // the containers below them being checked (QUESTIONS.md #32).
        let v = ok("a { b {\n.include \"left_open.inc\"\nk = 1");
        assert_eq!(keys(&obj(&obj(&obj(&v)["a"])["b"])["x"]), ["y{", "k"]);
        assert_eq!(
            err("a {\n.include \"left_open.inc\"\nm { n = 1 }"),
            ErrorKind::UnterminatedObject
        );
        // Nested under a key: the key's containers close at the end of the file.
        let v = ok("x { .include(key=\"k\") \"braced.inc\"\ny = 1 }\nz = 2");
        assert_eq!(keys(&obj(&v)["x"]), ["k", "y"]);
        let v = ok(".include(key=\"k\") \"left_open.inc\"\nq = 1");
        assert_eq!(keys(&v), ["k", "q"]);
        // A `}` in a file nested under a key: an error unless the object where the macro stands
        // holds a taken-over brace, of which the key's object then uses up its share (spec
        // §9.4, *Nesting under a key*; the other forms from oracle runs).
        assert!(matches!(
            err(".include(key=\"k\") \"close.inc\"\nq = 1"),
            ErrorKind::UnmatchedClose { .. }
        ));
        let v = ok(".include \"open.inc\"\n.include(key=\"k\") \"close.inc\"\nq = 1\n}");
        assert_eq!(keys(&v), ["a", "k", "q"]);
        assert_eq!(keys(&obj(&v)["k"]), ["a"]);
        let v = ok(
            ".include \"open.inc\"\n.include(key=\"k\", target=\"array\") \"close.inc\"\nq = 1\n}",
        );
        assert_eq!(keys(&v), ["a", "k", "q"]);
        assert_eq!(keys(&obj(&v)["k"].as_array().unwrap()[0]), ["a"]);
        let v = ok(".include \"open.inc\"\n.include(prefix=true) \"close.inc\"\nq = 1\n}");
        assert_eq!(keys(&v), ["a", "close.inc", "q"]);
        let v = ok(".include \"open.inc\"\n.include(key=\"k\") \"close.inc\"\n\
                    .include(key=\"k\") \"close.inc\"\nq = 1\n}");
        assert_eq!(obj(&obj(&v)["k"]).entry("a").unwrap().len(), 2);
        let v = ok(
            "x \"y{\" z\n.include \"open.inc\"\n.include(key=\"k\") \"close.inc\"\nq = 1\n}\nr = 2",
        );
        assert_eq!(keys(&v), ["x", "r"]);
        assert_eq!(keys(&obj(&v)["x"]), ["y{", "a", "k", "q"]);
        let v = ok(".include \"open.inc\"\n.include(key=\"k\") \"braced.inc\"\nq = 1\n}");
        assert_eq!(keys(&v), ["a", "k", "q"]);
        for input in [
            // The share is used once; the root's brace is still unclosed without the last `}`.
            ".include \"open.inc\"\n.include(key=\"k\") \"close2.inc\"\nq = 1",
            ".include \"open.inc\"\n.include(key=\"k\") \"close.inc\"\nq = 1",
            ".include \"open.inc\"\n.include \"open.inc\"\n.include(key=\"k\") \"close.inc\"\nq = 1\n}\n}",
            // Uncertain in the spec (libucl crashes): the object has only its own brace.
            "x { .include(key=\"k\") \"close.inc\"\nq = 1",
        ] {
            assert!(run(&files_with_close2(&files), input).is_err(), "{input:?}");
        }
    }

    #[test]
    fn macros_directly_after_a_name() {
        // spec §9.1, *A macro directly after a name*; the forms beyond its examples are from
        // oracle runs (QUESTIONS.md #34–#36, #38).
        let files = [
            A,
            ("/c/o.inc", "o {}\n"),
            ("/c/m.inc", "m {}\n"),
            ("/c/closed.inc", "\"s\".include \"o.inc\" # [\n"),
            ("/c/closed_ws.inc", "\"s\".include {o.inc}\n"),
            (
                "/c/closed_later.inc",
                "\"s\".include {o.inc} # c\n.priority {1} # d\n",
            ),
            (
                "/c/closed_then_macro.inc",
                "\"s\".include {o.inc} # c\n.priority {1}\n",
            ),
            ("/c/closed_brace.inc", "x {\n\"s\".include {o.inc} # c\n}\n"),
            ("/c/deep.inc", "\"s\".include(key=\"k\") \"m.inc\" # [\n"),
            ("/c/unit.inc", "a {\n\"s\".include(key=\"k\") {m.inc} # c\n"),
            ("/c/lower.inc", ".priority 1\n\"s\".include {o.inc} # c\n"),
            ("/c/key.inc", "\"s\".include {o.inc}\nz = 2\n"),
        ];
        for flags in [ParserFlags::DEFAULT, ParserFlags::SAVE_COMMENTS] {
            let ok = |input: &str| parser(&files, flags).parse(input.as_bytes()).unwrap();
            let err = |input: &str| {
                let result = parser(&files, flags).parse(input.as_bytes());
                assert!(result.is_err(), "{input:?}");
            };
            // The next key read in the unit counts as a word that follows a name.
            for input in [
                "\"s\".priority {3}k = [1]",
                "\"s\".priority {3}\n\"k\" = [1]",
                "\"s\".include \"files/a.inc\"k = [1]",
                "\"s\".priority {3}\n.priority 4\nk = [1]",
                "\"s\".priority {3}\n.include \"files/a.inc\"\nk = [1]",
                "x { \"s\".include {o.inc} }\nk = [1]",
                "\"s\".priority {3}\na = 1",
                "\"s\".priority {3}\nk = \n{ z = 1 }",
                ".include \"key.inc\"",
            ] {
                err(input);
            }
            let v = ok("\"s\".priority {3}k = l = n { z = 1 }\nm = 1");
            assert_eq!(keys(&v), ["s", "m"]);
            let l = &obj(&obj(&obj(&v)["s"])["k"])["l"];
            assert_eq!(obj(l).entry("n").unwrap().slots()[0].priority(), 3);
            let v = ok("\"s\".priority {3}\nk =\nl { z = 1 }");
            assert_eq!(keys(&obj(&obj(&obj(&v)["s"])["k"])["l"]), ["z"]);
            let v = ok("x { \"s\".priority {3}k = l {} }\nm = 1");
            assert_eq!(keys(&v), ["x", "m"]);
            let v = ok("\"s\".priority {3}\nk \"b{\" z\nm = [1]");
            assert_eq!(keys(&obj(&obj(&v)["s"])["k"]), ["b{", "m"]);
            // Comments to the end of the unit reopen the value created most recently.
            let v = ok(".include \"closed.inc\"\nk = 1");
            assert_eq!(keys(&v), ["s"]);
            assert_eq!(keys(&obj(&v)["s"]), ["o", "k"]);
            let v = ok(".include \"closed_later.inc\"\nk = 1");
            assert_eq!(keys(&obj(&v)["s"]), ["o", "k"]);
            for input in [
                ".include \"closed_ws.inc\"\nk = 1",
                ".include \"closed_then_macro.inc\"\nk = 1",
            ] {
                assert_eq!(keys(&ok(input)), ["s", "k"], "{input:?}");
            }
            assert_eq!(
                keys(&ok(".include \"closed_brace.inc\"\nk = 1")),
                ["x", "k"]
            );
            let v = ok(".include \"deep.inc\"\nk2 = 1");
            assert_eq!(keys(&obj(&obj(&obj(&v)["s"])["k"])["m"]), ["k2"]);
            // The reopened object belongs to the unit that reopens it (§9.4, the end check).
            let v = ok("a {\n.include \"closed.inc\"\nk = 1");
            assert_eq!(keys(&obj(&obj(&v)["a"])["s"]), ["o", "k"]);
            err("a {\n.include \"closed.inc\"\nk = 1\n}\nm = 1");
            err(".include \"unit.inc\"\nk2 = 1");
            // A discarded object is reopened discarded.
            let v = ok(".priority 5\ns = 1\n.include \"lower.inc\"\nk = 1");
            assert_eq!(keys(&v), ["s"]);
        }
    }

    #[test]
    fn end_of_unit_check_and_first_key_share() {
        // Oracle runs (QUESTIONS.md #41, #42).
        let files = [
            ("/c/left_open.inc", "x \"y{\" z\n"),
            ("/c/braced.inc", "{ a = 1 }\n"),
            ("/c/lo_in_b.inc", "b {\n.include \"left_open.inc\"\n"),
            ("/c/lost_brace.inc", "x { .include {braced.inc}\n"),
            ("/c/arr.inc", "a = [ {\n.include \"left_open.inc\"\n"),
            ("/c/first.inc", "{\nx \"y{\" z\n"),
            ("/c/first_closed.inc", "{ x \"y{\" z\n}\n"),
            ("/c/first_closed2.inc", "{ x \"y{\" z\n}\n}\n"),
            ("/c/first_two_names.inc", "{ a b \"y{\" z\n}\n"),
            (
                "/c/first_after_macro.inc",
                "{\n.priority 1\nx \"y{\" z\n}\n",
            ),
            ("/c/second.inc", "{ a = 1\nx \"y{\" z\n"),
            ("/c/first_run.inc", "{ \"s\".priority {3}\n}\n"),
            ("/c/brace_gone.inc", "{}x \"y{\" z\n"),
        ];
        let ok = |input: &str| run(&files, input).unwrap();
        let err = |input: &str| assert!(run(&files, input).is_err(), "{input:?}");
        // The check stops at the first container another unit opened, whatever it is.
        let v = ok("a {\n.include \"lo_in_b.inc\"\nm {}");
        assert_eq!(keys(&obj(&obj(&obj(&v)["a"])["b"])["x"]), ["y{", "m"]);
        let v = ok("a {\n.include \"lost_brace.inc\"\nk = 1");
        assert_eq!(keys(&obj(&obj(&v)["a"])["x"]), ["a", "k"]);
        let v = ok(".include \"arr.inc\"\nm { n = 1 }\n}");
        assert_eq!(keys(&v), ["a"]);
        // The first key after a file's leading `{`: its first name shares the brace.
        for input in [
            ".include \"first.inc\"",
            ".include \"first.inc\"\nq = 1\n}",
            "a {\n.include \"first.inc\"\nk = 1",
            ".include \"first_closed.inc\"\nq = 1",
            ".include \"first_closed2.inc\"\nq = 1\n}",
            ".include \"first_two_names.inc\"\nq = 1\n}",
            ".include \"first_after_macro.inc\"\nq = 1",
            ".include \"first_run.inc\"\nq = 1",
            ".include \"second.inc\"\nq = 1\n}",
        ] {
            err(input);
        }
        assert_eq!(
            keys(&ok(".include \"first_closed.inc\"\nq = 1\n}")),
            ["x", "q"]
        );
        assert_eq!(
            keys(&ok(".include \"first_closed2.inc\"\nq = 1")),
            ["x", "q"]
        );
        assert_eq!(
            keys(&ok(".include \"first_after_macro.inc\"\nq = 1\n}")),
            ["x", "q"]
        );
        assert_eq!(
            keys(&ok(".include \"first_run.inc\"\nq = 1\n}")),
            ["s", "q"]
        );
        assert_eq!(
            keys(&ok(".include(key=\"k\") \"first_closed.inc\"\nq = 1")),
            ["k", "q"]
        );
        let v = ok(".include \"second.inc\"\nq = 1");
        assert_eq!(keys(&obj(&v)["x"]), ["y{", "q"]);
        // Not once the `}` has removed that brace.
        let v = ok(".include \"brace_gone.inc\"\nq = 1");
        assert_eq!(keys(&obj(&v)["x"]), ["y{", "q"]);
    }

    #[test]
    fn comments_after_the_first_name_brace() {
        // spec §12.5 and §9.4 (QUESTIONS.md #83): the `}` that closes the first name's object is
        // its own bracket, so the value created most recently stays what it was; a bracketed
        // container that closes it, or a section object whose brace was taken over, makes the
        // outermost object closed the most recent value.
        let files = [
            ("/c/twice.inc", "{ x \"y{\" z\n}\n}"),
            ("/c/once.inc", "{ x \"y{\" z\n}"),
            ("/c/between.inc", "{ x \"y{\" z\n} # c\n}"),
            ("/c/entry.inc", "{ x \"y{\" z\nk = 2\n}\n}"),
            ("/c/by_object.inc", "{ x \"y{\" z\na { b = 1 }\n}"),
            ("/c/braced.inc", "{ a = 1 }"),
            ("/c/close_brace.inc", "a = 1 }"),
            // A file included in the name's object takes over its brace, or closes it (oracle
            // runs).
            ("/c/taken.inc", "{ x \"y{\" z\n.include \"braced.inc\"\n}"),
            (
                "/c/closed.inc",
                "{ x \"y{\" z\n.include \"close_brace.inc\"\n}",
            ),
        ];
        let after = |input: &str| -> Vec<String> {
            let mut p = parser(&files, ParserFlags::SAVE_COMMENTS);
            p.parse(input.as_bytes()).unwrap();
            let groups = p.attached_comments();
            assert_eq!(groups.len(), 1, "{input:?}");
            assert_eq!(groups[0].placement, CommentPlacement::After, "{input:?}");
            groups[0]
                .path
                .iter()
                .map(|segment| match segment {
                    PathSegment::Key { key, index: 0 } => key.clone(),
                    other => panic!("{input:?}: {other:?}"),
                })
                .collect()
        };
        for (input, path) in [
            ("_ = 1\n.include \"twice.inc\"\n# c", &["x", "y{"][..]),
            ("o { .include \"twice.inc\"\n# c", &["o", "x", "y{"]),
            (".include(key=\"k\") \"twice.inc\"\n# c", &["k", "x", "y{"]),
            (
                ".include(prefix=true) \"twice.inc\"\n# c",
                &["twice.inc", "x", "y{"],
            ),
            (".include \"between.inc\"\nq = 1", &["x", "y{"]),
            (".include \"entry.inc\"\n# c", &["x", "k"]),
            (".include \"once.inc\"\n# c\n}", &["x", "y{"]),
            (".include \"once.inc\"\n}\n# c", &["x", "y{"]),
            (".include \"closed.inc\"\n# c", &["x", "a"]),
            // Unchanged: closed by a bracketed container, as a section object, or by a nested
            // file that took its brace over.
            (".include \"by_object.inc\"\n# c", &["x"]),
            ("s \"t{\" u\n.include \"once.inc\"\n# c", &["s"]),
            ("_ = 1\n.include \"taken.inc\"\n# c", &["x"]),
        ] {
            assert_eq!(after(input), path, "{input:?}");
        }
    }

    #[test]
    fn first_name_brace_after_text_in_place() {
        // §13.2 keeps a section object open after text parsed in place, but not against the
        // brace of its own that §9.4 gives a first name, which a `}` closes (§12.5; oracle
        // runs). A bracketed container that closes in it no longer closes it.
        let files = [
            ("/c/entry.inc", "{ x \"y{\" z\n.emit \"\"\n}\nk = 2\n}"),
            ("/c/inner.inc", "{ x \"y{\" z\n.emit \"\"\na { }\n}\n}"),
            ("/c/open.inc", "{ x \"y{\" z\n.emit \"\"\na { }\n}"),
        ];
        let run = |input: &str| {
            let mut p = parser(&files, ParserFlags::DEFAULT);
            p.register_macro("emit", |call| {
                let text = call.value().to_vec();
                call.parse(text)
            });
            p.parse(input.as_bytes())
        };
        let v = run(".include \"entry.inc\"\nq = 3").unwrap();
        assert_eq!(keys(&v), ["x", "k", "q"]);
        assert_eq!(keys(&obj(&v)["x"]), ["y{"]);
        let v = run(".include \"inner.inc\"\nq = 1").unwrap();
        assert_eq!(keys(&v), ["x", "q"]);
        assert_eq!(keys(&obj(&v)["x"]), ["y{", "a"]);
        assert_eq!(
            run(".include \"open.inc\"\nq = 1").unwrap_err().kind(),
            &ErrorKind::UnterminatedObject
        );
    }

    #[test]
    fn first_name_brace_shared_with_a_key_object() {
        // A file nested under a key in the first name's object uses up a share of its brace
        // with a `}`, as under a brace taken over (§9.4, *Nesting under a key*). §9.4 leaves an
        // object that holds only its own bracket undefined; the crate keeps 0.6.0's result here
        // and rejects the `}` under an object written with braces (QUESTIONS.md #85).
        let files = [
            ("/c/close_brace.inc", "a = 1 }"),
            (
                "/c/nest.inc",
                "{ x \"y{\" z\n.include(key=\"k\") \"close_brace.inc\"\n}\n}",
            ),
        ];
        let v = run(&files, ".include \"nest.inc\"\nq = 1").unwrap();
        assert_eq!(keys(&v), ["x", "q"]);
        let x = &obj(&v)["x"];
        assert_eq!(keys(x), ["y{", "k"]);
        assert_eq!(obj(&obj(x)["k"])["a"].as_integer(), Some(1));
        assert!(matches!(
            run(&files, "x { .include(key=\"k\") \"close_brace.inc\"\nq = 1")
                .unwrap_err()
                .kind(),
            ErrorKind::UnmatchedClose { .. }
        ));
    }

    #[test]
    fn name_run_reopens_the_value_before_the_first_name_brace() {
        // §9.1: comments to the end of a unit after a macro directly after a name reopen the
        // value created most recently, which the first name's own `}` leaves as it was (§12.5).
        use crate::parse::Uncertain;
        let files = [
            ("/c/twice.inc", "{ x \"y{\" z\n}\n}"),
            ("/c/e.inc", "{ x \"y{\" z\n.emit \"\"\nk { }\n}\n}"),
            ("/c/reopen_e.inc", "\"s\".include(key=\"q\") {e.inc} # c"),
            (
                "/c/reopen_twice.inc",
                "\"s\".include(key=\"q\") {twice.inc} # c",
            ),
        ];
        let run = |input: &str| {
            let mut p = parser(&files, ParserFlags::DEFAULT);
            p.register_macro("emit", |call| {
                let text = call.value().to_vec();
                call.parse(text)
            });
            let v = p.parse(input.as_bytes()).unwrap();
            (v, p.uncertain_reached())
        };
        // `k` is the value created most recently: it is reopened, as by the oracle.
        let (v, reached) = run(".include \"reopen_e.inc\"\nm = 1");
        assert_eq!(keys(&v), ["s"]);
        let x = &obj(&obj(&obj(&v)["s"])["q"])["x"];
        assert_eq!(keys(x), ["y{", "k"]);
        assert_eq!(keys(&obj(x)["k"]), ["m"]);
        assert_eq!(reached, []);
        // `z` is: not an object, so nothing is reopened (§9.1, *Uncertain*; the oracle crashes).
        let (v, reached) = run(".include \"reopen_twice.inc\"\nm = 1");
        assert_eq!(keys(&v), ["s"]);
        assert_eq!(keys(&obj(&v)["s"]), ["q", "m"]);
        assert_eq!(keys(&obj(&obj(&obj(&v)["s"])["q"])["x"]), ["y{"]);
        assert_eq!(reached, [Uncertain::ReopenedNotObject]);
    }

    /// A parser over `files` that has the test macro `.emit` of spec §13.2: its VALUE is parsed
    /// in place.
    fn emitting(files: &[(&str, &str)], flags: ParserFlags) -> Parser {
        let mut p = parser(files, flags);
        p.register_macro("emit", |call| {
            let text = call.value().to_vec();
            call.parse(text)
        });
        p
    }

    /// The path of the one comment group saved by parsing `input`, which must attach after its
    /// value.
    fn comment_after(p: &mut Parser, input: &str) -> Vec<String> {
        p.parse(input.as_bytes()).unwrap();
        let groups = p.attached_comments();
        assert_eq!(groups.len(), 1, "{input:?}");
        assert_eq!(groups[0].placement, CommentPlacement::After, "{input:?}");
        groups[0]
            .path
            .iter()
            .map(|segment| match segment {
                PathSegment::Key { key, index: 0 } => key.clone(),
                other => panic!("{input:?}: {other:?}"),
            })
            .collect()
    }

    #[test]
    fn macros_before_the_first_key_of_a_braced_file() {
        // spec §9.4, *Quirk: macros before the first key* (QUESTIONS.md #87): until a braced
        // file reads a key of its own, it takes the brace over again after each macro, so a
        // nested `}` that removed it does not end the takeover, and the first name still gets
        // its brace.
        let files = [
            ("/c/braced.inc", "{ a = 1 }\n"),
            ("/c/close_brace.inc", "a = 1 }\n"),
            ("/c/left_open.inc", "x \"y{\" z\n"),
            ("/c/open_brace.inc", "{ a = 1\n"),
            ("/c/nb_entry.inc", "{ .include \"braced.inc\"\na2 = 2\n}"),
            (
                "/c/nb_name.inc",
                "{ .include \"braced.inc\"\nx \"y{\" z\n}\n}",
            ),
            (
                "/c/nc_name.inc",
                "{ .include \"close_brace.inc\"\nx \"y{\" z\n}\n}",
            ),
            (
                "/c/np_name.inc",
                "{ .include \"left_open.inc\"\np \"q{\" r\n}\n}",
            ),
            (
                "/c/np_entry.inc",
                "{ .include \"left_open.inc\"\nw = 1\n}\n}",
            ),
            ("/c/np_braces.inc", "{ .include \"left_open.inc\"\n}\n}"),
            ("/c/np_one.inc", "{ .include \"left_open.inc\"\nw = 1\n}"),
            ("/c/entry_nb.inc", "{ a0 = 0\n.include \"braced.inc\"\n}"),
            (
                "/c/twice_nb.inc",
                "{ .include \"braced.inc\"\n.include \"braced.inc\"\na2 = 2\n}",
            ),
            (
                "/c/inherit_nb.inc",
                "{ .include \"braced.inc\"\n.inherit \"w\"\nx \"y{\" z\n}\n}",
            ),
            ("/c/nested_open.inc", "{ .include \"open_brace.inc\"\n}"),
            ("/c/text_nb.inc", "{ .emit \"{ a = 1 }\"\nx \"y{\" z\n}\n}"),
            ("/c/text_entry.inc", "{ .emit \"a = 1\"\nx \"y{\" z\n}\n}"),
        ];
        let p = || emitting(&files, ParserFlags::DEFAULT);
        let ok = |input: &str| p().parse(input.as_bytes()).unwrap();
        let err = |input: &str| p().parse(input.as_bytes()).unwrap_err().kind().clone();
        assert_eq!(
            keys(&ok(".include \"nb_entry.inc\"\nq = 1")),
            ["a", "a2", "q"]
        );
        for file in [
            "nb_name.inc",
            "nc_name.inc",
            "text_nb.inc",
            "text_entry.inc",
        ] {
            let v = ok(&format!(".include \"{file}\"\nq = 1"));
            assert_eq!(keys(&v), ["a", "x", "q"], "{file}");
            assert_eq!(keys(&obj(&v)["x"]), ["y{"], "{file}");
        }
        let v = ok(".include(key=\"k\") \"nc_name.inc\"\nq = 1");
        assert_eq!(keys(&v), ["k", "q"]);
        assert_eq!(keys(&obj(&v)["k"]), ["a", "x"]);
        // A nested file that leaves a section path open: the name goes inside it.
        let v = ok(".include \"np_name.inc\"\nq = 1");
        assert_eq!(keys(&v), ["x", "q"]);
        assert_eq!(keys(&obj(&v)["x"]), ["y{", "p"]);
        // The brace taken over again is added to the one the file still holds.
        let v = ok(".include \"np_entry.inc\"\nq = 1");
        assert_eq!(keys(&v), ["x", "q"]);
        assert_eq!(keys(&obj(&v)["x"]), ["y{", "w"]);
        assert_eq!(keys(&ok(".include \"np_braces.inc\"\nq = 1")), ["x", "q"]);
        assert_eq!(
            err(".include \"np_one.inc\"\nq = 1"),
            ErrorKind::UnterminatedObject
        );
        let v = ok(".include \"np_one.inc\"\nq = 1\n}");
        assert_eq!(keys(&v), ["x", "q"]);
        assert_eq!(keys(&obj(&v)["x"]), ["y{", "w"]);
        // Not after a key of the file's own.
        assert!(matches!(
            err(".include \"entry_nb.inc\"\nq = 1"),
            ErrorKind::UnmatchedClose { .. }
        ));
        // After each macro; keys a macro adds are not the file's own.
        let v = ok(".include \"twice_nb.inc\"\nq = 1");
        assert_eq!(keys(&v), ["a", "a2", "q"]);
        assert_eq!(obj(&v).entry("a").unwrap().len(), 2);
        let v = ok("w { m = 1 }\n.include \"inherit_nb.inc\"\nq = 1");
        assert_eq!(keys(&v), ["w", "a", "m", "x", "q"]);
        // A brace the file still holds is not taken over a second time (oracle runs).
        assert_eq!(keys(&ok(".include \"nested_open.inc\"\nq = 1")), ["a", "q"]);
        assert!(matches!(
            err(".include \"nested_open.inc\"\nq = 1\n}"),
            ErrorKind::UnmatchedClose { .. }
        ));
        // Text parsed in place takes the brace over in the same way.
        let v = ok(".emit \"{ .include braced.inc;a2 = 2;}\"\nq = 1");
        assert_eq!(keys(&v), ["a", "a2", "q"]);
        // A comment after such a file goes to `z` (§12.5).
        for file in ["nb_name.inc", "text_entry.inc"] {
            let mut parser = emitting(&files, ParserFlags::SAVE_COMMENTS);
            let input = format!(".include \"{file}\"\n# c");
            assert_eq!(comment_after(&mut parser, &input), ["x", "y{"], "{file}");
        }
    }

    #[test]
    fn brace_taken_over_again_only_when_the_file_goes_on() {
        // Oracle runs: the brace is taken over again only when the file holds more than
        // whitespace and `;` after the macro (QUESTIONS.md #89), and the file's own `}` ends
        // the takeover as its first key does (QUESTIONS.md #90).
        let files = [
            ("/c/braced.inc", "{ a = 1 }\n"),
            ("/c/close_brace.inc", "a = 1 }\n"),
            ("/c/left_open.inc", "x \"y{\" z\n"),
            ("/c/end_nl.inc", "{ .include \"braced.inc\"\n"),
            ("/c/end_eof.inc", "{ .include \"braced.inc\""),
            ("/c/end_semi.inc", "{ .include \"braced.inc\"\n;\n"),
            ("/c/end_comment.inc", "{ .include \"braced.inc\"\n# c\n"),
            ("/c/end_block.inc", "{ .include \"braced.inc\"\n/* c */"),
            (
                "/c/end_macro.inc",
                "{ .include \"braced.inc\"\n.priority 1\n",
            ),
            (
                "/c/end_two.inc",
                "{ .include \"braced.inc\"\n.include \"close_brace.inc\"\n",
            ),
            ("/c/end_open.inc", "{ .include \"left_open.inc\"\n"),
            ("/c/end_open_c.inc", "{ .include \"left_open.inc\"\n# c\n"),
            (
                "/c/own_then_nb.inc",
                "{ }\n.include \"braced.inc\"\na2 = 2\n}",
            ),
            ("/c/own_then_macro.inc", "{ }\n.priority 1\n}"),
            (
                "/c/own_close_then_nb.inc",
                "{ .include \"left_open.inc\"\n}\n.include \"braced.inc\"\n}",
            ),
            (
                "/c/own_close_then_name.inc",
                "{ .include \"left_open.inc\"\n}\np \"q{\" r\n}\n}",
            ),
            (
                "/c/nc_name.inc",
                "{ .include \"close_brace.inc\"\nx \"y{\" z\n}\n}",
            ),
        ];
        let p = || emitting(&files, ParserFlags::DEFAULT);
        let ok = |input: &str| p().parse(input.as_bytes()).unwrap();
        let err = |input: &str| p().parse(input.as_bytes()).unwrap_err().kind().clone();
        let unmatched = |input: &str| {
            assert!(
                matches!(err(input), ErrorKind::UnmatchedClose { .. }),
                "{input:?}"
            );
        };
        // After the last macro, whitespace and `;` to the end of the file: no brace is held.
        for file in ["end_nl.inc", "end_eof.inc", "end_semi.inc", "end_two.inc"] {
            let v = ok(&format!(".include \"{file}\"\nq = 1"));
            assert_eq!(keys(&v), ["a", "q"], "{file}");
            unmatched(&format!(".include \"{file}\"\nq = 1\n}}"));
        }
        for file in ["end_comment.inc", "end_block.inc", "end_macro.inc"] {
            assert_eq!(
                err(&format!(".include \"{file}\"\nq = 1")),
                ErrorKind::UnterminatedObject,
                "{file}"
            );
            let v = ok(&format!(".include \"{file}\"\nq = 1\n}}"));
            assert_eq!(keys(&v), ["a", "q"], "{file}");
        }
        // The same for text parsed in place.
        assert_eq!(
            keys(&ok(".emit \"{ .include braced.inc;\"\nq = 1")),
            ["a", "q"]
        );
        assert_eq!(
            err(".emit \"{ .include braced.inc; /* c */\"\nq = 1"),
            ErrorKind::UnterminatedObject
        );
        // A section object the nested file left open keeps no brace at the end of the file,
        // and takes one over when the file goes on.
        unmatched(".include \"end_open.inc\"\nq = 1\n}\nr = 2");
        let v = ok(".include \"end_open_c.inc\"\nq = 1\n}\nr = 2\n}");
        assert_eq!(keys(&v), ["x", "r"]);
        assert_eq!(keys(&obj(&v)["x"]), ["y{", "q"]);
        assert_eq!(
            err(".include \"end_open_c.inc\"\nq = 1\n}\nr = 2"),
            ErrorKind::UnterminatedObject
        );
        for file in [
            "own_then_nb.inc",
            "own_then_macro.inc",
            "own_close_then_nb.inc",
            "own_close_then_name.inc",
        ] {
            unmatched(&format!(".include \"{file}\"\nq = 1"));
        }
        // The object the entries go into after the nested file: the root, when the nested
        // file's `}` closed the section object whose brace was taken over, or an object
        // written with braces, which then loses its brace.
        let v = ok("s \"t{\" u\n.include \"nc_name.inc\"\nq = 1");
        assert_eq!(keys(&v), ["s", "x", "q"]);
        assert_eq!(keys(&obj(&v)["s"]), ["t{", "a"]);
        unmatched("s \"t{\" u\n.include \"nc_name.inc\"\nq = 1\n}");
        let v = ok("o { s \"t{\" u\n.include \"nc_name.inc\"\nq = 1");
        assert_eq!(keys(&obj(&v)["o"]), ["s", "x", "q"]);
        unmatched("o { s \"t{\" u\n.include \"nc_name.inc\"\nq = 1\n}");
    }
    #[test]
    fn empty_files_and_merged_nulls() {
        // Oracle runs (QUESTIONS.md #43, #44).
        let files = [
            ("/c/empty.txt", ""),
            ("/c/ws.inc", "\n"),
            ("/c/kv.inc", "k =\n# c\n"),
        ];
        let p = |flags| parser(&files, flags | ParserFlags::SAVE_COMMENTS);
        let placements = |p: &Parser| -> Vec<(Vec<PathSegment>, CommentPlacement)> {
            p.attached_comments()
                .iter()
                .map(|g| (g.path.clone(), g.placement))
                .collect()
        };
        let key = |k: &str| PathSegment::Key {
            key: k.into(),
            index: 0,
        };
        // Comments pending before a file of no bytes stay pending.
        let mut parser = p(ParserFlags::DEFAULT);
        parser
            .parse(b"a = 1\n# c\n.include \"empty.txt\"\nb = 1")
            .unwrap();
        assert_eq!(
            placements(&parser),
            [(vec![key("b")], CommentPlacement::Before)]
        );
        let mut parser = p(ParserFlags::DEFAULT);
        parser
            .parse(b"a = 1\n# c\n.include \"ws.inc\"\nb = 1")
            .unwrap();
        assert_eq!(
            placements(&parser),
            [(vec![key("a")], CommentPlacement::After)]
        );
        let mut parser = p(ParserFlags::DEFAULT);
        parser
            .parse(b"# c\n.include(key=\"k\") \"empty.txt\"\nb = 1")
            .unwrap();
        assert_eq!(
            placements(&parser),
            [(vec![key("b")], CommentPlacement::Before)]
        );
        // Under merge, the null that ends a unit goes into a container first value.
        for (input, merged) in [
            ("k { a = 1 }\n# c\nk =\n", true),
            ("k = [1]\nk =\n", true),
            ("k = 1\nk =\n", false),
            ("k { a = 1 }\nk = null", false),
        ] {
            let mut parser = p(ParserFlags::DEFAULT);
            parser.set_strategy(DuplicateStrategy::Merge);
            let v = parser.parse(input.as_bytes()).unwrap();
            let k = obj(&v).entry("k").unwrap();
            assert_eq!(k.first().is_null(), !merged && k.len() == 1, "{input:?}");
            assert_eq!(
                k.first().is_object() || k.first().is_array(),
                merged,
                "{input:?}"
            );
        }
        let mut parser = p(ParserFlags::DEFAULT);
        parser.set_strategy(DuplicateStrategy::Merge);
        parser.parse(b"k { a = 1 }\n# c\nk =\n").unwrap();
        assert_eq!(
            placements(&parser),
            [(vec![key("k")], CommentPlacement::Before)]
        );
        let v = run(
            &files,
            "k { a = 1 }\n.include(duplicate=\"merge\") \"kv.inc\"\nm = 1",
        )
        .unwrap();
        assert_eq!(keys(&obj(&v)["k"]), ["a"]);
    }

    #[test]
    fn root_closed_by_included_file_and_names_before_end() {
        // Oracle runs (QUESTIONS.md #46, #47).
        let files = [
            ("/c/close.inc", "a = 1 }\n"),
            ("/c/close_more.inc", "a = 1 }\nb = 2\n"),
            ("/c/mid.inc", ".include \"close.inc\"\nz = 1\n"),
        ];
        let ok = |input: &str| keys(&run(&files, input).unwrap());
        for input in [
            "{\n.include \"close.inc\"\n",
            "{\n.include \"close.inc\";;\n\n",
            "{\n.include \"close_more.inc\"",
        ] {
            assert_eq!(ok(input), ["a"], "{input:?}");
        }
        for input in [
            "{\n.include \"close.inc\"\nk = 1",
            "{\n.include \"close.inc\"\n# c",
            "{\n.include \"close.inc\"\n}",
            "{\n.include \"mid.inc\"",
        ] {
            let e = run(&files, input).unwrap_err();
            assert_eq!(e.kind(), &ErrorKind::AfterRootClosedByInclude, "{input:?}");
        }
        // The bracket of a name in a comment after a VT or FF: the next name may come on a
        // later line, and the end of input keeps the objects.
        assert_eq!(ok("a \x0c# {"), ["a"]);
        assert_eq!(ok("a b \x0c/* { */\n\nc {}"), ["a"]);
        let v = run(&files, "a \x0c# {\n\nb {}").unwrap();
        assert_eq!(keys(&obj(&v)["a"]), ["b"]);
        for input in ["a \x0c# {\n #", "a \x0c/* { */ #", "\"a\" \x0c# {\nb = 1"] {
            assert!(run(&files, input).is_err(), "{input:?}");
        }
    }

    #[test]
    fn comments_follow_a_value_moved_into_a_key_array() {
        // Oracle runs: `target="array"` moves K's first value into a new array.
        let mut p = parser(&[A], ParserFlags::SAVE_COMMENTS);
        p.parse(b"# c1\nk = 1\n# c2\nk = 2\n.include(key=\"k\", target=\"array\") \"files/a.inc\"")
            .unwrap();
        let paths: Vec<_> = p
            .attached_comments()
            .iter()
            .map(|g| g.path.clone())
            .collect();
        let k = PathSegment::Key {
            key: "k".into(),
            index: 0,
        };
        assert_eq!(paths, [vec![k, PathSegment::Index(0)]]);
    }

    fn files_with_close2<'a>(files: &[(&'a str, &'a str)]) -> Vec<(&'a str, &'a str)> {
        let mut all = files.to_vec();
        all.push(("/c/close2.inc", "a = 1 } }\n"));
        all
    }

    #[test]
    fn nesting_under_a_key() {
        // spec §9.4, *Nesting under a key*; priorities from oracle runs.
        let files = [A, ("/c/c.conf", "q = 1\n"), ("/c/d.ucl", "w = 1\n")];
        let ok = |input: &str| run(&files, input).unwrap();
        let v = ok(
            ".include(prefix=true) \"files/a.inc\"\n.include(prefix=true) \"c.conf\"\n\
                    .include(prefix=true) \"d.ucl\"\n.include(prefix=true, key=\"\") \"d.ucl\"",
        );
        assert_eq!(keys(&v), ["a.inc", "c", "d", ""]);
        let v =
            ok("k = 1\nk = 2\n.include(key=\"k\", target=\"ARRAY\", priority=2) \"files/a.inc\"");
        let k = obj(&v).entry("k").unwrap();
        assert_eq!((k.len(), k.slots()[0].priority()), (1, 0));
        let items = k.first().as_array().unwrap();
        assert_eq!(items[0], UclValue::Integer(1));
        assert_eq!(keys(&items[1]), ["x", "y"]);
        let v = ok(
            ".include(key=\"k\", target=\"array\", priority=3) \"files/a.inc\"\n\
                    .include(key=\"k\", target=\"array\") \"c.conf\"",
        );
        let k = obj(&v).entry("k").unwrap();
        assert_eq!(k.slots()[0].priority(), 3);
        assert_eq!(k.first().as_array().unwrap().len(), 2);
        let v =
            ok(".priority 5\nk { a = 1 }\nk { b = 1 }\n.include(key=\"k\", priority=2) \"c.conf\"");
        let k = obj(&v).entry("k").unwrap();
        assert_eq!(k.len(), 2);
        assert_eq!(keys(k.first()), ["a", "q"]);
        assert_eq!(k.slots()[0].priority(), 5);
        for input in ["k = [1]\n", "k = 1\nk { b = 2 }\n"] {
            let e = run(&files, &format!("{input}.include(key=\"k\") \"c.conf\"")).unwrap_err();
            assert!(
                matches!(e.kind(), ErrorKind::IncludeTargetNotObject { .. }),
                "{input:?}"
            );
        }
        // Under NO_IMPLICIT_ARRAYS an array that replaced K's value collects repeats.
        let p = |input: &str| {
            parser(&files, ParserFlags::NO_IMPLICIT_ARRAYS)
                .parse(input.as_bytes())
                .unwrap()
        };
        let v = p("k = 1\n.include(key=\"k\", target=\"array\") \"c.conf\"\nk = 3");
        assert_eq!(obj(&v)["k"].as_array().unwrap().len(), 3);
        let v = p(".include(key=\"k\", target=\"array\") \"c.conf\"\nk = 3");
        assert_eq!(obj(&v)["k"].as_array().unwrap().len(), 2);
        // The key is not lowercased but found ignoring case.
        let v = parser(&files, ParserFlags::KEY_LOWERCASE)
            .parse(b".include(key=\"NEW\") \"c.conf\"\nnew { z = 1 }")
            .unwrap();
        assert_eq!(keys(&v), ["NEW"]);
        assert_eq!(obj(&v).entry("NEW").unwrap().len(), 2);
    }

    #[test]
    fn globs_and_search_paths() {
        let files = [
            ("/c/g/b.inc", "gb = 1\n"),
            ("/c/g/a.inc", "ga = 1\n"),
            ("/c/g/.h.inc", "h = 1\n"),
            ("/c/g/sub/x.inc", "gx = 1\n"),
            ("/c/p1/pa.inc", "pa = 1\n"),
            ("/c/p2/pa.inc", "pb = 1\n"),
            ("/c/p2/pc.inc", "pc = 1\n"),
        ];
        let ok = |input: &str| keys(&run(&files, input).unwrap());
        assert_eq!(ok(".include(glob=true) \"g/*.inc\""), ["ga", "gb"]);
        assert_eq!(ok(".include(glob=true, try=true) \"g/*\""), ["ga", "gb"]);
        assert!(run(&files, ".include(glob=true) \"g/*\"").is_err());
        assert_eq!(ok(".include(glob=true, try=true) \"g/.*\""), ["h"]);
        assert_eq!(
            ok(".include(glob=true, prefix=true) \"g/[ab]*\""),
            ["a.inc"]
        );
        assert_eq!(ok(".include(glob=true) \"*/sub/x.inc\""), ["gx"]);
        // A quoted `/` in the pattern still separates (spec §9.4, *Globs*; the C8c fuzzer).
        assert_eq!(ok(".include(glob=true) \"g\\/*.inc\""), ["ga", "gb"]);
        assert!(matches!(
            run(&files, ".include(glob=true) \"g/[ab].inc\"")
                .unwrap_err()
                .kind(),
            ErrorKind::FileNotFound { .. }
        ));
        let e = run(&files, "k = 1\n.include(glob=true) \"g/none*\"\nm = 1").unwrap_err();
        assert!(e.is_stopped());
        assert_eq!(ok(".try_include(glob=true) \"g/none*\"\nm = 1"), ["m"]);
        // A match that is the including file: an error for .include, skipped by .try_include,
        // with try=false too unless no other match is included (spec §9.4, *Globs*).
        let files = [
            (
                "/c/t/main.inc",
                ".try_include(glob=true, try=false) \"t/*.inc\"\nafter = 1\n",
            ),
            ("/c/t/other.inc", "other = 1\n"),
            (
                "/c/t2/only.inc",
                ".try_include(glob=true, try=false) \"t2/*.inc\"\n",
            ),
            (
                "/c/t3/only.inc",
                ".include(glob=true, try=true) \"t3/*.inc\"\n",
            ),
        ];
        let v = run(&files, ".include \"t/main.inc\"\nk = 1").unwrap();
        assert_eq!(keys(&v), ["other", "after", "k"]);
        for input in [".include \"t2/only.inc\"", ".include \"t3/only.inc\""] {
            let e = run(&files, input).unwrap_err();
            assert!(
                matches!(e.kind(), ErrorKind::IncludeSelf { .. }),
                "{input:?}: {e}"
            );
        }
        let files = [
            ("/c/g/b.inc", "gb = 1\n"),
            ("/c/g/a.inc", "ga = 1\n"),
            ("/c/p1/pa.inc", "pa = 1\n"),
            ("/c/p2/pa.inc", "pb = 1\n"),
            ("/c/p2/pc.inc", "pc = 1\n"),
        ];
        let ok = |input: &str| keys(&run(&files, input).unwrap());
        // Search paths: the first directory decides for .include; .try_include searches.
        assert_eq!(ok(".include(path=[\"p1\", \"p2\"]) \"pa.inc\""), ["pa"]);
        assert!(run(&files, ".include(path=[\"p1\", \"p2\"]) \"pc.inc\"").is_err());
        assert_eq!(ok(".try_include(path=[\"p1\", \"p2\"]) \"pc.inc\""), ["pc"]);
        assert!(run(&files, ".try_include(path=[\"p1\"]) \"zz.inc\"").is_err());
        assert_eq!(
            ok(".include(path=[\"p2\"]) \"pa.inc\"\n.include \"pc.inc\""),
            ["pb", "pc"]
        );
        assert_eq!(
            ok(".include(path=[\"p1\", \"p2\"], glob=true) \"p*.inc\""),
            ["pa", "pb", "pc"]
        );
        assert!(
            run(
                &files,
                ".include(path=[\"p2\", \"p1\"], glob=true) \"pc*.inc\""
            )
            .is_err()
        );
        assert!(run(&files, ".include(path=[], try=true) \"g/a.inc\"").is_err());
        assert_eq!(ok(".include(path=\"p1\") \"g/a.inc\""), ["ga"]);
    }

    #[test]
    fn unreadable_regular_file_is_an_error_even_for_optional_includes() {
        use crate::parse::{FileKind, Loader};
        use std::io;
        use std::path::PathBuf;

        struct DenyRead(MemoryLoader);
        impl Loader for DenyRead {
            fn current_dir(&self) -> io::Result<PathBuf> {
                self.0.current_dir()
            }
            fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
                self.0.canonicalize(path)
            }
            fn kind(&self, path: &Path) -> Option<FileKind> {
                self.0.kind(path)
            }
            fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
                if path.file_name().is_some_and(|name| name == "denied.inc") {
                    return Err(io::Error::from(io::ErrorKind::PermissionDenied));
                }
                self.0.read(path)
            }
            fn read_dir(&self, path: &Path) -> io::Result<Vec<String>> {
                self.0.read_dir(path)
            }
        }

        let mut files = MemoryLoader::new();
        files
            .add_file("/c/denied.inc", "hidden = 1\n")
            .add_file("/c/g/denied.inc", "hidden = 1\n")
            .add_file("/c/g/good.inc", "good = 1\n")
            .add_file("/c/other/good.inc", "good = 1\n")
            .add_dir("/c/nonregular")
            .add_dir("/c/other/subdir");
        let run = |input: &str| {
            let mut parser = Parser::new();
            parser
                .set_loader(DenyRead(files.clone()))
                .set_base_dir("/c");
            parser.parse(input.as_bytes())
        };

        for input in [
            ".include \"denied.inc\"",
            ".include(try=true) \"denied.inc\"\nafter = 1",
            ".try_include \"denied.inc\"\nafter = 1",
            ".try_include(try=false) \"denied.inc\"\nafter = 1",
            ".include(glob=true, try=true) \"g/*.inc\"\nafter = 1",
            ".try_include(glob=true) \"g/*.inc\"\nafter = 1",
            ".try_include(glob=true, try=false) \"g/*.inc\"\nafter = 1",
        ] {
            let error = run(input).unwrap_err();
            assert!(
                matches!(error.kind(), ErrorKind::NotAFile { .. }),
                "{input}: {error}"
            );
        }

        assert_eq!(
            keys(&run(".include(try=true) \"missing.inc\"\nafter = 1").unwrap()),
            ["after"]
        );
        assert_eq!(
            keys(&run(".include(try=true) \"nonregular\"\nafter = 1").unwrap()),
            ["after"]
        );
        assert!(
            run(".try_include \"missing.inc\"\nafter = 1")
                .unwrap_err()
                .is_stopped()
        );
        assert!(
            run(".try_include \"nonregular\"\nafter = 1")
                .unwrap_err()
                .is_stopped()
        );
        for input in [
            ".include(glob=true, try=true) \"other/*\"\nafter = 1",
            ".try_include(glob=true) \"other/*\"\nafter = 1",
        ] {
            assert_eq!(keys(&run(input).unwrap()), ["good", "after"], "{input}");
        }
    }

    #[test]
    fn search_path_set_on_the_parser() {
        // The parser's list is in effect from the start, as a `path` list given to an earlier
        // include macro would be (spec §9.4): each input behaves as it does after such a macro.
        let files = [
            ("/c/g/a.inc", "ga = 1\n"),
            ("/c/p1/pa.inc", "pa = 1\n"),
            ("/c/p2/pa.inc", "pb = 1\n"),
            ("/c/p2/pc.inc", "pc = 1\n"),
            ("/c/p2/abs/x.inc", "px = 1\n"),
            ("/abs/x.inc", "ax = 1\n"),
            ("/c/prio.conf", "priority = 3\n"),
            ("/c/p1/prio.conf", "priority = 5\n"),
        ];
        let with_list = |dirs: &[&str], input: &str| {
            let mut parser = parser(&files, ParserFlags::DEFAULT);
            parser.set_search_path(dirs.iter().copied());
            assert_eq!(parser.search_path().map(<[String]>::len), Some(dirs.len()));
            parser.parse(input.as_bytes())
        };
        let outcome = |result: Result<UclValue, Error>| match result {
            Ok(v) => Ok(keys(&v)),
            Err(e) => Err((e.kind().clone(), e.is_stopped())),
        };
        for (dirs, input, expected) in [
            // .include: the first directory decides.
            (&["p1", "p2"][..], ".include \"pa.inc\"", Some(vec!["pa"])),
            (&["p1", "p2"], ".include \"pc.inc\"", None),
            (
                &["p1", "p2"],
                ".include(try=true) \"pc.inc\"\nk = 1",
                Some(vec!["k"]),
            ),
            // .try_include searches, and a file found nowhere is an error, not a stop.
            (&["p1", "p2"], ".try_include \"pc.inc\"", Some(vec!["pc"])),
            (&["p1"], ".try_include \"zz.inc\"", None),
            // Globs are expanded in every directory; the last one must match.
            (
                &["p1", "p2"],
                ".include(glob=true) \"p*.inc\"",
                Some(vec!["pa", "pb", "pc"]),
            ),
            (&["p2", "p1"], ".include(glob=true) \"pc*.inc\"", None),
            // Absolute paths are tried below the directories too.
            (&["p2"], ".include \"/abs/x.inc\"", Some(vec!["px"])),
        ] {
            let in_document = format!(
                ".include(path=[{}], try=true) \"none.inc\"\n{input}",
                dirs.iter()
                    .map(|d| format!("{d:?}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            let set = outcome(with_list(dirs, input));
            assert_eq!(
                set,
                outcome(run(&files, &in_document)),
                "{dirs:?} {input:?}"
            );
            match expected {
                Some(keys) => assert_eq!(set, Ok(keys.iter().map(|k| k.to_string()).collect())),
                None => assert!(
                    set.is_err_and(|(_, stopped)| !stopped),
                    "{dirs:?} {input:?}"
                ),
            }
        }
        // An empty list makes every include an error.
        let e = with_list(&[], ".include(try=true) \"g/a.inc\"").unwrap_err();
        assert!(matches!(e.kind(), ErrorKind::FileNotFound { .. }), "{e}");
        // A `path` parameter replaces the list for the rest of the parse.
        let v = with_list(
            &["p1"],
            ".include(path=[\"p2\"]) \"pa.inc\"\n.include \"pc.inc\"",
        );
        assert_eq!(keys(&v.unwrap()), ["pb", "pc"]);
        // Without a list, paths resolve against the base directory as before.
        let mut parser = parser(&files, ParserFlags::DEFAULT);
        parser.set_search_path(["p1"]).clear_search_path();
        assert_eq!(
            keys(&parser.parse(b".include \"g/a.inc\"").unwrap()),
            ["ga"]
        );
        // Macro argument documents start with the parser's list, not with a `path` list of the
        // document holding the macro.
        let v = with_list(&["p1"], ".priority(.include \"prio.conf\");\na = 1").unwrap();
        assert_eq!(obj(&v).entry("a").unwrap().slots()[0].priority(), 5);
        let v = run(
            &files,
            ".include(path=[\"p1\"], try=true) \"none.inc\"\n.priority(.include \"prio.conf\");\na = 1",
        )
        .unwrap();
        assert_eq!(obj(&v).entry("a").unwrap().slots()[0].priority(), 3);
    }

    #[test]
    fn first_search_miss_is_recovered_only_by_later_skipped_url() {
        let files = [
            ("/c/p2/pa.inc", "pa = 1\n"),
            (
                "/c/document.ucl",
                ".include(path=[\"p1\", \"p2\"]) \"pa.inc\"\n.include(try=true, url=true) ://\nafter = 2\n",
            ),
        ];
        for url in [
            ".include(try=true, url=true) ://",
            ".try_include(url=true) ://",
        ] {
            let input = format!(
                ".include(path=[\"p1\", \"p2\"]) \"pa.inc\"\nbetween = 1\n{url}\nafter = 2\n"
            );
            let value = run(&files, &input).unwrap();
            assert_eq!(keys(&value), ["pa", "between", "after"]);
            assert_eq!(
                obj(&value).get("pa").and_then(UclValue::as_integer),
                Some(1)
            );
            assert_eq!(
                obj(&value).get("between").and_then(UclValue::as_integer),
                Some(1)
            );
            assert_eq!(
                obj(&value).get("after").and_then(UclValue::as_integer),
                Some(2)
            );
        }
        let value = parser(&files, ParserFlags::DEFAULT)
            .parse_file("document.ucl")
            .unwrap();
        assert_eq!(keys(&value), ["pa", "after"]);
        assert_eq!(
            obj(&value).get("pa").and_then(UclValue::as_integer),
            Some(1)
        );
        assert_eq!(
            obj(&value).get("after").and_then(UclValue::as_integer),
            Some(2)
        );

        for input in [
            ".include(path=[\"p1\", \"p2\"]) \"pa.inc\"\nafter = 2",
            ".include(path=[\"p1\", \"p2\"]) \"pa.inc\"\n.include(url=true) ://",
            ".include(path=[\"p1\", \"p2\"]) \"pa.inc\"\n.include(try=true) \"missing.inc\"",
            ".include(path=[\"p1\", \"p3\"]) \"pa.inc\"\n.include(try=true, url=true) ://",
        ] {
            assert!(run(&files, input).is_err(), "{input}");
        }
    }

    /// spec §9.4, *Quirk: a later skipped URL include or `.load`*, and §9.6 (QUESTIONS.md #86):
    /// a `.load` with `try=true` that reads nothing counts as a skip too.
    #[cfg(feature = "load")]
    #[test]
    fn first_search_miss_is_recovered_by_a_later_skipped_load() {
        let miss = ".include(path=[\"p1\", \"p2\"]) \"pa.inc\"";
        let skip = ".load(try=true, key=\"t\") \"missing.txt\"";
        let url = ".include(try=true, url=true) ://";
        let files = [
            ("/c/p2/pa.inc", "pa = 1\n"),
            ("/c/v.txt", "v"),
            ("/c/empty.txt", ""),
            (
                "/c/document.ucl",
                ".include(path=[\"p1\", \"p2\"]) \"pa.inc\"\nx = 1\n\
                 .load(try=true, key=\"t\") \"missing.txt\"\nafter = 2\n",
            ),
        ];
        let value = run(&files, &format!("{miss}\nx = 1\n{skip}\nafter = 2\n")).unwrap();
        assert_eq!(keys(&value), ["pa", "x", "after"]);
        assert_eq!(
            obj(&value).get("pa").and_then(UclValue::as_integer),
            Some(1)
        );
        // A directory is unusable, so nothing is read either.
        let directory = format!("{miss}\n.load(try=true, key=\"t\") \"dir\"\nafter = 2\n");
        assert_eq!(keys(&run(&files, &directory).unwrap()), ["pa", "after"]);
        // A document given as a file.
        let value = parser(&files, ParserFlags::DEFAULT)
            .parse_file("document.ucl")
            .unwrap();
        assert_eq!(keys(&value), ["pa", "x", "after"]);
        // A skip covers the misses before it, of either kind of skip.
        let again = ".include \"pa.inc\"";
        for input in [
            format!("{miss}\n{skip}\n{again}\n{skip}"),
            format!("{miss}\n{again}\n{skip}"),
            format!("{miss}\n{url}\n{again}\n{skip}"),
            format!("{miss}\n{skip}\n{again}\n{url}"),
        ] {
            let value = run(&files, &input).unwrap();
            assert_eq!(keys(&value), ["pa"], "{input}");
            assert_eq!(obj(&value).entry("pa").unwrap().len(), 2, "{input}");
        }
        for input in [
            // The `.load` reads its file, an empty one included.
            format!("{miss}\n.load(try=true, key=\"t\") \"v.txt\""),
            format!("{miss}\n.load(try=true, key=\"t\") \"empty.txt\""),
            // Without `try=true`, a missing file is an error of its own.
            format!("{miss}\n.load(key=\"t\") \"missing.txt\""),
            // The skip comes before the miss, or a later miss has no skip after it.
            format!("{skip}\n{miss}"),
            format!("{miss}\n{skip}\n{again}"),
        ] {
            assert!(run(&files, &input).is_err(), "{input}");
        }
    }

    #[test]
    fn first_search_miss_precedes_errors_after_a_speculative_later_match() {
        let first = ".include(path=[\"p1\", \"p2\"]) \"pa.inc\"";
        let malformed_file = run(&[("/c/p2/pa.inc", "pa = [")], first).unwrap_err();
        assert_eq!(
            malformed_file.kind(),
            &ErrorKind::FileNotFound {
                path: "p1/pa.inc".to_owned(),
            }
        );

        let bad_trailing = run(
            &[("/c/p2/pa.inc", "pa = 1\n")],
            &format!("{first}\n.unknown 1"),
        )
        .unwrap_err();
        assert_eq!(bad_trailing.kind(), malformed_file.kind());
        assert_eq!(bad_trailing.position(), malformed_file.position());
    }

    /// `.load` does not use the parser's search path (spec §9.6).
    #[cfg(feature = "load")]
    #[test]
    fn load_ignores_the_search_path() {
        let files = [("/c/g/a.inc", "text"), ("/c/p1/g/a.inc", "other")];
        let mut parser = parser(&files, ParserFlags::DEFAULT);
        parser.set_search_path(["p1"]);
        let v = parser.parse(b".load(key=\"k\") \"g/a.inc\"").unwrap();
        assert_eq!(obj(&v)["k"].as_str(), Some("text"));
    }

    /// The kind's limit and path, and the error's offset, for an input limit error.
    fn too_large(result: Result<UclValue, Error>) -> Option<(u64, Option<String>, usize)> {
        let e = result.err()?;
        match e.kind() {
            ErrorKind::InputTooLarge { limit, path } => {
                Some((*limit, path.clone(), e.position().offset))
            }
            _ => None,
        }
    }

    #[test]
    fn input_limit() {
        let files = [
            ("/c/a.inc", "a = 1\n"),
            ("/c/g/x.inc", "x = 1\n"),
            ("/c/g/y.inc", "y = 22\n"),
            ("/c/prio.conf", "priority = 3\n"),
        ];
        let with_limit = |limit: u64, input: &str| {
            let mut parser = parser(&files, ParserFlags::DEFAULT);
            parser.set_max_input_bytes(Some(limit));
            assert_eq!(parser.max_input_bytes(), Some(limit));
            parser.parse(input.as_bytes())
        };
        let len = |s: &str| s.len() as u64;
        // The document alone.
        assert!(with_limit(5, "k = 1").is_ok());
        assert_eq!(too_large(with_limit(4, "k = 1")), Some((4, None, 0)));
        // The document and the files it includes, together; at the macro that goes over.
        let input = "k = 1\n.include \"a.inc\"";
        let total = len(input) + len("a = 1\n");
        assert!(with_limit(total, input).is_ok());
        let over = Some((total - 1, Some("a.inc".to_string()), 15));
        assert_eq!(too_large(with_limit(total - 1, input)), over);
        // Neither `try=true` nor `.try_include` softens it.
        for input in [
            "k = 1\n.include(try=true) \"a.inc\"",
            "k = 1\n.try_include \"a.inc\"",
        ] {
            let total = len(input) + len("a = 1\n");
            assert!(with_limit(total, input).is_ok(), "{input}");
            let found = too_large(with_limit(total - 1, input));
            assert_eq!(
                found.map(|(_, p, _)| p),
                Some(Some("a.inc".into())),
                "{input}"
            );
        }
        // A file included twice counts twice, and every match of a glob counts.
        let twice = ".include \"a.inc\"\n.include \"a.inc\"";
        assert!(with_limit(len(twice) + 12, twice).is_ok());
        assert!(too_large(with_limit(len(twice) + 11, twice)).is_some());
        let glob = ".include(glob=true) \"g/*.inc\"";
        let total = len(glob) + len("x = 1\n") + len("y = 22\n");
        assert!(with_limit(total, glob).is_ok());
        let found = too_large(with_limit(total - 1, glob)).unwrap();
        assert_eq!(found.1.as_deref(), Some("/c/g/y.inc"));
        // Files read from macro argument documents count too.
        let args = ".priority(.include \"prio.conf\");\na = 1";
        let total = len(args) + len("priority = 3\n");
        assert!(with_limit(total, args).is_ok());
        assert!(too_large(with_limit(total - 1, args)).is_some());
        // Without a limit, nothing changes.
        let mut parser = parser(&files, ParserFlags::DEFAULT);
        assert_eq!(parser.max_input_bytes(), None);
        assert!(parser.parse(twice.as_bytes()).is_ok());
        // A document read by `parse_file` counts its own bytes.
        let mut parser = parser_with_limit(&files, 5);
        assert!(too_large(parser.parse_file("/c/a.inc")).is_some());
        let mut parser = parser_with_limit(&files, 6);
        assert!(parser.parse_file("/c/a.inc").is_ok());
    }

    fn parser_with_limit(files: &[(&str, &str)], limit: u64) -> Parser {
        let mut parser = parser(files, ParserFlags::DEFAULT);
        parser.set_max_input_bytes(Some(limit));
        parser
    }

    /// With a limit, files are read through `Loader::read_limited`, asked for at most the bytes
    /// the parse has left.
    #[test]
    fn input_limit_reads_files_limited() {
        use crate::parse::{FileKind, Loader};
        use std::cell::RefCell;
        use std::rc::Rc;

        struct Recording(MemoryLoader, Rc<RefCell<Vec<Option<u64>>>>);
        impl Loader for Recording {
            fn current_dir(&self) -> std::io::Result<std::path::PathBuf> {
                self.0.current_dir()
            }
            fn canonicalize(&self, path: &Path) -> std::io::Result<std::path::PathBuf> {
                self.0.canonicalize(path)
            }
            fn kind(&self, path: &Path) -> Option<FileKind> {
                self.0.kind(path)
            }
            fn read(&self, path: &Path) -> std::io::Result<Vec<u8>> {
                self.1.borrow_mut().push(None);
                self.0.read(path)
            }
            fn read_dir(&self, path: &Path) -> std::io::Result<Vec<String>> {
                self.0.read_dir(path)
            }
            fn read_limited(&self, path: &Path, limit: u64) -> std::io::Result<Vec<u8>> {
                self.1.borrow_mut().push(Some(limit));
                self.0.read_limited(path, limit)
            }
        }
        let mut files = MemoryLoader::new();
        files.add_file("/c/big.inc", "x".repeat(1 << 20));
        files.add_file("/c/main.conf", ".include \"big.inc\"");
        let calls = Rc::new(RefCell::new(Vec::new()));
        let mut parser = Parser::new();
        parser
            .set_loader(Recording(files, Rc::clone(&calls)))
            .set_base_dir("/c")
            .set_max_input_bytes(Some(100));
        assert!(too_large(parser.parse_file("main.conf")).is_some());
        let document = ".include \"big.inc\"".len() as u64;
        assert_eq!(*calls.borrow(), [Some(100), Some(100 - document)]);
        parser.set_max_input_bytes(None);
        calls.borrow_mut().clear();
        assert!(parser.parse(b"a = 1").is_ok());
        assert!(parser.parse_file("main.conf").is_err());
        assert_eq!(*calls.borrow(), [None, None]);
    }

    /// `.load` counts against the limit, and `try=true` does not soften it.
    #[cfg(feature = "load")]
    #[test]
    fn input_limit_counts_load() {
        let files = [("/c/t.txt", "0123456789")];
        for input in [
            ".load(key=\"k\") \"t.txt\"",
            ".load(key=\"k\", try=true) \"t.txt\"",
        ] {
            let total = input.len() as u64 + 10;
            assert!(
                parser_with_limit(&files, total)
                    .parse(input.as_bytes())
                    .is_ok()
            );
            let found = too_large(parser_with_limit(&files, total - 1).parse(input.as_bytes()));
            assert_eq!(
                found.map(|(_, p, _)| p),
                Some(Some("t.txt".into())),
                "{input}"
            );
        }
    }

    #[test]
    fn urls_and_signatures() {
        // Project decisions: URLs are never fetched, signatures never verified.
        for input in [
            ".include(url=true) \"http://example.invalid/x.inc\"",
            ".try_include(url=true, try=false) \"http://example.invalid/x.inc\"",
            ".include(url=true, glob=true, key=\"k\") \"http://example.invalid/*\"",
        ] {
            let e = run(&[A], input).unwrap_err();
            assert!(
                matches!(e.kind(), ErrorKind::UrlNotSupported { .. }),
                "{input:?}"
            );
        }
        // With `try`, and for .try_include, it is skipped without a stop, before globs, keys and
        // the search path (spec §9.4); its own `path` list still takes effect (oracle runs).
        let files = [
            A,
            ("/c/p1/pa.inc", "pa = 1\n"),
            ("/c/p2/pc.inc", "pc = 1\n"),
        ];
        for (input, expected) in [
            (
                "a = 1\n.include(url=true, try=true) \"http://example.invalid/x.inc\"\nb = 2",
                &["a", "b"][..],
            ),
            (
                "a = 1\n.try_include(url=true) \"http://example.invalid/x.inc\"\nb = 2",
                &["a", "b"],
            ),
            (
                ".include(url=true, try=true, key=\"k\") \"http://example.invalid/x.inc\"\nb = 2",
                &["b"],
            ),
            (
                ".include(path=[\"p1\"]) \"pa.inc\"\n\
                 .include(url=true, try=true) \"http://example.invalid/x.inc\"\nb = 2",
                &["pa", "b"],
            ),
            (
                ".include(url=true, try=true, path=[\"p2\"]) \"http://x.invalid/y\"\n\
                 .include \"pc.inc\"",
                &["pc"],
            ),
        ] {
            assert_eq!(keys(&run(&files, input).unwrap()), expected, "{input:?}");
        }
        assert_eq!(
            keys(&run(&[A], ".include(url=true) \"files/a.inc\"").unwrap()),
            ["x", "y"]
        );
        assert_eq!(
            keys(
                &run(
                    &[A],
                    ".include(try=true) \"http://example.invalid/x.inc\"\nk = 1"
                )
                .unwrap()
            ),
            ["k"]
        );
        assert!(
            run(&[A], ".includes \"files/a.inc\"")
                .unwrap_err()
                .is_unsupported()
        );
        assert!(
            run(&[A], ".include(sign=true) \"files/a.inc\"")
                .unwrap_err()
                .is_unsupported()
        );
        assert!(run(&[A], ".include(sign=false) \"files/a.inc\"").is_ok());
    }

    #[test]
    fn argument_documents_may_include() {
        let files = [("/c/pri.inc", "priority = 3\n")];
        let v = run(&files, ".priority(.include \"pri.inc\");\na = 1").unwrap();
        assert_eq!(obj(&v).entry("a").unwrap().slots()[0].priority(), 3);
        // A silent stop there makes the macro fail (oracle run).
        let e = run(
            &files,
            ".priority(.try_include \"nope\"; priority = 3);\na = 1",
        )
        .unwrap_err();
        assert!(
            matches!(e.kind(), ErrorKind::StoppedInArguments { .. }),
            "{e}"
        );
        let e = run(
            &[("/c/bad.inc", "a = \"")],
            ".priority(.include \"bad.inc\") 1",
        )
        .unwrap_err();
        assert_eq!(e.file(), Some(Path::new("/c/bad.inc")));
        assert_eq!(e.position().offset, 4);
    }

    #[test]
    fn comments_across_units() {
        // spec §9.4: comments pending before the macro may attach to the included file's first
        // value; the end of the file attaches pending comments as the end of input does.
        let files = [("/c/c.inc", "\n# in\nx = 1 # g\n")];
        let mut p = parser(&files, ParserFlags::SAVE_COMMENTS);
        p.set_strategy(DuplicateStrategy::Append);
        p.parse(b"# c\n.include \"c.inc\"\nb = 1 # t").unwrap();
        let texts: Vec<_> = p
            .comments()
            .iter()
            .map(|c| (c.text.as_str(), c.position.line, c.position.column))
            .collect();
        assert_eq!(
            texts,
            [("# c", 1, 1), ("# in", 2, 1), ("# g", 3, 7), ("# t", 3, 7)]
        );
        let groups: Vec<_> = p
            .attached_comments()
            .iter()
            .map(|g| (g.path.clone(), g.placement, g.comments.clone()))
            .collect();
        let key = |k: &str| {
            vec![PathSegment::Key {
                key: k.into(),
                index: 0,
            }]
        };
        assert_eq!(
            groups,
            [
                (key("x"), CommentPlacement::Before, vec![0, 1, 2]),
                (key("b"), CommentPlacement::After, vec![3]),
            ]
        );
    }

    #[cfg(feature = "load")]
    #[test]
    fn load_macro() {
        // spec §9.6; check order from oracle runs.
        let files = [
            ("/c/num.txt", "42\n"),
            ("/c/text.txt", "  a\"b\n"),
            ("/c/empty.txt", ""),
            ("/c/ws.txt", " \t\n "),
            ("/c/bad.txt", "\u{0}\u{1}"),
        ];
        let ok = |input: &str| run(&files, input).unwrap();
        let err = |input: &str| run(&files, input).unwrap_err().kind().clone();
        let v = ok(
            ".load(key=\"k\") \"num.txt\"\n.load(key=\"n\", target=\"Int\", priority=19) \"num.txt\"",
        );
        assert_eq!(obj(&v)["k"].as_str(), Some("42\n"));
        assert_eq!(obj(&v)["n"], UclValue::Integer(42));
        assert_eq!(obj(&v).entry("n").unwrap().slots()[0].priority(), 3);
        let v = ok(".load(key=\"t\", tri=true, escape=true) \"text.txt\"");
        assert_eq!(obj(&v)["t"].as_str(), Some("a\\\"b"));
        assert_eq!(keys(&ok(".load(key=\"k\") \"empty.txt\"\nj = 1")), ["j"]);
        assert_eq!(
            obj(&ok(".load(key=\"k\", trim=true) \"ws.txt\""))["k"].as_str(),
            Some("")
        );
        assert_eq!(
            obj(&ok(".load(key=\"k\", target=\"int\") \"empty.txt\""))["k"],
            UclValue::Integer(0)
        );
        assert_eq!(
            keys(&ok(".load(key=\"k\", target=\"float\") \"num.txt\"\nj = 1")),
            ["j"]
        );
        assert_eq!(
            keys(&ok("t = 1\n.load(key=\"t\", try=true) \"nope\"\nj = 1")),
            ["t", "j"]
        );
        assert_eq!(
            keys(&ok(".load(key=\"k\", try=true) \"dir\"\nj = 1")),
            ["j"]
        );
        assert_eq!(err(".load \"num.txt\""), ErrorKind::LoadKeyMissing);
        assert_eq!(
            err(".load(key=\"\", try=true) \"nope\""),
            ErrorKind::LoadKeyMissing
        );
        assert!(matches!(
            err(".load(key=\"k\", try=true) \"\""),
            ErrorKind::FileNotFound { .. }
        ));
        assert!(matches!(
            err(".load(key=\"k\") \"nope\""),
            ErrorKind::FileNotFound { .. }
        ));
        assert!(matches!(
            err(".load(key=\"k\") \"dir\""),
            ErrorKind::NotAFile { .. }
        ));
        assert!(matches!(
            err("t = 1\n.load(key=\"t\", target=\"float\") \"num.txt\""),
            ErrorKind::LoadKeyExists { .. }
        ));
        assert!(matches!(
            err("t = 1\n.load(key=\"t\") \"empty.txt\""),
            ErrorKind::LoadKeyExists { .. }
        ));
        let v = ok(".load(key=\"k\") \"bad.txt\"");
        assert_eq!(obj(&v)["k"].as_str(), Some("\u{0}\u{1}"));
        // The project requires UTF-8 (spec §11.3).
        let mut p = Parser::new();
        let mut loader = MemoryLoader::new();
        loader.add_file("/x.bin", vec![0xFF_u8]);
        p.set_loader(loader);
        assert_eq!(
            p.parse(b".load(key=\"k\") \"/x.bin\"").unwrap_err().kind(),
            &ErrorKind::InvalidUtf8
        );
        // Under KEY_LOWERCASE: compared ignoring case, and kept as written.
        let p = |input: &str| parser(&files, ParserFlags::KEY_LOWERCASE).parse(input.as_bytes());
        assert!(p("K = 1\n.load(key=\"k\") \"num.txt\"").is_err());
        let v = p(".load(key=\"K\") \"num.txt\"\nk = 2").unwrap();
        assert_eq!(keys(&v), ["K"]);
        assert_eq!(obj(&v).entry("K").unwrap().len(), 2);
    }

    // Without the `load` feature, `.load` is unsupported: tests/features/tests/load_feature.rs.
}
