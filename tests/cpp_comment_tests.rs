use serde_json::{Value, json};
use ucl_lexer::lexer::{CommentType, LexerConfig, Token, UclLexer};
use ucl_lexer::{UclError, from_str};

#[cfg(test)]
mod cpp_comment_tests {
    use super::*;

    #[test]
    fn test_single_line_cpp_comments() {
        // Test // comment syntax treats rest of line as comment
        let config = r#"
            // This is a C++ style comment
            key1 = "value1"
            // Another comment
            key2 = "value2"
            
            // Comments can have various content: symbols !@#$%^&*()
            key3 = "value3"
        "#;

        let result: Value = from_str(config).expect("Should parse C++ style comments");
        assert_eq!(result["key1"], "value1");
        assert_eq!(result["key2"], "value2");
        assert_eq!(result["key3"], "value3");
    }

    #[test]
    fn test_inline_cpp_comments() {
        // Test // appears inline with other content
        let config = r#"
            key1 = "value1"  // This is an inline comment
            key2 = 42        // Number with comment
            key3 = true      // Boolean with comment
            array = [1, 2, 3] // Array with comment
        "#;

        let result: Value = from_str(config).expect("Should parse inline C++ comments");
        assert_eq!(result["key1"], "value1");
        assert_eq!(result["key2"], 42);
        assert_eq!(result["key3"], true);
        assert_eq!(result["array"], json!([1, 2, 3]));
    }

    #[test]
    fn test_mixed_comment_styles() {
        // Test hash, C++, and multi-line comments together
        let config = r#"
            # Hash comment
            key1 = "value1"
            
            // C++ style comment
            key2 = "value2"  # Inline hash comment
            
            /*
             * Multi-line comment
             * with multiple lines
             */
            key3 = "value3"  // Inline C++ comment
            
            key4 = "value4"  /* Inline multi-line */ // And C++ comment
        "#;

        let result: Value = from_str(config).expect("Should parse mixed comment styles");
        assert_eq!(result["key1"], "value1");
        assert_eq!(result["key2"], "value2");
        assert_eq!(result["key3"], "value3");
        assert_eq!(result["key4"], "value4");
    }

    #[test]
    fn test_cpp_comments_in_strings_ignored() {
        // Test // inside quoted strings is treated as literal text
        let config = r#"
            url = "http://example.com/path"
            comment_text = "This // is not a comment"
            path = "/path/to//file"
            regex = "pattern//with//slashes"
        "#;

        let result: Value = from_str(config).expect("Should treat // in strings as literal");
        assert_eq!(result["url"], "http://example.com/path");
        assert_eq!(result["comment_text"], "This // is not a comment");
        assert_eq!(result["path"], "/path/to//file");
        assert_eq!(result["regex"], "pattern//with//slashes");
    }

    #[test]
    fn test_multiple_cpp_comments_same_line() {
        // Test multiple // on same line - everything after first // is comment
        let config = r#"
            key1 = "value1"  // First comment // Second comment // Third comment
            key2 = "value2"  // Comment with // double slashes // in it
        "#;

        let result: Value = from_str(config).expect("Should handle multiple // on same line");
        assert_eq!(result["key1"], "value1");
        assert_eq!(result["key2"], "value2");
    }

    #[test]
    fn test_cpp_comments_with_different_line_endings() {
        // Test C++ comments work with both CRLF and LF line endings
        let config_lf = "key1 = \"value1\"  // Comment\nkey2 = \"value2\"";
        let config_crlf = "key1 = \"value1\"  // Comment\r\nkey2 = \"value2\"";

        let result_lf: Value = from_str(config_lf).expect("Should parse C++ comments with LF");
        let result_crlf: Value =
            from_str(config_crlf).expect("Should parse C++ comments with CRLF");

        assert_eq!(result_lf["key1"], "value1");
        assert_eq!(result_lf["key2"], "value2");
        assert_eq!(result_crlf["key1"], "value1");
        assert_eq!(result_crlf["key2"], "value2");
    }

