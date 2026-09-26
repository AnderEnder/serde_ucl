//! Parsing, the emitters and every serde entry point survive the deepest documents the parser
//! accepts, on a 2 MiB thread stack, the default of a spawned thread (clean-room work items C7
//! and C10).
//!
//! The parser accepts 1024 containers nested inside one another, the root included (spec §11.2,
//! `parse::MAX_NESTING`). Copies made by `.inherit` nest a value up to the parser's `.inherit`
//! depth limit, 1024 by default and at most `parse::MAX_INHERIT_DEPTH_LIMIT`; the tests at the
//! end take every entry point to that depth: parsing, cloning, comparing and dropping the value,
//! the emitters, and serde to and from `UclValue` and typed targets. A `UclValue` is
//! deserialized and serialized at any of these depths. Other types are read and written by
//! recursion through their serde impls, and fail with `SerdeError::TooDeep` past
//! `MAX_SERDE_NESTING` maps and sequences.
//!
//! `cargo test` builds this crate optimised (`[profile.test.package.ucl-rust-lexer]` in
//! `Cargo.toml`), which takes less stack than a debug build. `scripts/ci.sh` also runs this file
//! with the crate unoptimised, as a debug build of an application builds it; that run is what
//! shows `MAX_INHERIT_DEPTH_LIMIT` to be safe.

use serde::{Deserialize, Serialize};
use std::io::Write;
use ucl_lexer::emit::Format;
use ucl_lexer::error::SerdeError;
use ucl_lexer::parse::{ErrorKind, MAX_INHERIT_DEPTH_LIMIT, MAX_NESTING, Parser, ParserBuilder};
use ucl_lexer::{MAX_SERDE_NESTING, ParserFlags, UclDeserializer, UclError, UclObject, UclValue};

/// Runs `f` on a thread with a 2 MiB stack. A stack overflow aborts the whole test process.
fn on_small_stack(f: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(2 << 20)
        .spawn(f)
        .expect("spawn")
        .join()
        .expect("the test thread panicked");
}

/// The most containers nested inside one another in `value`, itself included.
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

/// Whether `a == b`: keys in any order, the values of each key in order, with their priorities
/// and marks. `==` compares with a heap stack at any depth.
fn same(a: &UclValue, b: &UclValue) -> bool {
    a == b
}

/// Objects nested `depth` deep, the root included: `a { a { … b = 1 } }`.
fn objects(depth: usize) -> String {
    format!(
        "{}b = 1{}",
        "a { ".repeat(depth - 1),
        " }".repeat(depth - 1)
    )
}

/// An array inside the root object, then arrays: `depth` containers with the root.
fn arrays(depth: usize) -> String {
    format!("a = {}1{}", "[".repeat(depth - 1), "]".repeat(depth - 1))
}

/// A root array, then arrays: `depth` containers.
fn root_arrays(depth: usize) -> String {
    format!("{}1{}", "[".repeat(depth), "]".repeat(depth))
}

/// Each object holds a key with two values, a number and the next object, and ends in a
/// one-element array: `depth` containers with the root. A multi-value entry reads as a
/// sequence, one more level for a typed target.
fn multi_values(depth: usize) -> String {
    format!(
        "{}k = [1]{}",
        "k = 1; k { ".repeat(depth - 2),
        " }".repeat(depth - 2)
    )
}

/// `.inherit` copies an object 600 deep into one 424 deep: 1024 containers with the root. The
/// copies are marked inherited.
fn inherited() -> String {
    format!(
        "d {{ {}leaf = 1;{}\ne {{ {}.inherit \"d\";{}\n",
        "a { ".repeat(599),
        " }".repeat(600),
        "a { ".repeat(423),
        " }".repeat(424)
    )
}

/// Objects nested `depth` deep, the root included, each after a comment, and a single-quoted
/// string in the last: comments and output facts at every level.
fn commented(depth: usize) -> String {
    format!(
        "{}v = 'x'{}",
        "# c\na {\n".repeat(depth - 1),
        "\n}".repeat(depth - 1)
    )
}

/// The deepest documents the parser accepts, each nested 1024 deep, with their names.
fn deepest() -> Vec<(&'static str, String)> {
    let documents = vec![
        ("objects", objects(MAX_NESTING)),
        ("commented", commented(MAX_NESTING)),
        ("arrays", arrays(MAX_NESTING)),
        ("root arrays", root_arrays(MAX_NESTING)),
        ("multi-values", multi_values(MAX_NESTING)),
        ("inherited", inherited()),
    ];
    for (name, document) in &documents {
        let value = ucl_lexer::parse::parse(document.as_bytes()).expect(name);
        assert_eq!(nesting(&value), MAX_NESTING, "{name}");
    }
    documents
}

