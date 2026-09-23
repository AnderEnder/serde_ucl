//! Emitter tests on parsed documents. Expected texts come from oracle runs; the conformance runner
//! (`tests/conformance.rs`) compares every case with libucl's output.

use super::*;
use crate::parse::{MemoryLoader, Parser};
use crate::value::{DuplicateStrategy, ParserFlags, UclObject};

fn parse(input: &str, flags: ParserFlags) -> (Parser, UclValue) {
    let mut parser = Parser::with_flags(flags);
    let value = parser.parse(input.as_bytes()).unwrap();
    (parser, value)
}

fn emit(input: &str, format: Format) -> String {
    let (parser, value) = parse(input, ParserFlags::DEFAULT);
    parser.emitter(format).emit(&value)
}

#[test]
fn empty_documents() {
    // spec §10.4-§10.6; an empty root array is `[]` in every format (oracle runs)
    assert_eq!(emit("", Format::Config), "");
    assert_eq!(emit("", Format::Yaml), "");
    assert_eq!(emit("", Format::Json), "{}");
    assert_eq!(emit("", Format::JsonCompact), "{}");
    for format in [
        Format::Config,
        Format::Json,
        Format::JsonCompact,
        Format::Yaml,
    ] {
        assert_eq!(emit("[]", format), "[]");
    }
}

#[test]
fn multi_value_layouts() {
    // spec §10.7: inline layout after a number, normal after a non-empty string, only the first
    // value when it is an explicit array, and the stray comma at the root of YAML.
    let input = "a = 1\na = 2\nb = \"x\"\nb = 1\nc = [1]\nc = 2\nd = 3\n";
    assert_eq!(
        emit(input, Format::Json),
        "{\n    \"a\": [        1,\n        2],\n    \"b\": [\n        \"x\",\n        1\n    ],\n    \
         \"c\": [\n        1\n    ],\n    \"d\": 3\n}"
    );
    assert_eq!(
        emit(input, Format::Yaml),
        "a: [    1,\n    2],\nb: [\n    \"x\",\n    1\n],\nc: [\n    1\n]\nd: 3"
    );
    assert_eq!(
        emit(input, Format::JsonCompact),
        r#"{"a":[1,2],"b":["x",1],"c":[1],"d":3}"#
    );
    assert_eq!(
        emit(input, Format::Config),
        "a = 1;\na = 2;\nb = \"x\";\nb = 1;\nc [\n    1,\n]\nc = 2;\nd = 3;\n"
    );
}

#[test]
fn string_forms_follow_the_parse() {
    // spec §10.5: single-quoted and heredoc origins, the JSON form otherwise.
    let input = "a = 'it\\'s'\nb = <<EOD\nl1\nl2\nEOD\nc = \"l1\\nl2\"\nd = [ 'x' ]\n";
    assert_eq!(
        emit(input, Format::Config),
        "a = 'it\\'s';\nb = <<EOD\nl1\nl2\nEOD;\nc = \"l1\\nl2\";\nd [\n    'x',\n]\n"
    );
    assert_eq!(
        emit(input, Format::Yaml),
        "a: \"it's\"\nb: \"l1\\nl2\"\nc: \"l1\\nl2\"\nd: [\n    \"x\"\n]"
    );
    // Without the parser's facts every string is in the JSON form.
    let (_, value) = parse(input, ParserFlags::DEFAULT);
    assert!(to_config(&value).starts_with("a = \"it's\";\nb = \"l1\\nl2\";"));
}

