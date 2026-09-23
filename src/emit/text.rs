//! Strings and keys in the output formats (spec §10.1, §10.2, §10.5).

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