/// Asserts that `result` is the error for nesting past `MAX_SERDE_NESTING`: of serialization, or
/// of deserialization.
fn assert_too_deep<T>(result: Result<T, UclError>, what: &str) {
    match result {
        Err(UclError::Serde(SerdeError::TooDeep { limit })) => {
            assert_eq!(limit, MAX_SERDE_NESTING, "{what}")
        }
        Err(UclError::Deserialize(e)) => match e.error() {
            SerdeError::TooDeep { limit } => assert_eq!(*limit, MAX_SERDE_NESTING, "{what}"),
            other => panic!("{what}: expected TooDeep, got {other}"),
        },
        Err(other) => panic!("{what}: expected TooDeep, got {other}"),
        Ok(_) => panic!("{what}: expected TooDeep, got a value"),
    }
}

/// A file under the test's scratch directory holding `document`.
fn file(name: &str, document: &str) -> std::path::PathBuf {
    let path =
        std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("stack_depth_{name}.ucl"));
    let mut f = std::fs::File::create(&path).expect("create");
    f.write_all(document.as_bytes()).expect("write");
    path
}

#[test]
fn parse_and_emit_at_the_parser_limit() {
    on_small_stack(|| {
        for (name, document) in deepest() {
            let mut parser = Parser::with_flags(ParserFlags::SAVE_COMMENTS);
            let value = parser.parse(document.as_bytes()).expect(name);
            assert_eq!(nesting(&value), MAX_NESTING, "{name}");
            // libucl's formats, with the output facts of the parse.
            for format in [
                Format::Json,
                Format::JsonCompact,
                Format::Config,
                Format::Yaml,
            ] {
                assert!(!parser.emitter(format).emit(&value).is_empty(), "{name}");
            }
            let with_comments = parser
                .emitter(Format::Config)
                .with_comments(parser.comments(), parser.attached_comments())
                .emit(&value);
            assert!(
                with_comments.contains("# c") || name != "commented",
                "{name}"
            );
        }
    });
}

#[test]
fn text_into_ucl_value_at_the_parser_limit() {
    on_small_stack(|| {
        for (name, document) in deepest() {
            let parsed = ucl_lexer::parse::parse(document.as_bytes()).unwrap();
            let values: Vec<(&str, UclValue)> = vec![
                ("from_str", ucl_lexer::from_str(&document).expect(name)),
                (
                    "from_slice",
                    ucl_lexer::from_slice(document.as_bytes()).expect(name),
                ),
                (
                    "from_reader",
                    ucl_lexer::from_reader(document.as_bytes()).expect(name),
                ),
                (
                    "from_file",
                    ucl_lexer::from_file(file(&name.replace(' ', "_"), &document)).expect(name),
                ),
                (
                    "from_str_with_variables",
                    ucl_lexer::from_str_with_variables(&document, [("V", "v")]).expect(name),
                ),
                (
                    "from_str_with_map",
                    ucl_lexer::from_str_with_map(&document, Default::default()).expect(name),
                ),
                (
                    "from_str_with_env",
                    ucl_lexer::from_str_with_env(&document).expect(name),
                ),
            ];
            for (entry_point, value) in values {
                // The value itself, priorities and `.inherit` marks included.
                assert!(same(&value, &parsed), "{entry_point}, {name}");
            }
        }
    });
}

#[test]
fn from_value_into_ucl_value_at_the_parser_limit() {
    on_small_stack(|| {
        for (name, document) in deepest() {
            let parsed = ucl_lexer::parse::parse(document.as_bytes()).unwrap();
            let value: UclValue = ucl_lexer::from_value(parsed.clone()).expect(name);
            assert!(same(&value, &parsed), "{name}");
            if parsed.is_object() {
                let object: UclObject = ucl_lexer::from_value(parsed.clone()).expect(name);
                assert!(same(&UclValue::Object(object), &parsed), "{name}");
            }
        }
    });
}

