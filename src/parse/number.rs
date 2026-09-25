//! Numbers, multipliers and time suffixes (spec §5).

use crate::value::UclValue;

/// A decimal number whose digits, `.` and exponent take this many characters or more is a
/// string; so is a hex number with this many digits after the `x` (spec §5.3, *Quirk*).
const LENGTH_LIMIT: usize = 127;

/// The outcome of reading an unquoted value as a number.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Number {
    /// The value is a number that ends at the given offset (after any suffix).
    Value(UclValue, usize),
    /// The value is not a number; it is read as an unquoted string instead.
    Text,
    /// The value is a number outside the representable range, which rejects the document
    /// (spec §5.8).
    OutOfRange,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Suffix {
    /// `k`, `m`, `g`: × 10³, 10⁶, 10⁹; the type is kept.
    Decimal(i64),
    /// `kb`, `mb`, `gb`: × 2¹⁰, 2²⁰, 2³⁰; always an int.
    Binary(i64),
    /// `ms`: ÷ 1000 seconds.
    Millis,
    /// `s`, `ks`, `gs`, `min`, `h`, `d`, `w`, `y`: × seconds.
    Seconds(f64),
}

impl Suffix {
    fn parse(letters: &[u8], no_time: bool) -> Option<Self> {
        let lower = letters.to_ascii_lowercase();
        let suffix = match lower.as_slice() {
            b"k" => Suffix::Decimal(1_000),
            b"m" => Suffix::Decimal(1_000_000),
            b"g" => Suffix::Decimal(1_000_000_000),
            b"kb" => Suffix::Binary(1 << 10),
            b"mb" => Suffix::Binary(1 << 20),
            b"gb" => Suffix::Binary(1 << 30),
            // `ms`, `ks` and `gs` are time values even under `no-time` (spec §12.3).
            b"ms" => Suffix::Millis,
            b"ks" => Suffix::Seconds(1e3),
            b"gs" => Suffix::Seconds(1e9),
            _ if no_time => return None,
            b"s" => Suffix::Seconds(1.0),
            b"min" => Suffix::Seconds(60.0),
            b"h" => Suffix::Seconds(3_600.0),
            b"d" => Suffix::Seconds(86_400.0),
            b"w" => Suffix::Seconds(604_800.0),
            b"y" => Suffix::Seconds(31_536_000.0),
            _ => return None,
        };
        Some(suffix)
    }
}

enum Base {
    Int(i64),
    Float(f64),
}

/// Reads the unquoted value starting at `src[start]` as a number (spec §5).
///
/// The forms are an optional `-`, then either decimal digits with an optional fraction and
/// exponent, or digits, `x` and hex digits (the digits before the `x` are ignored, spec §5.2),
/// then an optional suffix of ASCII letters. The number is then checked for range, and finally
/// for what follows it (spec §5.5): with a suffix, the end of input, a terminator, `#`, `}` or
/// `]` must follow at once; without one, spaces and tabs may come first.
///
/// Before the range check, a malformed number text makes the value a string (spec §5.8): a second
/// `.` or exponent, an `e` or `E` followed by neither a digit nor a sign, an `x` or `X` without a
/// hex digit after it, or a `.` right after hex digits. Any other trailing text makes it a string
/// only after the range check, so `99999999999999999999x` is a string while
/// `99999999999999999999 x` and `1e400b` are errors.
pub(crate) fn scan(src: &[u8], start: usize, no_time: bool) -> Number {
    let at = |i: usize| src.get(i).copied();

    let mut i = start;
    let negative = at(i) == Some(b'-');
    if negative {
        i += 1;
    }
    let int_start = i;
    while at(i).is_some_and(|b| b.is_ascii_digit()) {
        i += 1;
    }
    if i == int_start {
        return Number::Text;
    }

    if matches!(at(i), Some(b'x' | b'X')) {
        let digits = i + 1;
        let Some(end) = hex_run(src, digits) else {
            return Number::Text;
        };
        return match hex_value(&src[digits..end], negative) {
            Some(v) => finish(src, end, Base::Int(v), true, no_time),
            None => Number::OutOfRange,
        };
    }

    // The rest of the number text: at most one `.` and at most one exponent. A `.` may also come
    // after the exponent; the value then ends before it (spec §5.8).
    let mut dot = false;
    let mut exponent = false;
    loop {
        match at(i) {
            Some(b'0'..=b'9') => i += 1,
            Some(b'.') if dot => return Number::Text,
            Some(b'.') => {
                dot = true;
                i += 1;
            }
            Some(b'e' | b'E') => {
                let signed = matches!(at(i + 1), Some(b'+' | b'-'));
                if exponent || !(signed || at(i + 1).is_some_and(|b| b.is_ascii_digit())) {
                    return Number::Text;
                }
                exponent = true;
                i += if signed { 2 } else { 1 };
            }
            _ => break,
        }
    }
    if i - int_start >= LENGTH_LIMIT {
        return Number::Text;
    }
    if matches!(at(i), Some(b'x' | b'X')) {
        return decimal_after_x(src, i + 1, negative, no_time);
    }

    if dot || exponent {
        let end = float_text_end(src, int_start);
        // The number's text is ASCII: sign, digits, `.`, exponent.
        let text = std::str::from_utf8(&src[start..end]).expect("ASCII");
        match float_value(text) {
            Some(v) => finish(src, end, Base::Float(v), false, no_time),
            None => Number::OutOfRange,
        }
    } else {
        let text = std::str::from_utf8(&src[start..i]).expect("ASCII");
        match text.parse::<i64>() {
            Ok(v) => finish(src, i, Base::Int(v), false, no_time),
            Err(_) => Number::OutOfRange,
        }
    }
}

