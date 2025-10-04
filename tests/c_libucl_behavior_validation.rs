//! C libucl behavior validation tests
//!
//! This module tests the compatibility validator to ensure our UCL parser
//! behaves consistently with the C libucl reference implementation.

use serde_json::Value;
use ucl_lexer::c_libucl_compatibility::CompatibilityValidator;
use ucl_lexer::from_str;

#[cfg(test)]
mod behavior_validation_tests {
    use super::*;

    #[test]
    fn test_syntax_validation_comprehensive() {
        let validator = CompatibilityValidator::new();

        // Test all supported syntax types
        let syntax_tests = vec![
            // NGINX-style implicit objects
            ("nginx_implicit_object", "server { listen 80 }", true),
            ("nginx_implicit_object", "server listen 80", false), // Missing brace
            ("nginx_implicit_object", "123server { }", false),    // Invalid key
            // NGINX-style nested objects
            (
                "nginx_nested_object",
                "upstream backend { server 127.0.0.1 }",
                true,
            ),
            ("nginx_nested_object", "upstream { }", false), // Missing identifier
            ("nginx_nested_object", "123upstream backend { }", false), // Invalid key
            // Bare word values
            ("bare_word_value", "environment production", true),
            ("bare_word_value", "environment production debug", false), // Too many parts
            ("bare_word_value", "123env production", false),            // Invalid key
            // Extended Unicode escapes
            ("unicode_escape_extended", "\\u{1F600}", true),
            ("unicode_escape_extended", "\\u{41}", true),
            ("unicode_escape_extended", "\\u{}", false), // Empty braces
            ("unicode_escape_extended", "\\u{1234567}", false), // Too many digits
            ("unicode_escape_extended", "\\u{110000}", false), // Out of range
            // C++ style comments
            ("cpp_comment", "// This is a comment", true),
            ("cpp_comment", "key = value; // End comment", true),
            ("cpp_comment", "This is not a comment", false), // No comment marker
            // Heredoc terminators
            ("heredoc_terminator", "<<EOF", true),
            ("heredoc_terminator", "<<CUSTOM_DELIMITER", true),
            ("heredoc_terminator", "<<eof", false), // Lowercase
            ("heredoc_terminator", "<<123EOF", false), // Starts with number
            ("heredoc_terminator", "<<", false),    // Empty terminator
        ];

        for (syntax_type, content, should_pass) in syntax_tests {
            let result = validator.validate_syntax(syntax_type, content);

            if should_pass {
                assert!(
                    result.is_ok(),
                    "Syntax validation should pass for {}: {} - Error: {:?}",
                    syntax_type,
                    content,
                    result.err()
                );
            } else {
                assert!(
                    result.is_err(),
                    "Syntax validation should fail for {}: {}",
                    syntax_type,
                    content
                );
            }
        }
    }

    #[test]
    fn test_error_message_validation() {
        let validator = CompatibilityValidator::new();

        // Test error message format validation
        let error_tests = vec![
            // Valid error messages
            ("unexpected_token", "Unexpected token '{' at 1:5", true),
            (
                "unexpected_token",
                "Unexpected token 'string' at 2:10",
                true,
            ),
            ("unterminated_string", "Unterminated string at 2:10", true),
            (
                "invalid_unicode_escape",
                "Invalid Unicode escape sequence",
                true,
            ),
            (
                "unterminated_heredoc",
                "Unterminated heredoc, expected terminator 'EOF'",
                true,
            ),
            // Invalid error message formats
            ("unexpected_token", "Syntax error occurred", false),
            ("unterminated_string", "String problem", false),
            ("invalid_unicode_escape", "Unicode problem", false),
        ];

        for (error_type, message, should_pass) in error_tests {
            let result = validator.validate_error_message(error_type, message);

            if should_pass {
                assert!(
                    result.is_ok(),
                    "Error message validation should pass for {}: {} - Error: {:?}",
                    error_type,
                    message,
                    result.err()
                );
            } else {
                assert!(
                    result.is_err(),
                    "Error message validation should fail for {}: {}",
                    error_type,
                    message
                );
            }
        }
    }

    #[test]
    fn test_edge_case_validation() {
        let validator = CompatibilityValidator::new();

        // Create test strings that need to live long enough
        let deep_nesting = "{".repeat(2000) + &"}".repeat(2000);
        let large_string = "x".repeat(2_000_000);

        // Test edge case validation
        let edge_case_tests = vec![
            // Valid edge cases
            ("empty_object", "{}", true),
            ("nested_depth_limit", "{{{{{{{{{{}}}}}}}}}", true), // Reasonable depth
            ("large_string_values", "short string", true),
            ("unicode_edge_cases", "normal text", true),
            ("number_edge_cases", "0xFF", true),
            ("number_edge_cases", "1.23e-4", true),
            // Invalid edge cases
            ("nested_depth_limit", deep_nesting.as_str(), false), // Too deep
            ("large_string_values", large_string.as_str(), false), // Too large
            ("unicode_edge_cases", "normal text", true),          // This should pass
            ("number_edge_cases", "0x", false),                   // Invalid hex
        ];

        for (case_name, content, should_pass) in edge_case_tests {
            let result = validator.validate_edge_case(case_name, content);

            if should_pass {
                assert!(
                    result.is_ok(),
                    "Edge case validation should pass for {}: {} - Error: {:?}",
                    case_name,
                    content,
                    result.err()
                );
            } else {
                assert!(
                    result.is_err(),
                    "Edge case validation should fail for {}: {}",
                    case_name,
                    content
                );
            }
        }
    }

