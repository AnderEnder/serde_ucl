//! The public entry points on the parser core: text, bytes, readers and files, the
//! document-level deserializer, errors with their kind and position, and WORKLIST C5 decision 1:
//! text input has no file access by default, while `from_file` reads the file's includes from
//! the filesystem, resolving relative paths against the file's directory.

use serde::Deserialize;
use serde_json::{Value, json};
use std::borrow::Cow;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use ucl_lexer::parse::{ErrorKind, FsLoader, Parser, ParserBuilder};
use ucl_lexer::{
    ParserFlags, UclDeserializer, UclError, from_file, from_reader, from_slice, from_str,
    from_value,
};

/// A fresh, empty directory for one test under Cargo's scratch directory for integration tests.
fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("api_tests")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(path: &Path, contents: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

fn syntax_kind(result: Result<Value, UclError>) -> ErrorKind {
    match result {
        Err(UclError::Syntax(e)) => e.kind().clone(),
        other => panic!("expected a parse error, got {other:?}"),
    }
}

#[test]
fn text_input_reads_no_files() {
    let dir = scratch("text_input_reads_no_files");
    let part = dir.join("part.conf");
    write(&part, "secret = 1\n");
    let part = part.display().to_string();

    // `.include` of an existing file finds nothing (spec §9.4, missing file), through every
    // entry point that takes text.
    let include = format!("a = 1\n.include \"{part}\"\n");
    let not_found = |kind: ErrorKind| matches!(kind, ErrorKind::FileNotFound { .. });
    assert!(not_found(syntax_kind(from_str(&include))));
    assert!(not_found(syntax_kind(from_slice(include.as_bytes()))));
    assert!(not_found(syntax_kind(from_reader(include.as_bytes()))));
    assert!(not_found(syntax_kind(Value::deserialize(
        UclDeserializer::new(&include)
    ))));
    assert!(not_found(
        ucl_lexer::parse::parse(include.as_bytes())
            .unwrap_err()
            .kind()
            .clone()
    ));

    // `.try_include` of it stops the parse silently (spec §9.4), which the serde entry points
    // report as `UclError::Stopped` with the entries parsed before it.
    let try_include = format!("a = 1\n.try_include \"{part}\"\nb = 2\n");
    match from_str::<Value>(&try_include) {
        Err(UclError::Stopped(e)) => {
            let partial = e.partial().unwrap().as_object().unwrap();
            assert_eq!(partial.keys().collect::<Vec<_>>(), ["a"]);
        }
        other => panic!("expected a silent stop, got {other:?}"),
    }

    // A caller opts text input into filesystem access through the parser builder.
    let value = ParserBuilder::new()
        .with_loader(FsLoader::new())
        .build()
        .parse(include.as_bytes())
        .unwrap();
    assert_eq!(
        from_value::<Value>(value).unwrap(),
        json!({"a": 1, "secret": 1})
    );
}

#[cfg(feature = "load")]
#[test]
fn text_input_loads_no_files() {
    let dir = scratch("text_input_loads_no_files");
    let file = dir.join("value.txt");
    write(&file, "42\n");
    let file = file.display().to_string();
    // spec §9.6: a missing file is an error, and with `try=true` nothing is inserted.
    let load = format!(".load(key=\"k\") \"{file}\"\n");
    assert!(matches!(
        syntax_kind(from_str(&load)),
        ErrorKind::FileNotFound { .. }
    ));
    let value: Value =
        from_str(&format!("a = 1\n.load(key=\"k\", try=true) \"{file}\"\n")).unwrap();
    assert_eq!(value, json!({"a": 1}));
    // With the filesystem loader the file is read.
    let value = ParserBuilder::new()
        .with_loader(FsLoader::new())
        .build()
        .parse(load.as_bytes())
        .unwrap();
    assert_eq!(from_value::<Value>(value).unwrap(), json!({"k": "42\n"}));
}

#[test]
fn file_variables_of_text_input() {
    // spec §7.8 for a document given as bytes; `CURDIR` is the default loader's current
    // directory, `/`, not the process's working directory (WORKLIST C5 decision 1).
    let value: Value = from_str("f = $FILENAME\nd = $CURDIR\nx = \"${CURDIR}x\"").unwrap();
    assert_eq!(value, json!({"f": "undef", "d": "/", "x": "/x"}));
    // A base directory set on the parser is `CURDIR`.
    let value = ParserBuilder::new()
        .with_base_dir("/srv/app")
        .build()
        .parse(b"d = $CURDIR")
        .unwrap();
    assert_eq!(
        from_value::<Value>(value).unwrap(),
        json!({"d": "/srv/app"})
    );
}

#[test]
fn from_file_resolves_includes_against_the_file_directory() {
    let dir = scratch("from_file_resolves_includes");
    let main = dir.join("main.conf");
    write(
        &main,
        "name = main\n\
         .include \"c5a_part.conf\"\n\
         .include \"sub/c5a_inner.conf\"\n\
         file = \"$FILENAME\"\n\
         dir = \"$CURDIR\"\n",
    );
    write(&dir.join("c5a_part.conf"), "part = 1\n");
    // Relative paths in an included file resolve against the main file's directory too, never
    // against the included file's (spec §9.3).
    write(
        &dir.join("sub/c5a_inner.conf"),
        "inner = \"$CURDIR\"\n.include \"c5a_leaf.conf\"\n",
    );
    write(&dir.join("c5a_leaf.conf"), "leaf = top\n");
    write(&dir.join("sub/c5a_leaf.conf"), "leaf = sub\n");
    // The process's working directory does not hold the included files.
    assert!(!Path::new("c5a_part.conf").exists());

    let canonical = std::fs::canonicalize(&main).unwrap();
    let canonical_dir = canonical.parent().unwrap();
    let expected = json!({
        "name": "main",
        "part": 1,
        "inner": canonical_dir.join("sub").display().to_string(),
        "leaf": "top",
        "file": canonical.display().to_string(),
        "dir": canonical_dir.display().to_string(),
    });
    let value: Value = from_file(&main).unwrap();
    assert_eq!(value, expected);

    // A relative path to the file itself resolves against the process's working directory.
    let cwd = std::env::current_dir().unwrap();
    if let Ok(relative) = main.strip_prefix(&cwd) {
        let value: Value = from_file(relative).unwrap();
        assert_eq!(value, expected);
    }

    // A parser with the filesystem loader parses files the same way.
    let value = ParserBuilder::new()
        .with_loader(FsLoader::new())
        .build()
        .parse_file(&main)
        .unwrap();
    assert_eq!(from_value::<Value>(value).unwrap(), expected);
    // A default parser's loader holds no files.
    assert!(matches!(
        Parser::new().parse_file(&main).unwrap_err().kind(),
        ErrorKind::Io { .. }
    ));
}

#[test]
fn from_file_errors() {
    let dir = scratch("from_file_errors");
    // A file that cannot be read is an I/O error.
    match from_file::<Value>(dir.join("missing.conf")) {
        Err(UclError::Io(e)) => assert_eq!(e.kind(), io::ErrorKind::NotFound),
        other => panic!("expected an I/O error, got {other:?}"),
    }
    // An error in an included file names the file and its position there.
    let main = dir.join("main.conf");
    write(&main, "a = 1\n.include \"bad.conf\"\n");
    write(&dir.join("bad.conf"), "ok = 1\nbroken = \"open");
    match from_file::<Value>(&main) {
        Err(UclError::Syntax(e)) => {
            assert_eq!(e.kind(), &ErrorKind::UnterminatedString);
            assert_eq!(
                e.file(),
                Some(
                    std::fs::canonicalize(dir.join("bad.conf"))
                        .unwrap()
                        .as_path()
                )
            );
            assert_eq!((e.position().line, e.position().column), (2, 10));
        }
        other => panic!("expected a parse error, got {other:?}"),
    }
}

#[test]
fn errors_keep_kind_and_position() {
    let err = from_str::<Value>("a = 1\nb = \"open").unwrap_err();
    let UclError::Syntax(e) = &err else {
        panic!("{err:?}")
    };
    assert_eq!(e.kind(), &ErrorKind::UnterminatedString);
    assert_eq!((e.position().line, e.position().column), (2, 5));
    assert_eq!(
        err.parse_error().map(|e| e.kind()),
        Some(&ErrorKind::UnterminatedString)
    );
    let position = err.position().unwrap();
    assert_eq!(
        (position.line, position.column, position.offset),
        (2, 5, 10)
    );
    assert!(err.to_string().contains("line 2, column 5"), "{err}");

    // Errors from serde itself carry no position.
    let err = from_str::<u32>("a = 1").unwrap_err();
    assert!(matches!(err, UclError::Serde(_)));
    assert!(err.position().is_none());
}

#[test]
fn non_utf8_keys_and_strings_are_errors() {
    // Keys and strings must be valid UTF-8 (a project divergence, spec README); comments may
    // hold any bytes.
    for input in [
        &b"k = \"a\xffb\""[..],
        b"k = a\xff",
        b"\xff = 1",
        b"\"k\xfe\" = 1",
    ] {
        match from_slice::<Value>(input) {
            Err(UclError::Syntax(e)) => assert_eq!(e.kind(), &ErrorKind::InvalidUtf8, "{input:?}"),
            other => panic!("{input:?}: expected an error, got {other:?}"),
        }
    }
    let err = from_slice::<Value>(b"a = 1\nb = \"x\xffy\"").unwrap_err();
    assert_eq!(err.position().map(|p| p.line), Some(2));
    let value: Value = from_slice(b"# comment \xff\xfe\nk = 1\n/* \xff */\n").unwrap();
    assert_eq!(value, json!({"k": 1}));
}

#[test]
fn borrowed_str_fields_are_an_error() {
    // Values are owned, so a target that borrows from the input cannot be filled.
    #[derive(Debug, Deserialize)]
    #[allow(
        dead_code,
        reason = "a deserialization target; the test checks the error"
    )]
    struct Borrowed<'a> {
        name: &'a str,
    }
    let err = from_str::<Borrowed>("name = x").unwrap_err();
    assert!(err.to_string().contains("borrowed"), "{err}");

    // `String` and `Cow<str>` work.
    #[derive(Debug, Deserialize)]
    struct Owned<'a> {
        name: String,
        #[serde(borrow)]
        alias: Cow<'a, str>,
    }
    let owned: Owned = from_str("name = x\nalias = y").unwrap();
    assert_eq!((owned.name.as_str(), owned.alias.as_ref()), ("x", "y"));
}