#[test]
fn ucl_value_into_every_output_at_the_parser_limit() {
    on_small_stack(|| {
        for (name, document) in deepest() {
            let value = ucl_lexer::parse::parse(document.as_bytes()).unwrap();
            // `to_value` copies the value as it is.
            assert!(
                same(&ucl_lexer::to_value(&value).expect(name), &value),
                "{name}"
            );

            let mut written = Vec::new();
            ucl_lexer::to_writer(&mut written, &value).expect(name);
            let texts = [
                ("to_string", ucl_lexer::to_string(&value).expect(name)),
                ("to_writer", String::from_utf8(written).unwrap()),
                (
                    "to_json_string",
                    ucl_lexer::to_json_string(&value).expect(name),
                ),
                (
                    "to_json_string_compact",
                    ucl_lexer::to_json_string_compact(&value).expect(name),
                ),
                (
                    "to_yaml_string",
                    ucl_lexer::to_yaml_string(&value).expect(name),
                ),
            ];
            for (entry_point, text) in texts {
                // The output reads back as the value (spec §10.8); a parse has no `.inherit`
                // marks, so the inherited document is compared by depth only.
                let back = ucl_lexer::parse::parse(text.as_bytes()).expect(entry_point);
                assert_eq!(nesting(&back), MAX_NESTING, "{entry_point}, {name}");
                if name != "inherited" {
                    assert!(same(&back, &value), "{entry_point}, {name}");
                }
            }
            if let UclValue::Object(object) = &value {
                assert!(
                    same(&ucl_lexer::to_value(object).expect(name), &value),
                    "{name}"
                );
                let text = ucl_lexer::to_string(object).expect(name);
                assert!(ucl_lexer::parse::parse(text.as_bytes()).is_ok(), "{name}");
            }
        }
    });
}

#[test]
fn ucl_value_deeper_than_the_parser_limit() {
    // A value built in code may be deeper than any parsed one: it converts, but has no text.
    on_small_stack(|| {
        let mut value = UclValue::Integer(1);
        for _ in 0..=MAX_NESTING {
            value = UclValue::Array(vec![value]);
        }
        let copy = ucl_lexer::to_value(&value).unwrap();
        assert!(same(&copy, &value));
        let back: UclValue = ucl_lexer::from_value(copy).unwrap();
        assert!(same(&back, &value));
        for result in [
            ucl_lexer::to_string(&value),
            ucl_lexer::to_json_string(&value),
            ucl_lexer::to_json_string_compact(&value),
            ucl_lexer::to_yaml_string(&value),
        ] {
            let error = result.unwrap_err().to_string();
            assert!(error.contains("nested more than 1024"), "{error}");
        }
    });
}

/// A typed target that recurses through its own fields.
#[derive(Debug, Deserialize, Serialize, PartialEq)]
struct Node {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    a: Option<Box<Node>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    b: Option<i64>,
}

impl Node {
    /// `depth` nodes nested inside one another; each is a map.
    fn chain(depth: usize) -> Node {
        let mut node = Node {
            a: None,
            b: Some(1),
        };
        for _ in 1..depth {
            node = Node {
                a: Some(Box::new(node)),
                b: None,
            };
        }
        node
    }
}

impl Drop for Node {
    // The derived drop recurses; the chains here are too shallow for that to matter, but a
    // test that fails early must not trade its message for a stack overflow.
    fn drop(&mut self) {
        let mut next = self.a.take();
        while let Some(mut node) = next {
            next = node.a.take();
        }
    }
}

/// Drops `value` without recursion: `serde_json::Value`'s drop recurses once per level, and a
/// value 1024 deep takes more stack than any entry point under test.
fn drop_json(value: serde_json::Value) {
    let mut stack = vec![value];
    while let Some(value) = stack.pop() {
        match value {
            serde_json::Value::Object(map) => stack.extend(map.into_iter().map(|(_, v)| v)),
            serde_json::Value::Array(items) => stack.extend(items),
            _ => {}
        }
    }
}

/// `serde_json::Value` objects nested `depth` deep, built without recursion. (`json!` with a
/// value inside serializes that value, which recurses.)
fn json_objects(depth: usize) -> serde_json::Value {
    let mut value = serde_json::json!({ "b": 1 });
    for _ in 1..depth {
        let mut map = serde_json::Map::new();
        map.insert("a".to_owned(), value);
        value = serde_json::Value::Object(map);
    }
    value
}

