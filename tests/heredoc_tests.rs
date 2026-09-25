//! Heredoc strings (spec §6.3). Expected results are libucl's (spec §6.3, checked with the
//! oracle).

use serde_json::Value;
use ucl_lexer::parse::ErrorKind;
use ucl_lexer::{UclError, from_str};

#[cfg(test)]
mod heredoc_tests {
    use super::*;

    fn error_kind(config: &str) -> ErrorKind {
        match from_str::<Value>(config) {
            Err(UclError::Syntax(e)) => e.kind().clone(),
            other => panic!("{config:?}: expected a parse error, got {other:?}"),
        }
    }

    #[test]
    fn test_heredoc_with_leading_whitespace_terminator() {
        // An indented terminator is content (spec §6.3, `heredoc_indented_terminator_is_content`).
        let config = r#"
            content = <<EOF
            This is line 1
            This is line 2
                EOF
EOF
        "#;

        let result: Value =
            from_str(config).expect("Should parse heredoc with SPEC-compliant terminator");
        let content = result["content"].as_str().unwrap();
        assert!(content.contains("This is line 1"));
        assert!(content.contains("This is line 2"));
        // "    EOF" (with leading spaces) should be in the content
        assert!(content.contains("    EOF"));
    }

    #[test]
    fn test_heredoc_with_trailing_whitespace_terminator() {
        // NAME followed by spaces is not a terminator, so it is content (spec §6.3); a proper
        // terminator follows.
        let config = "content = <<EOF\nThis is line 1\nThis is line 2\nEOF   \nEOF\n";

        let result: Value =
            from_str(config).expect("Should parse heredoc with SPEC-compliant terminator");
        let content = result["content"].as_str().unwrap();
        assert!(content.contains("This is line 1"));
        assert!(content.contains("This is line 2"));
        // "EOF   " (with trailing spaces) should be in the content
        assert!(content.contains("EOF   "));
    }

    #[test]
    fn test_heredoc_with_both_leading_and_trailing_whitespace() {
        // Leading and trailing whitespace: content too (spec §6.3); a proper terminator follows.
        let config =
            "content = <<DELIMITER\nLine 1 content\nLine 2 content\n    DELIMITER   \nDELIMITER\n";

        let result: Value =
            from_str(config).expect("Should parse heredoc with SPEC-compliant terminator");
        let content = result["content"].as_str().unwrap();
        assert!(content.contains("Line 1 content"));
        assert!(content.contains("Line 2 content"));
        // "    DELIMITER   " (with leading and trailing spaces) should be in the content
        assert!(content.contains("    DELIMITER   "));
    }

    #[test]
    fn test_heredoc_partial_terminator_matches() {
        // Test heredoc content with lines that partially match terminator. The terminator line is NAME alone (spec §6.3).
        let config = "content = <<END\nThis line contains END but not as terminator\nAnother line with END in the middle\nENDING is not the terminator\nEND_SUFFIX is not the terminator\nPREFIX_END is not the terminator\nEND\n";

        let result: Value = from_str(config).expect("Should handle partial terminator matches");
        let content = result["content"].as_str().unwrap();
        assert!(content.contains("This line contains END but not as terminator"));
        assert!(content.contains("Another line with END in the middle"));
        assert!(content.contains("ENDING is not the terminator"));
        assert!(content.contains("END_SUFFIX is not the terminator"));
        assert!(content.contains("PREFIX_END is not the terminator"));
        // The final "END" should not be in content as it's the terminator
        let lines: Vec<&str> = content.lines().collect();
        assert!(!lines.iter().any(|line| line.trim() == "END"));
    }

    #[test]
    fn test_heredoc_with_crlf_line_endings() {
        // CR LF directly after the opener means it is not a heredoc: `<<EOF` is an unquoted
        // value, and the lines after it do not parse (spec §6.3, `heredoc_crlf_opener_error`).
        for config in [
            "content = <<EOF\r\nLine 1\r\nLine 2\r\nEOF\r\n",
            "content = <<EOF\r\nLine 1 with CRLF\nLine 2 with LF\r\nLine 3 with CRLF\nEOF\r\n",
        ] {
            assert!(
                matches!(from_str::<Value>(config), Err(UclError::Syntax(_))),
                "{config:?}"
            );
        }
        // After an LF opener, CR has no special meaning in the content (spec §6.3,
        // `heredoc_crlf_content`).
        let result: Value = from_str("content = <<EOF\nLine 1\r\nLine 2\r\nEOF\n").unwrap();
        assert_eq!(result["content"], "Line 1\r\nLine 2\r");
    }

