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
        // Every suffix has at most three letters; they are lowercased on the stack.
        if letters.len() > 3 {
            return None;
        }
        let mut buf = [0u8; 3];
        buf[..letters.len()].copy_from_slice(letters);
        buf.make_ascii_lowercase();
        let suffix = match &buf[..letters.len()] {
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
pub(crate) fn scan(src: &[u8], text: Option<&str>, start: usize, no_time: bool) -> Number {
    scan_with::<true>(src, text, start, no_time)
}

/// [`scan`]; with `FAST` false, without [`Decimal`] and the integer path, so that tests can
/// compare the two with the general scan.
fn scan_with<const FAST: bool>(
    src: &[u8],
    text: Option<&str>,
    start: usize,
    no_time: bool,
) -> Number {
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

    // One compare for both cases (`x` and `X` differ only in 0x20), so that the branch does not
    // depend on the case (clean-room work item C16, P3).
    if at(i).map(|b| b | 0x20) == Some(b'x') {
        let digits = i + 1;
        let Some(end) = hex_run(src, digits) else {
            return Number::Text;
        };
        return match hex_value(&src[digits..end], negative) {
            Some(v) => finish(src, end, Base::Int(v), true, no_time),
            None => Number::OutOfRange,
        };
    }

    if !FAST || matches!(at(i), Some(b'.' | b'e' | b'E')) {
        return fraction_or_exponent::<FAST>(src, text, start, int_start, i, no_time);
    }
    // An integer: what follows its digits is none of a digit, `.`, `e`, `E`, `x` or `X`, so the
    // general scan in [`fraction_or_exponent`] would read nothing more.
    if i - int_start >= LENGTH_LIMIT {
        return Number::Text;
    }
    match decimal_int(&src[int_start..i], negative) {
        Some(v) => finish(src, i, Base::Int(v), false, no_time),
        None => Number::OutOfRange,
    }
}

/// The rest of a decimal number whose integer digits are `src[int_start..int_end]`: a fraction,
/// an exponent, or with `FAST` false anything. A float of the usual form is read by [`Decimal`]
/// (with `FAST`); other forms by the general scan. Out of line, so that the code [`scan`] runs for
/// integers stays small.
#[inline(never)]
fn fraction_or_exponent<const FAST: bool>(
    src: &[u8],
    text: Option<&str>,
    start: usize,
    int_start: usize,
    int_end: usize,
    no_time: bool,
) -> Number {
    let at = |i: usize| src.get(i).copied();
    let negative = start < int_start;
    if FAST && let Some(float) = Decimal::read(src, int_start, int_end) {
        if float.end - int_start >= LENGTH_LIMIT {
            return Number::Text;
        }
        let value = match float.exact() {
            Some(v) => Some(if negative { -v } else { v }),
            None => float_value(number_text(src, text, start, float.end)),
        };
        return match value {
            Some(v) => finish(src, float.end, Base::Float(v), false, no_time),
            None => Number::OutOfRange,
        };
    }

    // The rest of the number text: at most one `.` and at most one exponent. A `.` may also come
    // after the exponent; the value then ends before it (spec §5.8).
    let mut i = int_end;
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
    if matches!(at(i), Some(b'x' | b'X')) {
        // The number before the `x` is dropped and never checked for range (spec §5.2), so its
        // length does not count either (§5.3); the digits after the `x` do (fuzz finding, C9).
        return decimal_after_x(src, i + 1, negative, no_time);
    }
    if i - int_start >= LENGTH_LIMIT {
        return Number::Text;
    }

    if dot || exponent {
        let end = float_text_end(src, int_start);
        match float_value(number_text(src, text, start, end)) {
            Some(v) => finish(src, end, Base::Float(v), false, no_time),
            None => Number::OutOfRange,
        }
    } else {
        match decimal_int(&src[int_start..i], negative) {
            Some(v) => finish(src, i, Base::Int(v), false, no_time),
            None => Number::OutOfRange,
        }
    }
}

/// The text of the number at `start..end`, which is ASCII: sign, digits, `.`, exponent.
fn number_text<'a>(src: &'a [u8], text: Option<&'a str>, start: usize, end: usize) -> &'a str {
    match text.and_then(|text| text.get(start..end)) {
        Some(text) => text,
        None => std::str::from_utf8(&src[start..end]).expect("ASCII"),
    }
}

/// `10^0` to `10^22`, each exactly a double (5²² < 2⁵³).
const POWERS_OF_TEN: [f64; 23] = [
    1e0, 1e1, 1e2, 1e3, 1e4, 1e5, 1e6, 1e7, 1e8, 1e9, 1e10, 1e11, 1e12, 1e13, 1e14, 1e15, 1e16,
    1e17, 1e18, 1e19, 1e20, 1e21, 1e22,
];

/// A decimal float of the usual form, read in one pass (clean-room work item C16, P3): digits,
/// then a `.` and digits, an exponent with at least one digit, or both, with none of `.`, `e`,
/// `E`, `x` or `X` after it. Its text ends where [`float_text_end`] says, and the general scan in
/// [`scan`] would read the same text and the same value; other forms are left to it.
struct Decimal {
    /// The digits before and after the `.` as one integer; exact when `digits` is at most 19.
    mantissa: u64,
    /// How many digits `mantissa` is made of, leading zeros included.
    digits: usize,
    /// The power of ten that `mantissa` is multiplied by: the exponent written, less the number
    /// of digits after the `.`.
    exponent: i64,
    /// The end of the number's text.
    end: usize,
}

impl Decimal {
    /// Reads the float whose integer digits are `src[int_start..int_end]`, if it has the usual
    /// form.
    fn read(src: &[u8], int_start: usize, int_end: usize) -> Option<Self> {
        let mut mantissa = 0u64;
        let mut i = digits(src, int_start, &mut mantissa);
        debug_assert_eq!(i, int_end);
        let mut exponent = 0i64;
        let dot = src.get(i) == Some(&b'.');
        if dot {
            let fraction = i + 1;
            i = digits(src, fraction, &mut mantissa);
            exponent = -((i - fraction) as i64);
        }
        let digit_count = i - int_start - usize::from(dot);
        if matches!(src.get(i), Some(b'e' | b'E')) {
            let negative = src.get(i + 1) == Some(&b'-');
            let first = i + 1 + usize::from(matches!(src.get(i + 1), Some(b'+' | b'-')));
            let mut written = 0i64;
            i = first;
            while let Some(&b) = src.get(i) {
                if !b.is_ascii_digit() {
                    break;
                }
                // Past this, no value is within the fast path's range; saturate.
                if written < 100_000 {
                    written = written * 10 + i64::from(b - b'0');
                }
                i += 1;
            }
            if i == first {
                return None;
            }
            exponent += if negative { -written } else { written };
        }
        if matches!(src.get(i), Some(b'.' | b'e' | b'E' | b'x' | b'X')) {
            return None;
        }
        Some(Self {
            mantissa,
            digits: digit_count,
            exponent,
            end: i,
        })
    }

    /// The value, without the sign, when the mantissa and the power of ten are both exact
    /// doubles: then one multiplication or division, rounded once, gives the nearest double to
    /// the decimal value, as `str::parse::<f64>` does. Such a value is never infinite, zero
    /// from nonzero digits, or below the smallest normal double, so spec §5.3's range rules
    /// hold. `None` otherwise.
    fn exact(&self) -> Option<f64> {
        if self.digits > 19 || self.mantissa > 1 << 53 {
            return None;
        }
        let mantissa = self.mantissa as f64;
        match self.exponent {
            0..=22 => Some(mantissa * POWERS_OF_TEN[self.exponent as usize]),
            -22..=-1 => Some(mantissa / POWERS_OF_TEN[self.exponent.unsigned_abs() as usize]),
            _ => None,
        }
    }
}

/// The end of the decimal digits from `from`, with their value appended to `value` (wrapping).
fn digits(src: &[u8], mut i: usize, value: &mut u64) -> usize {
    while let Some(&b) = src.get(i) {
        let digit = b.wrapping_sub(b'0');
        if digit > 9 {
            break;
        }
        *value = value.wrapping_mul(10).wrapping_add(u64::from(digit));
        i += 1;
    }
    i
}

/// The value of the decimal `digits`, with the sign applied, if it fits in an `i64`: what
/// `str::parse::<i64>` gives for the text, without making a `str` of it and reading the digits
/// again (clean-room work item C11). A negative number is accumulated below zero, so that
/// `i64::MIN` fits.
fn decimal_int(digits: &[u8], negative: bool) -> Option<i64> {
    digits.iter().try_fold(0i64, |value, &d| {
        let digit = i64::from(d - b'0');
        let value = value.checked_mul(10)?;
        if negative {
            value.checked_sub(digit)
        } else {
            value.checked_add(digit)
        }
    })
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
/// (QUESTIONS.md #19). With a leading `-`, when the hex digits begin with a letter, so that
/// nothing is read, the value is a string whatever follows (§5.2, *Quirk*; QUESTIONS.md #75).
fn decimal_after_x(src: &[u8], digits: usize, negative: bool, no_time: bool) -> Number {
    let Some(run_end) = hex_run(src, digits) else {
        return Number::Text;
    };
    let run = &src[digits..run_end];
    let mut len = run.iter().take_while(|b| b.is_ascii_digit()).count();
    if negative && len == 0 {
        return Number::Text;
    }
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
        scan(s.as_bytes(), Some(s), 0, false)
    }

    #[test]
    fn decimal_int_is_str_parse() {
        for text in [
            "0",
            "-0",
            "7",
            "0012",
            "-0012",
            "9223372036854775807",
            "9223372036854775808",
            "-9223372036854775808",
            "-9223372036854775809",
            "99999999999999999999",
        ] {
            let (negative, digits) = match text.strip_prefix('-') {
                Some(digits) => (true, digits),
                None => (false, text),
            };
            assert_eq!(
                decimal_int(digits.as_bytes(), negative),
                text.parse::<i64>().ok(),
                "{text}"
            );
        }
    }

    #[test]
    fn suffixes_of_more_than_three_letters_are_none() {
        assert_eq!(Suffix::parse(b"MIN", false), Some(Suffix::Seconds(60.0)));
        assert_eq!(Suffix::parse(b"Kb", false), Some(Suffix::Binary(1 << 10)));
        assert_eq!(Suffix::parse(b"mins", false), None);
        assert_eq!(Suffix::parse(b"", false), None);
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
        // With a leading `-`, hex digits that begin with a letter read nothing: a string, whatever
        // follows (QUESTIONS.md #75). A digit read, or plain hex, is unaffected.
        for s in [
            "-1.5xd", "-1.xD", "-1e1xD", "-1.5xd;", "-1.5xe5", "-1.5xdkb",
        ] {
            assert_eq!(num(s), Number::Text, "{s}");
        }
        for (s, v) in [("-1.5x1d", 0), ("-1.5x0d", 0), ("-1.x5", 0), ("-12xd", -13)] {
            assert_eq!(value(s), UclValue::Integer(v), "{s}");
        }
        assert_eq!(num("1.5x10 ;"), Number::Value(UclValue::Integer(0), 6));
        assert_eq!(scan(b"1.5x10s", None, 0, true), Number::Text);
        assert_eq!(scan(b"1.5x1d", None, 0, true), Number::Text);
        assert_eq!(
            scan(b"1.5x10ms", None, 0, true),
            Number::Value(UclValue::Integer(0), 8)
        );
    }

    #[test]
    fn length_limit() {
        let n126 = "1".repeat(125) + ".";
        assert!(matches!(value(&n126), UclValue::Float(_)));
        let n127 = "1".repeat(126) + ".";
        assert_eq!(num(&n127), Number::Text);
        // §5.2, *Quirk*: before an `x` after a fraction or exponent the number is dropped, and
        // its length does not count; the hex digits after the `x` do.
        let long = "0.".to_string() + &"0".repeat(300) + "1x0";
        assert_eq!(value(&long), UclValue::Integer(0));
        let long = "-0.".to_string() + &"0".repeat(200) + "1x5kb";
        assert_eq!(value(&long), UclValue::Integer(-5120));
        assert_eq!(value(&("1".repeat(130) + "e1x5")), UclValue::Integer(0));
        assert_eq!(
            value(&("1.5x".to_string() + &"1".repeat(126))),
            UclValue::Integer(0)
        );
        assert_eq!(num(&("1.5x".to_string() + &"1".repeat(127))), Number::Text);
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
        assert_eq!(scan(b"1s", None, 0, true), Number::Text);
        assert_eq!(
            scan(b"1ms", None, 0, true),
            Number::Value(UclValue::Time(0.001), 3)
        );
    }

    /// Whether two results are the same, floats compared by their bits, so that `-0.0` and `0.0`
    /// differ.
    fn same(a: &Number, b: &Number) -> bool {
        fn bits(v: &UclValue) -> (u8, u64) {
            match v {
                UclValue::Float(f) => (0, f.to_bits()),
                UclValue::Time(f) => (1, f.to_bits()),
                UclValue::Integer(i) => (2, *i as u64),
                other => panic!("not a number: {other:?}"),
            }
        }
        match (a, b) {
            (Number::Value(x, i), Number::Value(y, j)) => i == j && bits(x) == bits(y),
            _ => a == b,
        }
    }

    /// Checks that `scan` with [`Decimal`] gives what it gives without, for `s` and every prefix
    /// of it, with and without `no_time`. Returns how many of them took [`Decimal::exact`].
    fn check_against_general_scan(s: &str) -> usize {
        let mut exact = 0;
        for end in 1..=s.len() {
            let t = &s[..end];
            for no_time in [false, true] {
                let fast = scan_with::<true>(t.as_bytes(), Some(t), 0, no_time);
                let general = scan_with::<false>(t.as_bytes(), Some(t), 0, no_time);
                assert!(
                    same(&fast, &general),
                    "{t:?} no_time={no_time}: {fast:?} {general:?}"
                );
                // The same without the `str`, as for input that is not UTF-8.
                let bytes = scan_with::<true>(t.as_bytes(), None, 0, no_time);
                assert!(same(&bytes, &general), "{t:?} without text: {bytes:?}");
            }
            let int_start = usize::from(t.starts_with('-'));
            let int_end = int_start
                + t.bytes()
                    .skip(int_start)
                    .take_while(u8::is_ascii_digit)
                    .count();
            if int_end > int_start
                && Decimal::read(t.as_bytes(), int_start, int_end)
                    .is_some_and(|d| d.exact().is_some())
            {
                exact += 1;
            }
        }
        exact
    }

    /// A small deterministic generator (splitmix64).
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
            let mut z = self.0;
            z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            z ^ (z >> 31)
        }

        fn below(&mut self, n: u64) -> u64 {
            self.next() % n
        }

        fn digits(&mut self, out: &mut String, n: u64) {
            for _ in 0..n {
                out.push(char::from(b'0' + self.below(10) as u8));
            }
        }
    }

    #[test]
    fn decimal_is_the_general_scan_on_generated_numbers() {
        // Lengths, exponents, signs, suffixes and what follows, mixed at random; every prefix of
        // each text is checked too, which truncates it at every position.
        const SUFFIXES: [&str; 22] = [
            "", "", "", "", "k", "kb", "KB", "m", "mb", "g", "gb", "ms", "s", "ks", "gs", "min",
            "h", "d", "w", "y", "b", "mins",
        ];
        const AFTER: [&str; 17] = [
            "", "", ",", ";", " ", "\n", "}", "]", "#", " ;", "\t#c", "a", "/", "-", "+", "x5",
            ".5",
        ];
        let mut rng = Rng(0x5eed);
        let mut exact = 0;
        for _ in 0..200_000 {
            let mut s = String::new();
            if rng.below(2) == 0 {
                s.push('-');
            }
            let int_len = [1, 1, 2, 3, 5, 9, 16, 17, 19, 20, 25][rng.below(11) as usize];
            if rng.below(4) == 0 {
                s.push('0');
            }
            rng.digits(&mut s, int_len);
            if rng.below(4) != 0 {
                s.push('.');
                let frac_len = [0, 1, 2, 3, 6, 12, 15, 16, 17, 18, 19, 25][rng.below(12) as usize];
                if rng.below(4) == 0 {
                    s.push_str("000");
                }
                rng.digits(&mut s, frac_len);
            }
            if rng.below(2) == 0 {
                s.push(if rng.below(2) == 0 { 'e' } else { 'E' });
                s.push_str(["", "+", "-"][rng.below(3) as usize]);
                if rng.below(8) == 0 {
                    s.push_str("00");
                }
                let e = [
                    0, 1, 9, 15, 21, 22, 23, 24, 37, 300, 307, 308, 309, 320, 324, 400,
                ];
                s.push_str(&e[rng.below(e.len() as u64) as usize].to_string());
            }
            s.push_str(SUFFIXES[rng.below(SUFFIXES.len() as u64) as usize]);
            s.push_str(AFTER[rng.below(AFTER.len() as u64) as usize]);
            exact += check_against_general_scan(&s);
        }
        // The fast path is taken often enough for the comparison to mean something.
        assert!(exact > 100_000, "{exact}");
    }

    #[test]
    fn decimal_is_the_general_scan_on_edge_cases() {
        let two53 = 1u64 << 53;
        let mut cases: Vec<String> = [
            "0.0",
            "-0.0",
            "0e0",
            "-0e5",
            "0.000",
            "0e-400",
            "-0.0e400",
            "1.",
            "1.e3",
            "-1.e-3",
            "1e+5",
            "1E-05",
            "1e0000000000000000000000000000000005",
            "1e99999999999999999999",
            "1e-99999999999999999999",
            // 2⁶⁴ + 5 and 2⁶⁴: an exponent accumulated without saturating would wrap to 5 and 0.
            "1e18446744073709551621",
            "1e-18446744073709551621",
            "1.5e18446744073709551616",
            "1e5.5",
            "1e999.5",
            "1e999.",
            "1e+;",
            "1e+.5",
            "1e+x5",
            "1.5e",
            "1.5e;",
            "1.5ex",
            "1.5.",
            "1.5x10kb",
            "0.1",
            "0.2",
            "0.3",
            "123456789012345678e-22",
            "1234567890123456789e-22",
            "12345678901234567890e-22",
            "9007199254740993.0",
            "9007199254740993e0",
            "9007199254740992.0",
            "9007199254740991e-22",
            "9007199254740991e22",
            "9007199254740991e23",
            "9007199254740991e-23",
            "1.7976931348623157e308",
            "1.7976931348623158e308",
            "1.7976931348623159e308",
            "-1e309",
            "2.2250738585072014e-308",
            "2.2250738585072011e-308",
            "2.2250738585072009e-308",
            "4.9406564584124654e-324",
            "1e-310",
            "0.0000000000000000000000000000001",
            "00000000000000000000000.5",
            "0.00000000000000000000000000000000000000000000000000000000000000000000001e70",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        // Integers at the length limit (§5.3), and zeros before a fraction.
        for n in [126, 127, 128] {
            cases.push("1".repeat(n));
            cases.push("0".repeat(n));
            cases.push(format!("-{}", "0".repeat(n - 2)));
            cases.push(format!("{}.5", "0".repeat(n - 2)));
        }
        // Mantissas around 2⁵³ and the 19-digit limit, at every exponent near ±22, in several
        // spellings.
        for m in [
            1,
            7,
            two53 - 1,
            two53,
            two53 + 1,
            999_999_999_999_999,
            9_999_999_999_999_999,
            9_999_999_999_999_999_999,
            1_000_000_000_000_000_000,
        ] {
            for e in -25i32..=25 {
                cases.push(format!("{m}e{e}"));
                cases.push(format!("-{m}E{e:+}"));
                let digits = m.to_string();
                for dot in 1..digits.len() {
                    cases.push(format!("{}.{}e{e}", &digits[..dot], &digits[dot..]));
                }
            }
        }
        // Halfway cases of the multiplication: m · 5^e odd and between 2⁵³ and 2⁵⁴, so that
        // m · 10^e lies halfway between two doubles.
        let mut rng = Rng(53);
        for e in 1..=22u32 {
            let five = 5u64.pow(e);
            for _ in 0..200 {
                let low = two53 / five + 1;
                let high = (two53 << 1) / five;
                if low > high {
                    break;
                }
                let m = (low + rng.below(high - low + 1)) | 1;
                if m * five > two53 && m * five < two53 << 1 {
                    cases.push(format!("{m}e{e}"));
                    cases.push(format!("{m}.0e{e}k"));
                }
            }
        }
        let mut exact = 0;
        for s in &cases {
            exact += check_against_general_scan(s);
        }
        assert!(exact > 1_000, "{exact}");
    }

    #[test]
    fn exact_is_str_parse() {
        // Every mantissa and exponent the fast path takes, by sample: the value, unsigned, is the
        // nearest double, as `str::parse` gives.
        let mut rng = Rng(22);
        let mut taken = 0;
        for _ in 0..1_000_000 {
            let m = match rng.below(4) {
                0 => rng.below(1 << 53) + 1,
                1 => (1 << 53) - rng.below(1000),
                2 => {
                    let digits = rng.below(20) as u32;
                    rng.below(10u64.pow(digits))
                }
                _ => rng.next() >> rng.below(64),
            };
            let e = rng.below(51) as i64 - 25;
            let text = format!("{m}e{e}");
            let d = Decimal::read(text.as_bytes(), 0, m.to_string().len()).unwrap();
            assert_eq!((d.mantissa, d.exponent), (m, e), "{text}");
            if let Some(v) = d.exact() {
                let parsed: f64 = text.parse().unwrap();
                assert_eq!(v.to_bits(), parsed.to_bits(), "{text}");
                taken += 1;
            }
        }
        assert!(taken > 300_000, "{taken}");
    }

    /// Every number of the conformance cases, of `benches/corpus/`, and of the JSON documents
    /// in `target/bench-corpus/` when `benches/fetch-documents.sh` has fetched them: `scan` at
    /// every digit and `-` gives what it gives without [`Decimal`].
    #[test]
    fn decimal_is_the_general_scan_on_the_corpus() {
        fn files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    files(&path, out);
                } else {
                    out.push(path);
                }
            }
        }
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut paths = Vec::new();
        for dir in [
            "tests/conformance/cases",
            "benches/corpus",
            "target/bench-corpus",
        ] {
            files(&root.join(dir), &mut paths);
        }
        let (mut floats, mut exact) = (0usize, 0usize);
        for path in &paths {
            let src = std::fs::read(path).unwrap();
            let text = std::str::from_utf8(&src).ok();
            for start in 0..src.len() {
                if !(src[start] == b'-' || src[start].is_ascii_digit()) {
                    continue;
                }
                for no_time in [false, true] {
                    let fast = scan_with::<true>(&src, text, start, no_time);
                    let general = scan_with::<false>(&src, text, start, no_time);
                    assert!(
                        same(&fast, &general),
                        "{} at {start}: {fast:?} {general:?}",
                        path.display()
                    );
                }
                if matches!(
                    scan(&src, text, start, false),
                    Number::Value(UclValue::Float(_), _)
                ) {
                    floats += 1;
                    let int_start = start + usize::from(src[start] == b'-');
                    let int_end = int_start
                        + src[int_start..]
                            .iter()
                            .take_while(|b| b.is_ascii_digit())
                            .count();
                    if Decimal::read(&src, int_start, int_end).is_some_and(|d| d.exact().is_some())
                    {
                        exact += 1;
                    }
                }
            }
        }
        eprintln!(
            "{} files, {floats} float starts, {exact} through the fast path",
            paths.len()
        );
        assert!(floats > 1_000, "{floats}");
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
