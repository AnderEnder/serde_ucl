//! Several inputs into one parser and registered macros (spec §13), through the public API.
//!
//! The conformance cases of `tests/conformance/cases/spec/13-inputs/` cover the rules of §13;
//! these tests cover the API around them, and the behaviour at the joins between inputs that
//! the spec does not state yet, from oracle runs (QUESTIONS.md #59).

use serde::Deserialize;
use std::cell::Cell;
use std::rc::Rc;
use ucl_lexer::emit::to_json_compact;
use ucl_lexer::parse::{
    Error, ErrorKind, Input, MacroCall, MacroError, MemoryLoader, Parser, ParserBuilder,
};
use ucl_lexer::{DuplicateStrategy, ParserFlags, UclDeserializer, UclError, UclObject, UclValue};

fn obj(v: &UclValue) -> &UclObject {
    v.as_object().expect("object")
}

/// Parses `inputs` in turn with `parser`, as bytes with the parser's priority and strategy. A
/// silent stop ends only its input.
fn run_with(parser: &mut Parser, inputs: &[&str]) -> Result<UclValue, Error> {
    let mut session = parser.inputs();
    for text in inputs {
        match session.add(Input::bytes(*text)) {
            Err(e) if !e.is_stopped() => return Err(e),
            _ => {}
        }
    }
    session.finish()
}

fn run(inputs: &[&str]) -> Result<UclValue, Error> {
    run_with(&mut Parser::new(), inputs)
}

fn json(inputs: &[&str]) -> String {
    to_json_compact(&run(inputs).unwrap_or_else(|e| panic!("{inputs:?}: {e}")))
}

fn kind(inputs: &[&str]) -> ErrorKind {
    run(inputs)
        .expect_err(&format!("{inputs:?}"))
        .kind()
        .clone()
}

/// The priorities of the values of `key`.
fn priorities(object: &UclObject, key: &str) -> Vec<u8> {
    let entry = object.entry(key).unwrap();
    entry.slots().iter().map(|s| s.priority()).collect()
}

// ----- several inputs (§13.1) ------------------------------------------------------------

#[test]
fn inputs_combine_with_their_own_priority_and_strategy() {
    // spec §13.1, *Entries combine*
    assert_eq!(
        json(&["a = 1; b = 2;", "a = 3; c = 4;"]),
        r#"{"a":[1,3],"b":2,"c":4}"#
    );
    let mut parser = Parser::new();
    let mut inputs = parser.inputs();
    inputs.add(Input::bytes("a = 1; b = 2;")).unwrap();
    inputs
        .add(Input::bytes("a = 3; c = 4;").with_priority(21))
        .unwrap();
    let value = inputs.finish().unwrap();
    assert_eq!(priorities(obj(&value), "a"), [5]);
    assert_eq!(obj(&value)["a"].as_integer(), Some(3));

    let strategies = |first: DuplicateStrategy, second: DuplicateStrategy| {
        let mut parser = Parser::new();
        let mut inputs = parser.inputs();
        inputs
            .add(Input::bytes("a = 1;\no { x = 1 }").with_strategy(first))
            .unwrap();
        inputs.add(Input::bytes("a = 3;\no { y = 2 }").with_strategy(second))?;
        inputs.finish().map(|v| to_json_compact(&v))
    };
    use DuplicateStrategy::*;
    assert_eq!(
        strategies(Append, Rewrite).unwrap(),
        r#"{"a":3,"o":{"y":2}}"#
    );
    assert_eq!(
        strategies(Append, Merge).unwrap(),
        r#"{"a":[1,3],"o":{"x":1,"y":2}}"#
    );
    assert_eq!(
        strategies(Rewrite, Append).unwrap(),
        r#"{"a":[1,3],"o":[{"x":1},{"y":2}]}"#
    );
    assert!(matches!(
        strategies(Append, Error).unwrap_err().kind(),
        ErrorKind::DuplicateKey { key } if key == "a"
    ));
    // `.priority` applies to the rest of its input only (§9.5).
    let value = run(&[".priority 5\na = 1;", "k = 1;"]).unwrap();
    assert_eq!(
        (priorities(obj(&value), "a"), priorities(obj(&value), "k")),
        (vec![5], vec![0])
    );
    // The parser's own priority and strategy are the defaults of every input.
    let mut parser = ParserBuilder::new()
        .with_priority(2)
        .with_strategy(Rewrite)
        .build();
    let value = run_with(&mut parser, &["a = 1;", "a = 2;"]).unwrap();
    assert_eq!(obj(&value)["a"].as_integer(), Some(2));
    assert_eq!(priorities(obj(&value), "a"), [2]);
}

