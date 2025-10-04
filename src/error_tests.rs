//! Comprehensive tests for error handling and diagnostics
//!
//! This module tests all error conditions with proper position reporting,
//! error message quality and consistency, and error recovery.

#[cfg(test)]
mod tests {
    #![allow(dead_code, unused_imports, unused_variables)]
    use crate::error::{ErrorContext, LexError, ParseError, Position, Span, UclError};
    use crate::lexer::UclLexer;
    use crate::parser::UclParser;

    #[test]
    fn test_position_tracking_accuracy() {
        let mut pos = Position::new();

        // Test basic character advancement
        pos.advance('a');
        assert_eq!(pos.line, 1);
        assert_eq!(pos.column, 2);
        assert_eq!(pos.offset, 1);

        // Test newline handling
        pos.advance('\n');
        assert_eq!(pos.line, 2);
        assert_eq!(pos.column, 1);
        assert_eq!(pos.offset, 2);

        // Test carriage return handling
        pos.advance('\r');
        assert_eq!(pos.line, 2);
        assert_eq!(pos.column, 1);
        assert_eq!(pos.offset, 3);

        // Test multi-byte UTF-8 character
        pos.advance('ü');
        assert_eq!(pos.line, 2);
        assert_eq!(pos.column, 2);
        assert_eq!(pos.offset, 5); // 'ü' is 2 bytes in UTF-8
    }

    #[test]
    fn test_unicode_validation_strict_mode() {
        use crate::lexer::LexerConfig;

        // Test valid UTF-8 in strict mode
        let mut config = LexerConfig::default();
        config.strict_unicode = true;

        let input = "\"héllo wörld 🌍\"";
        let mut lexer = UclLexer::with_config(input, config.clone());
        let result = lexer.next_token();
        assert!(result.is_ok());

        // Test that validation functions work
        let input = "\"valid string\"";
        let lexer = UclLexer::with_config(input, config);
        assert!(lexer.validate_utf8_string("valid UTF-8 string").is_ok());
    }

    #[test]
    fn test_unicode_escape_validation() {
        // Test valid Unicode escapes
        let input = r#""hello \u0041 world""#;
        let mut lexer = UclLexer::new(input);
        let result = lexer.next_token();
        assert!(result.is_ok());

        // Test invalid Unicode escapes - surrogate pairs
        let input = r#""hello \uD800 world""#;
        let mut lexer = UclLexer::new(input);
        let result = lexer.next_token();
        assert!(result.is_err());
        if let Err(error) = result {
            match error {
                LexError::InvalidUnicodeEscape { .. } => {}
                _ => panic!("Expected InvalidUnicodeEscape error, got: {:?}", error),
            }
        }
    }

    #[test]
    fn test_resource_limits() {
        use crate::lexer::LexerConfig;

        // Test string length limit
        let mut config = LexerConfig::default();
        config.max_string_length = 10;

        let input = "\"this string is too long\"";
        let mut lexer = UclLexer::with_config(input, config.clone());
        let result = lexer.next_token();
        assert!(result.is_err());

        // Test token limit
        config.max_tokens = 2;
        let input = "{ key: value }";
        let mut lexer = UclLexer::with_config(input, config.clone());

        // First token should work
        assert!(lexer.next_token().is_ok());
        // Second token should work
        assert!(lexer.next_token().is_ok());
        // Third token should fail
        assert!(lexer.next_token().is_err());
    }

    #[test]
    fn test_overflow_protection() {
        // Test very long number
        let long_number = "1".repeat(2000);
        let mut lexer = UclLexer::new(&long_number);
        let result = lexer.next_token();
        assert!(result.is_err());
        if let Err(error) = result {
            match error {
                LexError::InvalidNumber { .. } => {}
                _ => panic!(
                    "Expected InvalidNumber error for long number, got: {:?}",
                    error
                ),
            }
        }
    }

