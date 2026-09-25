//! Unicode in double-quoted strings and keys (spec §6.1).
//!
//! UCL's only Unicode escape is `\uXXXX`, exactly four hex digits in either case. The
//! variable-length form `\u{...}` is not part of the format: libucl rejects it
//! (`cases/spec/06-strings/dq_brace_unicode_error`). Characters outside the Basic Multilingual
//! Plane are written as UTF-8, which passes through unchanged (`dq_utf8`).

use serde_json::Value;
use ucl_lexer::parse::ErrorKind;
use ucl_lexer::{UclError, from_str};

fn error_kind(config: &str) -> ErrorKind {
    match from_str::<Value>(config) {
        Err(UclError::Syntax(e)) => e.kind().clone(),
        other => panic!("expected a parse error for {config:?}, got {other:?}"),
    }
}

#[test]
fn test_brace_unicode_escapes_are_errors() {
    // spec §6.1: a non-hex character among the four after `\u` is an error, and `{` is one.
    let configs = [
        r#"single_digit = "\u{A}""#,
        r#"five_digits = "\u{1F600}""#,
        r#"six_digits = "\u{10FFFF}""#,
        r#"min_value = "\u{0}""#,
        r#"mixed_format = "Hello \u0041\u{42}\u0043 World""#,
        r#"lowercase = "\u{1f600}""#,
        r#"empty_braces = "\u{}""#,
        r#"too_many_digits = "\u{1234567}""#,
        r#"out_of_range = "\u{110000}""#,
        r#"invalid_hex = "\u{GHIJ}""#,
        r#"unclosed_braces = "\u{1234""#,
        r#"mixed_invalid = "\u{12G3}""#,
        r#"emoji_array = ["\u{1F600}", "\u{1F601}"]"#,
        r#""\u{1F600}_key" = "emoji key""#,
        r#"multiline = "First line: \u{1F600}\nSecond line""#,
    ];
    for config in configs {
        assert_eq!(
            error_kind(config),
            ErrorKind::InvalidUnicodeEscape,
            "{config}"
        );
    }
}

#[test]
fn test_four_digit_unicode_escapes() {
    let config = r#"
        two_digits = "\u0041"
        three_digits = "\u03B1"
        four_digits = "\u1F60"
        "infinity" = "\u221E"
        integral = "\u222B"
        chinese = "\u4E2D\u6587"
        arabic = "\u0627\u0644\u0639\u0631\u0628\u064A\u0629"
        russian = "\u0440\u0443\u0441\u0441\u043A\u0438\u0439"
        mixed_format = "Hello \u0041\u0042\u0043 World"
        another_mix = "\u0048\u0065\u006C\u006C\u006F"
    "#;

    let result: Value = from_str(config).expect("Should parse four-digit Unicode escapes");
    assert_eq!(result["two_digits"], "A");
    assert_eq!(result["three_digits"], "\u{3B1}");
    assert_eq!(result["four_digits"], "\u{1F60}");
    assert_eq!(result["infinity"], "\u{221E}");
    assert_eq!(result["integral"], "\u{222B}");
    assert_eq!(result["chinese"], "\u{4E2D}\u{6587}");
    assert_eq!(
        result["arabic"],
        "\u{627}\u{644}\u{639}\u{631}\u{628}\u{64A}\u{629}"
    );
    assert_eq!(
        result["russian"],
        "\u{440}\u{443}\u{441}\u{441}\u{43A}\u{438}\u{439}"
    );
    assert_eq!(result["mixed_format"], "Hello ABC World");
    assert_eq!(result["another_mix"], "Hello");
}