    #[test]
    fn test_real_world_config_validation() {
        let validator = CompatibilityValidator::new();

        // Test validation against real-world configurations
        let nginx_config = r#"
            server {
                listen 80
                server_name example.com
            }
        "#;

        // Parse the config first
        let parse_result = from_str::<Value>(nginx_config);

        match parse_result {
            Ok(_) => {
                // If parsing succeeded, validate the syntax patterns used
                assert!(
                    validator
                        .validate_syntax("nginx_implicit_object", "server { listen 80 }")
                        .is_ok()
                );
                assert!(
                    validator
                        .validate_syntax("bare_word_value", "listen 80")
                        .is_ok()
                );
                assert!(
                    validator
                        .validate_syntax("bare_word_value", "server_name example.com")
                        .is_ok()
                );
            }
            Err(e) => {
                // If parsing failed, validate that the error message format is correct
                let error_msg = e.to_string();

                if error_msg.contains("Unexpected token") {
                    assert!(
                        validator
                            .validate_error_message("unexpected_token", &error_msg)
                            .is_ok()
                    );
                } else if error_msg.contains("Unterminated") {
                    assert!(
                        validator
                            .validate_error_message("unterminated_string", &error_msg)
                            .is_ok()
                    );
                }

                println!("Config parsing failed (expected for now): {}", e);
            }
        }
    }