#[test]
fn text_into_typed_targets_up_to_the_serde_limit() {
    on_small_stack(|| {
        let at_limit = objects(MAX_SERDE_NESTING);
        let json: serde_json::Value = ucl_lexer::from_str(&at_limit).unwrap();
        assert!(json == json_objects(MAX_SERDE_NESTING));
        let node: Node = ucl_lexer::from_str(&at_limit).unwrap();
        assert!(node == Node::chain(MAX_SERDE_NESTING));

        // One level more, and the parser's deepest documents, fail at the limit through every
        // entry point.
        let mut documents = vec![("one more", objects(MAX_SERDE_NESTING + 1))];
        documents.extend(deepest());
        for (name, document) in documents {
            let path = file(&format!("typed_{}", name.replace(' ', "_")), &document);
            assert_too_deep(ucl_lexer::from_str::<serde_json::Value>(&document), name);
            assert_too_deep(
                ucl_lexer::from_slice::<serde_json::Value>(document.as_bytes()),
                name,
            );
            assert_too_deep(
                ucl_lexer::from_reader::<serde_json::Value>(document.as_bytes()),
                name,
            );
            assert_too_deep(ucl_lexer::from_file::<serde_json::Value>(&path), name);
            // `Node` skips keys other than `a` and `b`, and reads an array as a map.
            if name == "one more" || name == "objects" {
                assert_too_deep(ucl_lexer::from_str::<Node>(&document), name);
            }
            let value = ucl_lexer::parse::parse(document.as_bytes()).unwrap();
            assert_too_deep(ucl_lexer::from_value::<serde_json::Value>(value), name);
        }

        // A multi-value entry reads as a sequence: one level more for a typed target.
        let multi = multi_values(MAX_SERDE_NESTING / 2 + 1);
        assert!(ucl_lexer::from_str::<serde_json::Value>(&multi).is_ok());
        let multi = multi_values(MAX_SERDE_NESTING / 2 + 2);
        assert_too_deep(
            ucl_lexer::from_str::<serde_json::Value>(&multi),
            "multi-values",
        );

        // A value the target skips is not entered.
        #[derive(Deserialize)]
        struct Skips {
            b: i64,
        }
        let skipped = format!("b = 2\n{}", objects(MAX_NESTING));
        assert_eq!(ucl_lexer::from_str::<Skips>(&skipped).unwrap().b, 2);
    });
}

#[test]
fn ucl_value_inside_a_typed_target_at_the_parser_limit() {
    #[derive(Deserialize, Serialize)]
    struct Wrapper {
        inner: UclValue,
    }
    on_small_stack(|| {
        // The wrapper's map is the root; `inner` holds the other 1023 containers.
        let document = format!("inner {{ {} }}", objects(MAX_NESTING - 1));
        let wrapper: Wrapper = ucl_lexer::from_str(&document).unwrap();
        assert_eq!(nesting(&wrapper.inner), MAX_NESTING - 1);
        let text = ucl_lexer::to_string(&wrapper).unwrap();
        let back = ucl_lexer::parse::parse(text.as_bytes()).unwrap();
        assert!(same(
            &back,
            &ucl_lexer::parse::parse(document.as_bytes()).unwrap()
        ));
    });
}

#[test]
fn typed_values_into_every_output_up_to_the_serde_limit() {
    on_small_stack(|| {
        let json = json_objects(MAX_SERDE_NESTING);
        let node = Node::chain(MAX_SERDE_NESTING);
        let expected = ucl_lexer::parse::parse(objects(MAX_SERDE_NESTING).as_bytes()).unwrap();
        assert!(same(&ucl_lexer::to_value(&json).unwrap(), &expected));
        assert!(same(&ucl_lexer::to_value(&node).unwrap(), &expected));
        let text = ucl_lexer::to_string(&node).unwrap();
        assert!(same(
            &ucl_lexer::parse::parse(text.as_bytes()).unwrap(),
            &expected
        ));

        for (name, json, node) in [
            (
                "one more",
                json_objects(MAX_SERDE_NESTING + 1),
                Node::chain(MAX_SERDE_NESTING + 1),
            ),
            (
                "parser limit",
                json_objects(MAX_NESTING),
                Node::chain(MAX_NESTING),
            ),
        ] {
            assert_too_deep(ucl_lexer::to_value(&json), name);
            assert_too_deep(ucl_lexer::to_value(&node), name);
            assert_too_deep(ucl_lexer::to_string(&json), name);
            assert_too_deep(ucl_lexer::to_json_string(&json), name);
            assert_too_deep(ucl_lexer::to_json_string_compact(&json), name);
            assert_too_deep(ucl_lexer::to_yaml_string(&json), name);
            assert_too_deep(ucl_lexer::to_writer(Vec::new(), &node), name);
            drop_json(json);
        }

        // Variants count their object, and the array or object inside tuple and struct variants.
        #[derive(Serialize)]
        enum Shape {
            Newtype(Box<Shape>),
            Tuple(i64, i64),
            Struct { x: i64 },
        }
        let mut shape = Shape::Tuple(1, 2);
        for _ in 0..MAX_SERDE_NESTING - 2 {
            shape = Shape::Newtype(Box::new(shape));
        }
        assert!(ucl_lexer::to_value(&shape).is_ok());
        assert_too_deep(
            ucl_lexer::to_value(&Shape::Newtype(Box::new(shape))),
            "variants",
        );
        let _ = Shape::Struct { x: 0 };
    });
}

