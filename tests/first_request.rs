//! The text entry points (`from_str`, `from_slice`, `from_reader`, `from_file`, the
//! `from_str_with_*` functions) parse the document at the target's first request (clean-room work
//! item C13, decision 6):
//! - on a document that parses and deserializes, the target's `Deserialize` runs once;
//! - a parse error reaches the caller as the parser reports it, with its kind, position and file,
//!   whatever the target does with the error it receives: rewrites it, ignores it and returns a
//!   value, or makes no request at all.

use serde::Deserialize;
use serde::de::{self, Deserializer};
use serde_ucl::parse::{self, FsLoader, Parser};
use serde_ucl::{UclError, UclObject, UclValue};
use std::cell::Cell;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

thread_local! {
    /// How many times a [`Counted`] ran its `Deserialize` on this thread.
    static RUNS: Cell<usize> = const { Cell::new(0) };
}

/// A `T` that counts the runs of its `Deserialize`, which forwards to `T`'s.
#[derive(Debug)]
struct Counted<T>(T);

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Counted<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        RUNS.with(|runs| runs.set(runs.get() + 1));
        T::deserialize(deserializer).map(Counted)
    }
}

/// `f`'s result, and how many times a [`Counted`] ran its `Deserialize` in it.
fn runs<R>(f: impl FnOnce() -> R) -> (R, usize) {
    RUNS.with(|runs| runs.set(0));
    let result = f();
    (result, RUNS.with(Cell::get))
}

/// A typed document; its fields count their runs too.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct Service {
    name: Counted<String>,
    port: Counted<u16>,
    tags: Vec<Counted<String>>,
}

/// The same, borrowing its strings.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ServiceBorrowed<'a> {
    #[serde(borrow)]
    name: Counted<&'a str>,
    port: Counted<u16>,
}

const TEXT: &str = "name = web\nport = 8080\ntags = [a, b, c]\n";

/// A scratch file under Cargo's scratch directory for integration tests.
fn file(name: &str, text: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("first_request");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, text).unwrap();
    std::fs::canonicalize(path).unwrap()
}

#[test]
fn deserialize_runs_once_on_a_document_that_parses_and_deserializes() {
    // The document, `name`, `port` and the three tags: six runs, each once.
    let each_once = 1 + 2 + 3;
    let (service, n) = runs(|| serde_ucl::from_str::<Counted<Service>>(TEXT));
    assert_eq!(service.unwrap().0.port.0, 8080);
    assert_eq!(n, each_once);
    let (_, n) = runs(|| serde_ucl::from_slice::<Counted<Service>>(TEXT.as_bytes()).unwrap());
    assert_eq!(n, each_once);
    let (_, n) = runs(|| serde_ucl::from_reader::<Counted<Service>>(TEXT.as_bytes()).unwrap());
    assert_eq!(n, each_once);
    let path = file("service.conf", TEXT);
    let (_, n) = runs(|| serde_ucl::from_file::<Counted<Service>>(&path).unwrap());
    assert_eq!(n, each_once);
    let with_vars = "name = $NAME\nport = 8080\ntags = [a, b, c]\n";
    let (service, n) = runs(|| {
        serde_ucl::from_str_with_variables::<Counted<Service>, _, _, _>(
            with_vars,
            [("NAME", "api")],
        )
        .unwrap()
    });
    assert_eq!((service.0.name.0.as_str(), n), ("api", each_once));
    let map = HashMap::from([("NAME".to_string(), "api".to_string())]);
    let (_, n) = runs(|| serde_ucl::from_str_with_map::<Counted<Service>>(with_vars, map).unwrap());
    assert_eq!(n, each_once);
    let (_, n) = runs(|| serde_ucl::from_str_with_env::<Counted<Service>>(TEXT).unwrap());
    assert_eq!(n, each_once);

    // A target that borrows.
    let (service, n) = runs(|| serde_ucl::from_str::<Counted<ServiceBorrowed<'_>>>(TEXT).unwrap());
    assert_eq!((service.0.name.0, n), ("web", 3));

    // Targets that take the whole value, and one that holds it in an `Option`.
    let whole = parse::parse(TEXT.as_bytes()).unwrap();
    let (value, n) = runs(|| serde_ucl::from_str::<Counted<UclValue>>(TEXT).unwrap());
    assert_eq!((value.0, n), (whole.clone(), 1));
    let (value, n) = runs(|| serde_ucl::from_str::<Counted<Box<UclValue>>>(TEXT).unwrap());
    assert_eq!((*value.0, n), (whole.clone(), 1));
    let (_, n) = runs(|| serde_ucl::from_str::<Counted<UclObject>>(TEXT).unwrap());
    assert_eq!(n, 1);
    let (value, n) = runs(|| serde_ucl::from_str::<Counted<Option<UclValue>>>(TEXT).unwrap());
    assert_eq!((value.0, n), (Some(whole.clone()), 1));
    let (_, n) = runs(|| serde_ucl::from_file::<Counted<UclValue>>(&path).unwrap());
    assert_eq!(n, 1);
    let (_, n) = runs(|| serde_ucl::from_str::<Counted<serde_json::Value>>(TEXT).unwrap());
    assert_eq!(n, 1);
}

/// Unchanged from before decision 6: a deserialization error runs the target a second time, to
/// find the error's path and position.
#[test]
fn a_deserialization_error_runs_the_target_again_for_its_position() {
    let (result, n) =
        runs(|| serde_ucl::from_str::<Counted<BTreeMap<String, Counted<u16>>>>("a = 1\nb = x"));
    let UclError::Deserialize(e) = result.unwrap_err() else {
        panic!("expected a deserialization error");
    };
    assert_eq!(e.position().map(|p| (p.line, p.column)), Some((2, 5)));
    // Twice the document and the entry `a`, twice the entry `b` that fails.
    assert_eq!(n, 6);
}

/// Asks for a map, and replaces any error with its own.
#[derive(Debug)]
struct Rewrites;
impl<'de> Deserialize<'de> for Rewrites {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        HashMap::<String, UclValue>::deserialize(deserializer)
            .map(|_| Rewrites)
            .map_err(|_| de::Error::custom("rewritten"))
    }
}

