//! Tests for error kinds and positions of the parser core.
//!
//! Which inputs are errors follows libucl (spec §6, §11; each input was checked with the
//! oracle). Positions are the crate's own: 1-based lines and columns in characters, and a
//! 0-based byte offset.

#[cfg(test)]
mod tests {
    use crate::parse::{ErrorKind, parse};
    use crate::value::UclValue;

    /// The kind and the line and column of the error `input` gives.
    fn error(input: &[u8]) -> (ErrorKind, usize, usize) {
        let e = parse(input).expect_err("an error");
        (e.kind().clone(), e.position().line, e.position().column)
    }

    /// The value of key `k` in the document `input`.
    fn value_of_k(input: &[u8]) -> UclValue {
        parse(input).expect("parses").as_object().unwrap()["k"].clone()
    }

    #[test]
    fn test_valid_utf8_passes_through() {
        assert_eq!(
            value_of_k("k = \"héllo wörld 🌍\"".as_bytes()),
            UclValue::String("héllo wörld 🌍".into())
        );
    }

    #[test]
    fn test_unicode_escapes() {
        // spec §6.1: `\uXXXX` gives the code point, `\u0000` a NUL byte.
        assert_eq!(
            value_of_k(br#"k = "hello A world""#),
            UclValue::String("hello A world".into())
        );
        assert_eq!(
            value_of_k(br#"k = "\u0000\u007F\u0080\uFFFF""#),
            UclValue::String("\u{0}\u{7F}\u{80}\u{FFFF}".into())
        );
        // A surrogate code point is encoded on its own, which is not UTF-8: an error in the
        // project (spec §6.1, *Quirk*; README, *Divergences decided by the project*).
        for input in [&br#"k = "hello \uD800 world""#[..], br#"k = "\uDFFF""#] {
            assert_eq!(error(input).0, ErrorKind::InvalidUtf8);
        }
    }

    #[test]
    fn test_control_character_handling() {
        // spec §6.1: a raw byte from 0x00 to 0x1E in double quotes is an error, TAB included;
        // 0x1F and DEL are allowed (a quirk).
        for byte in (0x00u8..=0x1E).filter(|b| *b != b'\n') {
            let input = [&b"k = \"hello"[..], &[byte], b"world\""].concat();
            assert_eq!(
                error(&input),
                (ErrorKind::ControlCharacter { byte }, 1, 11),
                "byte {byte:#04x}"
            );
        }
        // A raw line break is one too; it is reported where it is.
        assert_eq!(
            error(b"k = \"hello\nworld\"").0,
            ErrorKind::ControlCharacter { byte: b'\n' }
        );
        for byte in [0x1Fu8, 0x7F] {
            let input = [&b"k = \"hello"[..], &[byte], b"world\""].concat();
            let expected = format!("hello{}world", byte as char);
            assert_eq!(value_of_k(&input), UclValue::String(expected));
        }
    }

    #[test]
    fn test_unknown_escape_drops_the_backslash() {
        // spec §6.1: `\` before any other byte gives that byte; it is not an error.
        assert_eq!(
            value_of_k(br#"k = "hello \q world""#),
            UclValue::String("hello q world".into())
        );
    }

    #[test]
    fn test_malformed_numbers_are_strings() {
        // spec §5.5, §5.8: a malformed number is an unquoted string, and so is a run of digits
        // too long for any number type.
        for text in ["123e", "0x", "1e+"] {
            let input = format!("k = {text}");
            assert_eq!(value_of_k(input.as_bytes()), UclValue::String(text.into()));
        }
        let long = "1".repeat(2000);
        assert_eq!(
            value_of_k(format!("k = {long}").as_bytes()),
            UclValue::String(long)
        );
    }

    #[test]
    fn test_malformed_input_detection() {
        // A key must begin with a letter, digit, `_`, `/`, a quote or the like (spec §3.1).
        assert_eq!(
            error(b"@"),
            (ErrorKind::InvalidKey { found: Some('@') }, 1, 1)
        );
        // As a value, `@` is an unquoted string (spec §4.1).
        assert_eq!(value_of_k(b"k = @"), UclValue::String("@".into()));
        // The opening quote of an unterminated string.
        assert_eq!(
            error(b"k = \"unterminated"),
            (ErrorKind::UnterminatedString, 1, 5)
        );
        assert_eq!(
            error(b"a = 1\n/* unterminated comment").0,
            ErrorKind::UnterminatedComment
        );
    }

    #[test]
    fn test_malformed_heredoc() {
        // An empty NAME is allowed: after the first content line, the heredoc ends at the first
        // LF, `;` or `,`, and without one it is unterminated (spec §6.3, *Quirk*; QUESTIONS.md
        // #55).
        assert_eq!(
            value_of_k(b"k = <<\ncontent\n\n"),
            UclValue::String("content".into())
        );
        assert_eq!(
            error(b"k = <<\ncontent\n").0,
            ErrorKind::UnterminatedHeredoc
        );
        assert_eq!(error(b"[<<\nx\n]").0, ErrorKind::UnterminatedHeredoc);
        // NAME is uppercase letters only (spec §6.3): `<<term123` is an unquoted value, and the
        // line `term123` a key without a value.
        assert!(parse(b"k = <<term123\ncontent\nterm123").is_err());
        assert_eq!(
            value_of_k(b"k = <<term123\n"),
            UclValue::String("<<term123".into())
        );
    }
}
