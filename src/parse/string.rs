//! Quoted, single-quoted, heredoc and unquoted string values (spec §4.7, §4.8, §6).
//!
//! These functions work on bytes and return bytes; the caller expands variables and checks
//! UTF-8.

use super::error::position_at;
use super::{Error, ErrorKind};

fn error(src: &[u8], offset: usize, kind: ErrorKind) -> Error {
    Error::new(kind, position_at(src, offset))
}

/// Appends code point `cp` (at most U+FFFF) in UTF-8's three-byte-or-shorter form. A surrogate
/// is encoded on its own (spec §6.1, *Quirk*); the result is then not valid UTF-8, which the
/// caller's UTF-8 check rejects.
fn push_code_point(out: &mut Vec<u8>, cp: u32) {
    if cp < 0x80 {
        out.push(cp as u8);
    } else if cp < 0x800 {
        out.push(0xC0 | (cp >> 6) as u8);
        out.push(0x80 | (cp & 0x3F) as u8);
    } else {
        out.push(0xE0 | (cp >> 12) as u8);
        out.push(0x80 | ((cp >> 6) & 0x3F) as u8);
        out.push(0x80 | (cp & 0x3F) as u8);
    }
}

fn hex_value(b: u8) -> Option<u32> {
    (b as char).to_digit(16)
}

/// The escapes shared by double-quoted strings and unquoted values (spec §6.1, §4.7), other
/// than `\u`: the decoded byte for the byte after the backslash.
fn simple_escape(b: u8) -> u8 {
    match b {
        b'b' => 0x08,
        b'f' => 0x0C,
        b'n' => b'\n',
        b'r' => b'\r',
        b't' => b'\t',
        // `\"`, `\\`, `\/`, and any other byte: that byte, without the backslash.
        other => other,
    }
}

/// Reads the double-quoted string whose opening quote is at `src[start]` (spec §6.1). Returns
/// the decoded bytes and the offset just after the closing quote.
pub(crate) fn double_quoted(src: &[u8], start: usize) -> Result<(Vec<u8>, usize), Error> {
    let mut out = Vec::new();
    let mut i = start + 1;
    loop {
        let Some(&b) = src.get(i) else {
            return Err(error(src, start, ErrorKind::UnterminatedString));
        };
        match b {
            b'"' => return Ok((out, i + 1)),
            b'\\' => {
                let Some(&next) = src.get(i + 1) else {
                    return Err(error(src, start, ErrorKind::UnterminatedString));
                };
                if next <= 0x1E {
                    return Err(error(
                        src,
                        i + 1,
                        ErrorKind::ControlCharacter { byte: next },
                    ));
                }
                if next == b'u' {
                    let digits = src.get(i + 2..i + 6).unwrap_or(&[]);
                    let cp = (digits.len() == 4)
                        .then(|| {
                            digits
                                .iter()
                                .try_fold(0u32, |acc, &d| hex_value(d).map(|v| acc * 16 + v))
                        })
                        .flatten()
                        .ok_or_else(|| error(src, i, ErrorKind::InvalidUnicodeEscape))?;
                    push_code_point(&mut out, cp);
                    i += 6;
                } else {
                    out.push(simple_escape(next));
                    i += 2;
                }
            }
            0x00..=0x1E => return Err(error(src, i, ErrorKind::ControlCharacter { byte: b })),
            _ => {
                out.push(b);
                i += 1;
            }
        }
    }
}

/// Reads the single-quoted string whose opening quote is at `src[start]` (spec §6.2). Returns
/// the content and the offset just after the closing quote.
pub(crate) fn single_quoted(src: &[u8], start: usize) -> Result<(Vec<u8>, usize), Error> {
    let mut out = Vec::new();
    let mut i = start + 1;
    loop {
        let Some(&b) = src.get(i) else {
            return Err(error(src, start, ErrorKind::UnterminatedString));
        };
        match b {
            b'\'' => return Ok((out, i + 1)),
            b'\\' => match src.get(i + 1) {
                None => return Err(error(src, start, ErrorKind::UnterminatedString)),
                Some(b'\'') => {
                    out.push(b'\'');
                    i += 2;
                }
                // Line continuation: the backslash and the line break (LF, CR LF, or a CR that
                // no LF follows) are removed.
                Some(b'\n') => i += 2,
                Some(b'\r') if src.get(i + 2) == Some(&b'\n') => i += 3,
                Some(b'\r') => i += 2,
                Some(&other) => {
                    out.push(b'\\');
                    out.push(other);
                    i += 2;
                }
            },
            _ => {
                out.push(b);
                i += 1;
            }
        }
    }
}

