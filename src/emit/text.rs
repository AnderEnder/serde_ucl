//! Strings and keys in the output formats (spec §10.1, §10.2, §10.5), and the forms of round-trip
//! mode (§10.8).

use std::fmt::Write;

/// Writes `s` in the JSON form of spec §10.2: in double quotes, with `"`, `\`, LF, CR, TAB, BS
/// and FF escaped, VT written `\u000B`, and every other byte 0x00–0x1F and DEL written `\uFFFD`
/// (the original byte is lost, a quirk of libucl kept here). All other bytes are written as they
/// are.
pub(crate) fn write_json_string(out: &mut String, s: &str) {
    out.push('"');
    let mut plain = 0;
    for (i, b) in s.bytes().enumerate() {
        let escape = match b {
            b'"' => "\\\"",
            b'\\' => "\\\\",
            b'\n' => "\\n",
            b'\r' => "\\r",
            b'\t' => "\\t",
            0x08 => "\\b",
            0x0C => "\\f",
            0x0B => "\\u000B",
            0x00..=0x1F | 0x7F => "\\uFFFD",
            _ => continue,
        };
        out.push_str(&s[plain..i]);
        out.push_str(escape);
        plain = i + 1;
    }
    out.push_str(&s[plain..]);
    out.push('"');
}

/// Whether a key without recorded facts needs quoting (spec §10.1, fact 3, as it applies to keys
/// that `.load` creates): it contains a space, TAB, LF, CR, FF, BS, NUL, `"`, `+`, `:`, `=`, `[`,
/// `\` or `{`.
///
/// Keys that this rule leaves bare are not always readable bare: keys with `;`, `}`, `#` or `,`,
/// or ones that start with a byte a bare key cannot start with (spec §10.8).
pub fn key_needs_quoting(key: &str) -> bool {
    key.bytes().any(|b| {
        matches!(
            b,
            b' ' | b'\t'
                | b'\n'
                | b'\r'
                | 0x0C
                | 0x08
                | 0
                | b'"'
                | b'+'
                | b':'
                | b'='
                | b'['
                | b'\\'
                | b'{'
        )
    })
}

/// Writes a key of the config or YAML format: bare, or in the JSON form when it needs quoting.
pub(crate) fn write_key(out: &mut String, key: &str, quoted: bool) {
    if quoted {
        write_json_string(out, key);
    } else {
        out.push_str(key);
    }
}

/// Writes a string of the config format (spec §10.5): the heredoc form, the single-quoted form,
/// or the JSON form.
pub(crate) fn write_config_string(out: &mut String, s: &str, single_quoted: bool, multiline: bool) {
    let has_lf = s.contains('\n');
    if has_lf && (s.len() > 80 || multiline) && !has_eod_line(s) {
        out.push_str("<<EOD\n");
        out.push_str(s);
        out.push_str("\nEOD");
    } else if single_quoted && !s.contains("\\'") {
        out.push('\'');
        let mut plain = 0;
        for (i, b) in s.bytes().enumerate() {
            if b == b'\'' {
                out.push_str(&s[plain..i]);
                out.push_str("\\'");
                plain = i + 1;
            }
        }
        out.push_str(&s[plain..]);
        out.push('\'');
    } else {
        write_json_string(out, s);
    }
}

/// Round-trip mode: writes `s` double-quoted with the escapes of spec §6.1, which read back as
/// exactly `s`: `\"`, `\\`, `\n`, `\r`, `\t`, `\b`, `\f`, and `\u00XX` for every other byte below
/// 0x20 and for DEL. Everything else is written as it is. The result is also a JSON string.
///
/// A double-quoted string expands variable references when read (§7.2), so it reads back as `s`
/// only if it contains no reference to a variable registered when reading (§10.8).
pub(crate) fn write_escaped_string(out: &mut String, s: &str) {
    out.push('"');
    let mut plain = 0;
    for (i, b) in s.bytes().enumerate() {
        let escape = match b {
            b'"' => "\\\"",
            b'\\' => "\\\\",
            b'\n' => "\\n",
            b'\r' => "\\r",
            b'\t' => "\\t",
            0x08 => "\\b",
            0x0C => "\\f",
            0x00..=0x1F | 0x7F => {
                out.push_str(&s[plain..i]);
                let _ = write!(out, "\\u{b:04X}");
                plain = i + 1;
                continue;
            }
            _ => continue,
        };
        out.push_str(&s[plain..i]);
        out.push_str(escape);
        plain = i + 1;
    }
    out.push_str(&s[plain..]);
    out.push('"');
}

