//! Positions of deserialization errors (clean-room work item C8a): the error names the value it
//! is about, by its path, and where that value was written, also for values that the duplicate
//! rules of spec §8 merged, moved or replaced, values copied by `.inherit` (§9.7), and values
//! from included files (§9.4).

use serde::Deserialize;
use std::cell::Cell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use ucl_lexer::error::{DeserializeError, SerdeError};
use ucl_lexer::parse::{MemoryLoader, Parser, ParserBuilder, PathSegment};
use ucl_lexer::{DuplicateStrategy, ParserFlags, UclDeserializer, UclError, from_str};

/// The deserialization error of `result`.
fn error<T: std::fmt::Debug>(result: Result<T, UclError>) -> DeserializeError {
    match result {
        Err(UclError::Deserialize(e)) => e,
        other => panic!("expected a deserialization error, got {other:?}"),
    }
}

/// Line and column of a deserialization error.
fn at<T: std::fmt::Debug>(result: Result<T, UclError>) -> (usize, usize) {
    let e = error(result);
    let p = e.position().unwrap_or_else(|| panic!("no position: {e}"));
    (p.line, p.column)
}

fn key(key: &str, index: usize) -> PathSegment {
    PathSegment::Key {
        key: key.to_string(),
        index,
    }
}

/// Deserializes `input` with `parser`.
fn with_parser<'a, T: Deserialize<'a>>(parser: Parser, input: &'a str) -> Result<T, UclError> {
    T::deserialize(UclDeserializer::from_parser(parser, input.as_bytes()))
}

#[derive(Debug, Deserialize)]
#[allow(
    dead_code,
    reason = "deserialization targets; the tests check the errors"
)]
struct K {
    k: i64,
}

#[test]
fn a_scalar() {
    // The value's first byte, whatever its form.
    assert_eq!(at(from_str::<K>("k = foo")), (1, 5));
    assert_eq!(at(from_str::<K>("# c\n  k:\"foo\"")), (2, 5));
    assert_eq!(at(from_str::<K>("k = 'foo'")), (1, 5));
    assert_eq!(at(from_str::<K>("k = <<EOD\nfoo\nEOD\n")), (1, 5));
    assert_eq!(at(from_str::<K>("k = true")), (1, 5));
    // Columns count characters, not bytes.
    assert_eq!(at(from_str::<K>("é = 1; k = x")), (1, 12));
    let e = error(from_str::<K>("k = foo"));
    assert_eq!(e.path(), [key("k", 0)]);
    assert!(!e.at_key());
    assert!(matches!(e.error(), SerdeError::Custom(_)));
    assert_eq!(e.file(), None);
    assert_eq!(
        e.to_string(),
        "invalid type: string \"foo\", expected i64 at k (line 1, column 5)"
    );
    let err = from_str::<K>("k = foo").unwrap_err();
    assert_eq!(err.position().map(|p| p.offset), Some(4));
    assert_eq!(err.file(), None);
}

#[derive(Debug, Deserialize)]
#[allow(
    dead_code,
    reason = "deserialization targets; the tests check the errors"
)]
struct Server {
    host: String,
    port: u16,
    #[serde(default)]
    tags: Vec<u8>,
}

#[derive(Debug, Deserialize)]
#[allow(
    dead_code,
    reason = "deserialization targets; the tests check the errors"
)]
struct Config {
    server: Server,
}