/// If a heredoc opener `<<NAME` + LF starts at `src[start]`, returns NAME's range (spec §6.3).
/// NAME is zero or more ASCII uppercase letters. Fewer than four bytes from `<<` to the end of
/// the input never open a heredoc.
pub(crate) fn heredoc_opener(src: &[u8], start: usize) -> Option<(usize, usize)> {
    let rest = src.get(start..)?;
    if rest.len() < 4 || !rest.starts_with(b"<<") {
        return None;
    }
    let name_start = start + 2;
    let name_end = name_start
        + src[name_start..]
            .iter()
            .take_while(|b| b.is_ascii_uppercase())
            .count();
    (src.get(name_end) == Some(&b'\n')).then_some((name_start, name_end))
}

/// Reads the heredoc whose opener starts at `src[start]` (spec §6.3); [`heredoc_opener`] must
/// have accepted it. Returns the content and the offset just after NAME on the terminator line.
///
/// The content is every line after the opening line up to the terminator line: a line that
/// holds NAME followed by LF, `;`, `,` or the end of input. The first content line is never a
/// terminator. The line break before the terminator line is not content.
pub(crate) fn heredoc(src: &[u8], start: usize) -> Result<(Vec<u8>, usize), Error> {
    let (name_start, name_end) =
        heredoc_opener(src, start).expect("caller checked the heredoc opener");
    let name = &src[name_start..name_end];
    let content_start = name_end + 1;
    // Start of the second content line, the first that may be a terminator.
    let mut line = match src[content_start..].iter().position(|&b| b == b'\n') {
        Some(n) => content_start + n + 1,
        None => return Err(error(src, start, ErrorKind::UnterminatedHeredoc)),
    };
    loop {
        let after_name = line + name.len();
        if src[line..].starts_with(name)
            && matches!(src.get(after_name), None | Some(b'\n' | b';' | b','))
        {
            let content = src[content_start..line - 1].to_vec();
            return Ok((content, after_name));
        }
        match src[line..].iter().position(|&b| b == b'\n') {
            Some(n) => line += n + 1,
            None => return Err(error(src, start, ErrorKind::UnterminatedHeredoc)),
        }
    }
}

