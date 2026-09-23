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