#[test]
fn nested_values_elements_and_containers() {
    let text = "server {\n  host = h\n  port = 99999\n}";
    let e = error(from_str::<Config>(text));
    assert_eq!(e.path(), [key("server", 0), key("port", 0)]);
    assert_eq!(at(from_str::<Config>(text)), (3, 10));
    // An array element.
    let text = "server { host = h; port = 1; tags = [1, 2, 300] }";
    let e = error(from_str::<Config>(text));
    assert_eq!(
        e.path(),
        [key("server", 0), key("tags", 0), PathSegment::Index(2)]
    );
    assert_eq!(at(from_str::<Config>(text)), (1, 44));
    // A missing field is about the object that lacks it: its bracket, or the name of a section.
    assert_eq!(at(from_str::<Config>("\nserver {\n  host = h\n}")), (2, 8));
    assert_eq!(at(from_str::<Config>("x = 1\nserver { host = h }")), (2, 8));
    let e = error(from_str::<HashMap<String, HashMap<String, Server>>>(
        "a b { host = h }",
    ));
    assert_eq!(e.path(), [key("a", 0), key("b", 0)]);
    assert_eq!(e.position().map(|p| p.column), Some(5));
    let e = error(from_str::<HashMap<String, HashMap<String, Server>>>(
        "a b c { host = h }",
    ));
    // `b` is the object of a section name, which lacks `host`: at the name.
    assert_eq!(e.path(), [key("a", 0), key("b", 0)]);
    assert_eq!(e.position().map(|p| p.column), Some(3));
    // The document itself: its bracket, or the start of the input.
    assert_eq!(at(from_str::<u32>("  { a = 1 }")), (1, 3));
    assert_eq!(at(from_str::<u32>("\n\na = 1")), (1, 1));
    assert_eq!(at(from_str::<Vec<u32>>("[1, \"x\"]")), (1, 5));
}

#[test]
fn multi_value_entries_and_sequences() {
    #[derive(Debug, Deserialize)]
    #[allow(
        dead_code,
        reason = "deserialization targets; the tests check the errors"
    )]
    struct Many {
        v: Vec<u32>,
    }
    // Value 2 of a key with three values.
    let text = "v = 1\nv = 2\nv = x";
    let e = error(from_str::<Many>(text));
    assert_eq!(e.path(), [key("v", 2)]);
    assert_eq!(at(from_str::<Many>(text)), (3, 5));
    assert!(e.to_string().contains(" at v[2] "), "{e}");
    // A value of a multi-value entry that is itself an array.
    #[derive(Debug, Deserialize)]
    #[allow(
        dead_code,
        reason = "deserialization targets; the tests check the errors"
    )]
    struct Nested {
        v: Vec<Vec<u32>>,
    }
    let text = "v = [1]\nv = [2, x]";
    let e = error(from_str::<Nested>(text));
    assert_eq!(e.path(), [key("v", 1), PathSegment::Index(1)]);
    assert_eq!(at(from_str::<Nested>(text)), (2, 9));
    // A single value read as a sequence of one: the element is the value itself.
    let e = error(from_str::<Many>("v = x"));
    assert_eq!(e.path(), [key("v", 0)]);
    assert_eq!(at(from_str::<Many>("v = x")), (1, 5));
    // Several values where one is wanted: the entry, at its first value.
    let e = error(from_str::<K>("k = 1\nk = 2"));
    assert_eq!(e.path(), [key("k", 0)]);
    assert_eq!(at(from_str::<K>("k = 1\nk = 2")), (1, 5));
    // An array read as a map, and the values of a key read as a map: by index.
    let e = error(from_str::<HashMap<String, HashMap<String, u32>>>(
        "m = [1, x]",
    ));
    assert_eq!(e.path(), [key("m", 0), PathSegment::Index(1)]);
    assert_eq!(e.position().map(|p| p.column), Some(9));
    let e = error(from_str::<HashMap<String, HashMap<String, u32>>>(
        "m = 1\nm = x",
    ));
    assert_eq!(e.path(), [key("m", 1)]);
    assert_eq!(e.position().map(|p| p.line), Some(2));
}

#[test]
fn keys() {
    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    #[allow(
        dead_code,
        reason = "deserialization targets; the tests check the errors"
    )]
    struct Strict {
        a: u32,
    }
    // An unknown field is about its key.
    let text = "a = 1\n  unknown = 2";
    let e = error(from_str::<Strict>(text));
    assert_eq!(e.path(), [key("unknown", 0)]);
    assert!(e.at_key());
    assert_eq!(at(from_str::<Strict>(text)), (2, 3));
    assert!(e.to_string().contains("at the key of unknown"), "{e}");
    // So is a key that the map's key type does not accept.
    let text = "1 = a\n\"x y\" = b";
    let e = error(from_str::<HashMap<u32, String>>(text));
    assert_eq!(e.path(), [key("x y", 0)]);
    assert!(e.at_key());
    assert_eq!(at(from_str::<HashMap<u32, String>>(text)), (2, 1));
    assert!(e.to_string().contains("at the key of \"x y\""), "{e}");
    // A key read as an owned string is kept for the error of its value.
    let e = error(from_str::<HashMap<String, u32>>("a = 1\nb = x"));
    assert_eq!(e.path(), [key("b", 0)]);
    assert!(!e.at_key());
}