#[test]
fn test_unicode_escape_takes_exactly_four_digits() {
    // spec §6.1: `"\u1F600"` is U+1F60 followed by `0` (`dq_unicode_five_digits`).
    let result: Value = from_str(r#"k = "\u1F600""#).unwrap();
    assert_eq!(result["k"], "\u{1F60}0");
    // Fewer than four hex digits is an error (`dq_unicode_short_error`).
    assert_eq!(error_kind(r#"k = "\u12""#), ErrorKind::InvalidUnicodeEscape);
}

#[test]
fn test_unicode_boundary_values() {
    // spec §6.1: `\u0000` gives a NUL byte (`dq_unicode_nul`); `\uFFFF` (`dq_unicode_bmp`).
    let config = r#"
        min_value = "\u0000"
        ascii_max = "\u007F"
        latin1_max = "\u00FF"
        bmp_max = "\uFFFF"
    "#;

    let result: Value = from_str(config).expect("Should parse Unicode boundary values");
    assert_eq!(result["min_value"], "\u{0}");
    assert_eq!(result["ascii_max"], "\u{7F}");
    assert_eq!(result["latin1_max"], "\u{FF}");
    assert_eq!(result["bmp_max"], "\u{FFFF}");
}

#[test]
fn test_surrogate_escapes_are_invalid_utf8() {
    // spec §6.1, *Quirk*: libucl encodes each surrogate on its own, which is not valid UTF-8;
    // the project rejects such a string (README, *Divergences decided by the project*).
    assert_eq!(
        error_kind(r#"emoji_pair = "\uD83D\uDE00""#),
        ErrorKind::InvalidUtf8
    );
}

#[test]
fn test_utf8_passes_through() {
    // spec §6.1 (`dq_utf8`): characters outside the BMP are written as UTF-8.
    let config = "
        grinning_face = \"\u{1F600}\"
        thumbs_up = \"\u{1F44D}\"
        \"\u{1F600}_key\" = \"emoji key\"
        emoji_array = [\"\u{1F600}\", \"\u{1F601}\", \"\u{1F602}\"]
        mixed = \"Hello \\u0041 \u{1F30D}\"
    ";

    let result: Value = from_str(config).expect("Should parse UTF-8 text");
    assert_eq!(result["grinning_face"], "\u{1F600}");
    assert_eq!(result["thumbs_up"], "\u{1F44D}");
    assert_eq!(result["\u{1F600}_key"], "emoji key");
    assert_eq!(result["emoji_array"][0], "\u{1F600}");
    assert_eq!(result["emoji_array"][1], "\u{1F601}");
    assert_eq!(result["emoji_array"][2], "\u{1F602}");
    assert_eq!(result["mixed"], "Hello A \u{1F30D}");
}

#[test]
fn test_unicode_escapes_in_keys() {
    // Escapes in double-quoted keys are decoded like those in values (oracle).
    let config = r#"
        "\u0041\u0042C" = "mixed format key"
        "prefix_\u03B1" = "greek alpha key"
    "#;

    let result: Value = from_str(config).expect("Should parse Unicode escapes in keys");
    assert_eq!(result["ABC"], "mixed format key");
    assert_eq!(result["prefix_\u{3B1}"], "greek alpha key");
}

#[test]
fn test_unicode_escapes_in_arrays() {
    let config = r#"
        mixed_array = [
            "Hello \u4E16\u754C",
            "\u0048\u0065\u006C\u006C\u006F",
            "\u03A9 World"
        ]
    "#;

    let result: Value = from_str(config).expect("Should parse Unicode escapes in arrays");
    assert_eq!(result["mixed_array"][0], "Hello \u{4E16}\u{754C}");
    assert_eq!(result["mixed_array"][1], "Hello");
    assert_eq!(result["mixed_array"][2], "\u{3A9} World");
}

#[test]
fn test_unicode_escapes_with_other_escapes() {
    let config = r#"
        combined = "Line 1\nUnicode: \u03B1\tTab\r\nLine 2"
        quotes = "Quote: \"Hello \u00E9\""
        backslash = "Path\\to\\file \u00A7"
    "#;

    let result: Value = from_str(config).expect("Should parse Unicode with other escapes");
    assert_eq!(
        result["combined"],
        "Line 1\nUnicode: \u{3B1}\tTab\r\nLine 2"
    );
    assert_eq!(result["quotes"], "Quote: \"Hello \u{E9}\"");
    assert_eq!(result["backslash"], "Path\\to\\file \u{A7}");
}

#[test]
fn test_unicode_escapes_are_not_normalized() {
    // Each escape gives its code point; nothing is composed or decomposed.
    let config = r#"
        e_acute = "\u0065\u0301"
        e_acute_precomposed = "\u00E9"
    "#;

    let result: Value = from_str(config).expect("Should decode both forms");
    assert_eq!(result["e_acute"], "e\u{301}");
    assert_eq!(result["e_acute_precomposed"], "\u{E9}");
}

#[test]
fn test_unicode_hex_digits_in_either_case() {
    // spec §6.1: exactly four hex digits, either case.
    let config = r#"
        lowercase = "\u00e9\u03b1"
        uppercase = "\u00E9\u03B1"
        mixed_case = "\u00e9\u03B1"
    "#;

    let result: Value = from_str(config).expect("Should parse hex digits in either case");
    assert_eq!(result["lowercase"], "\u{E9}\u{3B1}");
    assert_eq!(result["uppercase"], "\u{E9}\u{3B1}");
    assert_eq!(result["mixed_case"], "\u{E9}\u{3B1}");
}

#[test]
fn test_unicode_in_multiline_strings() {
    // Heredocs process no escapes (spec §6.3); a double-quoted string with `\n` does.
    let config = r#"
multiline = "First line: \u00E9\nSecond line with Greek: \u03B1\u03B2\u03B3\nThird line with Chinese: \u4E2D\u6587"
    "#;

    let result: Value = from_str(config).expect("Should parse Unicode in multiline strings");
    assert_eq!(
        result["multiline"],
        "First line: \u{E9}\nSecond line with Greek: \u{3B1}\u{3B2}\u{3B3}\nThird line with Chinese: \u{4E2D}\u{6587}"
    );
}