/// The end of the hex digits that start at `from`. `None` if there are none, if there are too
/// many (spec §5.3), or if a `.` follows them, which makes the value a string before any range
/// check (spec §5.2, §5.8).
fn hex_run(src: &[u8], from: usize) -> Option<usize> {
    let len = src[from..]
        .iter()
        .take_while(|b| b.is_ascii_hexdigit())
        .count();
    let end = from + len;
    (len > 0 && len < LENGTH_LIMIT && src.get(end) != Some(&b'.')).then_some(end)
}

/// The end of the decimal float that starts at `from`: digits, an optional `.` and digits, an
/// optional exponent with at least one digit.
fn float_text_end(src: &[u8], from: usize) -> usize {
    let digits = |i: usize| i + src[i..].iter().take_while(|b| b.is_ascii_digit()).count();
    let mut i = digits(from);
    if src.get(i) == Some(&b'.') {
        i = digits(i + 1);
    }
    if matches!(src.get(i), Some(b'e' | b'E')) {
        let sign = usize::from(matches!(src.get(i + 1), Some(b'+' | b'-')));
        if src.get(i + 1 + sign).is_some_and(|b| b.is_ascii_digit()) {
            i = digits(i + 1 + sign);
        }
    }
    i
}

/// Reads the suffix after a number ending at `number_end`, checks what follows (spec §5.5), and
/// builds the value.
fn finish(src: &[u8], number_end: usize, base: Base, is_hex: bool, no_time: bool) -> Number {
    let end = letters_end(src, number_end);
    let suffix = if end > number_end {
        match Suffix::parse(&src[number_end..end], no_time) {
            Some(s) => Some(s),
            None => return Number::Text,
        }
    } else {
        None
    };
    if !followed_properly(src, end, suffix.is_some()) {
        return Number::Text;
    }
    Number::Value(apply(base, suffix, is_hex), end)
}

fn letters_end(src: &[u8], from: usize) -> usize {
    from + src[from..]
        .iter()
        .take_while(|b| b.is_ascii_alphabetic())
        .count()
}

/// Whether a number ending at `end` is followed by what spec §5.5 allows.
fn followed_properly(src: &[u8], mut end: usize, has_suffix: bool) -> bool {
    if !has_suffix {
        while matches!(src.get(end), Some(b' ' | b'\t')) {
            end += 1;
        }
    }
    matches!(
        src.get(end),
        None | Some(b'\n' | b'\r' | 0 | b',' | b';' | b'#' | b'}' | b']')
    )
}