#[test]
fn enums() {
    #[derive(Debug, Deserialize)]
    #[allow(
        dead_code,
        reason = "deserialization targets; the tests check the errors"
    )]
    enum Mode {
        Off,
        Limit(u32),
        Pair(u32, u32),
        Named { rate: u32 },
    }
    #[derive(Debug, Deserialize)]
    #[allow(
        dead_code,
        reason = "deserialization targets; the tests check the errors"
    )]
    struct WithMode {
        mode: Mode,
    }
    let path = |text: &str| error(from_str::<WithMode>(text)).path().to_vec();
    let column = |text: &str| at(from_str::<WithMode>(text)).1;
    // A unit variant that does not exist: the string.
    assert_eq!(path("mode = Fast"), [key("mode", 0)]);
    assert_eq!(column("mode = Fast"), 8);
    // The data of a variant.
    assert_eq!(
        path("mode { Limit = x }"),
        [key("mode", 0), key("Limit", 0)]
    );
    assert_eq!(column("mode { Limit = x }"), 16);
    assert_eq!(
        path("mode { Pair = [1, x] }"),
        [key("mode", 0), key("Pair", 0), PathSegment::Index(1)]
    );
    assert_eq!(column("mode { Pair = [1, x] }"), 19);
    assert_eq!(
        path("mode { Named { rate = x } }"),
        [key("mode", 0), key("Named", 0), key("rate", 0)]
    );
    // A variant that does not exist: its key.
    let e = error(from_str::<WithMode>("mode { Fast = 1 }"));
    assert_eq!(e.path(), [key("mode", 0), key("Fast", 0)]);
    assert!(e.at_key());
    assert_eq!(column("mode { Fast = 1 }"), 8);
}

#[test]
fn values_the_duplicate_rules_move() {
    // A value that replaced another by priority (§8.3): the value kept.
    assert_eq!(at(from_str::<K>("k = 1\n.priority 5\nk = x")), (3, 5));
    // A value that a lower priority did not replace.
    assert_eq!(
        at(from_str::<K>(".priority 5\nk = x\n.priority 1\nk = 2")),
        (2, 5)
    );
    let merge = || {
        ParserBuilder::new()
            .with_strategy(DuplicateStrategy::Merge)
            .build()
    };
    #[derive(Debug, Deserialize)]
    #[allow(
        dead_code,
        reason = "deserialization targets; the tests check the errors"
    )]
    struct A {
        a: HashMap<String, u32>,
    }
    // Merged objects (§8.4): a value from the second one.
    let text = "a { x = 1 }\na { y = x }";
    let e = error(with_parser::<A>(merge(), text));
    assert_eq!(e.path(), [key("a", 0), key("y", 0)]);
    assert_eq!(e.position().map(|p| (p.line, p.column)), Some((2, 9)));
    // Merged arrays: an element from the second one.
    let text = "a = [1]\na = [2, x]";
    let e = error(with_parser::<HashMap<String, Vec<u32>>>(merge(), text));
    assert_eq!(e.path(), [key("a", 0), PathSegment::Index(2)]);
    assert_eq!(e.position().map(|p| (p.line, p.column)), Some((2, 9)));
    // A scalar that took an object's place under `merge` (§8.4, *Quirk*).
    let text = "a { x = 1 }\na = x";
    assert_eq!(at(with_parser::<A>(merge(), text)), (2, 5));
    // `no-implicit-arrays` (§8.5): the array that collects a key's values is where its first
    // value was, and its elements where each was.
    let collect = || Parser::with_flags(ParserFlags::NO_IMPLICIT_ARRAYS);
    let text = "k = 1\nk = x";
    let e = error(with_parser::<HashMap<String, Vec<u32>>>(collect(), text));
    assert_eq!(e.path(), [key("k", 0), PathSegment::Index(1)]);
    assert_eq!(e.position().map(|p| (p.line, p.column)), Some((2, 5)));
    assert_eq!(at(with_parser::<K>(collect(), text)), (1, 5));
    // `key-lowercase` (§12.1): an entry that a later value replaced takes that value's
    // spelling, and the value is where it was written.
    #[derive(Debug, Deserialize)]
    #[allow(
        dead_code,
        reason = "deserialization targets; the tests check the errors"
    )]
    struct Lower {
        a: u32,
    }
    let text = "\"\\u0041\" = 1\n.priority 5\na = x";
    let e = error(with_parser::<Lower>(
        Parser::with_flags(ParserFlags::KEY_LOWERCASE),
        text,
    ));
    assert_eq!(e.path(), [key("a", 0)]);
    assert_eq!(e.position().map(|p| (p.line, p.column)), Some((3, 5)));
}

