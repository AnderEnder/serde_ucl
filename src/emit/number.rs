//! Numbers in every output format (spec §10.2, §10.3).

use std::fmt::Write;

const I32_MIN: f64 = -2_147_483_648.0;
const I32_MAX: f64 = 2_147_483_647.0;

/// Writes a float or a time (in seconds) as spec §10.3 describes: the bytes of the ISO C
/// conversion `%.1f`, `%.15g` or `%f`, chosen by the value, in the C locale.
///
/// Rust's formatting with a precision writes the exact decimal value of the double rounded to
/// that precision, ties to even, as the spec requires.
pub(crate) fn write_float(out: &mut String, v: f64) {
    if v.is_nan() {
        out.push_str(if v.is_sign_negative() { "-nan" } else { "nan" });
        return;
    }
    if v.is_infinite() {
        out.push_str(if v < 0.0 { "-inf" } else { "inf" });
        return;
    }
    let t = v.trunc();
    let t_in_range = (I32_MIN..=I32_MAX).contains(&t);
    if v == t && t_in_range {
        let _ = write!(out, "{v:.1}");
    } else if v != t && t_in_range && (v - t).abs() < 1e-7 {
        write_g15(out, v);
    } else {
        let _ = write!(out, "{v:.6}");
    }
}

/// `%.15g`: 15 significant digits, in plain decimal form when the decimal exponent X of the
/// rounded value satisfies −4 ≤ X < 15, otherwise in exponent form; trailing zeros after the
/// decimal point are removed, and the point too when nothing follows it.
fn write_g15(out: &mut String, v: f64) {
    let exp_form = format!("{v:.14e}");
    let (mantissa, exponent) = exp_form
        .split_once('e')
        .expect("exponent formatting has an 'e'");
    let x: i32 = exponent.parse().expect("the exponent is an integer");
    if (-4..15).contains(&x) {
        let precision = usize::try_from(14 - x).expect("x < 15");
        let plain = format!("{v:.precision$}");
        out.push_str(strip_zeros(&plain));
    } else {
        out.push_str(strip_zeros(mantissa));
        out.push('e');
        out.push(if x < 0 { '-' } else { '+' });
        let _ = write!(out, "{:02}", x.unsigned_abs());
    }
}

/// Round-trip mode: writes a float so that it reads back as the same double (spec §10.8).
///
/// A finite value is written with the fewest significant digits that identify it, with a `.` or an
/// exponent so that it reads as a float (§5.1): `0.1`, `1.0`, `-0.0`, `1e16`, `1.5e-10`. NaN is the
/// keyword `nan` (its sign and payload are not kept), +∞ the keyword `inf`, and −∞, which has no
/// keyword, `-1e308k`: a multiplier that overflows (§4.5, §5.4). A subnormal value has no form,
/// since every literal below the normal range is an error (§5.3).
pub(crate) fn write_exact_float(out: &mut String, v: f64) -> Result<(), String> {
    if v.is_nan() {
        out.push_str("nan");
    } else if v == f64::INFINITY {
        out.push_str("inf");
    } else if v == f64::NEG_INFINITY {
        out.push_str("-1e308k");
    } else {
        write_finite(out, v, "float")?;
    }
    Ok(())
}

/// Round-trip mode: writes a time (in seconds) so that it reads back as the same time
/// (spec §10.8): the digits of [`write_exact_float`] followed by `s`, as in `1.5s` or `1e-3s`. +∞
/// and −∞ are `1e308ks` and `-1e308ks`, a suffix that overflows (§5.4). A subnormal time, which
/// only `ms` gives, is written as the normal float that `ms` divides into it, as in
/// `2.2250738585072014e-308ms` ([`write_subnormal_time`]). NaN has no form: no text reads as a
/// NaN time.
pub(crate) fn write_exact_time(out: &mut String, v: f64) -> Result<(), String> {
    if v.is_nan() {
        return Err("a NaN time, which no text reads back as (spec §5.4, §10.8)".to_owned());
    } else if v == f64::INFINITY {
        out.push_str("1e308ks");
    } else if v == f64::NEG_INFINITY {
        out.push_str("-1e308ks");
    } else if v.is_subnormal() {
        write_subnormal_time(out, v)?;
    } else {
        write_finite(out, v, "time")?;
        out.push('s');
    }
    Ok(())
}

/// The smallest positive time that `ms` gives from a normal float: the smallest normal float
/// divided by 1000 (spec §10.8, `time_subnormal_through_ms`).
const SMALLEST_MS_TIME: f64 = f64::MIN_POSITIVE / 1000.0;