/// Asks for the whole value, and replaces any error with its own.
#[derive(Debug)]
struct RewritesWhole;
impl<'de> Deserialize<'de> for RewritesWhole {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        UclValue::deserialize(deserializer)
            .map(|_| RewritesWhole)
            .map_err(|_| de::Error::custom("rewritten"))
    }
}

/// Asks for a map, and returns a value whatever the answer.
#[derive(Debug)]
struct Ignores;
impl<'de> Deserialize<'de> for Ignores {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let _ = HashMap::<String, UclValue>::deserialize(deserializer);
        Ok(Ignores)
    }
}

/// Asks for the whole value, and returns a value whatever the answer.
#[derive(Debug)]
struct IgnoresWhole;
impl<'de> Deserialize<'de> for IgnoresWhole {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let _ = UclValue::deserialize(deserializer);
        Ok(IgnoresWhole)
    }
}

/// Asks the deserializer for nothing.
#[derive(Debug)]
struct Silent;
impl<'de> Deserialize<'de> for Silent {
    fn deserialize<D: Deserializer<'de>>(_: D) -> Result<Self, D::Error> {
        Ok(Silent)
    }
}

/// Checks that `result` is the parse error `expected`, as `From<parse::Error>` makes it: kind,
/// position, file and partial result all equal.
#[track_caller]
fn assert_parse_error<T: std::fmt::Debug>(result: Result<T, UclError>, expected: &parse::Error) {
    match result {
        Err(UclError::Syntax(e)) if !expected.is_stopped() => assert_eq!(&e, expected),
        Err(UclError::Stopped(e)) if expected.is_stopped() => assert_eq!(&e, expected),
        other => panic!("expected the parse error {expected:?}, got {other:?}"),
    }
}

/// Checks `$call`, in which `$t` names the target type, for each kind of target: that it returns
/// the parse error `$expected`.
macro_rules! every_target {
    ($t:ident => $call:expr, $expected:expr) => {{
        let expected: &parse::Error = $expected;
        {
            type $t = Rewrites;
            assert_parse_error($call, expected);
        }
        {
            type $t = RewritesWhole;
            assert_parse_error($call, expected);
        }
        {
            type $t = Ignores;
            assert_parse_error($call, expected);
        }
        {
            type $t = IgnoresWhole;
            assert_parse_error($call, expected);
        }
        {
            type $t = Silent;
            assert_parse_error($call, expected);
        }
        {
            type $t = HashMap<String, UclValue>;
            assert_parse_error($call, expected);
        }
        {
            type $t = UclValue;
            assert_parse_error($call, expected);
        }
        {
            type $t = Option<UclValue>;
            assert_parse_error($call, expected);
        }
    }};
}

#[test]
fn a_parse_error_reaches_the_caller_whatever_the_target_does() {
    let documents: [&[u8]; 5] = [
        b"a = {",
        b"a = [1,\nb = 2",
        b"k = \"\xff\"",
        // A silent stop (spec 9.4): the error holds what was parsed before it.
        b"a = 1\n.try_include \"missing\"\nb = 2",
        b"a = 1\n.unknown_macro {}",
    ];
    for text in documents {
        let expected = Parser::new().parse(text).unwrap_err();
        every_target!(T => serde_ucl::from_slice::<T>(text), &expected);
        every_target!(T => serde_ucl::from_reader::<T>(text), &expected);
        if let Ok(text) = std::str::from_utf8(text) {
            every_target!(T => serde_ucl::from_str::<T>(text), &expected);
            every_target!(
                T => serde_ucl::from_str_with_variables::<T, _, _, _>(text, [("V", "x")]),
                &expected
            );
            every_target!(T => serde_ucl::from_str_with_env::<T>(text), &expected);
        }
    }
}

/// `from_file`: the error of an included file names that file, and the error of the file itself
/// names none, as `Parser::parse_file` with the filesystem loader reports them.
#[test]
fn a_parse_error_in_a_file_keeps_its_file() {
    let inc = file("broken.conf", "\n\nk = [1,\n");
    let main = file("includes-broken.conf", "a = 1\n.include \"broken.conf\"\n");
    let alone = file("broken-alone.conf", "a = 1\nb = {\n");
    for (path, file) in [(&main, Some(inc.as_path())), (&alone, None)] {
        let mut parser = Parser::new();
        parser.set_loader(FsLoader::new());
        let expected = parser.parse_file(path).unwrap_err();
        assert_eq!(expected.file(), file);
        every_target!(T => serde_ucl::from_file::<T>(path), &expected);
    }
}

/// A target that receives the parse error through its first request gets the parser's error;
/// the entry point returns the same one.
#[test]
fn the_target_receives_the_parse_error() {
    thread_local! {
        static SEEN: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
    }
    #[derive(Debug)]
    struct Records;
    impl<'de> Deserialize<'de> for Records {
        fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let error = UclValue::deserialize(deserializer).unwrap_err();
            SEEN.with(|s| *s.borrow_mut() = Some(error.to_string()));
            Err(error)
        }
    }
    let err = serde_ucl::from_str::<Records>("a = {").unwrap_err();
    assert!(matches!(err, UclError::Syntax(_)), "{err:?}");
    assert_eq!(SEEN.with(|s| s.borrow().clone()), Some(err.to_string()));
}