// ----- the `.inherit` depth limit (C10) -----------------------------------------------------

/// The containers of each part of [`inherited_chain`].
#[derive(Debug, Clone, Copy)]
enum Shape {
    /// Objects: `a { a { … } }`.
    Objects,
    /// Objects written with quoted keys after comments, and a single-quoted string at the end:
    /// saved comments and output facts, which copies keep (spec §10.1).
    Commented,
    /// Each object holds a key with two values, a number and the next object: a multi-value
    /// entry at every level, which copies keep, since its first value is not a container
    /// (spec §9.7).
    MultiValues,
    /// Arrays, with an object at the end of each part for the next `.inherit`.
    Arrays,
}

const SHAPES: [Shape; 4] = [
    Shape::Objects,
    Shape::Commented,
    Shape::MultiValues,
    Shape::Arrays,
];

/// The text before and after the `levels` containers of part `index`: its object `r{index}`
/// and `levels - 1` containers inside it.
fn part(shape: Shape, index: usize, levels: usize) -> (String, String) {
    let (mut head, mut tail) = (String::new(), String::new());
    let mut wrap = |open: &str, close: &str| {
        head.push_str(open);
        tail.insert_str(0, close);
    };
    match shape {
        Shape::Objects => {
            wrap(&format!("r{index} {{ "), " }\n");
            (1..levels).for_each(|_| wrap("a { ", " }"));
        }
        Shape::Commented => {
            wrap(&format!("# c\nr{index} {{\n"), "\n}\n");
            (1..levels).for_each(|_| wrap("# c\n\"a\" {\n", "\n}"));
        }
        Shape::MultiValues => {
            wrap(&format!("r{index} {{ "), " }\n");
            (1..levels).for_each(|_| wrap("k = 1; k { ", " }"));
        }
        Shape::Arrays => {
            wrap(&format!("r{index} {{ "), " }\n");
            if levels > 1 {
                wrap("x = ", "");
                (2..levels).for_each(|_| wrap("[", "]"));
                wrap("{ ", " }");
            }
        }
    }
    (head, tail)
}

/// A document whose value is nested `depth` containers deep, the root included, made by copies
/// of copies: `r0` holds up to 1000 containers, and each `r{i}` up to 1000 more, with
/// `.inherit "r{i - 1}"` in its innermost object, until the copy in the last one reaches
/// `depth`.
fn inherited_chain(depth: usize, shape: Shape) -> String {
    const PART: usize = 1000;
    let leaf = match shape {
        Shape::Commented => "v = 'x'",
        Shape::MultiValues => "k = 2",
        Shape::Objects | Shape::Arrays => "b = 1",
    };
    let mut nested = (depth - 1).min(PART);
    let (head, tail) = part(shape, 0, nested);
    let mut document = format!("{head}{leaf}{tail}");
    let mut index = 1;
    while nested < depth - 1 {
        let levels = (depth - nested).min(PART + 1);
        let (head, tail) = part(shape, index, levels);
        document.push_str(&format!("{head}.inherit \"r{}\";{tail}", index - 1));
        nested += levels - 1;
        index += 1;
    }
    document
}

/// A parser with the largest `.inherit` depth limit.
fn deepest_parser(flags: ParserFlags) -> Parser {
    ParserBuilder::new()
        .with_flags(flags)
        .with_inherit_depth_limit(MAX_INHERIT_DEPTH_LIMIT)
        .build()
}