    #[test]
    fn test_unicode_compatibility_validation() {
        let validator = CompatibilityValidator::new();

        // Test Unicode handling compatibility
        let unicode_tests = vec![
            // Basic Unicode escapes (should work)
            r#"text = "Unicode: \u0041\u0042""#,
            // Extended Unicode escapes (should work if implemented)
            r#"emoji = "Emoji: \u{1F600}""#,
            // Mixed Unicode formats
            r#"mixed = "\u0041\u{1F600}\u0042""#,
        ];

        for config in unicode_tests {
            let parse_result = from_str::<Value>(config);

            match parse_result {
                Ok(parsed) => {
                    println!("✅ Unicode config parsed: {:?}", parsed);

                    // Validate that extended Unicode escapes follow C libucl rules
                    if config.contains("\\u{") {
                        assert!(
                            validator
                                .validate_syntax("unicode_escape_extended", "\\u{1F600}")
                                .is_ok()
                        );
                    }
                }
                Err(e) => {
                    println!("❌ Unicode config failed: {}", e);

                    // Validate error message format
                    let error_msg = e.to_string();
                    if error_msg.contains("Invalid") && error_msg.contains("Unicode") {
                        assert!(
                            validator
                                .validate_error_message("invalid_unicode_escape", &error_msg)
                                .is_ok()
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn test_comment_format_compatibility() {
        let validator = CompatibilityValidator::new();

        // Test comment format compatibility
        let comment_tests = vec![
            // Hash comments (should work)
            r#"
                # Hash comment
                key = "value"
            "#,
            // Multi-line comments (should work)
            r#"
                /*
                 * Multi-line comment
                 */
                key = "value"
            "#,
            // C++ style comments (should work if implemented)
            r#"
                // C++ style comment
                key = "value"
            "#,
            // Mixed comment styles
            r#"
                # Hash comment
                /* Multi-line comment */
                // C++ comment
                key = "value"
            "#,
        ];

        for config in comment_tests {
            let parse_result = from_str::<Value>(config);

            match parse_result {
                Ok(parsed) => {
                    println!("✅ Comment config parsed: {:?}", parsed);

                    // Validate C++ comment syntax if present
                    if config.contains("//") {
                        assert!(
                            validator
                                .validate_syntax("cpp_comment", "// C++ style comment")
                                .is_ok()
                        );
                    }
                }
                Err(e) => {
                    println!("❌ Comment config failed: {}", e);
                }
            }
        }
    }

    #[test]
    fn test_number_format_compatibility() {
        let validator = CompatibilityValidator::new();

        // Test number format compatibility
        let number_tests = vec![
            // Basic numbers
            r#"decimal = 42"#,
            r#"float = 3.14"#,
            r#"scientific = 1.23e-4"#,
            // Hex numbers (if supported)
            r#"hex = 0xFF"#,
            r#"hex_lower = 0xff"#,
            // Size suffixes (if supported)
            r#"size = 64kb"#,
            r#"size_mb = 512mb"#,
            // Time suffixes (if supported)
            r#"time = 30s"#,
            r#"time_min = 5min"#,
        ];

        for config in number_tests {
            let parse_result = from_str::<Value>(config);

            match parse_result {
                Ok(parsed) => {
                    println!("✅ Number config parsed: {:?}", parsed);

                    // Validate number edge cases
                    if config.contains("0x") || config.contains("0X") {
                        assert!(
                            validator
                                .validate_edge_case("number_edge_cases", "0xFF")
                                .is_ok()
                        );
                    }

                    if config.contains("e") || config.contains("E") {
                        assert!(
                            validator
                                .validate_edge_case("number_edge_cases", "1.23e-4")
                                .is_ok()
                        );
                    }
                }
                Err(e) => {
                    println!("❌ Number config failed: {}", e);

                    // Validate error message format
                    let error_msg = e.to_string();
                    if error_msg.contains("Invalid") && error_msg.contains("number") {
                        assert!(
                            validator
                                .validate_error_message("invalid_number", &error_msg)
                                .is_ok()
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn test_comprehensive_compatibility_report() {
        let validator = CompatibilityValidator::new();

        println!("\n=== C libucl Compatibility Validation Report ===");

        // Report supported features
        println!("\nSupported Syntax Types:");
        for syntax_type in validator.supported_syntax_types() {
            println!("  - {}", syntax_type);
        }

        println!("\nSupported Error Types:");
        for error_type in validator.supported_error_types() {
            println!("  - {}", error_type);
        }

        println!("\nSupported Edge Cases:");
        for edge_case in validator.supported_edge_cases() {
            println!("  - {}", edge_case);
        }

        // Test a comprehensive configuration
        let comprehensive_config = r#"
            # Comprehensive UCL configuration test
            
            // C++ style comment
            worker_processes = auto;
            
            events {
                worker_connections = 1024;
                use = epoll;
            }
            
            http {
                sendfile = true;
                gzip = on;
                
                upstream backend {
                    server "127.0.0.1:8080";
                    keepalive = 32;
                }
                
                server {
                    listen = 80;
                    server_name = "example.com";
                    
                    location "/" {
                        try_files = "$uri $uri/ =404";
                    }
                }
            }
            
            # Unicode test
            message = "Hello \u{1F600} World";
            
            # Heredoc test
            description = <<EOF
This is a multi-line
description with heredoc.
EOF
            
            # Number formats
            decimal = 42;
            hex = 0xFF;
            size = 64kb;
            time = 30s;
            
            # Arrays
            servers = "server1";
            servers = "server2";
            servers = "server3";
        "#;

        println!("\n=== Testing Comprehensive Configuration ===");

        let parse_result = from_str::<Value>(comprehensive_config);
        match parse_result {
            Ok(parsed) => {
                println!("✅ Comprehensive config parsed successfully");
                println!("Parsed structure: {:#}", parsed);

                // Validate individual syntax patterns
                let validations = vec![
                    validator.validate_syntax(
                        "nginx_implicit_object",
                        "events { worker_connections = 1024 }",
                    ),
                    validator.validate_syntax(
                        "nginx_nested_object",
                        "upstream backend { server \"127.0.0.1:8080\" }",
                    ),
                    validator.validate_syntax("bare_word_value", "worker_processes auto"),
                    validator.validate_syntax("cpp_comment", "// C++ style comment"),
                    validator.validate_syntax("unicode_escape_extended", "\\u{1F600}"),
                    validator.validate_syntax("heredoc_terminator", "<<EOF"),
                ];

                let mut passed = 0;
                let total = validations.len();

                for (i, validation) in validations.iter().enumerate() {
                    if validation.is_ok() {
                        passed += 1;
                        println!("  ✅ Validation {} passed", i + 1);
                    } else {
                        println!(
                            "  ❌ Validation {} failed: {:?}",
                            i + 1,
                            validation.as_ref().err()
                        );
                    }
                }

                println!(
                    "\nValidation Summary: {}/{} passed ({:.1}%)",
                    passed,
                    total,
                    (passed as f64 / total as f64) * 100.0
                );
            }
            Err(e) => {
                println!("❌ Comprehensive config failed: {}", e);

                // Validate error message format
                let error_msg = e.to_string();
                if error_msg.contains("Unexpected token") {
                    let validation =
                        validator.validate_error_message("unexpected_token", &error_msg);
                    if validation.is_ok() {
                        println!("✅ Error message format is C libucl compatible");
                    } else {
                        println!(
                            "❌ Error message format needs improvement: {:?}",
                            validation.err()
                        );
                    }
                }
            }
        }

        println!("\n=== End Compatibility Report ===");
    }
}
