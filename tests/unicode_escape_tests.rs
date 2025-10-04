use serde_json::Value;
use ucl_lexer::{UclError, from_str};

#[cfg(test)]
mod unicode_escape_tests {
    use super::*;

    #[test]
    fn test_variable_length_unicode_escapes() {
        // Test \u{X} format with 1-6 hex digits
        let config = r#"
            single_digit = "\u{A}"
            two_digits = "\u{41}"
            three_digits = "\u{3B1}"
            four_digits = "\u{1F60}"
            five_digits = "\u{1F600}"
            six_digits = "\u{10FFFF}"
        "#;

        let result: Value = from_str(config).expect("Should parse variable-length Unicode escapes");
        assert_eq!(result["single_digit"], "\u{A}"); // Line feed
        assert_eq!(result["two_digits"], "A"); // Latin A
        assert_eq!(result["three_digits"], "α"); // Greek alpha
        assert_eq!(result["four_digits"], "\u{1F60}"); // Partial emoji code
        assert_eq!(result["five_digits"], "😀"); // Grinning face emoji
        assert_eq!(result["six_digits"], "\u{10FFFF}"); // Maximum Unicode code point
    }

    #[test]
    fn test_emoji_and_extended_unicode() {
        // Test emoji and extended Unicode characters
        // Note: "infinity" is a reserved keyword (represents f64::INFINITY), so we quote it
        let config = r#"
grinning_face = "\u{1F600}"
thumbs_up = "\u{1F44D}"
rocket = "\u{1F680}"
fire = "\u{1F525}"
hundred_points = "\u{1F4AF}"

# Mathematical symbols
"infinity" = "\u{221E}"
integral = "\u{222B}"

# Various scripts
chinese = "\u{4E2D}\u{6587}"
arabic = "\u{0627}\u{0644}\u{0639}\u{0631}\u{0628}\u{064A}\u{0629}"
russian = "\u{0440}\u{0443}\u{0441}\u{0441}\u{043A}\u{0438}\u{0439}"
        "#;

        let result: Value = from_str(config).expect("Should parse emoji and extended Unicode");
        assert_eq!(result["grinning_face"], "😀");
        assert_eq!(result["thumbs_up"], "👍");
        assert_eq!(result["rocket"], "🚀");
        assert_eq!(result["fire"], "🔥");
        assert_eq!(result["hundred_points"], "💯");
        assert_eq!(result["infinity"], "∞");
        assert_eq!(result["integral"], "∫");
        assert_eq!(result["chinese"], "中文");
        assert_eq!(result["arabic"], "العربية");
        assert_eq!(result["russian"], "русский");
    }

    #[test]
    fn test_mixed_unicode_escape_formats() {
        // Test mixing \uXXXX and \u{...} formats in same string
        let config = r#"
            mixed_format = "Hello \u0041\u{42}\u0043 World \u{1F600}"
            another_mix = "\u{48}\u0065\u{6C}\u006C\u{6F} \u{1F44B}"
            complex_mix = "\u0048\u{65}\u006C\u{6C}\u006F\u{20}\u{1F30D}"
        "#;

        let result: Value = from_str(config).expect("Should parse mixed Unicode escape formats");
        assert_eq!(result["mixed_format"], "Hello ABC World 😀");
        assert_eq!(result["another_mix"], "Hello 👋");
        assert_eq!(result["complex_mix"], "Hello 🌍");
    }