#[test]
fn from_reader_reads_to_the_end() {
    let value: Value = from_reader(io::Cursor::new(b"a = 1\nb { c = [1, 2] }\n")).unwrap();
    assert_eq!(value, json!({"a": 1, "b": {"c": [1, 2]}}));

    struct Failing;
    impl Read for Failing {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("device gone"))
        }
    }
    assert!(matches!(
        from_reader::<Value>(Failing),
        Err(UclError::Io(_))
    ));
}

#[test]
fn the_document_deserializer_parses_with_its_parser() {
    #[derive(Debug, Deserialize, PartialEq)]
    struct Service {
        url: String,
        timeout: String,
    }
    let mut parser = Parser::with_flags(ParserFlags::NO_TIME);
    parser.register_variable("HOST", "example.org");
    let service = Service::deserialize(UclDeserializer::from_parser(
        parser,
        b"url = \"https://$HOST/\"\ntimeout = 10s",
    ))
    .unwrap();
    assert_eq!(
        service,
        Service {
            url: "https://example.org/".into(),
            timeout: "10s".into()
        }
    );

    // Settings can be changed through `parser_mut` before deserializing.
    let mut deserializer = UclDeserializer::new("port = $PORT");
    assert!(deserializer.parser().flags().is_empty());
    deserializer.parser_mut().register_variable("PORT", "8080");
    let value = Value::deserialize(deserializer).unwrap();
    assert_eq!(value, json!({"port": "8080"}));
}