#[test]
fn keys_as_written() {
    // spec §10.1, fact 3, and the key of each value of a multi-value entry.
    let input = "\"x\\u0041\" = 1\nxA = 2\n\"k y\" = 3\n\"kq\" = 4\n";
    assert_eq!(
        emit(input, Format::Config),
        "\"xA\" = 1;\nxA = 2;\n\"k y\" = 3;\nkq = 4;\n"
    );
    assert_eq!(
        emit(input, Format::Yaml),
        "\"xA\": [    1,\n    2]\n\"k y\": 3\nkq: 4"
    );
    let (parser, value) = parse("\"\\u0041\" = 1\na = 2\n", ParserFlags::KEY_LOWERCASE);
    assert_eq!(
        parser.emitter(Format::Config).emit(&value),
        "\"A\" = 1;\na = 2;\n"
    );
    assert_eq!(
        parser.emitter(Format::JsonCompact).emit(&value),
        r#"{"A":[1,2]}"#
    );
}

#[test]
fn hand_built_values() {
    let mut object = UclObject::new();
    object.insert("a b", UclValue::Integer(1));
    object.insert("", UclValue::Float(0.5));
    object.append("m", UclValue::Boolean(true));
    object.append("m", UclValue::Null);
    let value = UclValue::Object(object);
    assert_eq!(
        to_config(&value),
        "\"a b\" = 1;\n = 0.500000;\nm = true;\nm = null;\n"
    );
    assert_eq!(
        to_json(&value),
        "{\n    \"a b\": 1,\n    null: 0.500000,\n    \"m\": [        true,\n        null]\n}"
    );
    assert_eq!(
        to_json_compact(&value),
        r#"{"a b":1,null:0.500000,"m":[true,null]}"#
    );
    assert_eq!(
        to_yaml(&value),
        "\"a b\": 1\nnull: 0.500000,\nm: [    true,\n    null]"
    );
}