/// A subnormal time `v`, as `Mms` with M a normal float whose quotient by 1000 is `v`
/// (spec §5.4, §10.8). `ms` divides the number before it by 1000 without a range check, and the
/// quotient is rounded once.
///
/// M is the shortest digits of `v` with the exponent raised by 3 (`1e-310` → `1e-307ms`) when
/// they give `v`. Otherwise it is `v` × 1000, which gives `v` whenever that product is normal,
/// since its error is below half a unit in the last place of `v`; or, for ±[`SMALLEST_MS_TIME`],
/// whose product with 1000 falls just below the normal range, the smallest normal float. A time
/// closer to zero than that has no normal M and no form.
fn write_subnormal_time(out: &mut String, v: f64) -> Result<(), String> {
    let gives_v = |m: f64| m.is_normal() && (m / 1000.0).to_bits() == v.to_bits();
    let shortest = format!("{v:e}");
    let (mantissa, exponent) = shortest
        .split_once('e')
        .expect("exponent formatting has an 'e'");
    let exponent: i32 = exponent.parse().expect("the exponent is an integer");
    let shifted = format!("{mantissa}e{}", exponent + 3);
    if shifted.parse::<f64>().is_ok_and(gives_v) {
        out.push_str(&shifted);
    } else if let Some(m) = [v * 1000.0, f64::MIN_POSITIVE.copysign(v)]
        .into_iter()
        .find(|&m| gives_v(m))
    {
        let _ = write!(out, "{m:?}");
    } else {
        return Err(format!(
            "the subnormal time {v:e}: a time below the normal range is written as a normal \
             float followed by `ms`, and none gives a time closer to zero than \
             {SMALLEST_MS_TIME:e} (spec §10.8)"
        ));
    }
    out.push_str("ms");
    Ok(())
}

/// Round-trip mode in JSON and compact JSON: writes a float, or a time as its number of seconds,
/// as a JSON number (RFC 8259) that reads back as the same double: the digits of
/// [`write_exact_float`]. JSON has no number for NaN and the infinities, and no literal reads
/// back as a subnormal number (spec §5.3, §10.8); all three are errors.
pub(crate) fn write_json_number(out: &mut String, v: f64, kind: &str) -> Result<(), String> {
    if !v.is_finite() {
        let what = if v.is_nan() { "NaN" } else { "infinite" };
        let elsewhere = if v.is_nan() && kind == "time" {
            "no text reads back as a NaN time in any format (spec §5.4)"
        } else {
            "the config and YAML formats write it"
        };
        return Err(format!(
            "a {what} {kind}, which JSON has no number for (RFC 8259); {elsewhere}"
        ));
    }
    if v.is_subnormal() && kind == "time" {
        return Err(format!(
            "the subnormal time {v:e}: in JSON a time is its number of seconds, and no literal \
             reads back as a subnormal number (spec §5.3); the config and YAML formats write it \
             with `ms` (spec §10.8)"
        ));
    }
    write_finite(out, v, kind)
}

/// The shortest digits that identify the finite double `v`, in a form that has a `.` or an
/// exponent: Rust's `Debug` formatting of `f64`, which writes the shortest representation that
/// round-trips (at most 24 bytes, far below the length limit of §5.3).
fn write_finite(out: &mut String, v: f64, kind: &str) -> Result<(), String> {
    if v.is_subnormal() {
        return Err(format!(
            "the subnormal {kind} {v:e}: every literal below the normal range is an error \
             (spec §5.3, §10.8)"
        ));
    }
    let start = out.len();
    let _ = write!(out, "{v:?}");
    debug_assert!(out[start..].contains(['.', 'e']), "{}", &out[start..]);
    Ok(())
}

