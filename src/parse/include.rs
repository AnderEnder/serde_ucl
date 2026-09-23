//! `.include`, `.try_include`, `.includes` (spec §9.3, §9.4) and `.load` (§9.6): file paths,
//! glob patterns, search paths, and the input units that included files become.
//!
//! Paths are used as written. A relative path resolves against the base directory
//! ([`Includes::base`]): the parser's base directory, or its loader's current directory. That
//! holds inside included files too; `${CURDIR}` gives paths relative to the including file.
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
use super::{Error, ErrorKind, MAX_INCLUDE_DEPTH};
use crate::value::{DuplicateStrategy, UclValue};
use std::path::{Path, PathBuf};

/// The include state of one parse.
pub(crate) struct Includes<'l> {
    pub(crate) loader: &'l dyn Loader,
    /// The directory relative paths resolve against, and `CURDIR` of a document given as bytes
    /// (project decision 6).
    pub(crate) base: PathBuf,
    /// The `path` list in effect: set by any include macro, it stays for the rest of the parse
    /// (spec §9.4, *Signatures, URLs and search paths*).
    search: Option<Vec<String>>,
    /// The canonical path of each open input unit, the main document first; `None` for a
    /// document given as bytes.
    files: Vec<Option<PathBuf>>,
}

impl<'l> Includes<'l> {
    /// The state for a parse of the document `main` (`None`: given as bytes).
    pub(crate) fn new(loader: &'l dyn Loader, base: PathBuf, main: Option<PathBuf>) -> Self {
        Self {
            loader,
            base,
            search: None,
            files: vec![main],
        }
    }

    /// The state for a macro argument document, which is parsed as if a new parser were given
    /// it as bytes (spec §9.2).
    pub(crate) fn for_arguments(&self) -> Includes<'l> {
        Includes::new(self.loader, self.base.clone(), None)
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

impl Core<'_, '_, '_, '_> {
    /// The path a macro's value names; it must be UTF-8.
    fn macro_path(&self, call: &MacroCall) -> Result<String, Error> {
        String::from_utf8(call.value.clone())
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
        if params.bool("url") == Some(true) && path.contains("://") {
            return Err(self.error(ErrorKind::UrlNotSupported { path }, call.value_at));
        }
        if let Some(dirs) = params.array("path") {
            let dirs = dirs.iter().filter_map(UclValue::as_str).map(str::to_owned);
            self.includes.search = Some(dirs.collect());
        }
        let soft = call.kind == MacroKind::TryInclude;
        let request = Request {
            soft,
            try_: params.bool("try").unwrap_or(soft),
            glob: params.bool("glob").unwrap_or(false),
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
            None => self.include_path(&path, &request)?,
            Some(dirs) => self.include_searched(&dirs, &path, &request)?,
        };
        match outcome {
            Outcome::Done => Ok(()),
            Outcome::Unusable(_) => Err(self.error(ErrorKind::Stopped { path }, call.at)),
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
            last = self.include_path(&format!("{dir}/{path}"), request)?;
            if matches!(last, Outcome::Done) && !request.glob {
                break;
            }
        }
        match last {
            Outcome::Done => Ok(Outcome::Done),
            Outcome::Unusable(kind) => Err(self.error(kind, request.at)),
        }
    }