#[test]
fn a_single_input_is_what_parse_gives() {
    for text in [
        "a = 1\nb { c = [1, 2] }",
        "{ a = 1 } ignored",
        "[1, 2]",
        "",
        "a =\n",
        "# only a comment\n",
        "x \"y{\" z\nk = 1",
    ] {
        let parsed = Parser::new().parse(text.as_bytes()).unwrap();
        assert_eq!(run(&[text]).unwrap(), parsed, "{text:?}");
    }
    // `parse` reports a silent stop with the result; several inputs go on after it.
    let text = "a = 1\n.try_include \"missing\"\nb = 2";
    let stop = Parser::new().parse(text.as_bytes()).unwrap_err();
    assert!(stop.is_stopped());
    assert_eq!(to_json_compact(stop.partial().unwrap()), r#"{"a":1}"#);
    assert_eq!(json(&[text, "k = 1"]), r#"{"a":1,"k":1}"#);
}

#[test]
fn no_input_gives_an_empty_object() {
    let mut parser = Parser::new();
    assert_eq!(
        parser.inputs().finish().unwrap(),
        UclValue::Object(UclObject::new())
    );
}

#[test]
fn a_stop_ends_its_input_only() {
    // spec §13.1, *Errors and silent stops*
    let mut parser = Parser::new();
    let mut inputs = parser.inputs();
    inputs.add(Input::bytes("x = 0;")).unwrap();
    let stop = inputs
        .add(Input::bytes(
            "o {\na = 1;\n.try_include \"missing\"\nb = 2;\n}",
        ))
        .unwrap_err();
    assert!(stop.is_stopped(), "{stop}");
    assert!(matches!(stop.kind(), ErrorKind::Stopped { path } if path == "missing"));
    // The partial result holds what the open object has so far.
    assert_eq!(
        to_json_compact(stop.partial().unwrap()),
        r#"{"x":0,"o":{"a":1}}"#
    );
    // The next input goes on in the object the stopped one left open, and may close it.
    inputs.add(Input::bytes("k = 1\n}\nz = 2")).unwrap();
    assert_eq!(
        to_json_compact(&inputs.finish().unwrap()),
        r#"{"x":0,"o":{"a":1,"k":1},"z":2}"#
    );
    // The check at the end of a later input covers only the containers it opened.
    let e = run(&["o {\n.try_include \"missing\"\n", "p {\nq = 1\n"]).unwrap_err();
    assert_eq!(e.kind(), &ErrorKind::UnterminatedObject);
    assert_eq!(
        json(&["o {\n.try_include \"missing\"\n", "k = 1"]),
        r#"{"o":{"k":1}}"#
    );
}

#[test]
fn an_error_fails_the_whole_parse() {
    // spec §13.1, *Errors and silent stops*
    let mut parser = Parser::new();
    let mut inputs = parser.inputs();
    inputs.add(Input::bytes("k = 1;")).unwrap();
    let e = inputs.add(Input::bytes("x = \"abc")).unwrap_err();
    assert_eq!(e.kind(), &ErrorKind::UnterminatedString);
    assert_eq!((e.position().line, e.position().column), (1, 5));
    // Later inputs are not read: the same error comes back, not the loader's.
    assert_eq!(
        inputs
            .add(Input::file("/no/such/file.conf"))
            .unwrap_err()
            .kind(),
        &ErrorKind::UnterminatedString
    );
    assert_eq!(
        inputs.finish().unwrap_err().kind(),
        &ErrorKind::UnterminatedString
    );
    assert_eq!(
        kind(&["x = \"abc", "k = 1;"]),
        ErrorKind::UnterminatedString
    );
    // An error the end of the parse finds: `null` for a key waiting under `error`.
    let mut parser = Parser::new();
    let mut inputs = parser.inputs();
    inputs.add(Input::bytes("x = 1;")).unwrap();
    inputs
        .add(Input::bytes("\n\nx =\n").with_strategy(DuplicateStrategy::Error))
        .unwrap();
    let e = inputs.finish().unwrap_err();
    assert!(matches!(e.kind(), ErrorKind::DuplicateKey { .. }), "{e}");
    assert_eq!(e.position().line, 3);
}

#[test]
fn inputs_count_as_open_units() {
    // spec §13.1, *How many inputs*
    let texts: Vec<String> = (0..17).map(|i| format!("k{i} = {i};")).collect();
    let texts: Vec<&str> = texts.iter().map(String::as_str).collect();
    assert_eq!(obj(&run(&texts[..16]).unwrap()).len(), 16);
    assert_eq!(kind(&texts), ErrorKind::TooManyInputs { limit: 16 });
    // In the Nth input, includes nest at most 16 − N deep.
    let mut files = MemoryLoader::new();
    files
        .add_file("/one.inc", ".include \"/leaf.inc\"\n")
        .add_file("/leaf.inc", "leaf = 1\n");
    let mut parser = ParserBuilder::new().with_loader(files).build();
    let mut shared = texts[..14].to_vec();
    shared.push(".include \"/leaf.inc\"");
    assert!(obj(&run_with(&mut parser, &shared).unwrap()).contains_key("leaf"));
    shared.insert(0, "k = 1;");
    assert!(matches!(
        run_with(&mut parser, &shared).unwrap_err().kind(),
        ErrorKind::IncludeTooDeep { limit: 16 }
    ));
    shared.remove(0);
    shared.pop();
    shared.push(".include \"/one.inc\"");
    assert!(matches!(
        run_with(&mut parser, &shared).unwrap_err().kind(),
        ErrorKind::IncludeTooDeep { limit: 16 }
    ));
}

#[test]
fn only_the_first_input_sets_up_the_root() {
    // spec §13.1, *The root*
    assert_eq!(kind(&["{ a = 1; }", "b = 2;"]), ErrorKind::AfterRoot);
    assert_eq!(kind(&["[1, 2]", "[3]"]), ErrorKind::AfterRoot);
    assert!(matches!(
        kind(&["a = 1;", "{ b = 2; }"]),
        ErrorKind::InvalidKey { found: Some('{') }
    ));
    assert_eq!(json(&["\n# c\n", "b = 2;"]), r#"{"b":2}"#);
    assert_eq!(json(&["a = 1;\n", "", "b = 2;"]), r#"{"a":1,"b":2}"#);
    // After a closed root, a later input must begin with a line break, `;`, `,` or a comment,
    // after spaces and tabs; the rest of it is then ignored, unless it begins with `}` or `]`
    // (oracle runs, QUESTIONS.md #59).
    for later in [
        "# c\n",
        "  \n# c\n",
        ";",
        "\n;\n# c\n;,\n",
        " #",
        "/* c */",
        "\nk = 2",
        ";k = 2",
        "# c\nk",
        "\n\x0b{",
    ] {
        assert_eq!(json(&["{ a = 1; }", later]), r#"{"a":1}"#, "{later:?}");
        assert_eq!(json(&["[1]", later]), "[1]", "{later:?}");
    }
    for later in ["\x0b", "}", "k", " k = 2"] {
        assert_eq!(
            kind(&["{ a = 1; }", later]),
            ErrorKind::AfterRoot,
            "{later:?}"
        );
    }
    for later in ["\n}", ";]", "/* c */}"] {
        assert!(
            matches!(kind(&["[1]", later]), ErrorKind::UnmatchedClose { .. }),
            "{later:?}"
        );
    }
    // A zero-byte first input: only a leading group of comments, then whitespace (§13.1,
    // *Quirk*).
    for later in ["# c\n", "  \n\n", "/* c */", "# c\n  ", ""] {
        assert_eq!(json(&["", later]), "{}", "{later:?}");
    }
    for later in ["b = 2;", "[3]", "  \n# c\n", "# c\n # d\n", ";"] {
        assert_eq!(kind(&["", later]), ErrorKind::AfterRoot, "{later:?}");
    }
}

#[test]
fn the_end_of_an_input_is_not_a_separator() {
    // spec §13.1, *Where one input ends and the next begins*
    for first in [
        "x = 1",
        "x = 1 ",
        "x = \"s\"",
        "x = 's'",
        "x = abc",
        "x = true\t",
    ] {
        assert!(
            matches!(
                kind(&[first, "k = 1;"]),
                ErrorKind::UnseparatedInput { found: 'k' }
            ),
            "{first:?}"
        );
        for later in [
            "\nk = 1",
            ";k = 1",
            ",k = 1",
            "# c\nk = 1",
            "/* c */k = 1",
            " \nk = 1",
        ] {
            assert_eq!(
                obj(&run(&[first, later]).unwrap()).len(),
                2,
                "{first:?} {later:?}"
            );
        }
    }
    for first in [
        "x = 1;",
        "x = 1,",
        "x = 1 # c",
        "x { y = 1 }",
        "x [1]",
        "x = <<EOD\nhi\nEOD\n",
    ] {
        assert_eq!(obj(&run(&[first, "k = 1"]).unwrap()).len(), 2, "{first:?}");
    }
    // A quoted value ends at its quote: spaces after it at the end are enough (oracle runs).
    assert_eq!(json(&["x = \"s\" ", "k = 1"]), r#"{"x":"s","k":1}"#);
    // A zero-byte input changes nothing; one of whitespace alone separates (oracle runs).
    assert!(matches!(
        kind(&["x = 1", "", "k = 1"]),
        ErrorKind::UnseparatedInput { .. }
    ));
    assert_eq!(json(&["x = 1", " ", "k = 1"]), r#"{"x":1,"k":1}"#);
    assert!(matches!(
        kind(&["x = 1", "\x0b"]),
        ErrorKind::UnseparatedInput { .. }
    ));
    // A closing bracket may follow directly, in an object a stopped input left open.
    assert_eq!(
        json(&["o {\n.try_include \"missing\"\n", "k = 1", "}"]),
        r#"{"o":{"k":1}}"#
    );
    // An entry cannot be split across inputs.
    for inputs in [["x", " = 1"], ["x =", " 1"], ["x =", "\n"]] {
        assert_eq!(kind(&inputs), ErrorKind::MissingValue, "{inputs:?}");
    }
}

#[test]
fn whitespace_alone_between_entries_needs_a_separator_after_it() {
    // Oracle runs (QUESTIONS.md #59): a later input of whitespace alone, where an entry could
    // start, makes the next input need a separator first; another such input undoes that.
    assert!(matches!(
        kind(&["a = 1;\n", "  ", "k = 1"]),
        ErrorKind::UnseparatedInput { .. }
    ));
    assert!(matches!(
        kind(&["a = 1;\n", "\n", "k = 1"]),
        ErrorKind::UnseparatedInput { .. }
    ));
    assert_eq!(json(&["a = 1;\n", " ", " ", "k = 1"]), r#"{"a":1,"k":1}"#);
    assert_eq!(json(&["a = 1;\n", "  ", ";k = 1"]), r#"{"a":1,"k":1}"#);
    assert_eq!(json(&["a = 1;\n", "# c\n ", "k = 1"]), r#"{"a":1,"k":1}"#);
    assert_eq!(json(&["  ", "k = 1"]), r#"{"k":1}"#);
    assert!(matches!(
        kind(&["  ", "  ", "k = 1"]),
        ErrorKind::UnseparatedInput { .. }
    ));
    // Section names after a separator and a line break leave the same state.
    assert!(matches!(
        kind(&["c \"x{\" =\n", "d {}"]),
        ErrorKind::UnseparatedInput { .. }
    ));
    assert_eq!(
        json(&["c \"x{\" =\n", "# c\n", "d {}"]),
        r#"{"c":{"x{":{"d":{}}}}"#
    );
    // Where an entry could start, a `#` that is the input's last byte after whitespace is an
    // error, and the end of the input before counts as whitespace (§2.2; oracle runs).
    for later in ["#", " #", "\n#"] {
        assert_eq!(
            kind(&["a = 1;\n", later]),
            ErrorKind::HashAtEnd,
            "{later:?}"
        );
    }
    assert_eq!(json(&["a = 1;\n", "# c\n#"]), r#"{"a":1}"#);
}

#[test]
fn a_value_on_a_following_line_may_come_from_a_later_input() {
    // Oracle runs (QUESTIONS.md #59): after a key, its separator and a line break, the value
    // comes from the next input with text; it is inserted with the settings of the key's input.
    assert_eq!(json(&["x =\n", "v"]), r#"{"x":"v"}"#);
    assert_eq!(json(&["x =\n", "", "# c\n", "  v"]), r#"{"x":"v"}"#);
    assert_eq!(json(&["x =\n", "k = 1\n"]), r#"{"x":"k = 1"}"#);
    assert_eq!(json(&["x =\n"]), r#"{"x":null}"#);
    // An input that is a single `#` is an error there; with more after it, a comment.
    assert_eq!(kind(&["x =\n", "#"]), ErrorKind::HashAtEnd);
    assert_eq!(json(&["x =\n", " #"]), r#"{"x":null}"#);
    assert_eq!(json(&["x =\n", "#\nv"]), r#"{"x":"v"}"#);
    let mut parser = Parser::new();
    let mut inputs = parser.inputs();
    inputs.add(Input::bytes(".priority 3\nx =\n")).unwrap();
    inputs
        .add(Input::bytes("{ a = 1 }").with_priority(5))
        .unwrap();
    let value = inputs.finish().unwrap();
    let x = obj(&value).entry("x").unwrap();
    assert_eq!(x.slots()[0].priority(), 3);
    assert_eq!(priorities(obj(x.first()), "a"), [5]);
    let mut parser = Parser::new();
    let mut inputs = parser.inputs();
    inputs.add(Input::bytes("x = 1;\nx =\n")).unwrap();
    inputs
        .add(Input::bytes("v").with_strategy(DuplicateStrategy::Error))
        .unwrap();
    assert_eq!(
        to_json_compact(&inputs.finish().unwrap()),
        r#"{"x":[1,"v"]}"#
    );
}

#[test]
fn a_file_that_stops_stays_open() {
    // Oracle runs (QUESTIONS.md #59): an included file that stops silently stays open for the
    // rest of the parse: it counts as an open unit, its file variables stay, and including it
    // again is including itself. An input given as bytes goes on in the file before it.
    let mut files = MemoryLoader::new();
    files
        .add_file("/stop.inc", "i = 1;\n.try_include \"/missing\"\nj = 2;\n")
        .add_file("/one.inc", "o = 1;\n")
        .add_file("/self.inc", "s = 1;\n");
    let mut parser = ParserBuilder::new().with_loader(files).build();
    let value = run_with(
        &mut parser,
        &[
            ".include \"/stop.inc\"\n",
            ".include \"/one.inc\"\n",
            "h = \"$FILENAME\"\n",
        ],
    )
    .unwrap();
    assert_eq!(to_json_compact(&value), r#"{"i":1,"o":1,"h":"/stop.inc"}"#);
    let twice = [".include \"/stop.inc\"\n", ".include \"/stop.inc\"\n"];
    assert!(matches!(
        run_with(&mut parser, &twice).unwrap_err().kind(),
        ErrorKind::IncludeSelf { .. }
    ));
    let texts: Vec<String> = (1..16).map(|i| format!("k{i} = {i};")).collect();
    let mut inputs: Vec<&str> = vec![".include \"/stop.inc\"\n"];
    inputs.extend(texts[..14].iter().map(String::as_str));
    assert!(run_with(&mut parser, &inputs).is_ok());
    inputs.push("k = 0;");
    assert_eq!(
        run_with(&mut parser, &inputs).unwrap_err().kind(),
        &ErrorKind::TooManyInputs { limit: 16 }
    );
    // After an input given as a file, one given as bytes is in that file.
    let mut session = parser.inputs();
    session.add(Input::file("/self.inc")).unwrap();
    assert!(matches!(
        session
            .add(Input::bytes(".include \"/self.inc\"\n"))
            .unwrap_err()
            .kind(),
        ErrorKind::IncludeSelf { .. }
    ));
}

#[test]
fn file_inputs_read_through_the_loader_and_set_the_file_variables() {
    // spec §13.1, *File variables and paths*
    let mut files = MemoryLoader::new();
    files
        .add_file(
            "/etc/app/app.conf",
            "f = \"$FILENAME\"; d = \"$CURDIR\"\n.include \"part.conf\"\n",
        )
        .add_file("/etc/app/part.conf", "part = app\n")
        .add_file("/srv/part.conf", "part = srv\n")
        .set_current_dir("/srv");
    let texts = |parser: &mut Parser, file_first: bool| {
        let mut inputs = parser.inputs();
        let bytes = "b = \"$FILENAME $CURDIR\"\n.include \"part.conf\"\n";
        if file_first {
            inputs.add(Input::file("/etc/app/app.conf")).unwrap();
            inputs.add(Input::bytes(bytes)).unwrap();
        } else {
            inputs.add(Input::bytes(bytes)).unwrap();
            inputs.add(Input::file("/etc/app/app.conf")).unwrap();
            inputs.add(Input::bytes("g = \"$FILENAME\"")).unwrap();
        }
        to_json_compact(&inputs.finish().unwrap())
    };
    // Without a base directory, a file input resolves includes against its own directory, one
    // given as bytes against the loader's current directory (WORKLIST C8b decision 4).
    let mut parser = ParserBuilder::new().with_loader(files.clone()).build();
    assert_eq!(
        texts(&mut parser, true),
        r#"{"f":"/etc/app/app.conf","d":"/etc/app","part":["app","srv"],"b":"/etc/app/app.conf /etc/app"}"#
    );
    assert_eq!(
        texts(&mut parser, false),
        r#"{"b":"undef /srv","part":["srv","app"],"f":"/etc/app/app.conf","d":"/etc/app","g":"/etc/app/app.conf"}"#
    );
    // A base directory stands in for every input.
    let mut parser = ParserBuilder::new()
        .with_loader(files.clone())
        .with_base_dir("/srv")
        .build();
    assert!(texts(&mut parser, true).contains(r#""part":["srv","srv"]"#));
    // Under NO_FILEVARS, a file input still sets them, after the registered variables (oracle
    // runs, QUESTIONS.md #59), so `FILE` is matched first in `$FILENAME`.
    let mut parser = ParserBuilder::new()
        .with_loader(files)
        .with_flags(ParserFlags::NO_FILEVARS)
        .with_variable("FILE", "f")
        .build();
    let mut inputs = parser.inputs();
    inputs
        .add(Input::bytes("a = \"$FILENAME ${FILENAME}\"\n"))
        .unwrap();
    inputs.add(Input::file("/etc/app/part.conf")).unwrap();
    inputs
        .add(Input::bytes("b = \"$FILENAME ${FILENAME}\""))
        .unwrap();
    assert_eq!(
        to_json_compact(&inputs.finish().unwrap()),
        r#"{"a":"fNAME ${FILENAME}","part":"app","b":"fNAME /etc/app/part.conf"}"#
    );
    // A file that cannot be read fails the parse.
    let mut parser = Parser::new();
    let mut inputs = parser.inputs();
    assert!(matches!(
        inputs.add(Input::file("/nowhere.conf")).unwrap_err().kind(),
        ErrorKind::Io { .. }
    ));
}

#[test]
fn the_input_limit_counts_every_input() {
    let mut parser = ParserBuilder::new().with_max_input_bytes(10).build();
    let mut inputs = parser.inputs();
    inputs.add(Input::bytes("a = 1;")).unwrap();
    let e = inputs.add(Input::bytes("b = 2;")).unwrap_err();
    assert!(matches!(
        e.kind(),
        ErrorKind::InputTooLarge {
            limit: 10,
            path: None
        }
    ));
}

#[test]
fn saved_comments_and_output_facts_describe_the_result() {
    // spec §13.1, *Entries combine*: comments pending at the end of an input attach there, to
    // the value created most recently; after a silent stop they wait for the next value
    // (oracle runs).
    let mut parser = Parser::with_flags(ParserFlags::SAVE_COMMENTS);
    let mut inputs = parser.inputs();
    inputs.add(Input::bytes("a = 1; # tail\n")).unwrap();
    inputs.add(Input::bytes("# head\nb = 'q';\n")).unwrap();
    inputs.add(Input::bytes("# more\n")).unwrap();
    assert!(
        inputs
            .add(Input::bytes("# c\n.try_include \"missing\"\n"))
            .unwrap_err()
            .is_stopped()
    );
    inputs.add(Input::bytes("d = 4\n")).unwrap();
    let value = inputs.finish().unwrap();
    let texts: Vec<(String, Vec<&str>)> = parser
        .attached_comments()
        .iter()
        .map(|group| {
            let path = format!("{:?}", group.path);
            let texts = group
                .comments
                .iter()
                .map(|&i| parser.comments()[i].text.as_str())
                .collect();
            (path, texts)
        })
        .collect();
    assert_eq!(texts.len(), 3, "{texts:?}");
    assert_eq!(texts[0].1, ["# tail"]);
    assert_eq!(texts[1].1, ["# head", "# more"]);
    assert_eq!(texts[2].1, ["# c"]);
    // Positions are in the input each comment was read from.
    assert_eq!(parser.comments()[1].position.line, 1);
    // The output facts: `b` was single-quoted.
    let config = parser.emitter(ucl_lexer::emit::Format::Config).emit(&value);
    assert!(config.contains("b = 'q';"), "{config}");
}

// ----- registered macros (§13.2) ------------------------------------------------------------

/// A handler that adds `seen = { data: VALUE, args: ARGUMENTS or null }`, like the oracle's.
fn seen(call: &mut MacroCall<'_>) -> Result<(), MacroError> {
    let mut seen = UclObject::new();
    seen.insert(
        "data",
        UclValue::String(String::from_utf8_lossy(call.value()).into_owned()),
    );
    seen.insert("args", call.arguments().cloned().unwrap_or(UclValue::Null));
    call.add("seen", UclValue::Object(seen))
}

/// A handler that has its VALUE parsed in place.
fn emit(call: &mut MacroCall<'_>) -> Result<(), MacroError> {
    let text = call.value().to_vec();
    call.parse(text)
}

fn with_macros() -> Parser {
    ParserBuilder::new()
        .with_macro("seen", seen)
        .with_macro("emit", emit)
        .with_macro("fail", |_| Err(MacroError::stop()))
        .with_macro("bad", |call| {
            Err(MacroError::new(format!(
                "cannot use {:?}",
                call.value_str()
            )))
        })
        .with_context_macro("ctx", |call| {
            let root = call.root().cloned().unwrap();
            call.add("ctx", root)
        })
        .build()
}

fn macro_json(text: &str) -> String {
    let value = with_macros()
        .parse(text.as_bytes())
        .unwrap_or_else(|e| panic!("{text:?}: {e}"));
    to_json_compact(&value)
}

fn macro_error(text: &str) -> Error {
    with_macros().parse(text.as_bytes()).expect_err(text)
}

#[test]
fn handlers_add_entries_where_the_macro_stands() {
    // spec §13.2, *Add entries*
    assert_eq!(
        macro_json("a = 1\no { .seen \"v\" }\nk = 1"),
        r#"{"a":1,"o":{"seen":{"data":"v","args":null}},"k":1}"#
    );
    // The duplicate rules do not apply: priority 0, a further value of the key.
    let mut parser = with_macros();
    parser
        .set_priority(5)
        .set_strategy(DuplicateStrategy::Rewrite)
        .set_flags(ParserFlags::NO_IMPLICIT_ARRAYS);
    let value = parser
        .parse(b"seen = 1;\n.seen \"a\"\n.seen \"b\"\n")
        .unwrap();
    assert_eq!(priorities(obj(&value), "seen"), [5, 0, 0]);
    // Values a handler adds are not values created for saved comments (§12.5; oracle runs).
    let mut parser = with_macros();
    parser.set_flags(ParserFlags::SAVE_COMMENTS);
    parser.parse(b"# c\n.seen \"v\"\nb = 1\n").unwrap();
    let group = &parser.attached_comments()[0];
    assert_eq!(
        format!("{:?}", group.path),
        r#"[Key { key: "b", index: 0 }]"#
    );
}

#[test]
fn handlers_get_the_value_and_the_arguments() {
    // spec §13.2, *What the handler receives*
    let mut parser = with_macros();
    parser.register_variable("ABI", "unknown");
    let text = ".seen(a = 1, b = \"s\", n = 1, n = 2) \"a\\\\nb $ABI\"\n\
                .seen() {  x = 1; y  }\n.seen bare ;\n.seen([1, 2]) \"$$\"\n\
                .seen(o = { .priority 3; p = 1 }) \"$$ $ABI\"";
    let value = parser.parse(text.as_bytes()).unwrap();
    assert_eq!(
        to_json_compact(&value),
        r#"{"seen":[{"data":"a\\\\nb unknown","args":{"a":1,"b":"s","n":[1,2]}},{"data":"x = 1; y  ","args":{}},{"data":"bare ","args":null},{"data":"$$","args":[1,2]},{"data":"$ unknown","args":{"o":{"p":1}}}]}"#
    );
    let args = obj(&value).get_all("seen").nth(4).unwrap();
    let o = obj(&obj(args)["args"])["o"].as_object().unwrap();
    assert_eq!(priorities(o, "p"), [3]);
    // Under `key-lowercase` the names are lowercased, not the values.
    let mut parser = with_macros();
    parser.set_flags(ParserFlags::KEY_LOWERCASE);
    let value = parser.parse(b".seen(N = 1, K = \"A\") \"V\"").unwrap();
    assert_eq!(
        to_json_compact(&value),
        r#"{"seen":{"data":"V","args":{"n":1,"k":"A"}}}"#
    );
}

#[test]
fn handlers_have_text_parsed_in_place() {
    // spec §13.2, *Have text parsed in place*
    assert_eq!(
        macro_json(".emit \"a = 1; b { c = 2 }\"\ny = 2"),
        r#"{"a":1,"b":{"c":2},"y":2}"#
    );
    assert_eq!(
        macro_json(".emit \"{ x = 1 } y = 2\"\nz = 3"),
        r#"{"x":1,"y":2,"z":3}"#
    );
    // The text takes the settings where the macro stands; `.priority` in it stays in it.
    let value = with_macros()
        .parse(b".priority 4\n.emit {x = 1; .priority 7; y = 1}\nz = 1")
        .unwrap();
    let root = obj(&value);
    assert_eq!(
        (
            priorities(root, "x"),
            priorities(root, "y"),
            priorities(root, "z")
        ),
        (vec![4], vec![7], vec![4])
    );
    // Registered macros work in the text, and a handler can run inside its own text.
    assert_eq!(
        macro_json(".emit \".emit {x = 1}\"\n.emit {.seen \"in\"}"),
        r#"{"x":1,"seen":{"data":"in","args":null}}"#
    );
    // The text's error is the parse's, reported at the macro with the text's kind; the handler
    // sees it with its position in the text.
    let e = macro_error("a = 1\n.emit \"b = 1; c {\"");
    assert_eq!(e.kind(), &ErrorKind::UnterminatedObject);
    assert_eq!((e.position().line, e.position().column), (2, 1));
    let seen_line = Rc::new(Cell::new(0));
    let line = Rc::clone(&seen_line);
    let mut parser = ParserBuilder::new()
        .with_macro("emit", move |call| {
            let result = call.parse("b = 1\nc = \"open");
            let error = result.as_ref().unwrap_err().parse_error().unwrap();
            line.set(error.position().line);
            // Whatever the handler returns, the text's error decides.
            Ok(())
        })
        .build();
    let e = parser.parse(b"x = 1\n.emit {}").unwrap_err();
    assert_eq!(e.kind(), &ErrorKind::UnterminatedString);
    assert_eq!(seen_line.get(), 2);
    for (text, expected) in [
        (".emit \"[1]\"", ErrorKind::IncludeArrayRoot),
        (
            "o { .emit \"{ x = 1 }\" }",
            ErrorKind::UnmatchedClose { found: '}' },
        ),
        (".emit \"x\"", ErrorKind::MissingValue),
    ] {
        assert_eq!(macro_error(text).kind(), &expected, "{text:?}");
    }
    // A silent stop in the text stops the parse.
    let stop = macro_error("a = 1\n.emit \".try_include missing\"\nk = 1");
    assert!(stop.is_stopped());
    assert_eq!(to_json_compact(stop.partial().unwrap()), r#"{"a":1}"#);
}

#[test]
fn handlers_fail_with_a_stop_or_a_message() {
    // spec §13.2, *Fail*; the message is WORKLIST C8b decision 3.
    let stop = macro_error("o { a = 1\n.fail \"x\"\nb = 2 }");
    assert!(matches!(stop.kind(), ErrorKind::MacroStopped { name } if name == "fail"));
    assert!(stop.is_stopped());
    assert_eq!((stop.position().line, stop.position().column), (2, 1));
    assert_eq!(to_json_compact(stop.partial().unwrap()), r#"{"o":{"a":1}}"#);
    let e = macro_error("a = 1\n  .bad \"x\"");
    assert_eq!(
        e.kind(),
        &ErrorKind::MacroFailed {
            name: "bad".into(),
            message: "cannot use Some(\"x\")".into()
        }
    );
    assert_eq!((e.position().line, e.position().column), (2, 3));
    assert!(!e.is_stopped());
    // Through the crate's error type.
    assert!(matches!(UclError::from(stop), UclError::Stopped(_)));
    assert!(matches!(UclError::from(e), UclError::Syntax(_)));
    // A failure in a later input ends that input only (§13.1).
    let mut parser = with_macros();
    let value = run_with(
        &mut parser,
        &["o {\na = 1;\n.fail \"x\"\n", "k = 1\n}\nz = 2"],
    )
    .unwrap();
    assert_eq!(to_json_compact(&value), r#"{"o":{"a":1,"k":1},"z":2}"#);
    // Adding needs an open object.
    let mut parser = ParserBuilder::new()
        .with_macro("close", |call| {
            call.parse("}")?;
            call.add("k", UclValue::Integer(1))
        })
        .build();
    assert!(matches!(
        parser.parse(b"{ .close {}").unwrap_err().kind(),
        ErrorKind::MacroFailed { .. }
    ));
}

#[test]
fn registered_names() {
    // spec §13.2, *Where they are recognised*
    assert!(matches!(
        macro_error(".SEEN \"v\"").kind(),
        ErrorKind::UnknownMacro { name } if name == "SEEN"
    ));
    assert!(matches!(
        macro_error(".seen2 \"v\"").kind(),
        ErrorKind::UnknownMacro { .. }
    ));
    // Not in argument documents, which know only the built-in macros.
    assert!(matches!(
        macro_error(".seen(a = { .seen x; }) \"v\"").kind(),
        ErrorKind::UnknownMacro { name } if name == "seen"
    ));
    assert_eq!(
        macro_json("arr [ .seen \"v\" ]"),
        r#"{"arr":[".seen \"v\""]}"#
    );
    assert_eq!(macro_json("a = 1\n.seen\n"), r#"{"a":1}"#);
    let mut parser = with_macros();
    parser.set_flags(ParserFlags::DISABLE_MACRO);
    assert_eq!(
        parser.parse(b".seen \"v\"").unwrap_err().kind(),
        &ErrorKind::MacrosDisabled
    );
    // An application name replaces a built-in one; argument documents keep the built-in.
    let mut parser = ParserBuilder::new().with_macro("priority", seen).build();
    let value = parser
        .parse(b".priority(o = { .priority 3; p = 1 }) 3\na = 1")
        .unwrap();
    assert_eq!(
        to_json_compact(&value),
        r#"{"seen":{"data":"3","args":{"o":{"p":1}}},"a":1}"#
    );
    assert_eq!(priorities(obj(&value), "a"), [0]);
    // A name that is empty or holds whitespace never matches.
    let mut parser = ParserBuilder::new()
        .with_macro("", seen)
        .with_macro("a b", seen)
        .build();
    assert!(matches!(
        parser.parse(b". \"v\"").unwrap_err().kind(),
        ErrorKind::UnknownMacro { .. }
    ));
    // Registered macros apply to every input and to included files.
    let mut files = MemoryLoader::new();
    files.add_file("/m.inc", ".seen \"from include\"\ninc = 1\n");
    let mut parser = with_macros();
    parser.set_loader(files);
    let value = run_with(
        &mut parser,
        &["a = 1;", ".include \"/m.inc\"\n.seen \"second\""],
    )
    .unwrap();
    assert_eq!(
        to_json_compact(&value),
        r#"{"a":1,"seen":[{"data":"from include","args":null},{"data":"second","args":null}],"inc":1}"#
    );
}

#[test]
fn context_macros_see_the_root_built_so_far() {
    // spec §13.2, *What the handler receives*
    assert_eq!(
        macro_json("q = 1; o { a = 1; .ctx \"x\" }"),
        r#"{"q":1,"o":{"a":1,"ctx":{"q":1,"o":{"a":1}}}}"#
    );
    let mut parser = ParserBuilder::new()
        .with_macro("plain", |call| {
            assert!(call.root().is_none());
            Ok(())
        })
        .build();
    parser.parse(b".plain {}").unwrap();
}

#[test]
fn context_macros_get_the_root_priority() {
    // spec §13.2, *The root's priority*: that of the first input; `.priority` does not change
    // it. A copy added with it keeps it, and it takes part in §8.3 for later values.
    let snapshot = |call: &mut MacroCall<'_>| {
        let root = call.root().cloned().unwrap();
        let priority = call.root_priority().unwrap();
        call.add_with_priority("ctx", root, priority)
    };
    let priorities = |value: &UclValue, key: &str| -> Vec<u8> {
        let entry = obj(value).entry(key).unwrap();
        entry.slots().iter().map(|slot| slot.priority()).collect()
    };
    let mut parser = ParserBuilder::new()
        .with_priority(3)
        .with_context_macro("ctx", snapshot)
        .build();
    let v = parser.parse(b".priority 7\na = 1\n.ctx c").unwrap();
    assert_eq!(priorities(&v, "a"), [7]);
    assert_eq!(priorities(&v, "ctx"), [3]);
    assert_eq!(priorities(&obj(&v)["ctx"], "a"), [7]);
    let v = parser
        .parse(b"a = 1\n.ctx c\n.priority 1\nctx = 1")
        .unwrap();
    assert_eq!(priorities(&v, "ctx"), [3]);
    assert!(obj(&v)["ctx"].is_object());
    let v = parser
        .parse(b"a = 1\n.ctx c\n.priority 5\nctx = 1")
        .unwrap();
    assert_eq!(obj(&v)["ctx"], UclValue::Integer(1));
    // With several inputs, the first decides.
    let mut parser = ParserBuilder::new()
        .with_context_macro("ctx", snapshot)
        .build();
    let mut session = parser.inputs();
    session.add(Input::bytes("a = 1;")).unwrap();
    session
        .add(Input::bytes(".ctx c").with_priority(5))
        .unwrap();
    let v = session.finish().unwrap();
    assert_eq!(priorities(&v, "ctx"), [0]);
    // `add` adds at priority 0; a macro that is not a context macro gets no root priority.
    let mut parser = ParserBuilder::new()
        .with_priority(3)
        .with_macro("plain", |call| {
            assert_eq!(call.root_priority(), None);
            call.add_with_priority("p", UclValue::Integer(1), 0x13)?;
            call.add("q", UclValue::Integer(1))
        })
        .build();
    let v = parser.parse(b".plain {}").unwrap();
    assert_eq!(priorities(&v, "p"), [3]);
    assert_eq!(priorities(&v, "q"), [0]);
}

#[test]
fn deserialization_errors_do_not_run_handlers_again() {
    #[derive(Debug, Deserialize)]
    #[allow(dead_code)]
    struct Config {
        port: u16,
    }
    let calls = Rc::new(Cell::new(0));
    let counter = Rc::clone(&calls);
    let parser = ParserBuilder::new()
        .with_macro("port", move |call| {
            counter.set(counter.get() + 1);
            call.add("port", UclValue::String("http".into()))
        })
        .build();
    let e = Config::deserialize(UclDeserializer::from_parser(parser, b".port {}")).unwrap_err();
    assert!(matches!(e, UclError::Deserialize(_)), "{e}");
    assert_eq!(calls.get(), 1);
    assert!(e.position().is_none());
}
