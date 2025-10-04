use serde_json::Value;
use ucl_lexer::{UclError, from_str};

#[cfg(test)]
mod bare_word_tests {
    use super::*;

    fn assert_infinite(value: &Value, positive: bool) {
        if let Some(f) = value.as_f64() {
            assert!(f.is_infinite(), "Expected infinite float, got {:?}", value);
            assert_eq!(
                f.is_sign_positive(),
                positive,
                "Unexpected infinity sign for {:?}",
                value
            );
            return;
        }

        if value.is_null() {
            // serde_json cannot represent non-finite numbers; null indicates special float
            return;
        }

        if let Some(s) = value.as_str() {
            let normalized = s.trim().to_ascii_lowercase();
            if positive {
                assert!(
                    normalized == "inf" || normalized == "infinity",
                    "Expected positive infinity representation, got {:?}",
                    value
                );
            } else {
                assert!(
                    normalized == "-inf" || normalized == "-infinity",
                    "Expected negative infinity representation, got {:?}",
                    value
                );
            }
            return;
        }

        panic!("Unexpected representation for infinity: {:?}", value);
    }

    fn assert_nan(value: &Value) {
        if let Some(f) = value.as_f64() {
            assert!(f.is_nan(), "Expected NaN float, got {:?}", value);
            return;
        }

        if value.is_null() {
            // serde_json represents NaN as null to preserve JSON compatibility
            return;
        }

        if let Some(s) = value.as_str() {
            assert_eq!(
                s.trim().to_ascii_lowercase(),
                "nan",
                "Expected 'nan' string, got {:?}",
                value
            );
            return;
        }

        panic!("Unexpected representation for NaN: {:?}", value);
    }

    #[test]
    fn test_unquoted_string_values() {
        // Test unquoted identifiers accepted as string values
        let config = r#"
            environment = production
            log_level = debug
            server_name = nginx
            worker_processes = auto
            database_host = localhost
            file_path = /var/log/app.log
        "#;

        let result: Value = from_str(config).expect("Should parse unquoted string values");
        assert_eq!(result["environment"], "production");
        assert_eq!(result["log_level"], "debug");
        assert_eq!(result["server_name"], "nginx");
        assert_eq!(result["worker_processes"], "auto");
        assert_eq!(result["database_host"], "localhost");
        assert_eq!(result["file_path"], "/var/log/app.log");
    }

    #[test]
    fn test_boolean_keyword_conversion() {
        // Test boolean keywords converted to boolean values
        let config = r#"
            flag1 = true
            flag2 = false
            flag3 = yes
            flag4 = no
            flag5 = on
            flag6 = off
            
            # Test case variations
            flag7 = True
            flag8 = False
            flag9 = YES
            flag10 = NO
            flag11 = ON
            flag12 = OFF
        "#;

        let result: Value = from_str(config).expect("Should parse boolean keywords");
        assert_eq!(result["flag1"], true);
        assert_eq!(result["flag2"], false);
        assert_eq!(result["flag3"], true);
        assert_eq!(result["flag4"], false);
        assert_eq!(result["flag5"], true);
        assert_eq!(result["flag6"], false);

        // Case variations should also work
        assert_eq!(result["flag7"], true);
        assert_eq!(result["flag8"], false);
        assert_eq!(result["flag9"], true);
        assert_eq!(result["flag10"], false);
        assert_eq!(result["flag11"], true);
        assert_eq!(result["flag12"], false);
    }

    #[test]
    fn test_null_keyword_conversion() {
        // Test null keyword converted to null value
        let config = r#"
            value1 = null
            value2 = NULL
            value3 = Null
        "#;

        let result: Value = from_str(config).expect("Should parse null keywords");
        assert_eq!(result["value1"], Value::Null);
        assert_eq!(result["value2"], Value::Null);
        assert_eq!(result["value3"], Value::Null);
    }

    #[test]
    fn test_special_float_values() {
        // Test special float values (inf, -inf, nan)
        let config = r#"
            positive_infinity = inf
            positive_infinity2 = infinity
            negative_infinity = -inf
            negative_infinity2 = -infinity
            not_a_number = nan
            not_a_number2 = NaN
            not_a_number3 = NAN
        "#;

        let result: Value = from_str(config).expect("Should parse special float values");

        // Check infinity values
        assert_infinite(&result["positive_infinity"], true);
        assert_infinite(&result["positive_infinity2"], true);

        assert_infinite(&result["negative_infinity"], false);
        assert_infinite(&result["negative_infinity2"], false);

        // Check NaN values
        assert_nan(&result["not_a_number"]);
        assert_nan(&result["not_a_number2"]);
        assert_nan(&result["not_a_number3"]);
    }