    #[test]
    fn test_unicode_escape_error_handling() {
        // Test error handling for invalid Unicode escapes
        let invalid_configs = vec![
            (r#"empty_braces = "\u{}""#, "Empty braces should be invalid"),
            (
                r#"too_many_digits = "\u{1234567}""#,
                "More than 6 digits should be invalid",
            ),
            (
                r#"out_of_range = "\u{110000}""#,
                "Code point > 0x10FFFF should be invalid",
            ),
            (
                r#"invalid_hex = "\u{GHIJ}""#,
                "Non-hex characters should be invalid",
            ),
            (
                r#"unclosed_braces = "\u{1234""#,
                "Unclosed braces should be invalid",
            ),
            (
                r#"mixed_invalid = "\u{12G3}""#,
                "Mixed valid/invalid hex should be invalid",
            ),
        ];

        for (config, description) in invalid_configs {
            let result: Result<Value, UclError> = from_str(config);
            assert!(result.is_err(), "{}: {}", description, config);

            // Verify error message contains helpful information
            let error = result.unwrap_err();
            let error_msg = error.to_string();
            assert!(
                error_msg.contains("Unicode") || error_msg.contains("escape"),
                "Error should mention Unicode or escape: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_unicode_boundary_values() {
        // Test Unicode escape boundary values
        let config = r#"
            min_value = "\u{0}"
            ascii_max = "\u{7F}"
            latin1_max = "\u{FF}"
            bmp_max = "\u{FFFF}"
            astral_min = "\u{10000}"
            unicode_max = "\u{10FFFF}"
        "#;

        let result: Value = from_str(config).expect("Should parse Unicode boundary values");
        assert_eq!(result["min_value"], "\u{0}");
        assert_eq!(result["ascii_max"], "\u{7F}");
        assert_eq!(result["latin1_max"], "\u{FF}");
        assert_eq!(result["bmp_max"], "\u{FFFF}");
        assert_eq!(result["astral_min"], "\u{10000}");
        assert_eq!(result["unicode_max"], "\u{10FFFF}");
    }

    #[test]
    fn test_unicode_escapes_in_keys() {
        // Test Unicode escapes in object keys
        let config = r#"
            "\u{1F600}_key" = "emoji key"
            "\u0041\u{42}C" = "mixed format key"
            "prefix_\u{03B1}" = "greek alpha key"
        "#;

        let result: Value = from_str(config).expect("Should parse Unicode escapes in keys");
        assert_eq!(result["😀_key"], "emoji key");
        assert_eq!(result["ABC"], "mixed format key");
        assert_eq!(result["prefix_α"], "greek alpha key");
    }

    #[test]
    fn test_unicode_escapes_in_arrays() {
        // Test Unicode escapes within array values
        let config = r#"
            emoji_array = [
                "\u{1F600}",
                "\u{1F601}",
                "\u{1F602}"
            ]
            
            mixed_array = [
                "Hello \u{1F30D}",
                "\u0048\u{65}\u006C\u{6C}\u006F",
                "\u{1F44B} World"
            ]
        "#;

        let result: Value = from_str(config).expect("Should parse Unicode escapes in arrays");
        assert_eq!(result["emoji_array"][0], "😀");
        assert_eq!(result["emoji_array"][1], "😁");
        assert_eq!(result["emoji_array"][2], "😂");
        assert_eq!(result["mixed_array"][0], "Hello 🌍");
        assert_eq!(result["mixed_array"][1], "Hello");
        assert_eq!(result["mixed_array"][2], "👋 World");
    }

    #[test]
    fn test_unicode_escapes_with_other_escapes() {
        // Test Unicode escapes combined with other escape sequences
        let config = r#"
            combined = "Line 1\nUnicode: \u{1F600}\tTab\r\nLine 2"
            quotes = "Quote: \"Hello \u{1F30D}\""
            backslash = "Path\\to\\file \u{1F4C1}"
        "#;

        let result: Value = from_str(config).expect("Should parse Unicode with other escapes");
        assert_eq!(result["combined"], "Line 1\nUnicode: 😀\tTab\r\nLine 2");
        assert_eq!(result["quotes"], "Quote: \"Hello 🌍\"");
        assert_eq!(result["backslash"], "Path\\to\\file 📁");
    }

    #[test]
    fn test_unicode_normalization() {
        // Test that Unicode escapes produce properly encoded UTF-8
        let config = r#"
            # Combining characters
            e_acute = "\u{0065}\u{0301}"
            # Precomposed character
            e_acute_precomposed = "\u{00E9}"
            
            # Surrogate pair equivalent (should be handled as single code point)
            emoji_pair = "\u{1F600}"
        "#;

        let result: Value = from_str(config).expect("Should handle Unicode normalization");

        // Both should represent é (though potentially in different normalization forms)
        let combining = result["e_acute"].as_str().unwrap();
        let precomposed = result["e_acute_precomposed"].as_str().unwrap();

        // At minimum, both should be valid UTF-8 strings
        assert!(!combining.is_empty());
        assert!(!precomposed.is_empty());
        assert_eq!(result["emoji_pair"], "😀");
    }

    #[test]
    fn test_unicode_case_sensitivity() {
        // Test that hex digits are case-insensitive
        let config = r#"
            lowercase = "\u{1f600}"
            uppercase = "\u{1F600}"
            mixed_case = "\u{1F60a}"
            mixed_case2 = "\u{1f60A}"
        "#;

        let result: Value = from_str(config).expect("Should parse case-insensitive hex digits");
        assert_eq!(result["lowercase"], "😀");
        assert_eq!(result["uppercase"], "😀");
        assert_eq!(result["mixed_case"], "😊");
        assert_eq!(result["mixed_case2"], "😊");
    }

    #[test]
    fn test_unicode_in_multiline_strings() {
        // Test Unicode escapes in regular multiline strings (not heredoc)
        // Note: UCL heredoc strings (<<EOF...EOF) do NOT process escape sequences per spec
        // Use regular strings with \n for multiline text with escapes
        let config = r#"
multiline = "First line with emoji: \u{1F600}\nSecond line with Greek: \u{03B1}\u{03B2}\u{03B3}\nThird line with Chinese: \u{4E2D}\u{6587}"
        "#;

        let result: Value = from_str(config).expect("Should parse Unicode in multiline strings");
        let multiline_text = result["multiline"].as_str().unwrap();
        assert!(multiline_text.contains("😀"));
        assert!(multiline_text.contains("αβγ"));
        assert!(multiline_text.contains("中文"));
    }
}