#[test]
fn comments_only_when_asked() {
    // spec §10.10; project decision C4.1: off by default.
    let input = "# c\na = 1 # d\nb {\n  c = [1, # e\n  2]\n}\n";
    let (parser, value) = parse(input, ParserFlags::SAVE_COMMENTS);
    let plain = "a = 1;\nb {\n    c [\n        1,\n        2,\n    ]\n}\n";
    assert_eq!(parser.emitter(Format::Config).emit(&value), plain);
    let with = parser
        .emitter(Format::Config)
        .with_comments(parser.comments(), parser.attached_comments());
    assert_eq!(
        with.emit(&value),
        "# c\na = 1;\n# d\nb {\n    c [\n        1,\n        # e\n        2,\n    ]\n}\n"
    );
    // JSON never writes comments.
    let json = parser
        .emitter(Format::JsonCompact)
        .with_comments(parser.comments(), parser.attached_comments());
    assert_eq!(json.emit(&value), r#"{"a":1,"b":{"c":[1,2]}}"#);
}

#[test]
fn facts_follow_values_the_parser_moves() {
    // Oracle runs: copies made by `.inherit`, also from an enclosing object; a
    // `no-implicit-arrays` collection; a value replaced by a higher priority.
    assert_eq!(
        emit("o { x = 'q'; e { .inherit \"o\" } }\n", Format::Config),
        "o {\n    x = 'q';\n    e {\n        x = 'q';\n        e {\n            x = 'q';\n        }\n    }\n}\n"
    );
    let (parser, value) = parse(
        "a = 'x'\na = <<EOD\nl1\nl2\nEOD\n",
        ParserFlags::NO_IMPLICIT_ARRAYS,
    );
    assert_eq!(
        parser.emitter(Format::Config).emit(&value),
        "a [\n    'x',\n    <<EOD\nl1\nl2\nEOD,\n]\n"
    );
    assert_eq!(
        emit("a = 'x'\n.priority 2\na = \"y\"\n", Format::Config),
        "a = \"y\";\n"
    );
    // The first value of a collection is itself an array whose elements have facts.
    let (parser, value) = parse("a = ['x']\na = 1\n", ParserFlags::NO_IMPLICIT_ARRAYS);
    assert_eq!(
        parser.emitter(Format::Config).emit(&value),
        "a [\n    [\n        'x',\n    ]\n    1,\n]\n"
    );
    // A scalar that takes an object's place under `merge` keeps the object's key, and a
    // number there keeps the normal layout (QUESTIONS.md #50).
    let mut parser = Parser::new();
    parser.set_strategy(DuplicateStrategy::Merge);
    let value = parser
        .parse(b"\"x\\u0041\" { a = 1 }\nxA = 2\nxA = 3\n")
        .unwrap();
    assert_eq!(
        parser.emitter(Format::Config).emit(&value),
        "\"xA\" = 2;\nxA = 3;\n"
    );
    assert_eq!(
        parser.emitter(Format::Json).emit(&value),
        "{\n    \"xA\": [\n        2,\n        3\n    ]\n}"
    );
}

#[test]
fn collection_keys_and_merge_quirk_layouts() {
    // spec-v7 §10.1: the key of a `no-implicit-arrays` collection never needs quoting, whatever
    // its values' keys were; a scalar that replaces the collection under `merge` keeps that key,
    // and one that replaces an explicit array keeps the array's (oracle runs).
    let input = "\"x y\" = 1\n\"x y\" = 2\n\"x y\" = 3\n\"a=b\" = [1]\n\"a=b\" = 2\n";
    let (parser, value) = parse(input, ParserFlags::NO_IMPLICIT_ARRAYS);
    assert_eq!(
        parser.emitter(Format::Config).emit(&value),
        "x y [\n    1,\n    2,\n    3,\n]\na=b [\n    [\n        1,\n    ]\n    2,\n]\n"
    );
    assert_eq!(
        parser.emitter(Format::Yaml).emit(&value),
        "x y: [\n    1,\n    2,\n    3\n]\na=b: [\n    [\n        1\n    ],\n    2\n]"
    );
    let mut parser = Parser::with_flags(ParserFlags::NO_IMPLICIT_ARRAYS);
    parser.set_strategy(DuplicateStrategy::Merge);
    let value = parser.parse(input.as_bytes()).unwrap();
    assert_eq!(
        parser.emitter(Format::Config).emit(&value),
        "x y = 3;\n\"a=b\" = 2;\n"
    );
    assert_eq!(
        parser.emitter(Format::Yaml).emit(&value),
        "x y: 3\n\"a=b\": 2"
    );
    // spec-v7 §10.7: under the merge quirk, `nan` and `inf` follow their own kind (inline),
    // while a number written with digits, also one that overflows to +∞, and a boolean keep
    // the layout of the non-empty container they replaced.
    let mut parser = Parser::new();
    parser.set_strategy(DuplicateStrategy::Merge);
    let value = parser
        .parse(
            b"b = [1]\nb = nan\nb = 6\nc = [1]\nc = 1e308k\nc = 6\nd { x = 1 }\nd = yes\nd = 6\n",
        )
        .unwrap();
    assert_eq!(
        parser.emitter(Format::Json).emit(&value),
        "{\n    \"b\": [        nan,\n        6],\n    \"c\": [\n        inf,\n        6\n    ],\n    \
         \"d\": [\n        true,\n        6\n    ]\n}"
    );
    assert_eq!(
        parser.emitter(Format::Yaml).emit(&value),
        "b: [    nan,\n    6],\nc: [\n    inf,\n    6\n],\nd: [\n    true,\n    6\n]"
    );
}

#[test]
fn keys_that_include_creates() {
    // spec §10.1, *Quirk*; the array made in place of existing values too (oracle runs).
    let mut loader = MemoryLoader::new();
    loader.add_file("/c/f.inc", "a = 1\n");
    let mut parser = Parser::new();
    parser.set_loader(loader).set_base_dir("/c");
    let value = parser
        .parse(b"\"p q\" = 1\n.include(key=\"p q\", target=\"array\") \"f.inc\"\n.include(key=\"r s\") \"f.inc\"\n")
        .unwrap();
    assert_eq!(
        parser.emitter(Format::Config).emit(&value),
        "p q [\n    1,\n    {\n        a = 1;\n    }\n]\nr s {\n    a = 1;\n}\n"
    );
}