#[test]
fn from_str_with_variables_registers_them_in_order() {
    // spec §7.3, §7.4 (oracle with `-v`): braced and unbraced references; an unbraced
    // reference takes the first registered name that is a prefix of the text after `$`.
    let input =
        "url = \"https://${HOST}:$PORT/\"\npath = $ROOT/data\nkeep = \"$HOSTNAME ${HOSTX}\"\n";
    let variables = [("HOST", "example.org"), ("PORT", "8443"), ("ROOT", "/srv")];
    let expected = json!({
        "url": "https://example.org:8443/",
        "path": "/srv/data",
        "keep": "example.orgNAME ${HOSTX}",
    });
    let value: Value = ucl_lexer::from_str_with_variables(input, variables).unwrap();
    assert_eq!(value, expected);
    // From a map, a longer name wins over its prefixes.
    let map = variables
        .iter()
        .chain(&[("HOSTNAME", "h.example.org")])
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    let value: Value = ucl_lexer::from_str_with_map(input, map).unwrap();
    assert_eq!(value["keep"], "h.example.org ${HOSTX}");
    // Text input has no file access here either.
    assert!(matches!(
        ucl_lexer::from_str_with_variables::<Value, _, _, _>(".include \"$ROOT/x\"", variables),
        Err(UclError::Syntax(e)) if matches!(e.kind(), ErrorKind::FileNotFound { .. })
    ));
}