    #[test]
    fn test_cpp_comments_in_objects_and_arrays() {
        // Test C++ comments within object and array structures
        let config = r#"
            object = {
                // Comment inside object
                key1 = "value1"  // Inline comment
                key2 = "value2"
                // Another comment
            }
            
            array = [
                // Comment in array
                "item1",  // Comment after item
                "item2",
                // Final comment
            ]
        "#;

        let result: Value = from_str(config).expect("Should parse C++ comments in structures");
        assert_eq!(result["object"]["key1"], "value1");
        assert_eq!(result["object"]["key2"], "value2");
        assert_eq!(result["array"][0], "item1");
        assert_eq!(result["array"][1], "item2");
    }

    #[test]
    fn test_comment_preservation_functionality() {
        // Test comment preservation when enabled
        let config = r#"
            // Header comment
            key1 = "value1"  // Inline comment
            # Hash comment
            key2 = "value2"
        "#;

        // Test that parsing succeeds regardless of comment preservation setting
        let result: Value = from_str(config).expect("Should parse with comment preservation");
        assert!(result.is_object(), "Top-level value should be an object");

        // Verify that comments are collected when preservation is enabled
        let mut lexer_config = LexerConfig::default();
        lexer_config.save_comments = true;
        let mut lexer = UclLexer::with_config(config, lexer_config);

        loop {
            match lexer.next_token().expect("Lexer should produce tokens") {
                Token::Eof => break,
                _ => {}
            }
        }

        let comments = lexer.comments();
        assert_eq!(comments.len(), 3, "Should collect all comment styles");

        assert_eq!(comments[0].comment_type, CommentType::CppStyle);
        assert!(comments[0].text.contains("Header comment"));

        assert_eq!(comments[1].comment_type, CommentType::CppStyle);
        assert!(comments[1].text.contains("Inline comment"));

        assert_eq!(comments[2].comment_type, CommentType::SingleLine);
        assert!(comments[2].text.contains("Hash comment"));
    }

    #[test]
    fn test_cpp_comments_edge_cases() {
        // Test edge cases for C++ comment parsing
        let config = r#"
            // Comment at start of file
            key1 = "value1"
            
            key2 = "value2" //
            key3 = "value3" // 
            
            // Comment with only slashes: ////
            key4 = "value4"
            
            // Comment at end of file
        "#;

        let result: Value = from_str(config).expect("Should handle C++ comment edge cases");
        assert_eq!(result["key1"], "value1");
        assert_eq!(result["key2"], "value2");
        assert_eq!(result["key3"], "value3");
        assert_eq!(result["key4"], "value4");
    }

    #[test]
    fn test_cpp_comments_with_unicode() {
        // Test C++ comments containing Unicode characters
        let config = r#"
            // Comment with emoji: 🚀 and Unicode: αβγ
            key1 = "value1"
            
            key2 = "value2"  // Inline with Unicode: 中文 русский العربية
        "#;

        let result: Value = from_str(config).expect("Should parse C++ comments with Unicode");
        assert_eq!(result["key1"], "value1");
        assert_eq!(result["key2"], "value2");
    }

    #[test]
    fn test_cpp_comment_error_handling() {
        // Test that malformed comment-like syntax is handled gracefully
        let configs = vec![
            r#"key = "value" / // Not a comment"#, // Single slash before //
            r#"key = "value" /// Triple slash"#,   // Triple slash
        ];

        for config in configs {
            // These should either parse successfully or fail gracefully
            let result: Result<Value, UclError> = from_str(config);
            // The exact behavior depends on implementation, but should not panic
            match result {
                Ok(val) => {
                    // If it parses, verify basic structure
                    assert!(val.is_object());
                }
                Err(_) => {
                    // If it fails, that's also acceptable for malformed input
                }
            }
        }
    }
}