/// The variables that readers define by default: libucl and [`crate::parse`] define `FILENAME`
/// and `CURDIR` for every document unless `no-filevars` is set (spec §7.8).
const FILE_VARIABLES: [&str; 2] = ["FILENAME", "CURDIR"];

/// Whether `s` refers to a file variable (spec §7.3, §7.4): it contains `$FILENAME`, `$CURDIR`,
/// `${FILENAME}` or `${CURDIR}`. An unbraced reference also matches when more text follows the
/// name, as in `$CURDIRx`. Conservative: `$$` before the name, which can keep the text as written
/// (§7.5), is not taken into account.
pub(crate) fn refers_to_file_variable(s: &str) -> bool {
    FILE_VARIABLES.iter().any(|name| {
        s.match_indices('$').any(|(i, _)| {
            let rest = &s[i + 1..];
            rest.starts_with(name)
                || rest
                    .strip_prefix('{')
                    .and_then(|r| r.strip_prefix(name))
                    .is_some_and(|r| r.starts_with('}'))
        })
    })
}

/// Round-trip mode: writes a string of JSON, compact JSON or YAML, which is always double-quoted
/// ([`write_escaped_string`]). A string that refers to a file variable would expand when read
/// with the default settings, and has no form there; it is an error. References to variables
/// that the application registers when reading are the reader's business, like its flags.
pub(crate) fn write_exact_double_quoted(out: &mut String, s: &str) -> Result<(), String> {
    if refers_to_file_variable(s) {
        return Err(format!(
            "the string {s:?} in double quotes: it refers to FILENAME or CURDIR, which readers \
             define by default and which double quotes expand when read (spec §7.2, §7.8, \
             §10.8); the config format writes it in single quotes"
        ));
    }
    write_escaped_string(out, s);
    Ok(())
}

/// Whether `key` reads back as itself when written bare (spec §3.1): it starts with an ASCII
/// letter or digit, `/`, `_` or a byte from 0x80, and goes on with those bytes, `-` and `.`.
pub(crate) fn is_bare_key(key: &str) -> bool {
    let bytes = key.as_bytes();
    let bare = |b: u8| b.is_ascii_alphanumeric() || b == b'/' || b == b'_' || b >= 0x80;
    match bytes.split_first() {
        Some((&first, rest)) => {
            bare(first) && rest.iter().all(|&b| bare(b) || b == b'-' || b == b'.')
        }
        None => false,
    }
}

/// Round-trip mode: writes a string of the config format so that it reads back as exactly `s`
/// whatever variables are registered when reading (spec §10.8).
///
/// A string without `$` is written by [`write_escaped_string`]. One with `$` would expand in
/// double quotes, so it is written in single quotes, which never expand (§6.2, §7.2), with each
/// `'` written `\'`. Single quotes cannot hold it when a backslash in it stands before `'`, LF or
/// CR, or ends it, counting backslashes in pairs from the left (see [`single_quotes_hold`]); such
/// a string has no form that reads back exactly with every set of variables, and is an error.
pub(crate) fn write_exact_config_string(out: &mut String, s: &str) -> Result<(), String> {
    if !s.contains('$') {
        write_escaped_string(out, s);
        return Ok(());
    }
    if !single_quotes_hold(s) {
        return Err(format!(
            "the string {s:?} in the config format: it contains `$`, which double quotes would \
             expand when read, and a backslash before `'`, a line break or the end, which single \
             quotes cannot hold (spec §6.2, §10.8); the JSON and YAML formats write it in double \
             quotes"
        ));
    }
    out.push('\'');
    let mut plain = 0;
    for (i, b) in s.bytes().enumerate() {
        if b == b'\'' {
            out.push_str(&s[plain..i]);
            out.push_str("\\'");
            plain = i + 1;
        }
    }
    out.push_str(&s[plain..]);
    out.push('\'');
    Ok(())
}

/// Whether `s`, written in single quotes with each `'` as `\'`, reads back as `s` (spec §6.2).
///
/// A reader takes a backslash together with the byte after it: `\'` is a quote, a backslash with
/// LF, CR LF or a lone CR is removed as a line continuation, and any other pair stays as written.
/// So every backslash of `s`, paired from the left, must be followed by a byte other than `'`, LF
/// or CR. This is the condition of spec §10.8: every run of backslashes directly before a `'`, an
/// LF, a CR or the end of the string has even length.
pub(crate) fn single_quotes_hold(s: &str) -> bool {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            match bytes.get(i + 1) {
                None | Some(b'\'' | b'\n' | b'\r') => return false,
                Some(_) => i += 2,
            }
        } else {
            i += 1;
        }
    }
    true
}