    #[test]
    fn test_heredoc_with_lf_line_endings() {
        // Test heredoc with LF line endings
        let config = "content = <<EOF\nLine 1\nLine 2\nEOF\n";

        let result: Value = from_str(config).expect("Should parse heredoc with LF line endings");
        let content = result["content"].as_str().unwrap();
        assert!(content.contains("Line 1"));
        assert!(content.contains("Line 2"));
    }

    #[test]
    fn test_heredoc_unterminated_error() {
        // Test heredoc error when terminator is not found
        let config = r#"
            content = <<EOF
            This heredoc is never terminated
            It should cause an error
        "#;

        assert_eq!(error_kind(config), ErrorKind::UnterminatedHeredoc);
    }

    #[test]
    fn test_heredoc_with_custom_terminators() {
        // Test heredoc with various custom terminators. The terminator line is NAME alone (spec §6.3).
        let config = "sql_query = <<SQL\nSELECT * FROM users\nWHERE active = true\nORDER BY created_at DESC\nSQL\n\nhtml_content = <<HTML\n<div class=\"container\">\n    <h1>Hello World</h1>\n    <p>This is a paragraph.</p>\n</div>\nHTML\n\nscript_content = <<SCRIPT\n#!/bin/bash\necho \"Hello World\"\nexit 0\nSCRIPT\n";

        let result: Value =
            from_str(config).expect("Should parse heredocs with custom terminators");

        let sql = result["sql_query"].as_str().unwrap();
        assert!(sql.contains("SELECT * FROM users"));
        assert!(sql.contains("WHERE active = true"));

        let html = result["html_content"].as_str().unwrap();
        assert!(html.contains("<div class=\"container\">"));
        assert!(html.contains("<h1>Hello World</h1>"));

        let script = result["script_content"].as_str().unwrap();
        assert!(script.contains("#!/bin/bash"));
        assert!(script.contains("echo \"Hello World\""));
    }

    #[test]
    fn test_heredoc_preserves_internal_whitespace() {
        // Test that heredoc preserves internal whitespace and indentation. The terminator line is NAME alone (spec §6.3).
        let config = "formatted_text = <<TEXT\n    This line has leading spaces\nThis line has no leading spaces\n        This line has many leading spaces\n\tThis line has a tab\nTEXT\n";

        let result: Value =
            from_str(config).expect("Should preserve internal whitespace in heredoc");
        let content = result["formatted_text"].as_str().unwrap();

        // Heredoc preserves ALL whitespace
        let lines: Vec<&str> = content.lines().collect();

        // Check that lines have different amounts of whitespace preserved
        assert!(lines[0].starts_with("    This line has leading spaces"));
        assert_eq!(lines[1], "This line has no leading spaces");
        assert!(lines[2].starts_with("        This line has many leading spaces"));
        assert!(lines[3].contains("\tThis line has a tab"));

        // Verify whitespace amounts are different
        let ws0 = lines[0].len() - lines[0].trim_start().len();
        let ws1 = lines[1].len() - lines[1].trim_start().len();
        let ws2 = lines[2].len() - lines[2].trim_start().len();
        assert!(
            ws0 > ws1,
            "First line should have more leading whitespace than second"
        );
        assert!(
            ws2 > ws0,
            "Third line should have more leading whitespace than first"
        );
    }

    #[test]
    fn test_heredoc_empty_content() {
        // An empty heredoc cannot be written: the first line after the opener is always
        // content, so `EOF` there is content and no terminator follows (spec §6.3, *Quirk*,
        // `heredoc_empty_error`). One blank line gives the empty string
        // (`heredoc_blank_line_content`).
        assert_eq!(
            error_kind("empty_content = <<EOF\nEOF\n"),
            ErrorKind::UnterminatedHeredoc
        );
        let result: Value = from_str("empty_content = <<EOF\n\nEOF\n").unwrap();
        assert_eq!(result["empty_content"], "");
    }

