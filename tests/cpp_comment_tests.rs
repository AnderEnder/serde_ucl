//! `//` is not a comment in UCL (spec §2.5). `/` may start a key, so a line `// text` is the
//! entry `"//" = "text"`; after a quoted value a `/` is a missing delimiter, an error (§1.3);
//! after an unquoted value `// text` is part of the value (§4.1). Inside quoted strings `//` is
//! ordinary text. Expected results are libucl's (spec §2.5, checked with the oracle).

use serde_json::{Value, json};
use ucl_lexer::parse::{ErrorKind, Parser};
use ucl_lexer::{ParserFlags, UclError, UclValue, from_str};

fn error_kind(config: &str) -> ErrorKind {
    match from_str::<Value>(config) {
        Err(UclError::Syntax(e)) => e.kind().clone(),
        other => panic!("expected a parse error for {config:?}, got {other:?}"),
    }
}

#[test]
fn test_slash_slash_lines_are_entries() {
    let config = r#"
            // This is a C++ style comment
            key1 = "value1"
            // Another comment
            key2 = "value2"

            // Comments can have various content: symbols !@#$%^&*()
            key3 = "value3"
        "#;

    let result: Value = from_str(config).expect("Should parse // lines as entries");
    assert_eq!(result["key1"], "value1");
    assert_eq!(result["key2"], "value2");
    assert_eq!(result["key3"], "value3");
    // The three lines are values of the key `//`; `#` ends the third (spec §2.2).
    assert_eq!(
        result["//"],
        json!([
            "This is a C++ style comment",
            "Another comment",
            "Comments can have various content: symbols !@"
        ])
    );
}

#[test]
fn test_slash_slash_after_a_quoted_value_is_an_error() {
    // spec §1.3: a quoted value must be followed by whitespace, a comment, a terminator or a
    // closing bracket.
    for config in [
        "key1 = \"value1\"  // This is an inline comment",
        "key1 = \"value1\"  // First comment // Second comment // Third comment",
        "key1 = \"value1\"  // Comment\nkey2 = \"value2\"",
        "key1 = \"value1\"  // Comment\r\nkey2 = \"value2\"",
        "key2 = \"value2\" //",
        "key2 = \"value2\"  // Inline with Unicode: 中文 русский العربية",
        r#"key = "value" / // Not a comment"#,
        r#"key = "value" /// Triple slash"#,
    ] {
        assert_eq!(
            error_kind(config),
            ErrorKind::MissingDelimiter { found: '/' },
            "{config}"
        );
    }
}

#[test]
fn test_slash_slash_after_an_unquoted_value_is_part_of_it() {
    // spec §4.1: an unquoted value runs to the end of the line.
    let result: Value = from_str("key2 = 42        // Number with comment").unwrap();
    assert_eq!(result["key2"], "42        // Number with comment");
    let result: Value = from_str("key = value // trailing").unwrap();
    assert_eq!(result["key"], "value // trailing");
}

#[test]
fn test_mixed_comment_styles() {
    // `#` and `/* */` are comments; the `//` line is an entry.
    let config = r#"
            # Hash comment
            key1 = "value1"

            // C++ style comment
            key2 = "value2"  # Inline hash comment

            /*
             * Multi-line comment
             * with multiple lines
             */
            key3 = "value3"
        "#;

    let result: Value = from_str(config).expect("Should parse mixed comment styles");
    assert_eq!(result["key1"], "value1");
    assert_eq!(result["//"], "C++ style comment");
    assert_eq!(result["key2"], "value2");
    assert_eq!(result["key3"], "value3");

    // A block comment after a quoted value counts as a terminator (spec §2.4), so the `//` after
    // it starts the next entry.
    let result: Value =
        from_str("key4 = \"value4\"  /* Inline multi-line */ // And C++ comment").unwrap();
    assert_eq!(result["key4"], "value4");
    assert_eq!(result["//"], "And C++ comment");
}

#[test]
fn test_slash_slash_in_strings_is_text() {
    let config = r#"
            url = "http://example.com/path"
            comment_text = "This // is not a comment"
            path = "/path/to//file"
            regex = "pattern//with//slashes"
            bare = http://example.com/x//y
        "#;

    let result: Value = from_str(config).expect("Should treat // in strings as literal");
    assert_eq!(result["url"], "http://example.com/path");
    assert_eq!(result["comment_text"], "This // is not a comment");
    assert_eq!(result["path"], "/path/to//file");
    assert_eq!(result["regex"], "pattern//with//slashes");
    assert_eq!(result["bare"], "http://example.com/x//y");
}

#[test]
fn test_slash_slash_line_endings() {
    // CR LF ends the value of a `//` entry like LF does.
    let result: Value = from_str("key1 = \"value1\"\n// Comment\r\nkey2 = \"value2\"").unwrap();
    assert_eq!(result["key1"], "value1");
    assert_eq!(result["//"], "Comment");
    assert_eq!(result["key2"], "value2");
}

#[test]
fn test_slash_slash_in_objects_and_arrays() {
    let config = r#"
            object = {
                // Comment inside object
                key1 = "value1"
                key2 = "value2"
            }

            array = [
                // Comment in array
                "item1",  // Comment after item
                "item2",
                // Final comment
            ]
        "#;

    let result: Value = from_str(config).expect("Should parse // lines in structures");
    assert_eq!(result["object"]["//"], "Comment inside object");
    assert_eq!(result["object"]["key1"], "value1");
    assert_eq!(result["object"]["key2"], "value2");
    // In an array, each `//` text is an unquoted element (spec §4.1).
    assert_eq!(
        result["array"],
        json!([
            "// Comment in array",
            "item1",
            "// Comment after item",
            "item2",
            "// Final comment"
        ])
    );
}

#[test]
fn test_only_hash_and_block_comments_are_saved() {
    // spec §12.5: with `save-comments`, `#` comments are saved; a `//` line is an entry.
    let config = "// Header comment\nkey1 = \"value1\"\n# Hash comment\nkey2 = \"value2\"\n";
    let mut parser = Parser::with_flags(ParserFlags::SAVE_COMMENTS);
    let value = parser.parse(config.as_bytes()).expect("parses");
    let root = value.as_object().unwrap();
    assert_eq!(root["//"], UclValue::String("Header comment".into()));
    let texts: Vec<&str> = parser.comments().iter().map(|c| c.text.as_str()).collect();
    assert_eq!(texts, ["# Hash comment"]);
}

#[test]
fn test_slash_slash_edge_cases() {
    let config = r#"
            // Comment at start of file
            key1 = "value1"

            // Comment with only slashes: ////
            key4 = "value4"

            // Comment with emoji: 🚀 and Unicode: αβγ
            // Comment at end of file
        "#;

    let result: Value = from_str(config).expect("Should parse // edge cases");
    assert_eq!(result["key1"], "value1");
    assert_eq!(result["key4"], "value4");
    assert_eq!(
        result["//"],
        json!([
            "Comment at start of file",
            "Comment with only slashes: ////",
            "Comment with emoji: 🚀 and Unicode: αβγ",
            "Comment at end of file"
        ])
    );
    // `//` alone on a line is a key without a value.
    assert_eq!(
        error_kind("//\n"),
        ErrorKind::InvalidKey { found: Some('\n') }
    );
}