    #[test]
    fn test_malformed_input_detection() {
        // Test invalid characters that should trigger validation
        let input = "@";
        let mut lexer = UclLexer::new(input);
        let result = lexer.next_token();
        if result.is_ok() {
            println!("Unexpected success for '@', got: {:?}", result);
        }
        assert!(result.is_err(), "Expected error for invalid character '@'");

        // Test invalid escape sequences
        let input = r#""hello \q world""#;
        let mut lexer = UclLexer::new(input);
        let result = lexer.next_token();
        if result.is_ok() {
            println!("Unexpected success for invalid escape, got: {:?}", result);
        }
        assert!(
            result.is_err(),
            "Expected error for invalid escape sequence"
        );

        // Test malformed numbers - test cases that should actually fail during number parsing
        let test_cases = vec![
            "123e", // Incomplete exponent
            "0x",   // Incomplete hex number
            "1e+",  // Incomplete exponent with sign
        ];

        for case in test_cases {
            let mut lexer = UclLexer::new(case);
            let result = lexer.next_token();
            if result.is_ok() {
                println!("Unexpected success for case: {}, got: {:?}", case, result);
            }
            assert!(
                result.is_err(),
                "Expected error for malformed number: {}",
                case
            );
        }

        // Test unterminated strings
        let input = r#""unterminated string"#;
        let mut lexer = UclLexer::new(input);
        let result = lexer.next_token();
        assert!(result.is_err(), "Expected error for unterminated string");

        // Test unterminated comments
        let input = "/* unterminated comment";
        let mut lexer = UclLexer::new(input);
        let result = lexer.next_token();
        assert!(result.is_err(), "Expected error for unterminated comment");
    }

    #[test]
    fn test_malformed_heredoc() {
        // Test empty terminator
        let input = "<<\ncontent\n";
        let mut lexer = UclLexer::new(input);
        let result = lexer.next_token();
        assert!(result.is_err());

        // Test invalid terminator characters
        let input = "<<term123\ncontent\nterm123";
        let mut lexer = UclLexer::new(input);
        let result = lexer.next_token();
        assert!(result.is_err());
    }

    #[test]
    fn test_error_recovery() {
        let input = "{ @invalid: \"valid\" }";
        let mut lexer = UclLexer::new(input);

        // First token should be ObjectStart
        assert!(matches!(
            lexer.next_token(),
            Ok(crate::lexer::Token::ObjectStart)
        ));

        // Next should be an error for the invalid symbol
        assert!(lexer.next_token().is_err());

        // Test recovery function
        assert!(lexer.recover_from_error().is_ok());
    }

    #[test]
    fn test_number_format_edge_cases() {
        // Test edge cases in number validation
        let test_cases = vec![
            ("", "Empty number"),
            ("+", "Empty number"),
            ("-", "Empty number"),
            ("1..2", "Multiple consecutive decimal points"),
            ("1ee2", "Multiple exponent markers"),
            ("1e", "Incomplete exponent"),
            ("01", "Leading zeros not allowed"),
        ];

        for (number_text, _expected_error) in test_cases {
            let lexer = UclLexer::new("");
            let result = lexer.validate_number_format(number_text, Position::new());
            assert!(
                result.is_err(),
                "Expected error for number: {}",
                number_text
            );
        }

        // Test valid cases
        let valid_cases = vec!["0", "0.0", "0x123", "0X123", "1.23", "1e5", "1E-5", "1.0"];
        for number_text in valid_cases {
            let lexer = UclLexer::new("");
            let result = lexer.validate_number_format(number_text, Position::new());
            assert!(result.is_ok(), "Expected valid number: {}", number_text);
        }
    }