#[test]
fn from_str_with_env_reads_braced_references_only() {
    // SAFETY: no other test reads or writes this variable.
    unsafe { std::env::set_var("UCL_API_TESTS_VAR", "from-env") };
    let value: Result<Value, _> = ucl_lexer::from_str_with_env(
        "braced = \"${UCL_API_TESTS_VAR}\"\nunbraced = \"$UCL_API_TESTS_VAR\"\nbare = ${UCL_API_TESTS_VAR}\nunset = \"${UCL_API_TESTS_UNSET}\"",
    );
    unsafe { std::env::remove_var("UCL_API_TESTS_VAR") };
    assert_eq!(
        value.unwrap(),
        json!({
            "braced": "from-env",
            "unbraced": "$UCL_API_TESTS_VAR",
            "bare": "from-env",
            "unset": "${UCL_API_TESTS_UNSET}",
        })
    );
}

#[test]
fn variable_handlers() {
    // spec §7.7 (oracle with `-H`, whose handler answers names starting with `H_`): the handler
    // is asked only for braced references to names that are not registered; registered
    // variables take precedence; a refused reference is kept as written.
    let parse = |input: &str| {
        let value = ParserBuilder::new()
            .with_variable("H_Y", "registered")
            .with_variable_handler(|name| name.starts_with("H_").then(|| "[handled]".to_string()))
            .build()
            .parse(input.as_bytes())
            .unwrap();
        from_value::<Value>(value).unwrap()
    };
    assert_eq!(
        parse("a = \"${H_X}\"\nb = \"$H_X\"\nc = \"${H_Y}\"\ne = \"${OTHER}\"\nf = ${H_Z}\n"),
        json!({
            "a": "[handled]",
            "b": "$H_X",
            "c": "registered",
            "e": "${OTHER}",
            "f": "[handled]",
        })
    );

    // Handlers chain as closures: here a map first, then the environment.
    let defaults: std::collections::HashMap<&str, &str> = [("REGION", "eu-west-1")].into();
    let mut parser = ParserBuilder::new()
        .with_variable_handler(move |name| {
            defaults
                .get(name)
                .map(|v| v.to_string())
                .or_else(|| std::env::var(name).ok())
        })
        .build();
    let value = parser
        .parse(b"region = \"${REGION}\"\nmissing = \"${UCL_API_TESTS_NOT_SET}\"")
        .unwrap();
    assert_eq!(
        from_value::<Value>(value).unwrap(),
        json!({"region": "eu-west-1", "missing": "${UCL_API_TESTS_NOT_SET}"})
    );
}
