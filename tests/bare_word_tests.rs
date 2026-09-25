//! Unquoted values (spec §4): extent, keywords, numbers and strings. Expected results are
//! libucl's (spec §4, checked with the oracle).

use serde_json::Value;
use ucl_lexer::parse::{self, ErrorKind};
use ucl_lexer::{UclError, UclValue, from_str};

#[cfg(test)]
mod bare_word_tests {
    use super::*;

    /// The value of `key` in the root object of `config`, as the new core parses it.
    fn value_of(config: &str, key: &str) -> UclValue {
        let root = parse::parse(config.as_bytes()).expect("parses");
        root.as_object().expect("an object")[key].clone()
    }

    fn error_kind(config: &str) -> ErrorKind {
        match from_str::<Value>(config) {
            Err(UclError::Syntax(e)) => e.kind().clone(),
            other => panic!("{config:?}: expected a parse error, got {other:?}"),
        }
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
        // Boolean keywords are ASCII case-insensitive (spec §4.5).
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
        // `null` is lowercase only; `NULL` and `Null` are strings (spec §4.5).
        let config = r#"
            value1 = null
            value2 = NULL
            value3 = Null
        "#;

        let result: Value = from_str(config).expect("Should parse null keywords");
        assert_eq!(result["value1"], Value::Null);
        assert_eq!(result["value2"], "NULL");
        assert_eq!(result["value3"], "Null");
    }

    #[test]
    fn test_special_float_values() {
        // `inf` and `nan` are lowercase-only keywords; `infinity`, `-inf`, `-infinity`, `NaN` and
        // `NAN` are strings (spec §4.5).
        let config = r#"
            positive_infinity = inf
            positive_infinity2 = infinity
            negative_infinity = -inf
            negative_infinity2 = -infinity
            not_a_number = nan
            not_a_number2 = NaN
            not_a_number3 = NAN
        "#;

        assert_eq!(
            value_of(config, "positive_infinity"),
            UclValue::Float(f64::INFINITY)
        );
        assert!(matches!(
            value_of(config, "not_a_number"),
            UclValue::Float(f) if f.is_nan()
        ));
        for (key, text) in [
            ("positive_infinity2", "infinity"),
            ("negative_infinity", "-inf"),
            ("negative_infinity2", "-infinity"),
            ("not_a_number2", "NaN"),
            ("not_a_number3", "NAN"),
        ] {
            assert_eq!(
                value_of(config, key),
                UclValue::String(text.into()),
                "{key}"
            );
        }
    }

    #[test]
    fn test_bare_word_extent() {
        // An unquoted value runs to a terminator, `#` or an unmatched closing bracket; spaces,
        // `@` and balanced or unclosed opening brackets are part of it (spec §4.1, §4.2).
        for (config, expected) in [
            ("key = hello world", "hello world"),
            ("key = hello@world", "hello@world"),
            ("key = hello@domain.com", "hello@domain.com"),
            ("key = hello#world", "hello"),
            ("key = hello#comment", "hello"),
            ("key = hello{world", "hello{world"),
            ("key = hello[world", "hello[world"),
        ] {
            let result: Value = from_str(config).expect(config);
            assert_eq!(result["key"], expected, "{config}");
        }
        // An unmatched `}` or `]` ends the value and closes nothing (spec §4.2).
        assert_eq!(
            error_kind("key = hello}world"),
            ErrorKind::UnmatchedClose { found: '}' }
        );
        assert_eq!(
            error_kind("key = hello]world"),
            ErrorKind::UnmatchedClose { found: ']' }
        );
        // `,` and `;` end the value; the word after them is a key without a value (spec §4.1).
        assert_eq!(error_kind("key = hello,world"), ErrorKind::MissingValue);
        assert_eq!(error_kind("key = hello;world"), ErrorKind::MissingValue);
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
        // Quoted strings are never keywords or numbers (spec §4.5, §6).
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
        // Digits followed by letters that are not a suffix: a string (spec §5.5).
        assert_eq!(result["number_prefix"], "123abc");
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

        // Booleans are case-insensitive, `null` is lowercase only (spec §4.5).
        assert_eq!(result["word_true"], true);
        assert_eq!(result["word_false"], false);
        assert_eq!(result["word_null"], "Null");
    }

    #[test]
    fn test_bare_word_with_numbers() {
        // Test bare words that look like numbers but aren't
        let config = r#"
            version = v1.2.3
            port = 8080
            mixed = abc123
            hex_like = 0xabc
            float_like = 2.75
            scientific = 1e10
        "#;

        let result: Value = from_str(config).expect("Should parse number-like bare words");

        // Pure numbers should be parsed as numbers
        assert_eq!(result["port"], 8080);
        assert_eq!(result["float_like"], 2.75);

        // Mixed alphanumeric should be strings
        assert_eq!(result["version"], "v1.2.3");
        assert_eq!(result["mixed"], "abc123");

        // Hexadecimal integers are ints, exponents floats (spec §5.1, §5.2).
        assert_eq!(value_of(config, "hex_like"), UclValue::Integer(0xabc));
        assert_eq!(value_of(config, "scientific"), UclValue::Float(1e10));
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