    #[test]
    fn test_control_character_handling() {
        // Test various control characters in strings
        let control_chars = vec![
            '\x00', '\x01', '\x02', '\x03', '\x04', '\x05', '\x06', '\x07', '\x08', '\x0B', '\x0C',
            '\x0E', '\x0F', '\x10', '\x11', '\x12', '\x13', '\x14', '\x15', '\x16', '\x17', '\x18',
            '\x19', '\x1A', '\x1B', '\x1C', '\x1D', '\x1E', '\x1F', '\x7F',
        ];

        for ch in control_chars {
            let input = format!("\"hello{}world\"", ch);
            let mut lexer = UclLexer::new(&input);
            let result = lexer.next_token();
            if result.is_ok() {
                println!(
                    "Unexpected success for control character: {:?} ({}), got: {:?}",
                    ch, ch as u32, result
                );
            }
            assert!(
                result.is_err(),
                "Expected error for control character: {:?}",
                ch
            );
        }

        // Tab should be allowed
        let input = "\"hello\tworld\"";
        let mut lexer = UclLexer::new(input);
        let result = lexer.next_token();
        assert!(result.is_ok(), "Tab character should be allowed in strings");
    }

    #[test]
    fn test_unicode_edge_cases() {
        use crate::lexer::LexerConfig;

        // Test various Unicode edge cases
        let mut config = LexerConfig::default();
        config.strict_unicode = true;

        // Test valid Unicode ranges
        let valid_cases = vec![
            "\"\\u0000\"", // Null character (valid Unicode)
            "\"\\u007F\"", // DEL character
            "\"\\u0080\"", // First non-ASCII
            "\"\\uFFFF\"", // Last BMP character
        ];

        for case in valid_cases {
            let mut lexer = UclLexer::with_config(case, config.clone());
            let result = lexer.next_token();
            assert!(result.is_ok(), "Expected valid Unicode for: {}", case);
        }

        // Test invalid Unicode ranges
        let invalid_cases = vec![
            "\"\\uD800\"", // High surrogate
            "\"\\uDFFF\"", // Low surrogate
        ];

        for case in invalid_cases {
            let mut lexer = UclLexer::with_config(case, config.clone());
            let result = lexer.next_token();
            assert!(result.is_err(), "Expected invalid Unicode for: {}", case);
        }
    }

    #[test]
    fn test_nesting_depth_limits() {
        use crate::lexer::LexerConfig;

        let mut config = LexerConfig::default();
        config.max_nesting_depth = 3;

        // Test nested objects within limit
        let input = "{ a: { b: { c: 1 } } }";
        let mut lexer = UclLexer::with_config(input, config.clone());

        // Should be able to parse up to the limit
        assert!(lexer.next_token().is_ok()); // {
        assert!(lexer.next_token().is_ok()); // key 'a'
        assert!(lexer.next_token().is_ok()); // :
        assert!(lexer.next_token().is_ok()); // {
        assert!(lexer.next_token().is_ok()); // key 'b'
        assert!(lexer.next_token().is_ok()); // :
        assert!(lexer.next_token().is_ok()); // {
        assert!(lexer.next_token().is_ok()); // key 'c'

        // Test exceeding nesting limit
        let deep_input = "{ a: { b: { c: { d: 1 } } } }";
        let mut lexer = UclLexer::with_config(deep_input, config);

        // Parse until we hit the limit
        let mut error_found = false;
        for _ in 0..20 {
            match lexer.next_token() {
                Ok(crate::lexer::Token::Eof) => break,
                Ok(_) => continue,
                Err(_) => {
                    error_found = true;
                    break;
                }
            }
        }
        assert!(error_found, "Expected nesting depth error");
    }

    #[test]
    fn test_memory_exhaustion_protection() {
        use crate::lexer::LexerConfig;

        // Test comment length protection
        let mut config = LexerConfig::default();
        config.max_comment_length = 100;

        let long_comment = format!("# {}", "x".repeat(200));
        let mut lexer = UclLexer::with_config(&long_comment, config.clone());
        let result = lexer.next_token();
        assert!(result.is_err(), "Expected error for long comment");

        // Test string length protection during parsing
        config.max_string_length = 50;
        let long_string = format!("\"{}\"", "x".repeat(100));
        let mut lexer = UclLexer::with_config(&long_string, config);
        let result = lexer.next_token();
        assert!(result.is_err(), "Expected error for long string");
    }
}