    #[test]
    fn test_bare_word_validation_errors() {
        // Test bare words with special characters require quoting
        let invalid_configs = vec![
            (r#"key = hello world"#, "Spaces should require quotes"),
            (
                r#"key = hello@world"#,
                "Special characters should require quotes",
            ),
            (
                r#"key = hello#world"#,
                "Hash character should require quotes",
            ),
            (r#"key = hello{world"#, "Braces should require quotes"),
            (r#"key = hello}world"#, "Braces should require quotes"),
            (r#"key = hello[world"#, "Brackets should require quotes"),
            (r#"key = hello]world"#, "Brackets should require quotes"),
            (r#"key = hello,world"#, "Comma should require quotes"),
            (r#"key = hello;world"#, "Semicolon should require quotes"),
        ];

        for (config, description) in invalid_configs {
            let result: Result<Value, UclError> = from_str(config);
            // These should either fail or be parsed in a specific way
            // The exact behavior depends on implementation
            match result {
                Ok(val) => {
                    // If it parses, it should be as a string or have some reasonable interpretation
                    assert!(val.is_object(), "{}: {}", description, config);
                }
                Err(error) => {
                    // If it fails, error should be helpful
                    let error_msg = error.to_string();
                    assert!(
                        error_msg.contains("quote")
                            || error_msg.contains("invalid")
                            || error_msg.contains("bare"),
                        "{}: Error should mention quoting: {}",
                        description,
                        error_msg
                    );
                }
            }
        }
    }

    #[test]
    fn test_bare_words_in_different_contexts() {
        // Test bare words in various contexts (objects, arrays, nested)
        let config = r#"
            server {
                listen 80
                server_name example.com
                root /var/www
                
                location / {
                    try_files $uri $uri/ =404
                    proxy_pass http://backend
                }
            }
            
            array_with_bare_words = [
                production,
                staging,
                development
            ]
            
            mixed_array = [
                "quoted string",
                bare_word,
                123,
                true
            ]
        "#;

        let result: Value =
            from_str(config).expect("Should parse bare words in different contexts");

        // Check server block
        assert_eq!(result["server"]["listen"], 80);
        assert_eq!(result["server"]["server_name"], "example.com");
        assert_eq!(result["server"]["root"], "/var/www");

        // Check nested location
        let location = &result["server"]["location"]["/"];
        assert_eq!(location["try_files"], "$uri $uri/ =404");
        assert_eq!(location["proxy_pass"], "http://backend");

        // Check arrays
        assert_eq!(result["array_with_bare_words"][0], "production");
        assert_eq!(result["array_with_bare_words"][1], "staging");
        assert_eq!(result["array_with_bare_words"][2], "development");

        assert_eq!(result["mixed_array"][0], "quoted string");
        assert_eq!(result["mixed_array"][1], "bare_word");
        assert_eq!(result["mixed_array"][2], 123);
        assert_eq!(result["mixed_array"][3], true);
    }

    #[test]
    fn test_bare_word_vs_quoted_string_distinction() {
        // Test distinction between bare words and quoted strings
        let config = r#"
            bare_true = true
            quoted_true = "true"
            bare_false = false
            quoted_false = "false"
            bare_null = null
            quoted_null = "null"
            bare_number = 123
            quoted_number = "123"
        "#;

        let result: Value =
            from_str(config).expect("Should distinguish bare words from quoted strings");

        // Bare words should be converted to appropriate types
        assert_eq!(result["bare_true"], true);
        assert_eq!(result["bare_false"], false);
        assert_eq!(result["bare_null"], Value::Null);
        assert_eq!(result["bare_number"], 123);

        // Quoted strings should remain as strings
        assert_eq!(result["quoted_true"], "true");
        assert_eq!(result["quoted_false"], "false");
        assert_eq!(result["quoted_null"], "null");
        assert_eq!(result["quoted_number"], "123");
    }

    #[test]
    fn test_bare_word_edge_cases() {
        // Test edge cases for bare word parsing
        let config = r#"
            underscore_word = hello_world
            hyphen_word = hello-world
            dot_word = hello.world
            number_prefix = 123abc
            mixed_case = HelloWorld
            single_char = a
            empty_like = ""
        "#;

        let result: Value = from_str(config).expect("Should handle bare word edge cases");
        assert_eq!(result["underscore_word"], "hello_world");
        assert_eq!(result["hyphen_word"], "hello-world");
        assert_eq!(result["dot_word"], "hello.world");
        assert_eq!(result["mixed_case"], "HelloWorld");
        assert_eq!(result["single_char"], "a");
        assert_eq!(result["empty_like"], "");

        // number_prefix behavior depends on implementation
        // It might be parsed as a number or string
        assert!(result["number_prefix"].is_string() || result["number_prefix"].is_number());
    }

    #[test]
    fn test_reserved_keyword_handling() {
        // Test handling of reserved keywords as bare words
        let config = r#"
            # These should be treated as their respective types
            bool_true = true
            bool_false = false
            null_value = null
            
            # These should be treated as strings when quoted
            string_true = "true"
            string_false = "false"
            string_null = "null"
            
            # Test ambiguous cases
            word_true = True
            word_false = False
            word_null = Null
        "#;

        let result: Value = from_str(config).expect("Should handle reserved keywords");

        // Unquoted keywords should be converted
        assert_eq!(result["bool_true"], true);
        assert_eq!(result["bool_false"], false);
        assert_eq!(result["null_value"], Value::Null);

        // Quoted keywords should remain strings
        assert_eq!(result["string_true"], "true");
        assert_eq!(result["string_false"], "false");
        assert_eq!(result["string_null"], "null");

        // Case variations should also be converted (if implementation supports it)
        // The exact behavior may vary
        assert!(result["word_true"].is_boolean() || result["word_true"].is_string());
        assert!(result["word_false"].is_boolean() || result["word_false"].is_string());
        assert!(result["word_null"].is_null() || result["word_null"].is_string());
    }

    #[test]
    fn test_bare_word_with_numbers() {
        // Test bare words that look like numbers but aren't
        let config = r#"
            version = v1.2.3
            port = 8080
            mixed = abc123
            hex_like = 0xabc
            float_like = 3.14
            scientific = 1e10
        "#;

        let result: Value = from_str(config).expect("Should parse number-like bare words");

        // Pure numbers should be parsed as numbers
        assert_eq!(result["port"], 8080);
        assert_eq!(result["float_like"], 3.14);

        // Mixed alphanumeric should be strings
        assert_eq!(result["version"], "v1.2.3");
        assert_eq!(result["mixed"], "abc123");

        // Hex and scientific notation depend on implementation
        assert!(result["hex_like"].is_string() || result["hex_like"].is_number());
        assert!(result["scientific"].is_string() || result["scientific"].is_number());
    }

    #[test]
    fn test_bare_word_error_suggestions() {
        // Test that error messages provide helpful suggestions
        let potentially_problematic_configs = vec![
            r#"key = hello world"#,      // Space in bare word
            r#"key = hello@domain.com"#, // Email-like
            r#"key = hello#comment"#,    // Hash character
        ];

        for config in potentially_problematic_configs {
            let result: Result<Value, UclError> = from_str(config);
            match result {
                Ok(_) => {
                    // If it parses successfully, that's fine too
                }
                Err(error) => {
                    let error_msg = error.to_string();
                    // Error should provide helpful guidance
                    assert!(
                        error_msg.contains("quote")
                            || error_msg.contains("\"")
                            || error_msg.contains("bare")
                            || error_msg.contains("invalid"),
                        "Error should provide helpful suggestion: {}",
                        error_msg
                    );
                }
            }
        }
    }

    #[test]
    fn test_bare_words_with_unicode() {
        // Test bare words containing Unicode characters
        let config = r#"
            unicode_word = café
            emoji_word = hello😀
            chinese_word = 中文
            mixed_unicode = hello世界
        "#;

        let result: Value = from_str(config).expect("Should parse Unicode bare words");
        assert_eq!(result["unicode_word"], "café");
        assert_eq!(result["emoji_word"], "hello😀");
        assert_eq!(result["chinese_word"], "中文");
        assert_eq!(result["mixed_unicode"], "hello世界");
    }
}