/// Decodes the backslash escapes of an unquoted value (spec §4.7, §4.8).
///
/// `raw` is the value as written, trailing whitespace already removed. Returns the decoded bytes
/// and whether the value as written has a `$` that is not written as `\$`: only then are
/// variables expanded, in the decoded text as a whole (spec §7.6).
///
/// `\u` in an unquoted value is never an error (spec §4.8). With four or more bytes after it,
/// the escape covers four: all hex gives that code point, otherwise 16 × the value of the hex
/// digits before the first non-hex byte, or 0 if there are none. With three, the same, the end
/// of the value counting as a fourth, non-hex byte. With two or fewer (**quirk**), the backslash
/// is dropped, the `u` is kept and the byte after it is dropped.
///
/// The same decoding applies to a quoted key under `KEY_LOWERCASE`, after the key has been
/// lowercased as written (spec §12.1).
pub(crate) fn decode_unquoted(raw: &[u8]) -> (Vec<u8>, bool) {
    let mut out = Vec::with_capacity(raw.len());
    let mut escaped_dollars = 0;
    let mut i = 0;
    while i < raw.len() {
        let b = raw[i];
        if b != b'\\' {
            out.push(b);
            i += 1;
            continue;
        }
        let Some(&next) = raw.get(i + 1) else {
            // A backslash as the last byte of the value is kept.
            out.push(b'\\');
            break;
        };
        if next == b'u' {
            let available = raw.len() - (i + 2);
            if available >= 3 {
                let digits = &raw[i + 2..i + 2 + available.min(4)];
                let hex_prefix = digits.iter().take_while(|d| d.is_ascii_hexdigit()).count();
                let value = digits[..hex_prefix]
                    .iter()
                    .fold(0u32, |acc, &d| acc * 16 + hex_value(d).unwrap_or(0));
                let cp = if hex_prefix == 4 { value } else { value * 16 };
                push_code_point(&mut out, cp);
                i += 2 + digits.len();
            } else {
                out.push(b'u');
                i += 2 + available.min(1);
            }
        } else {
            if next == b'$' {
                escaped_dollars += 1;
            }
            out.push(simple_escape(next));
            i += 2;
        }
    }
    let dollars = raw.iter().filter(|&&b| b == b'$').count();
    (out, dollars > escaped_dollars)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dq(s: &str) -> Result<String, ErrorKind> {
        double_quoted(s.as_bytes(), 0)
            .map(|(b, _)| String::from_utf8_lossy(&b).into_owned())
            .map_err(|e| e.kind().clone())
    }

    #[test]
    fn double_quoted_escapes() {
        assert_eq!(dq(r#""a\"b\\c\/d""#).unwrap(), "a\"b\\c/d");
        assert_eq!(dq(r#""\b\f\n\r\t""#).unwrap(), "\u{8}\u{c}\n\r\t");
        assert_eq!(dq(r#""\u0041\u00e9\u20AC""#).unwrap(), "Aé€");
        assert_eq!(dq(r#""\u1F640""#).unwrap(), "\u{1F64}0");
        assert_eq!(dq(r#""\q\$x""#).unwrap(), "q$x");
        assert_eq!(dq("\"\x1f\x7f\"").unwrap(), "\x1f\x7f");
    }

    #[test]
    fn double_quoted_errors() {
        assert_eq!(dq(r#""\uZZZZ""#), Err(ErrorKind::InvalidUnicodeEscape));
        assert_eq!(dq(r#""\u12""#), Err(ErrorKind::InvalidUnicodeEscape));
        assert_eq!(
            dq("\"a\tb\""),
            Err(ErrorKind::ControlCharacter { byte: b'\t' })
        );
        assert_eq!(
            dq("\"a\\\nb\""),
            Err(ErrorKind::ControlCharacter { byte: b'\n' })
        );
        assert_eq!(dq("\"abc"), Err(ErrorKind::UnterminatedString));
        assert_eq!(dq("\"abc\\"), Err(ErrorKind::UnterminatedString));
    }

    #[test]
    fn surrogate_escape_is_not_utf8() {
        let (bytes, _) = double_quoted(br#""\uD83D""#, 0).unwrap();
        assert!(std::str::from_utf8(&bytes).is_err());
    }

    #[test]
    fn single_quoted_rules() {
        let sq = |s: &str| {
            single_quoted(s.as_bytes(), 0)
                .map(|(b, _)| String::from_utf8(b).unwrap())
                .map_err(|e| e.kind().clone())
        };
        assert_eq!(sq(r"'it\'s'").unwrap(), "it's");
        assert_eq!(sq("'one\\\ntwo'").unwrap(), "onetwo");
        assert_eq!(sq("'one\\\r\ntwo'").unwrap(), "onetwo");
        // A lone CR after a backslash is a line break too (spec-v7 §6.2); a CR after it stays.
        assert_eq!(sq("'x\\\ry'").unwrap(), "xy");
        assert_eq!(sq("'x\\\r\ry'").unwrap(), "x\ry");
        assert_eq!(sq("'x\\\r\r\ny'").unwrap(), "x\r\ny");
        assert_eq!(sq("'x\\\r'").unwrap(), "x");
        // Backslashes pair from the left.
        assert_eq!(sq("'x\\\\\ry'").unwrap(), "x\\\\\ry");
        assert_eq!(sq("'x\\\\\\\ry'").unwrap(), "x\\\\y");
        assert_eq!(sq(r"'x\ny\\z'").unwrap(), r"x\ny\\z");
        assert_eq!(sq("'x\ny'").unwrap(), "x\ny");
        assert_eq!(sq("'x"), Err(ErrorKind::UnterminatedString));
        assert_eq!(sq("'x\\"), Err(ErrorKind::UnterminatedString));
    }

    fn hd(s: &str) -> Result<(String, usize), ErrorKind> {
        heredoc(s.as_bytes(), 0)
            .map(|(b, end)| (String::from_utf8(b).unwrap(), end))
            .map_err(|e| e.kind().clone())
    }

    #[test]
    fn heredoc_rules() {
        assert_eq!(hd("<<EOD\na\nb\nEOD\n").unwrap(), ("a\nb".into(), 13));
        assert_eq!(hd("<<EOD\nEOD\nx\nEOD\n").unwrap().0, "EOD\nx");
        assert_eq!(hd("<<EOD\n\nEOD\n").unwrap().0, "");
        assert_eq!(hd("<<EOD\nx\nEOD").unwrap().0, "x");
        assert_eq!(hd("<<EOD\nx\nEOD;").unwrap(), ("x".into(), 11));
        assert_eq!(hd("<<\nx\n\n").unwrap().0, "x");
        assert_eq!(hd("<<EOD\nx\r\nEOD\n").unwrap().0, "x\r");
        assert_eq!(hd("<<EOD\n EOD\nEODX\nEOD\n").unwrap().0, " EOD\nEODX");
        assert_eq!(hd("<<EOD\nEOD\n"), Err(ErrorKind::UnterminatedHeredoc));
        assert_eq!(hd("<<EOD\nx\nEOD \n"), Err(ErrorKind::UnterminatedHeredoc));
        assert_eq!(hd("<<EOD\nx\nEOD}\n"), Err(ErrorKind::UnterminatedHeredoc));
    }

    #[test]
    fn heredoc_openers() {
        assert!(heredoc_opener(b"<<EOD\n", 0).is_some());
        assert!(heredoc_opener(b"<<\nx", 0).is_some());
        assert!(heredoc_opener(b"<<\n", 0).is_none());
        assert!(heredoc_opener(b"<<eod\n", 0).is_none());
        assert!(heredoc_opener(b"<<EOD \n", 0).is_none());
        assert!(heredoc_opener(b"<<EOD\r\n", 0).is_none());
    }

    fn unq(s: &str) -> String {
        String::from_utf8(decode_unquoted(s.as_bytes()).0).unwrap()
    }

    #[test]
    fn unquoted_escapes() {
        assert_eq!(unq(r"x\;y\#z\,w"), "x;y#z,w");
        assert_eq!(unq(r#"x\ty\"z"#), "x\ty\"z");
        assert_eq!(unq(r"a\u0041b"), "aAb");
        assert_eq!(unq(r"x\q"), "xq");
        assert_eq!(unq(r"ends\"), r"ends\");
        assert_eq!(unq("x\\\ny"), "x\ny");
    }

    #[test]
    fn unquoted_invalid_unicode_quirk() {
        assert_eq!(unq(r"x\uZZZZ"), "x\u{0}");
        assert_eq!(unq(r"x\u00ZZ"), "x\u{0}");
        assert_eq!(unq(r"x\u4Z00y"), "x@y");
        assert_eq!(unq(r"x\u1ZZZ"), "x\u{10}");
        assert_eq!(unq(r"x\u12ZZ"), "x\u{120}");
        assert_eq!(unq(r"x\u123Z"), "x\u{1230}");
    }

    #[test]
    fn unquoted_short_unicode_escapes() {
        // spec §4.8: three bytes left form an escape; two or fewer drop the next byte.
        assert_eq!(unq(r"x\u123"), "x\u{1230}");
        assert_eq!(unq(r"x\u1Z2"), "x\u{10}");
        assert_eq!(unq(r"x\u12\"), "x\u{120}");
        assert_eq!(unq(r"x\u12"), "xu2");
        assert_eq!(unq(r"x\u41"), "xu1");
        assert_eq!(unq(r"x\u1"), "xu");
        assert_eq!(unq(r"x\uZ"), "xu");
        assert_eq!(unq(r"x\u"), "xu");
        assert_eq!(unq(r"a\u\n"), "aun");
        assert_eq!(unq(r"x\u1\"), "xu\\");
    }

    #[test]
    fn unquoted_dollars_decide_expansion() {
        // spec §7.6: expansion only if some `$` is not written as `\$`.
        let expands = |s: &str| decode_unquoted(s.as_bytes()).1;
        assert_eq!(decode_unquoted(br"\$A$B").0, b"$A$B");
        assert!(expands(r"\$A$B"));
        assert!(!expands(r"\$A\$B"));
        assert!(!expands(r"\\\$ABI"));
        assert!(expands(r"\\$ABI"));
        assert!(expands(r"\$ABI\u$000"));
        assert!(!expands("plain"));
    }
}