/// A parser whose loader holds `files`, with the base directory `/c`.
fn with_files(files: &[(&str, &str)]) -> Parser {
    let mut loader = MemoryLoader::new();
    for (path, text) in files {
        loader.add_file(path, *text);
    }
    ParserBuilder::new()
        .with_loader(loader)
        .with_base_dir("/c")
        .build()
}

#[test]
fn values_from_included_files() {
    let files = [
        ("/c/a.conf", "# a\nk = x\n"),
        ("/c/obj.conf", "a {\n  y = x\n}\n"),
        ("/c/inner.conf", "z = 1\n.include \"a.conf\"\n"),
        ("/c/key.conf", "x = 1\n"),
    ];
    // In the included file.
    let e = error(with_parser::<K>(
        with_files(&files),
        "j = 1\n.include \"a.conf\"",
    ));
    assert_eq!(e.position().map(|p| (p.line, p.column)), Some((2, 5)));
    assert_eq!(e.file(), Some(Path::new("/c/a.conf")));
    assert!(
        e.to_string().ends_with("(line 2, column 5 of /c/a.conf)"),
        "{e}"
    );
    // Two levels deep.
    let e = error(with_parser::<K>(
        with_files(&files),
        ".include \"inner.conf\"",
    ));
    assert_eq!(e.file(), Some(Path::new("/c/a.conf")));
    // Values of the including document after an include are in the document again.
    let text = ".include \"inner.conf\"\nm = x";
    let e = error(with_parser::<HashMap<String, u32>>(
        with_files(&files),
        text,
    ));
    assert_eq!(e.path(), [key("k", 0)]);
    let text = ".include \"key.conf\"\nm = y";
    let e = error(with_parser::<HashMap<String, u32>>(
        with_files(&files),
        text,
    ));
    assert_eq!(e.path(), [key("m", 0)]);
    assert_eq!(e.file(), None);
    assert_eq!(e.position().map(|p| (p.line, p.column)), Some((2, 5)));
    // Merged into an object of the including document.
    #[derive(Debug, Deserialize)]
    #[allow(
        dead_code,
        reason = "deserialization targets; the tests check the errors"
    )]
    struct A {
        a: HashMap<String, u32>,
    }
    let text = "a { x = 1 }\n.include(duplicate=\"merge\") \"obj.conf\"";
    let e = error(with_parser::<A>(with_files(&files), text));
    assert_eq!(e.path(), [key("a", 0), key("y", 0)]);
    assert_eq!(e.position().map(|p| (p.line, p.column)), Some((2, 7)));
    assert_eq!(e.file(), Some(Path::new("/c/obj.conf")));
    // Nested under a key (§9.4): the values are in the file, and the object the macro creates
    // is where the macro's value is.
    #[derive(Debug, Deserialize)]
    #[allow(
        dead_code,
        reason = "deserialization targets; the tests check the errors"
    )]
    struct Nest {
        n: HashMap<String, String>,
    }
    let text = "\n.include(key=\"n\") \"key.conf\"";
    let e = error(with_parser::<Nest>(with_files(&files), text));
    assert_eq!(e.path(), [key("n", 0), key("x", 0)]);
    assert_eq!(e.file(), Some(Path::new("/c/key.conf")));
    assert_eq!(e.position().map(|p| (p.line, p.column)), Some((1, 5)));
    let e = error(with_parser::<HashMap<String, u32>>(
        with_files(&files),
        text,
    ));
    assert_eq!(e.path(), [key("n", 0)]);
    assert_eq!(e.file(), None);
    assert_eq!(e.position().map(|p| (p.line, p.column)), Some((2, 19)));
}

