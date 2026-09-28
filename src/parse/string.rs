//! Quoted, single-quoted, heredoc and unquoted string values (spec §4.7, §4.8, §6).
//!
//! These functions work on bytes and return bytes; the caller expands variables and checks
//! UTF-8.

use super::error::position_at;
use super::{Error, ErrorKind};

#[cold]
#[inline(never)]
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

/// The classes of bytes in a double-quoted string ([`DOUBLE_QUOTED`]).
const STOP: u8 = 1;
const NEEDS_QUOTING: u8 = 2;
const DOLLAR: u8 = 4;

/// For each byte, its classes in a double-quoted string: [`STOP`] for a byte that ends a run of
/// plain bytes (`"`, `\\` and the control bytes up to 0x1E), [`NEEDS_QUOTING`] for a byte that
/// makes a key need quoting, and [`DOLLAR`] for `$`. One lookup tests a byte for every class, the
/// idea of logos's byte-class tables (`logos-codegen`, `src/generator/tables.rs`); clean-room
/// work item C11.
static DOUBLE_QUOTED: [u8; 256] = {
    let mut table = [0u8; 256];
    let mut b = 0;
    while b < 256 {
        let byte = b as u8;
        let mut class = 0;
        if byte == b'"' || byte == b'\\' || byte <= 0x1E {
            class |= STOP;
        }
        if crate::emit::byte_needs_quoting(byte) {
            class |= NEEDS_QUOTING;
        }
        if byte == b'$' {
            class |= DOLLAR;
        }
        table[b] = class;
        b += 1;
    }
    table
};

/// A double-quoted string read by [`double_quoted_scan`].
#[derive(Debug)]
pub(crate) struct DoubleQuoted {
    /// The decoded bytes of a string with a backslash escape; `None` for a string without one,
    /// whose bytes are those between its quotes, `src[start + 1..end - 1]`, not copied yet.
    pub(crate) bytes: Option<Vec<u8>>,
    /// The offset just after the closing quote.
    pub(crate) end: usize,
    /// Outside its escapes, the string has a byte that makes a key need quoting
    /// ([`crate::emit::key_needs_quoting`]). Without escapes, that is whether the decoded bytes
    /// need quoting as a key.
    pub(crate) needs_quoting: bool,
    /// Outside its escapes, the string has a `$`.
    pub(crate) dollar: bool,
}

/// Reads the double-quoted string whose opening quote is at `src[start]` (spec §6.1). Returns
/// the decoded bytes and the offset just after the closing quote.
pub(crate) fn double_quoted(src: &[u8], start: usize) -> Result<(Vec<u8>, usize), Error> {
    let read = double_quoted_scan(src, start)?;
    let bytes = read
        .bytes
        .unwrap_or_else(|| src[start + 1..read.end - 1].to_vec());
    Ok((bytes, read.end))
}

impl DoubleQuoted {
    /// The string has a backslash escape.
    pub(crate) fn escaped(&self) -> bool {
        self.bytes.is_some()
    }
}

/// [`double_quoted`], with what the reading saw on the way, so that the caller does not read the
/// string again for it.
///
/// Bytes that need no decoding are copied a run at a time, up to the next `"`, `\\` or control
/// byte (the idea of `serde_json`'s `SliceRead::parse_str_bytes`, `src/read.rs`, and of
/// `toml_edit`'s `basic_chars`, `src/parser/strings.rs`; clean-room work item C11).
///
/// The bytes are copied only from the first escape on: a string without one is left in the input
/// for the caller to take as it is.
pub(crate) fn double_quoted_scan(src: &[u8], start: usize) -> Result<DoubleQuoted, Error> {
    let content = start + 1;
    // The decoded bytes, from the first escape on.
    let mut decoded: Option<Vec<u8>> = None;
    let mut i = content;
    let mut seen = 0u8;
    loop {
        let mut run = i;
        while let Some(&b) = src.get(run) {
            let class = DOUBLE_QUOTED[usize::from(b)];
            if class & STOP != 0 {
                break;
            }
            seen |= class;
            run += 1;
        }
        if let Some(out) = &mut decoded {
            out.extend_from_slice(&src[i..run]);
        }
        i = run;
        let Some(&b) = src.get(i) else {
            return Err(error(src, start, ErrorKind::UnterminatedString));
        };
        match b {
            b'"' => {
                return Ok(DoubleQuoted {
                    bytes: decoded,
                    end: i + 1,
                    needs_quoting: seen & NEEDS_QUOTING != 0,
                    dollar: seen & DOLLAR != 0,
                });
            }
            b'\\' => {
                // Until the first escape, the plain bytes were left in the input.
                let out = decoded.get_or_insert_with(|| src[content..i].to_vec());
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
                    push_code_point(out, cp);
                    i += 6;
                } else {
                    out.push(simple_escape(next));
                    i += 2;
                }
            }
            _ => return Err(error(src, i, ErrorKind::ControlCharacter { byte: b })),
        }
    }
}