/// Whether a line after the first line of `s` is exactly `EOD`, which would end a heredoc early.
fn has_eod_line(s: &str) -> bool {
    s.match_indices("\nEOD")
        .any(|(i, _)| matches!(s.as_bytes().get(i + 4), None | Some(b'\n')))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn json(s: &str) -> String {
        let mut out = String::new();
        write_json_string(&mut out, s);
        out
    }

    fn config(s: &str, single_quoted: bool, multiline: bool) -> String {
        let mut out = String::new();
        write_config_string(&mut out, s, single_quoted, multiline);
        out
    }

    #[test]
    fn json_form() {
        assert_eq!(json("a\"b\\c/é"), r#""a\"b\\c/é""#);
        assert_eq!(json("\n\r\t\x08\x0c"), r#""\n\r\t\b\f""#);
        assert_eq!(json("\x0b\0\x01\x7f"), r#""\u000B\uFFFD\uFFFD\uFFFD""#);
    }

    #[test]
    fn config_forms() {
        assert_eq!(config("a\nb", false, true), "<<EOD\na\nb\nEOD");
        assert_eq!(config("a\nb", false, false), "\"a\\nb\"");
        assert_eq!(config("single", false, true), "\"single\"");
        let long = format!("{}\nend", "y".repeat(77));
        assert_eq!(long.len(), 81);
        assert!(config(&long, false, false).starts_with("<<EOD\n"));
        let boundary = format!("{}\nend", "x".repeat(76));
        assert!(config(&boundary, false, false).starts_with('"'));
        assert!(config("x\nEOD\ny", false, true).starts_with('"'));
        assert!(config("x\nEOD", false, true).starts_with('"'));
        assert!(config("x\nEODx", false, true).starts_with("<<EOD"));
        assert!(config("EOD\nx", false, true).starts_with("<<EOD\nEOD\n"));
        assert_eq!(config("it's", true, false), r"'it\'s'");
        assert_eq!(config("a\\\\'b", true, false), r#""a\\\\'b""#);
        assert_eq!(config("x\ty", true, false), "'x\ty'");
    }

    #[test]
    fn exact_forms() {
        let escaped = |s: &str| {
            let mut out = String::new();
            write_escaped_string(&mut out, s);
            out
        };
        assert_eq!(escaped("a\"b\\c/é"), r#""a\"b\\c/é""#);
        assert_eq!(
            escaped("\n\r\t\x08\x0c\x0b\0\x01\x1f\x7f"),
            r#""\n\r\t\b\f\u000B\u0000\u0001\u001F\u007F""#
        );
        let config = |s: &str| {
            let mut out = String::new();
            write_exact_config_string(&mut out, s).map(|()| out)
        };
        assert_eq!(config("plain").unwrap(), r#""plain""#);
        assert_eq!(config("$ABI it's").unwrap(), r"'$ABI it\'s'");
        assert_eq!(config("$x\\y\n").unwrap(), "'$x\\y\n'");
        assert_eq!(config("$x\\\\").unwrap(), r"'$x\\'");
        assert_eq!(config("$x\\\\'").unwrap(), r"'$x\\\''");
        for s in ["$x\\", "$x\\'", "$x\\\n", "$x\\\r", "$x\\\\\\"] {
            assert!(config(s).is_err(), "{s:?}");
        }
        for s in [
            "$CURDIR",
            "x$CURDIRy",
            "${FILENAME}",
            "$$FILENAME",
            "a\n${CURDIR}",
        ] {
            assert!(refers_to_file_variable(s), "{s:?}");
        }
        for s in [
            "$CURDI",
            "${CURDIR",
            "${FILENAMEx}",
            "$ABI",
            "CURDIR",
            "$ {CURDIR}",
            "$",
        ] {
            assert!(!refers_to_file_variable(s), "{s:?}");
        }
        for k in ["a", "A1", "1", "_a", "/a", "é", "a-b.c/d", "a_"] {
            assert!(is_bare_key(k), "{k}");
        }
        for k in [
            "", "-a", ".a", "a b", "a;b", "a}", "a#", "a,b", "$A", "a=b", "a:b", "a\"", "'a",
        ] {
            assert!(!is_bare_key(k), "{k}");
        }
    }

    #[test]
    fn keys() {
        for k in [
            "k y", "a\nb", "x+y", "p:q", "a=b", "a[b", "a{b", "a\"b", "a\\b",
        ] {
            assert!(key_needs_quoting(k), "{k}");
        }
        for k in ["kq", "a.b", "s/t", "a;b", "a}b", "a#b", "a,b", "$ABI", ""] {
            assert!(!key_needs_quoting(k), "{k}");
        }
    }
}