#[test]
fn values_copied_by_inherit() {
    #[derive(Debug, Deserialize)]
    #[allow(
        dead_code,
        reason = "deserialization targets; the tests check the errors"
    )]
    struct Pair {
        base: HashMap<String, String>,
        derived: HashMap<String, u32>,
    }
    // A copy is where the value it copies was written (§9.7).
    let text = "base {\n  x = \"s\"\n}\nderived {\n  .inherit \"base\"\n}";
    let e = error(from_str::<Pair>(text));
    assert_eq!(e.path(), [key("derived", 0), key("x", 0)]);
    assert_eq!(e.position().map(|p| (p.line, p.column)), Some((2, 7)));
    // Also when the value it copies came from an included file.
    let files = [("/c/base.conf", "base {\n  x = \"s\"\n}\n")];
    let text = ".include \"base.conf\"\nderived { .inherit \"base\" }";
    let e = error(with_parser::<Pair>(with_files(&files), text));
    assert_eq!(e.path(), [key("derived", 0), key("x", 0)]);
    assert_eq!(e.file(), Some(Path::new("/c/base.conf")));
    assert_eq!(e.position().map(|p| (p.line, p.column)), Some((2, 7)));
}

#[test]
fn nesting_past_the_serde_limit() {
    let depth = ucl_lexer::MAX_SERDE_NESTING + 2;
    let text = format!("a = {}1{}", "[".repeat(depth), "]".repeat(depth));
    let e = error(from_str::<serde_json::Value>(&text));
    assert!(matches!(e.error(), SerdeError::TooDeep { .. }));
    // The array that is one level too deep.
    assert_eq!(e.path().len(), ucl_lexer::MAX_SERDE_NESTING);
    let p = e.position().unwrap();
    assert_eq!(
        (p.line, p.column),
        (1, 5 + ucl_lexer::MAX_SERDE_NESTING - 1)
    );
}

#[test]
fn from_value_has_neither_path_nor_position() {
    // It consumes the value, so it cannot deserialize a second time to find them.
    let value = ucl_lexer::parse::parse(b"server { host = h; port = x }").unwrap();
    let e = error(ucl_lexer::from_value::<Config>(value));
    assert!(e.path().is_empty());
    assert_eq!(e.position(), None);
    assert_eq!(e.to_string(), "invalid type: string \"x\", expected u16");
}

#[test]
fn the_deserializer_itself_finds_them_as_it_goes() {
    // `UclDeserializer` cannot run its visitor twice, so it records paths while deserializing.
    let parser = Parser::new();
    let text = b"server { host = h; port = x }";
    let e = error(Config::deserialize(UclDeserializer::from_parser(
        parser, text,
    )));
    assert_eq!(e.path(), [key("server", 0), key("port", 0)]);
    assert_eq!(e.position().map(|p| p.column), Some(27));
}

