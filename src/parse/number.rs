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
    NotNumber,
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
/// The order matters for out-of-range numbers and follows the oracle (QUESTIONS.md #15): an `e`,
/// `E`, `x` or `X` right after a decimal number makes the value a string before the range is
/// checked;
/// any other trailing text, an unknown suffix included, makes it a string only after the range
/// check. So `99999999999999999999x` is a string, while `99999999999999999999 x` and
/// `1e400b` are errors.
pub(crate) fn scan(src: &[u8], start: usize, no_time: bool) -> Number {
    let at = |i: usize| src.get(i).copied();
    let is_digit = |i: usize| at(i).is_some_and(|b| b.is_ascii_digit());

    let mut i = start;
    let negative = at(i) == Some(b'-');
    if negative {
        i += 1;
    }
    let int_start = i;
    while is_digit(i) {
        i += 1;
    }
    if i == int_start {
        return Number::NotNumber;
    }

    let mut hex_digits = None;
    let mut is_float = false;
    if matches!(at(i), Some(b'x' | b'X')) {
        let hex_start = i + 1;
        let mut j = hex_start;
        while at(j).is_some_and(|b| b.is_ascii_hexdigit()) {
            j += 1;
        }
        if j == hex_start || j - hex_start >= LENGTH_LIMIT {
            return Number::NotNumber;
        }
        hex_digits = Some(hex_start..j);
        i = j;
    } else {
        if at(i) == Some(b'.') {
            is_float = true;
            i += 1;
            while is_digit(i) {
                i += 1;
            }
        }
        if matches!(at(i), Some(b'e' | b'E')) {
            let mut j = i + 1;
            if matches!(at(j), Some(b'+' | b'-')) {
                j += 1;
            }
            if is_digit(j) {
                while is_digit(j) {
                    j += 1;
                }
                is_float = true;
                i = j;
            }
            // Otherwise the `e` is left to be read as a suffix, which it is not.
        }
        if i - int_start >= LENGTH_LIMIT {
            return Number::NotNumber;
        }
    }
    let number_end = i;
    let is_hex = hex_digits.is_some();
    if !is_hex && matches!(at(i), Some(b'e' | b'E' | b'x' | b'X')) {
        return Number::NotNumber;
    }

    let base = if let Some(digits) = hex_digits {
        match hex_value(&src[digits], negative) {
            Some(v) => Base::Int(v),
            None => return Number::OutOfRange,
        }
    } else {
        // The number's text is ASCII: sign, digits, `.`, exponent.
        let text = std::str::from_utf8(&src[start..number_end]).expect("ASCII");
        if is_float {
            match float_value(text) {
                Some(v) => Base::Float(v),
                None => return Number::OutOfRange,
            }
        } else {
            match text.parse::<i64>() {
                Ok(v) => Base::Int(v),
                Err(_) => return Number::OutOfRange,
            }
        }
    };

    while at(i).is_some_and(|b| b.is_ascii_alphabetic()) {
        i += 1;
    }
    let suffix = if i > number_end {
        match Suffix::parse(&src[number_end..i], no_time) {
            Some(s) => Some(s),
            None => return Number::NotNumber,
        }
    } else {
        None
    };

    let end = i;
    let mut follow = i;
    if suffix.is_none() {
        while matches!(at(follow), Some(b' ' | b'\t')) {
            follow += 1;
        }
    }
    if !matches!(
        at(follow),
        None | Some(b'\n' | b'\r' | 0 | b',' | b';' | b'#' | b'}' | b']')
    ) {
        return Number::NotNumber;
    }

    Number::Value(apply(base, suffix, is_hex), end)
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
            assert_eq!(num(s), Number::NotNumber, "{s}");
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
            assert_eq!(num(s), Number::NotNumber, "{s}");
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
        assert_eq!(num("99999999999999999999x"), Number::NotNumber);
        assert_eq!(num("1e999E"), Number::NotNumber);
        assert_eq!(value("0e-400"), UclValue::Float(0.0));
        assert_eq!(
            value("2.2250738585072014e-308"),
            UclValue::Float(f64::MIN_POSITIVE)
        );
    }

    #[test]
    fn length_limit() {
        let n126 = "1".repeat(125) + ".";
        assert!(matches!(value(&n126), UclValue::Float(_)));
        let n127 = "1".repeat(126) + ".";
        assert_eq!(num(&n127), Number::NotNumber);
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
            assert_eq!(num(s), Number::NotNumber, "{s}");
        }
        assert_eq!(scan(b"1s", 0, true), Number::NotNumber);
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
            assert_eq!(num(s), Number::NotNumber, "{s}");
        }
    }
}