/// Reads the single-quoted string whose opening quote is at `src[start]` (spec §6.2). Returns
/// the content and the offset just after the closing quote.
///
/// Bytes other than `'` and `\\` are copied a run at a time, as in [`double_quoted`].
#[cfg(test)]
pub(crate) fn single_quoted(src: &[u8], start: usize) -> Result<(Vec<u8>, usize), Error> {
    let (bytes, end) = single_quoted_scan(src, start)?;
    Ok((
        bytes.unwrap_or_else(|| src[start + 1..end - 1].to_vec()),
        end,
    ))
}

/// [`single_quoted`], without a copy for a string that has no backslash: its bytes are then
/// `src[start + 1..end - 1]`, and `None` is returned for them.
pub(crate) fn single_quoted_scan(
    src: &[u8],
    start: usize,
) -> Result<(Option<Vec<u8>>, usize), Error> {
    let content = start + 1;
    let close = src[content..]
        .iter()
        .position(|&b| b == b'\'' || b == b'\\')
        .map(|n| content + n);
    if let Some(close) = close
        && src[close] == b'\''
    {
        return Ok((None, close + 1));
    }
    let mut out = Vec::new();
    let mut i = start + 1;
    loop {
        let run = src[i..]
            .iter()
            .position(|&b| b == b'\'' || b == b'\\')
            .map_or(src.len(), |n| i + n);
        out.extend_from_slice(&src[i..run]);
        i = run;
        let Some(&b) = src.get(i) else {
            return Err(error(src, start, ErrorKind::UnterminatedString));
        };
        if b == b'\'' {
            return Ok((Some(out), i + 1));
        }
        // A backslash.
        match src.get(i + 1) {
            None => return Err(error(src, start, ErrorKind::UnterminatedString)),
            Some(b'\'') => {
                out.push(b'\'');
                i += 2;
            }
            // Line continuation: the backslash and the line break (LF, CR LF, or a CR that no LF
            // follows) are removed.
            Some(b'\n') => i += 2,
            Some(b'\r') if src.get(i + 2) == Some(&b'\n') => i += 3,
            Some(b'\r') => i += 2,
            Some(&other) => {
                out.push(b'\\');
                out.push(other);
                i += 2;
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

/// Whether the text at `src[start]` is a heredoc opener that the end of its unit cuts short
/// (spec §6.3, *Quirk*): four or more bytes from `<<` to the end, all of them after `<<`
/// uppercase ASCII letters. That is an error; any other byte before the end makes the text an
/// ordinary unquoted value.
pub(crate) fn heredoc_opener_cut_by_end(src: &[u8], start: usize) -> bool {
    src.get(start..).is_some_and(|rest| {
        rest.len() >= 4 && rest.starts_with(b"<<") && rest[2..].iter().all(u8::is_ascii_uppercase)
    })
}

/// A heredoc read by [`heredoc`].
#[derive(Debug)]
pub(crate) struct Heredoc {
    /// The content: `src[content]`, as written.
    pub(crate) content: std::ops::Range<usize>,
    /// The offset where the input continues after the heredoc.
    pub(crate) end: usize,
    /// Whether variables are expanded in the content (spec §7.2): always when NAME is not
    /// empty; with an empty NAME only when the first content line has a `$`.
    pub(crate) expand: bool,
}

/// Whether a line of a heredoc that ends with NAME at `src[at]` is a terminator line: NAME is
/// followed by LF, `;`, `,` or the end of input (spec §6.3).
fn ends_terminator(src: &[u8], at: usize) -> bool {
    matches!(src.get(at), None | Some(b'\n' | b';' | b','))
}

/// Reads the heredoc whose opener starts at `src[start]` (spec §6.3); [`heredoc_opener`] must
/// have accepted it.
///
/// The content is every line after the opening line up to the terminator line: a line that
/// holds NAME followed by LF, `;`, `,` or the end of input. The first content line is never a
/// terminator. The line break before the terminator line is not content, and the input
/// continues just after NAME on the terminator line.
///
/// **Quirk.** A NAME of one letter repeated n times is also ended by a line of m > n of that
/// letter followed by LF, `;`, `,` or the end of input; the line break before that line and
/// m − n − 1 of the letters stay in the content, and the input continues after the m letters.
///
/// An empty NAME has an end rule of its own, [`empty_name_heredoc`].
pub(crate) fn heredoc(src: &[u8], start: usize) -> Result<Heredoc, Error> {
    let (name_start, name_end) =
        heredoc_opener(src, start).expect("caller checked the heredoc opener");
    let name = &src[name_start..name_end];
    let content_start = name_end + 1;
    if name.is_empty() {
        return empty_name_heredoc(src, start, content_start);
    }
    let repeated = name.iter().all(|&b| b == name[0]);
    // Start of the second content line, the first that may be a terminator.
    let mut line = match src[content_start..].iter().position(|&b| b == b'\n') {
        Some(n) => content_start + n + 1,
        None => return Err(error(src, start, ErrorKind::UnterminatedHeredoc)),
    };
    loop {
        let after_name = line + name.len();
        if src[line..].starts_with(name) && ends_terminator(src, after_name) {
            return Ok(Heredoc {
                content: content_start..line - 1,
                end: after_name,
                expand: true,
            });
        }
        if repeated {
            let letters = src[line..].iter().take_while(|&&b| b == name[0]).count();
            if letters > name.len() && ends_terminator(src, line + letters) {
                let kept = letters - name.len() - 1;
                return Ok(Heredoc {
                    content: content_start..line + kept,
                    end: line + letters,
                    expand: true,
                });
            }
        }
        match src[line..].iter().position(|&b| b == b'\n') {
            Some(n) => line += n + 1,
            None => return Err(error(src, start, ErrorKind::UnterminatedHeredoc)),
        }
    }
}

/// A heredoc with an empty NAME (spec §6.3, *Quirk*), whose content starts at
/// `src[content_start]`. The first content line and its LF are content. After that LF the
/// heredoc ends at the first LF, `;` or `,`: the content is the text up to that byte without
/// its last byte, and the input continues at the byte itself. Only a `$` in the first content
/// line turns on variable expansion.
fn empty_name_heredoc(src: &[u8], start: usize, content_start: usize) -> Result<Heredoc, Error> {
    let unterminated = || error(src, start, ErrorKind::UnterminatedHeredoc);
    let first_end = src[content_start..]
        .iter()
        .position(|&b| b == b'\n')
        .map(|n| content_start + n)
        .ok_or_else(unterminated)?;
    let end = src[first_end + 1..]
        .iter()
        .position(|&b| matches!(b, b'\n' | b';' | b','))
        .map(|n| first_end + 1 + n)
        .ok_or_else(unterminated)?;
    Ok(Heredoc {
        content: content_start..end - 1,
        end,
        expand: src[content_start..first_end].contains(&b'$'),
    })
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
    let mut i = 0;
    while i < raw.len() {
        // The bytes up to the next backslash are copied as a run.
        let run = raw[i..]
            .iter()
            .position(|&b| b == b'\\')
            .map_or(raw.len(), |n| i + n);
        out.extend_from_slice(&raw[i..run]);
        i = run;
        if i == raw.len() {
            break;
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
            out.push(simple_escape(next));
            i += 2;
        }
    }
    (out, has_unescaped_dollar(raw))
}

/// Whether `raw`, an unquoted value as written, has a `$` that is not written as `\$` (spec
/// §7.6). Backslashes pair from the left, each with the byte after it, whatever a `\u` escape
/// then makes of those bytes: in `\u\$` the `$` is written as `\$`, although decoding drops the
/// backslash as the byte after `\u` (§4.8; oracle runs, C8c).
fn has_unescaped_dollar(raw: &[u8]) -> bool {
    let mut i = 0;
    while i < raw.len() {
        match raw[i] {
            b'\\' => i += 2,
            b'$' => return true,
            _ => i += 1,
        }
    }
    false
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
    fn double_quoted_byte_classes() {
        for b in 0..=255u8 {
            let class = DOUBLE_QUOTED[usize::from(b)];
            assert_eq!(
                class & STOP != 0,
                b == b'"' || b == b'\\' || b <= 0x1E,
                "{b:#x}"
            );
            assert_eq!(class & DOLLAR != 0, b == b'$', "{b:#x}");
            if b.is_ascii() {
                let key = char::from(b).to_string();
                let needs = crate::emit::key_needs_quoting(&key);
                assert_eq!(class & NEEDS_QUOTING != 0, needs, "{b:#x}");
            } else {
                assert_eq!(class & NEEDS_QUOTING, 0, "{b:#x}");
            }
        }
        let read = double_quoted_scan(br#""a b$\t""#, 0).unwrap();
        assert!(read.escaped() && read.needs_quoting && read.dollar);
        assert_eq!(read.bytes.as_deref(), Some(&b"a b$\t"[..]));
        let read = double_quoted_scan(br#""ab""#, 0).unwrap();
        assert!(!read.escaped() && !read.needs_quoting && !read.dollar);
        assert_eq!(read.bytes, None);
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
            .map(|h| (s[h.content].to_owned(), h.end))
            .map_err(|e| e.kind().clone())
    }

    #[test]
    fn heredoc_repeated_letter_name() {
        // spec §6.3, *Quirk*: a longer line of NAME's one letter ends the heredoc.
        assert_eq!(hd("<<A\nx\nAA\n").unwrap(), ("x\n".into(), 8));
        assert_eq!(hd("<<A\nx\nAAA\n").unwrap(), ("x\nA".into(), 9));
        assert_eq!(hd("<<EE\nx\nEEE\n").unwrap().0, "x\n");
        assert_eq!(hd("<<AA\nx\nAAAAA\n").unwrap().0, "x\nAA");
        assert_eq!(hd("<<A\nx\nAA;").unwrap(), ("x\n".into(), 8));
        assert_eq!(hd("<<A\nx\nAA").unwrap().0, "x\n");
        assert_eq!(hd("<<A\nx\nAAB\nA\n").unwrap().0, "x\nAAB");
        // The first content line is still content; fewer letters are content; the quirk needs
        // one repeated letter.
        assert_eq!(hd("<<A\nAA\nx\nA\n").unwrap().0, "AA\nx");
        assert_eq!(hd("<<AAA\nx\nA\nAAA\n").unwrap().0, "x\nA");
        assert_eq!(hd("<<AB\nx\nABB\nAB\n").unwrap().0, "x\nABB");
        assert_eq!(hd("<<A\nx\nAA}\n"), Err(ErrorKind::UnterminatedHeredoc));
    }

    #[test]
    fn heredoc_empty_name() {
        // spec §6.3, *Quirk*: after the first content line, the first LF, `;` or `,` ends the
        // heredoc; its last byte is dropped and the byte itself is read next.
        assert_eq!(hd("<<\nx\n\n").unwrap(), ("x".into(), 5));
        assert_eq!(hd("<<\n\n\n").unwrap(), ("".into(), 4));
        assert_eq!(hd("<<\nx\n;").unwrap(), ("x".into(), 5));
        assert_eq!(hd("<<\na\nb\n").unwrap(), ("a\n".into(), 6));
        assert_eq!(hd("<<\nab\ncd\n").unwrap().0, "ab\nc");
        assert_eq!(hd("<<\nx\ny;\n").unwrap(), ("x\n".into(), 6));
        assert_eq!(hd("<<\nx\ny,1").unwrap(), ("x\n".into(), 6));
        assert_eq!(hd("<<\n;x\n\n").unwrap().0, ";x");
        assert_eq!(hd("<<\nx;\n\n").unwrap().0, "x;");
        assert_eq!(hd("<<\nx\ny}\n").unwrap().0, "x\ny");
        for unterminated in ["<<\ncontent\n", "<<\nx\ny", "<<\n\n", "<<\nx"] {
            assert_eq!(
                hd(unterminated),
                Err(ErrorKind::UnterminatedHeredoc),
                "{unterminated:?}"
            );
        }
        // Only a `$` in the first content line turns on variable expansion (§7.2).
        let expand = |s: &str| heredoc(s.as_bytes(), 0).unwrap().expand;
        assert!(!expand("<<\na\n${ABI}x\n"));
        assert!(expand("<<\n$ABI\n${ABI}x\n"));
        assert!(expand("<<EOD\na\nEOD\n"));
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

    #[test]
    fn heredoc_openers_cut_by_the_end() {
        // spec §6.3, *Quirk*: four or more bytes, uppercase letters only after `<<`.
        for cut in [&b"<<EO"[..], b"<<AA", b"<<EOD", b"k = <<ABCDEF"] {
            let start = cut.windows(2).position(|w| w == b"<<").unwrap();
            assert!(heredoc_opener_cut_by_end(cut, start), "{cut:?}");
        }
        for not in [
            &b"<<E"[..],
            b"<<",
            b"<<EO ",
            b"<<EO;",
            b"<<AB1",
            b"<<Ab",
            b"<<ab",
            b"<<EOD]",
            b"<<EOD\n",
        ] {
            assert!(!heredoc_opener_cut_by_end(not, 0), "{not:?}");
        }
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
        // Written as `\$` even where a `\u` escape takes the backslash (oracle runs, C8c).
        assert!(!expands(r"\$ABI\u\$"));
        assert_eq!(decode_unquoted(br"\$ABI\u\$").0, b"$ABIu$");
        assert!(!expands(r"\$ABI\u1\$"));
        assert!(!expands(r"\u\$ABI"));
        assert!(expands(r"\$ABI\u\\$"));
    }
}