/// An `x` after a number with a `.` or an exponent (spec §5.2, *Quirk*): the hex digits after it,
/// starting at `digits`, are read as a decimal number (digits, then an optional exponent), and
/// the rest of them, with any letters after them, is that number's suffix.
///
/// The value is `int 0`, except with a binary multiplier: the decimal number, with the sign
/// applied and truncated (saturating), times the multiplier, wrapping. A decimal number that
/// overflows is an error (spec §5.8). Details beyond the spec's examples follow the oracle
/// (QUESTIONS.md #19).
fn decimal_after_x(src: &[u8], digits: usize, negative: bool, no_time: bool) -> Number {
    let Some(run_end) = hex_run(src, digits) else {
        return Number::Text;
    };
    let run = &src[digits..run_end];
    let mut len = run.iter().take_while(|b| b.is_ascii_digit()).count();
    if len > 0
        && matches!(run.get(len), Some(b'e' | b'E'))
        && run.get(len + 1).is_some_and(|b| b.is_ascii_digit())
    {
        len += 1 + run[len + 1..]
            .iter()
            .take_while(|b| b.is_ascii_digit())
            .count();
    }
    let decimal = if len == 0 {
        0.0
    } else {
        let text = std::str::from_utf8(&run[..len]).expect("ASCII");
        match text.parse::<f64>() {
            Ok(v) if v.is_finite() => v,
            _ => return Number::OutOfRange,
        }
    };
    let end = letters_end(src, run_end);
    let rest = &src[digits + len..end];
    let suffix = if rest.is_empty() {
        None
    } else {
        match Suffix::parse(rest, no_time) {
            Some(s) => Some(s),
            None => return Number::Text,
        }
    };
    if !followed_properly(src, end, suffix.is_some()) {
        return Number::Text;
    }
    let value = match suffix {
        Some(Suffix::Binary(m)) => {
            let signed = if negative { -decimal } else { decimal };
            (signed as i64).wrapping_mul(m)
        }
        _ => 0,
    };
    Number::Value(UclValue::Integer(value), end)
}

/// The hex digits' value with the sign applied, if it fits in an `i64`.
fn hex_value(digits: &[u8], negative: bool) -> Option<i64> {
    let text = std::str::from_utf8(digits).ok()?;
    let magnitude = u64::from_str_radix(text, 16).ok()?;
    if negative {
        if magnitude == 1 << 63 {
            Some(i64::MIN)
        } else {
            i64::try_from(magnitude).ok().map(|v| -v)
        }
    } else {
        i64::try_from(magnitude).ok()
    }
}

/// The float value of `text`, or `None` if it overflows, or is nonzero and below the smallest
/// normal double (spec §5.3).
fn float_value(text: &str) -> Option<f64> {
    let value: f64 = text.parse().ok()?;
    if value.is_infinite() {
        return None;
    }
    if value == 0.0 {
        let mantissa = text.split(['e', 'E']).next().unwrap_or("");
        let nonzero = mantissa.bytes().any(|b| (b'1'..=b'9').contains(&b));
        return (!nonzero).then_some(value);
    }
    (value.abs() >= f64::MIN_POSITIVE).then_some(value)
}

fn apply(base: Base, suffix: Option<Suffix>, is_hex: bool) -> UclValue {
    match (base, suffix) {
        (Base::Int(v), None) => UclValue::Integer(v),
        (Base::Float(v), None) => UclValue::Float(v),
        // Multiplying an int wraps around (spec §5.4, *Quirk*).
        (Base::Int(v), Some(Suffix::Decimal(m) | Suffix::Binary(m))) => {
            UclValue::Integer(v.wrapping_mul(m))
        }
        (Base::Float(v), Some(Suffix::Decimal(m))) => UclValue::Float(v * m as f64),
        // A float is truncated toward zero first (spec §5.4, *Quirk*).
        (Base::Float(v), Some(Suffix::Binary(m))) => UclValue::Integer((v as i64).wrapping_mul(m)),
        // Hex numbers accept time suffixes and ignore them (spec §5.4, *Quirk*).
        (Base::Int(v), Some(Suffix::Millis | Suffix::Seconds(_))) if is_hex => UclValue::Integer(v),
        (Base::Int(v), Some(time)) => UclValue::Time(seconds(v as f64, time)),
        (Base::Float(v), Some(time)) => UclValue::Time(seconds(v, time)),
    }
}