    #[test]
    fn test_heredoc_with_only_whitespace_lines() {
        // Test heredoc containing only whitespace lines. The terminator line is NAME alone (spec §6.3).
        let config = "whitespace_content = <<EOF\n\n    \n\t\nEOF\n";

        let result: Value = from_str(config).expect("Should parse heredoc with whitespace lines");
        // Whitespace and line breaks are kept; the line break before the terminator is not
        // content (spec §6.3).
        assert_eq!(result["whitespace_content"], "\n    \n\t");
    }

    #[test]
    fn test_heredoc_terminator_case_sensitivity() {
        // Test that heredoc terminators are case-sensitive. The terminator line is NAME alone (spec §6.3).
        let config = "content = <<EOF\nThis is the content\neof should not terminate\nEof should not terminate\neOf should not terminate\nEOF\n";

        let result: Value = from_str(config).expect("Should handle case-sensitive terminators");
        let content = result["content"].as_str().unwrap();
        assert!(content.contains("eof should not terminate"));
        assert!(content.contains("Eof should not terminate"));
        assert!(content.contains("eOf should not terminate"));
    }

    #[test]
    fn test_heredoc_names_are_uppercase_letters_only() {
        // NAME is uppercase ASCII letters: with `_` or digits after `<<`, the text is an
        // ordinary unquoted value (spec §6.3).
        for (config, expected) in [
            ("k = <<END_OF_DATA\n", "<<END_OF_DATA"),
            ("k = <<MARKER123\n", "<<MARKER123"),
        ] {
            let result: Value = from_str(config).unwrap();
            assert_eq!(result["k"], expected);
        }
        // So the lines after such an opener are read as entries, and a line holding only the
        // would-be terminator is a key without a value.
        let config = "content1 = <<END_OF_DATA\nSome content here\nEND_OF_DATA\n\ncontent2 = <<MARKER123\nMore content here\nMARKER123\n";
        assert!(matches!(
            from_str::<Value>(config),
            Err(UclError::Syntax(_))
        ));
    }

    #[test]
    fn test_multiple_heredocs_in_same_config() {
        // Test multiple heredocs in the same configuration. The terminator line is NAME alone (spec §6.3).
        let config = "first = <<FIRST\nContent of first heredoc\nFIRST\n\nsecond = <<SECOND\nContent of second heredoc\nSECOND\n\nthird = <<THIRD\nContent of third heredoc\nTHIRD\n";

        let result: Value = from_str(config).expect("Should parse multiple heredocs");
        assert!(
            result["first"]
                .as_str()
                .unwrap()
                .contains("Content of first heredoc")
        );
        assert!(
            result["second"]
                .as_str()
                .unwrap()
                .contains("Content of second heredoc")
        );
        assert!(
            result["third"]
                .as_str()
                .unwrap()
                .contains("Content of third heredoc")
        );
    }

    #[test]
    fn test_heredoc_unterminated_names() {
        // A heredoc without its terminator line is an error (spec §6.3).
        let config = r#"content = <<MISSING
            This heredoc is missing its terminator"#;
        assert_eq!(error_kind(config), ErrorKind::UnterminatedHeredoc);
        // `<<CUSTOM_TERM` is not a heredoc opener (`_` is not allowed in NAME), so the document
        // parses: an unquoted value, then an entry on the next line (spec §6.3, oracle).
        let config = r#"content = <<CUSTOM_TERM
            Another unterminated heredoc"#;
        let result: Value = from_str(config).unwrap();
        assert_eq!(result["content"], "<<CUSTOM_TERM");
        assert_eq!(result["Another"], "unterminated heredoc");
    }

    #[test]
    fn test_heredoc_in_nested_structures() {
        // Test heredoc within nested objects and arrays. The terminator line is NAME alone (spec §6.3).
        let config = "database = {\n    migrations = [\n        {\n            name = \"create_users\"\n            sql = <<SQL\nCREATE TABLE users (\n    id SERIAL PRIMARY KEY,\n    name VARCHAR(255) NOT NULL\n);\nSQL\n        }\n    ]\n}\n";

        let result: Value = from_str(config).expect("Should parse heredoc in nested structures");
        let sql = result["database"]["migrations"][0]["sql"].as_str().unwrap();
        assert!(sql.contains("CREATE TABLE users"));
        assert!(sql.contains("id SERIAL PRIMARY KEY"));
    }
}