fn strip_zeros(s: &str) -> &str {
    if !s.contains('.') {
        return s;
    }
    let s = s.trim_end_matches('0');
    s.strip_suffix('.').unwrap_or(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(v: f64) -> String {
        let mut s = String::new();
        write_float(&mut s, v);
        s
    }

    #[test]
    fn rows_of_the_table() {
        // spec §10.3, examples of each row
        for (v, text) in [
            (2.0, "2.0"),
            (-0.0, "-0.0"),
            (86400.0, "86400.0"),
            (2147483647.0, "2147483647.0"),
            (-2147483648.0, "-2147483648.0"),
            (1e-10, "1e-10"),
            (9e-08, "9e-08"),
            (-9e-08, "-9e-08"),
            (2.00000001, "2.00000001"),
            (-2.00000001, "-2.00000001"),
            (5.00000009, "5.00000009"),
            (1.5, "1.500000"),
            (0.1, "0.100000"),
            (100.25, "100.250000"),
            (3000000000.5, "3000000000.500000"),
            (2147483648.0, "2147483648.000000"),
            (-2147483649.0, "-2147483649.000000"),
            (1e20, "100000000000000000000.000000"),
            (f64::NAN, "nan"),
            (f64::INFINITY, "inf"),
            (f64::NEG_INFINITY, "-inf"),
            (2.99999999, "3.000000"),
            (0.99999999, "1.000000"),
            (-1.99999999, "-2.000000"),
            (1e-7, "0.000000"),
            (1.0000001, "1.000000"),
            (0.0078125, "0.007812"),
            (0.0234375, "0.023438"),
            (0.001, "0.001000"),
        ] {
            assert_eq!(f(v), text, "{v:?}");
        }
        assert_eq!(f(1e300).len(), 301 + 7);
    }

    #[test]
    fn exact_forms() {
        let float = |v: f64| {
            let mut s = String::new();
            write_exact_float(&mut s, v).map(|()| s)
        };
        let time = |v: f64| {
            let mut s = String::new();
            write_exact_time(&mut s, v).map(|()| s)
        };
        for (v, text) in [
            (0.1, "0.1"),
            (1.0, "1.0"),
            (-0.0, "-0.0"),
            (1e16, "1e16"),
            (1.5e-10, "1.5e-10"),
            (f64::MAX, "1.7976931348623157e308"),
            (f64::MIN_POSITIVE, "2.2250738585072014e-308"),
            (f64::NAN, "nan"),
            (f64::INFINITY, "inf"),
            (f64::NEG_INFINITY, "-1e308k"),
        ] {
            assert_eq!(float(v).unwrap(), text, "{v:?}");
        }
        assert!(float(5e-324).is_err());
        assert_eq!(time(1.5).unwrap(), "1.5s");
        assert_eq!(time(0.001).unwrap(), "0.001s");
        assert_eq!(time(-0.0).unwrap(), "-0.0s");
        assert_eq!(time(f64::INFINITY).unwrap(), "1e308ks");
        assert_eq!(time(f64::NEG_INFINITY).unwrap(), "-1e308ks");
        assert!(time(f64::NAN).is_err());
        assert_eq!(
            format!("{SMALLEST_MS_TIME:.16e}"),
            "2.2250738585069563e-311"
        );
        // Subnormal times through `ms` (spec §10.8), both signs, down to the smallest one.
        assert_eq!(time(-1e-310).unwrap(), "-1e-307ms");
        assert_eq!(time(SMALLEST_MS_TIME).unwrap(), "2.2250738585072014e-308ms");
        assert_eq!(
            time(-SMALLEST_MS_TIME).unwrap(),
            "-2.2250738585072014e-308ms"
        );
        let largest_subnormal = f64::MIN_POSITIVE - 5e-324;
        assert!(largest_subnormal.is_subnormal());
        assert!(time(largest_subnormal).unwrap().ends_with("e-305ms"));
        let below = f64::from_bits(SMALLEST_MS_TIME.to_bits() - 1);
        assert!(time(below).is_err());
        assert!(time(-below).is_err());
        assert!(time(5e-324).is_err());
        // Subnormal times from the smallest one up have a form (spec §10.8): the first ones, the
        // last ones and a spread between them.
        let first = SMALLEST_MS_TIME.to_bits();
        let end = f64::MIN_POSITIVE.to_bits();
        let step = (end - first) / 20_011;
        let spread = (0..20_011).map(|i| first + i * step);
        for bits in (first..first + 100).chain(spread).chain(end - 100..end) {
            for v in [f64::from_bits(bits), -f64::from_bits(bits)] {
                let text = time(v).unwrap();
                let m: f64 = text.strip_suffix("ms").unwrap().parse().unwrap();
                assert!(
                    m.is_normal() && (m / 1000.0).to_bits() == v.to_bits(),
                    "{v:e}"
                );
            }
        }
        // JSON: the digits alone; no NaN, infinity or subnormal.
        let json = |v: f64| {
            let mut s = String::new();
            write_json_number(&mut s, v, "time").map(|()| s)
        };
        assert_eq!(json(1.5).unwrap(), "1.5");
        assert_eq!(json(-0.0).unwrap(), "-0.0");
        assert_eq!(json(1e16).unwrap(), "1e16");
        for v in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 1e-310] {
            assert!(json(v).is_err(), "{v:?}");
        }
    }

    #[test]
    fn fifteen_digit_forms() {
        let g = |v: f64| {
            let mut s = String::new();
            write_g15(&mut s, v);
            s
        };
        assert_eq!(g(2.2250738585072014e-308), "2.2250738585072e-308");
        assert_eq!(g(5e-324), "4.94065645841247e-324");
        assert_eq!(g(0.0001), "0.0001");
        assert_eq!(g(0.00001), "1e-05");
        assert_eq!(g(123456789012345.0), "123456789012345");
        assert_eq!(g(1234567890123456.0), "1.23456789012346e+15");
        assert_eq!(g(999.9999999999999), "1000");
    }
}