    /// One path, expanded when it is a glob pattern.
    fn include_path(&mut self, path: &str, request: &Request) -> Result<Outcome, Error> {
        if !(request.glob && glob::has_wildcard(path)) {
            if path.is_empty() {
                // The empty path names nothing, not the base directory.
                return self.unusable_missing(path, request);
            }
            let candidate = self.includes.resolve(path);
            return self.include_candidate(&candidate, path, request, request.key.clone());
        }
        let matches = glob::expand(self.includes.loader, &self.includes.base, path);
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
        for candidate in &matches {
            let shown = candidate.to_string_lossy().into_owned();
            match self.include_candidate(candidate, &shown, request, key.clone())? {
                Outcome::Done => {}
                Outcome::Unusable(_) if request.try_ => {}
                Outcome::Unusable(kind) => return Err(self.error(kind, request.at)),
            }
        }
        Ok(Outcome::Done)
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
        let Ok(canonical) = loader.canonicalize(candidate) else {
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
        let Ok(bytes) = loader.read(&canonical) else {
            return unusable(self, not_a_file());
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
        let result = self.parse_included(bytes, request.settings, canonical);
        self.includes.files.pop();
        self.expander.leave_file(saved);
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
        if path.is_empty() {
            return Err(self.error(ErrorKind::FileNotFound { path }, call.value_at));
        }
        let loader = self.includes.loader;
        let not_found = || ErrorKind::FileNotFound { path: path.clone() };
        let not_a_file = || ErrorKind::NotAFile { path: path.clone() };
        let read = match loader.canonicalize(&self.includes.resolve(&path)) {
            Err(_) => Err(not_found()),
            Ok(canonical) => match loader.kind(&canonical) {
                None => Err(not_found()),
                Some(FileKind::File) => loader.read(&canonical).map_err(|_| not_a_file()),
                Some(_) => Err(not_a_file()),
            },
        };
        let bytes = match read {
            Ok(bytes) => bytes,
            Err(_) if try_ => return Ok(()),
            Err(kind) => return Err(self.error(kind, call.value_at)),
        };
        let key_lowercase = self.settings.flags.contains(ParserFlags::KEY_LOWERCASE);
        let exists = {
            let object = self
                .current()
                .as_object()
                .expect("macros are read inside objects");
            super::macros::find_key(object, &key, key_lowercase).is_some()
        };
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
            UclValue::String(text)
        } else if target.eq_ignore_ascii_case("int") {
            UclValue::Integer(leading_integer(&bytes))
        } else {
            return Ok(());
        };
        let priority = params.int("priority").map_or(0, priority_bits);
        if key_lowercase && key.bytes().any(|b| b.is_ascii_uppercase()) {
            self.uppercase_keys = true;
        }
        self.current()
            .as_object_mut()
            .expect("macros are read inside objects")
            .insert_entry(key, Entry::from_slot(Slot::new(value, priority)));
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
        obj(v).keys().cloned().collect()
    }

    const A: (&str, &str) = ("/c/files/a.inc", "x = 1\ny = \"inc\"\n");

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
        // A section object whose brace a file took over closes at its `}`.
        let v = ok("x \"y{\" z\n.include \"braced.inc\"\nq = 1");
        assert_eq!(keys(&v), ["x", "q"]);
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
        assert!(matches!(
            run(&files, ".include(glob=true) \"g/[ab].inc\"")
                .unwrap_err()
                .kind(),
            ErrorKind::FileNotFound { .. }
        ));
        let e = run(&files, "k = 1\n.include(glob=true) \"g/none*\"\nm = 1").unwrap_err();
        assert!(e.is_stopped());
        assert_eq!(ok(".try_include(glob=true) \"g/none*\"\nm = 1"), ["m"]);
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
    fn urls_and_signatures() {
        // Project decisions: URLs are never fetched, signatures never verified.
        for input in [
            ".include(url=true) \"http://example.invalid/x.inc\"",
            ".include(url=true, try=true) \"http://example.invalid/x.inc\"",
            ".try_include(url=true) \"http://example.invalid/x.inc\"",
        ] {
            let e = run(&[A], input).unwrap_err();
            assert!(
                matches!(e.kind(), ErrorKind::UrlNotSupported { .. }),
                "{input:?}"
            );
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

    #[cfg(not(feature = "load"))]
    #[test]
    fn load_needs_its_feature() {
        assert!(
            run(&[], ".load(key=\"k\") \"x\"")
                .unwrap_err()
                .is_unsupported()
        );
    }
}