fn seconds(value: f64, suffix: Suffix) -> f64 {
    match suffix {
        Suffix::Millis => value / 1000.0,
        Suffix::Seconds(m) => value * m,
        Suffix::Decimal(_) | Suffix::Binary(_) => unreachable!("not a time suffix"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn num(s: &str) -> Number {
        scan(s.as_bytes(), 0, false)
    }

    fn value(s: &str) -> UclValue {
        match num(s) {
            Number::Value(v, _) => v,
            other => panic!("{s}: {other:?}"),
        }
    }

    #[test]
    fn decimal_forms() {
        assert_eq!(value("0"), UclValue::Integer(0));
        assert_eq!(value("-0"), UclValue::Integer(0));
        assert_eq!(value("007"), UclValue::Integer(7));
        assert_eq!(value("1."), UclValue::Float(1.0));
        assert_eq!(value("1.e3"), UclValue::Float(1000.0));
        assert!(matches!(value("-0.0"), UclValue::Float(f) if f == 0.0 && f.is_sign_negative()));
        for s in [
            "1e", "1e+", "1ee3", "1..2", "1.2.3", "1e3.5", "1.5e", "1_000", "-", "-a",
        ] {
            assert_eq!(num(s), Number::Text, "{s}");
        }
    }

    #[test]
    fn hex_forms() {
        assert_eq!(value("0x10"), UclValue::Integer(16));
        assert_eq!(value("0X1f"), UclValue::Integer(31));
        assert_eq!(value("-0x10"), UclValue::Integer(-16));
        assert_eq!(value("12x34"), UclValue::Integer(0x34));
        assert_eq!(value("-12x34"), UclValue::Integer(-0x34));
        assert_eq!(value("-0x8000000000000000"), UclValue::Integer(i64::MIN));
        for s in ["0x", "0xg", "0x1.5", "0x-1", "-0x", "1x"] {
            assert_eq!(num(s), Number::Text, "{s}");
        }
        assert_eq!(num("0xFFFFFFFFFFFFFFFF"), Number::OutOfRange);
    }

    #[test]
    fn range() {
        assert_eq!(value("9223372036854775807"), UclValue::Integer(i64::MAX));
        assert_eq!(value("-9223372036854775808"), UclValue::Integer(i64::MIN));
        for s in [
            "9223372036854775808",
            "1e309",
            "1e-400",
            "1e-310",
            "99999999999999999999 x",
            "99999999999999999999kx",
            "1e400mins",
            "1e-400b",
            "0x8000000000000000mins",
            "0x8000000000000000x",
            "0x7fffffffffffffffdx",
        ] {
            assert_eq!(num(s), Number::OutOfRange, "{s}");
        }
        assert_eq!(num("99999999999999999999x"), Number::Text);
        assert_eq!(num("1e999E"), Number::Text);
        assert_eq!(value("0e-400"), UclValue::Float(0.0));
        assert_eq!(
            value("2.2250738585072014e-308"),
            UclValue::Float(f64::MIN_POSITIVE)
        );
    }

    #[test]
    fn malformed_text_is_a_string_before_the_range_check() {
        // spec §5.8; the `.` after hex digits follows the oracle (QUESTIONS.md #19).
        for s in [
            "1e999x",
            "1e999e",
            "99999999999999999999X",
            "99999999999999999999e",
            "99999999999999999999e+",
            "1e999..",
            "1.5e999.5",
            "1e999.5.",
            "1e999.x",
            "0x8000000000000000.",
            "12x8000000000000000.5",
        ] {
            assert_eq!(num(s), Number::Text, "{s}");
        }
        for s in [
            "1e999.5",
            "1e999.",
            "1e999.k",
            "1e999-",
            "1e999 .",
            "99999999999999999999k5",
            "0x8000000000000000-",
        ] {
            assert_eq!(num(s), Number::OutOfRange, "{s}");
        }
        assert_eq!(value("99999999999999999999."), UclValue::Float(1e20));
        assert_eq!(value("99999999999999999999.5"), UclValue::Float(1e20));
        assert_eq!(num("1e5.5"), Number::Text);
    }

    #[test]
    fn x_after_a_fraction_or_exponent() {
        // spec §5.2, *Quirk*; details beyond the spec's examples follow the oracle
        // (QUESTIONS.md #19).
        for (s, v) in [
            ("1.5x10", 0),
            ("1e5x10", 0),
            ("1.x5", 0),
            ("1.5x1e5", 0),
            ("1.5X10", 0),
            ("1.5x10kb", 10240),
            ("1.5x10KB", 10240),
            ("1.5x1e5kb", 102_400_000),
            ("-1.5x10kb", -10240),
            ("1.5x10mb", 10 << 20),
            ("1.5x10k", 0),
            ("1.5x10s", 0),
            ("1.5x10min", 0),
            ("1.5x1d", 0),
            ("1.5xd", 0),
            ("1e999x5kb", 5120),
            ("1.5x99999999999999999999kb", -1024),
            ("-1.5x99999999999999999999kb", 0),
        ] {
            assert_eq!(value(s), UclValue::Integer(v), "{s}");
        }
        for s in [
            "1.5x1f",
            "1e5x",
            "1.5x10.5",
            "1.5x1.5kb",
            "1.5x-10kb",
            "1.5x10b",
            "1.5x1dkb",
            "1.5x10e",
            "1.5x1e",
            "1.5x1e+5",
            "1.5x1e1e1",
            "1.5x10x",
            "1.5x10 x",
            "1.5x10kb ",
            "1.5x1d ",
            "1.5x1e999.",
        ] {
            assert_eq!(num(s), Number::Text, "{s}");
        }
        for s in ["1.5x1e999", "1.5x1e999kb", "1.5x1e999 x", "1.5x1e999e"] {
            assert_eq!(num(s), Number::OutOfRange, "{s}");
        }
        assert_eq!(num("1.5x10 ;"), Number::Value(UclValue::Integer(0), 6));
        assert_eq!(scan(b"1.5x10s", 0, true), Number::Text);
        assert_eq!(scan(b"1.5x1d", 0, true), Number::Text);
        assert_eq!(
            scan(b"1.5x10ms", 0, true),
            Number::Value(UclValue::Integer(0), 8)
        );
    }

    #[test]
    fn length_limit() {
        let n126 = "1".repeat(125) + ".";
        assert!(matches!(value(&n126), UclValue::Float(_)));
        let n127 = "1".repeat(126) + ".";
        assert_eq!(num(&n127), Number::Text);
    }

    #[test]
    fn suffixes() {
        assert_eq!(value("1k"), UclValue::Integer(1000));
        assert_eq!(value("1.5k"), UclValue::Float(1500.0));
        assert_eq!(value("1KB"), UclValue::Integer(1024));
        assert_eq!(value("1.5kb"), UclValue::Integer(1024));
        assert_eq!(value("1m"), UclValue::Integer(1_000_000));
        assert_eq!(value("1min"), UclValue::Time(60.0));
        assert_eq!(value("3ms"), UclValue::Time(0.003));
        assert_eq!(value("-1.5h"), UclValue::Time(-5400.0));
        assert_eq!(value("1ks"), UclValue::Time(1000.0));
        assert_eq!(value("9223372036854775807k"), UclValue::Integer(-1000));
        assert_eq!(value("1e308k"), UclValue::Float(f64::INFINITY));
        assert_eq!(value("0x10k"), UclValue::Integer(16000));
        assert_eq!(value("0x10ms"), UclValue::Integer(16));
        for s in ["1t", "1tb", "1b", "1mins", "1sec", "1k5", "1k ", "1s/"] {
            assert_eq!(num(s), Number::Text, "{s}");
        }
        assert_eq!(scan(b"1s", 0, true), Number::Text);
        assert_eq!(
            scan(b"1ms", 0, true),
            Number::Value(UclValue::Time(0.001), 3)
        );
    }

    #[test]
    fn followers() {
        assert_eq!(num("10  ;"), Number::Value(UclValue::Integer(10), 2));
        assert_eq!(num("1}"), Number::Value(UclValue::Integer(1), 1));
        assert_eq!(num("1#c"), Number::Value(UclValue::Integer(1), 1));
        for s in ["1 2", "1-2", "1/2", "1=2", "10 s", "1{", "30/* c */"] {
            assert_eq!(num(s), Number::Text, "{s}");
        }
    }
}