#[test]
fn a_second_run_that_fails_differently_gives_no_path() {
    // The text functions deserialize a second time to find the path. A target that fails
    // differently then, or not at all, leaves the first error without a path.
    use std::sync::atomic::{AtomicUsize, Ordering};
    static CALLS: AtomicUsize = AtomicUsize::new(0);
    #[derive(Debug)]
    struct Flaky;
    impl<'de> Deserialize<'de> for Flaky {
        fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            let n = CALLS.fetch_add(1, Ordering::SeqCst);
            serde::de::IgnoredAny::deserialize(d)?;
            match n {
                0 | 2 => Err(serde::de::Error::custom(format!("call {n}"))),
                _ => Ok(Flaky),
            }
        }
    }
    #[derive(Debug, Deserialize)]
    #[allow(
        dead_code,
        reason = "deserialization targets; the tests check the errors"
    )]
    struct Holder {
        f: Flaky,
    }
    // Calls 0 and 1: the second run succeeds.
    let e = error(from_str::<Holder>("f = 1"));
    assert_eq!((e.path(), e.position()), (&[][..], None));
    assert_eq!(e.to_string(), "call 0");
    // Calls 2 and 3.
    let e = error(from_str::<Holder>("f = 1"));
    assert_eq!(e.position(), None);
    assert_eq!(e.to_string(), "call 2");
}

#[test]
fn the_variable_handler_is_asked_once() {
    // The second parse, which finds the position, gives the handler's answers again instead of
    // asking it: a handler that answers differently each time still leads to the same value.
    let calls = Rc::new(Cell::new(0));
    let counter = Rc::clone(&calls);
    let mut loader = MemoryLoader::new();
    loader.add_file("/c/f1.conf", "\nk = x\n");
    loader.add_file("/c/f2.conf", "k = 1\n");
    let parser = ParserBuilder::new()
        .with_loader(loader)
        .with_base_dir("/c")
        .with_variable_handler(move |_| {
            counter.set(counter.get() + 1);
            Some(format!("f{}", counter.get()))
        })
        .build();
    let text = ".include \"${F}.conf\"\ns = \"${S}\"";
    let e = error(with_parser::<HashMap<String, u32>>(parser, text));
    assert_eq!(calls.get(), 2);
    assert_eq!(e.path(), [key("k", 0)]);
    assert_eq!(e.file(), Some(Path::new("/c/f1.conf")));
    assert_eq!(e.position().map(|p| p.line), Some(2));
}

#[test]
fn every_text_entry_point() {
    let text = "a = 1\nk = foo";
    assert_eq!(at(ucl_lexer::from_slice::<K>(text.as_bytes())), (2, 5));
    assert_eq!(at(ucl_lexer::from_reader::<K>(text.as_bytes())), (2, 5));
    assert_eq!(
        at(ucl_lexer::from_str_with_variables::<K, _, _, _>(
            "k = $V",
            [("V", "x")]
        )),
        (1, 5)
    );
    assert_eq!(
        at(ucl_lexer::from_str_with_map::<K>(text, HashMap::new())),
        (2, 5)
    );
    assert_eq!(at(ucl_lexer::from_str_with_env::<K>(text)), (2, 5));
    // A parser that saves comments keeps them: the second parse does not touch them.
    let mut parser = Parser::with_flags(ParserFlags::SAVE_COMMENTS);
    parser.register_variable("V", "x");
    let mut deserializer = UclDeserializer::from_parser(parser, b"# c\nk = $V");
    deserializer.parser_mut().set_priority(1);
    assert_eq!(at(K::deserialize(deserializer)), (2, 5));
}

/// A scratch file under the test's target directory holding `text`.
fn file(name: &str, text: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("error_positions");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, text).unwrap();
    std::fs::canonicalize(path).unwrap()
}

#[test]
fn from_file_and_its_includes() {
    let inc = file("inc.conf", "\n\nk = x\n");
    let main = file("main.conf", "a = 1\n.include \"inc.conf\"\n");
    let e = error(ucl_lexer::from_file::<HashMap<String, u32>>(&main));
    assert_eq!(e.path(), [key("k", 0)]);
    assert_eq!(e.file(), Some(inc.as_path()));
    assert_eq!(e.position().map(|p| (p.line, p.column)), Some((3, 5)));
    // In the file itself, which is the document: no file is named.
    let main = file("main2.conf", "a = 1\nb = y\n");
    let e = error(ucl_lexer::from_file::<HashMap<String, u32>>(&main));
    assert_eq!(e.file(), None);
    assert_eq!(e.position().map(|p| (p.line, p.column)), Some((2, 5)));
}