#[test]
fn copies_nest_no_deeper_than_the_largest_inherit_limit() {
    on_small_stack(|| {
        for shape in SHAPES {
            let document = inherited_chain(MAX_INHERIT_DEPTH_LIMIT + 1, shape);
            let error = deepest_parser(ParserFlags::DEFAULT)
                .parse(document.as_bytes())
                .unwrap_err();
            assert_eq!(
                error.kind(),
                &ErrorKind::NestingTooDeep {
                    limit: MAX_INHERIT_DEPTH_LIMIT
                },
                "{shape:?}"
            );
        }
    });
}

#[test]
fn parse_clone_compare_emit_and_drop_at_the_inherit_limit() {
    on_small_stack(|| {
        for shape in SHAPES {
            let document = inherited_chain(MAX_INHERIT_DEPTH_LIMIT, shape);
            let mut parser = deepest_parser(ParserFlags::SAVE_COMMENTS);
            let value = parser.parse(document.as_bytes()).expect("parse");
            assert_eq!(nesting(&value), MAX_INHERIT_DEPTH_LIMIT, "{shape:?}");
            let copy = value.clone();
            assert!(copy == value, "{shape:?}");
            drop(copy);
            // libucl's formats with the output facts of the parse, and without.
            for format in [
                Format::Json,
                Format::JsonCompact,
                Format::Config,
                Format::Yaml,
            ] {
                assert!(!parser.emitter(format).emit(&value).is_empty(), "{shape:?}");
            }
            let with_comments = parser
                .emitter(Format::Config)
                .with_comments(parser.comments(), parser.attached_comments())
                .emit(&value);
            let commented = matches!(shape, Shape::Commented);
            assert_eq!(with_comments.contains("# c"), commented, "{shape:?}");
            for text in [
                ucl_lexer::emit::to_json(&value),
                ucl_lexer::emit::to_json_compact(&value),
                ucl_lexer::emit::to_config(&value),
                ucl_lexer::emit::to_yaml(&value),
            ] {
                assert!(!text.is_empty(), "{shape:?}");
            }
        }
    });
}

#[test]
fn serde_at_the_inherit_limit() {
    #[derive(Deserialize, Serialize)]
    struct Wrapper {
        inner: UclValue,
    }
    on_small_stack(|| {
        for shape in SHAPES {
            let document = inherited_chain(MAX_INHERIT_DEPTH_LIMIT, shape);
            let value = deepest_parser(ParserFlags::DEFAULT)
                .parse(document.as_bytes())
                .expect("parse");
            // To and from `UclValue` and `UclObject`.
            let copy = ucl_lexer::to_value(&value).unwrap();
            assert!(same(&copy, &value), "{shape:?}");
            let back: UclValue = ucl_lexer::from_value(copy).unwrap();
            assert!(same(&back, &value), "{shape:?}");
            let object: UclObject = ucl_lexer::from_value(back).unwrap();
            assert_eq!(object.len(), value.as_object().unwrap().len());
            drop(object);
            let parser = deepest_parser(ParserFlags::DEFAULT);
            let read =
                UclValue::deserialize(UclDeserializer::from_parser(parser, document.as_bytes()))
                    .unwrap();
            assert!(same(&read, &value), "{shape:?}");
            drop(read);
            // Inside a typed target.
            let wrapper = Wrapper {
                inner: value.clone(),
            };
            let wrapped: Wrapper =
                ucl_lexer::from_value(ucl_lexer::to_value(&wrapper).unwrap()).unwrap();
            assert!(same(&wrapped.inner, &value), "{shape:?}");
            drop(wrapped);
            // Typed targets stop at the serde limit; the error's position comes from parsing
            // the document again, with the same parser.
            let parser = deepest_parser(ParserFlags::DEFAULT);
            assert_too_deep(
                serde_json::Value::deserialize(UclDeserializer::from_parser(
                    parser,
                    document.as_bytes(),
                )),
                "from_parser",
            );
            assert_too_deep(
                ucl_lexer::from_value::<serde_json::Value>(value.clone()),
                "from_value",
            );
            // Text that could not be parsed again, deeper than containers can be open
            // (spec §11.2), is an error.
            let mut written = Vec::new();
            for result in [
                ucl_lexer::to_string(&value),
                ucl_lexer::to_json_string(&value),
                ucl_lexer::to_json_string_compact(&value),
                ucl_lexer::to_yaml_string(&value),
                ucl_lexer::to_writer(&mut written, &value).map(|()| String::new()),
                ucl_lexer::to_string(&wrapper),
            ] {
                let error = result.unwrap_err().to_string();
                assert!(
                    error.contains("nested more than 1024"),
                    "{shape:?}: {error}"
                );
            }
        }
    });
}
