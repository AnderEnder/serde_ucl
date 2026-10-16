//! Numbers in every output format (spec §10.2, §10.3).

use std::fmt::Write;

const I32_MIN: f64 = -2_147_483_648.0;
const I32_MAX: f64 = 2_147_483_647.0;

/// Writes a float or a time (in seconds) as spec §10.3 describes: the bytes of the ISO C
/// conversion `%.1f`, `%.15g` or `%f`, chosen by the value, in the C locale.
///
/// The digits are those of the exact decimal value of the double, rounded to the precision,
/// ties to even: [`write_whole`] and [`write_fixed6`] compute them with integers, and Rust's
/// formatting with a precision, which rounds the same way, writes the rest.
///
/// Out of line, so that the callers' string arms keep a frame without the float writers' state.
#[inline(never)]
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
        write_whole(out, v);
    } else if v != t && t_in_range && (v - t).abs() < 1e-7 {
        write_g15(out, v);
    } else {
        write_fixed6(out, v);
    }
}

/// Writes an integer in decimal (spec §10.2): an optional `-` and the digits.
#[inline(never)]
pub(crate) fn write_int(out: &mut String, i: i64) {
    let mut digits = Digits::new();
    digits.put_u64(i.unsigned_abs());
    digits.put_sign(i < 0);
    out.push_str(digits.as_str());
}

/// `%.1f` of a whole number `v` with −2³¹ ≤ `v` ≤ 2³¹−1 (spec §10.3): an optional `-`, also for
/// −0.0, the digits and `.0`.
#[inline(never)]
fn write_whole(out: &mut String, v: f64) {
    debug_assert!(v == v.trunc() && (I32_MIN..=I32_MAX).contains(&v), "{v:?}");
    let mut digits = Digits::new();
    digits.put_bytes(b".0");
    // Exact: `v` is whole and below 2^32 in magnitude.
    digits.put_u64(v.abs() as u64);
    digits.put_sign(v.is_sign_negative());
    out.push_str(digits.as_str());
}

/// `%f` of a finite `v` (spec §10.3): [`fixed6`], or Rust's formatting where that has no result.
#[inline(never)]
fn write_fixed6(out: &mut String, v: f64) {
    match fixed6(v) {
        Some(digits) => out.push_str(digits.as_str()),
        None => {
            let _ = write!(out, "{v:.6}");
        }
    }
}

/// `%f` of `v` (spec §10.3): an optional `-`, the integer digits, `.` and 6 decimals, those of
/// the exact decimal value of `v` rounded to 6 decimals, ties to even. `None` for zero,
/// subnormal and non-finite values, and for values below 2^−75 or of 2^64 and above in
/// magnitude; the `%f` row has no value below 10^−7 (spec §10.3).
///
/// A normal double is `m` × 2^−`k` with `m` < 2^53. For 1 ≤ `k` ≤ 127, the integer part is
/// `m` >> `k` and the fraction `f` = `m` mod 2^`k`; the 6 decimals are `f` × 10^6 / 2^`k`
/// rounded, computed exactly in 128 bits (`f` × 10^6 < 2^73), and a carry out of them goes to the
/// integer part. For −11 ≤ `k` ≤ 0 the value is the whole number `m` × 2^−`k`, below 2^64.
fn fixed6(v: f64) -> Option<Digits> {
    const FRACTION_BITS: u64 = (1 << 52) - 1;
    let bits = v.to_bits();
    let biased = (bits >> 52) & 0x7ff;
    if biased == 0 || biased == 0x7ff {
        return None;
    }
    let m = (bits & FRACTION_BITS) | (1 << 52);
    let k = 1075 - biased.cast_signed();
    let (whole, decimals) = if k <= 0 {
        if k < -11 {
            return None;
        }
        (m << -k, 0)
    } else {
        if k > 127 {
            return None;
        }
        let mask = (1u128 << k) - 1;
        let m = u128::from(m);
        let whole = u64::try_from(m >> k).expect("m < 2^53");
        let scaled = (m & mask) * 1_000_000;
        let mut decimals = u32::try_from(scaled >> k).expect("below 10^6");
        let rest = scaled & mask;
        let half = 1u128 << (k - 1);
        if rest > half || (rest == half && decimals & 1 == 1) {
            decimals += 1;
        }
        if decimals == 1_000_000 {
            (whole + 1, 0)
        } else {
            (whole, decimals)
        }
    };
    let mut digits = Digits::new();
    digits.put_pair(decimals % 100);
    digits.put_pair(decimals / 100 % 100);
    digits.put_pair(decimals / 10_000);
    digits.put_bytes(b".");
    digits.put_u64(whole);
    digits.put_sign(bits >> 63 == 1);
    Some(digits)
}

/// The decimal digit pairs `00` to `99`.
const DIGIT_PAIRS: [u8; 200] = {
    let mut pairs = [0; 200];
    let mut i = 0;
    while i < 100 {
        pairs[2 * i] = b'0' + (i / 10) as u8;
        pairs[2 * i + 1] = b'0' + (i % 10) as u8;
        i += 1;
    }
    pairs
};

/// ASCII text written from its end, at most 32 bytes: a number's decimals first, then its
/// integer digits and sign.
struct Digits {
    buf: [u8; 32],
    start: usize,
}

impl Digits {
    fn new() -> Self {
        Self {
            buf: [0; 32],
            start: 32,
        }
    }

    fn put_bytes(&mut self, bytes: &[u8]) {
        self.start -= bytes.len();
        self.buf[self.start..self.start + bytes.len()].copy_from_slice(bytes);
    }

    /// Two digits of `pair` < 100.
    fn put_pair(&mut self, pair: u32) {
        let i = 2 * pair as usize;
        self.start -= 2;
        self.buf[self.start..self.start + 2].copy_from_slice(&DIGIT_PAIRS[i..i + 2]);
    }

    /// The decimal digits of `n`, without leading zeros (`0` for zero).
    fn put_u64(&mut self, mut n: u64) {
        while n >= 100 {
            self.put_pair((n % 100) as u32);
            n /= 100;
        }
        if n >= 10 {
            self.put_pair(n as u32);
        } else {
            self.start -= 1;
            self.buf[self.start] = b'0' + n as u8;
        }
    }

    fn put_sign(&mut self, negative: bool) {
        if negative {
            self.start -= 1;
            self.buf[self.start] = b'-';
        }
    }

    fn as_str(&self) -> &str {
        std::str::from_utf8(&self.buf[self.start..]).expect("ASCII")
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

    /// A splitmix64 sequence: reproducible test values without a dependency.
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
            let mut z = self.0;
            z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            z ^ (z >> 31)
        }
    }

    /// `write_float` as it was before the integer writers (`main` at `ee0271f`), the reference
    /// for the choice of row.
    fn reference_float(v: f64) -> String {
        let mut out = String::new();
        if v.is_nan() {
            out.push_str(if v.is_sign_negative() { "-nan" } else { "nan" });
            return out;
        }
        if v.is_infinite() {
            out.push_str(if v < 0.0 { "-inf" } else { "inf" });
            return out;
        }
        let t = v.trunc();
        let t_in_range = (I32_MIN..=I32_MAX).contains(&t);
        if v == t && t_in_range {
            let _ = write!(out, "{v:.1}");
        } else if v != t && t_in_range && (v - t).abs() < 1e-7 {
            write_g15(&mut out, v);
        } else {
            let _ = write!(out, "{v:.6}");
        }
        out
    }

    /// `fixed6` against Rust's `{:.6}`, for `v` and `-v` and their neighbours; `fixed6` must
    /// have a result for every normal value below 2^64 in magnitude.
    fn check_fixed6(v: f64) {
        for v in [v, -v, v.next_up(), v.next_down()] {
            let expected = format!("{v:.6}");
            match fixed6(v) {
                Some(digits) => assert_eq!(digits.as_str(), expected, "{v:?}"),
                None => assert!(
                    !v.is_normal()
                        || !(2f64.powi(-75)..18_446_744_073_709_551_616.0).contains(&v.abs()),
                    "{v:?}"
                ),
            }
            let mut out = String::from("x");
            write_fixed6(&mut out, v);
            assert_eq!(&out[1..], expected, "{v:?}");
        }
    }

    #[test]
    fn fixed6_matches_std() {
        let mut rng = Rng(6);
        // Random mantissas for every exponent field where `fixed6` has a result, and a few
        // beyond it on each side (spec §10.3).
        for biased in 1075 - 130..=1075 + 14 {
            for _ in 0..200 {
                let bits = (biased << 52) | (rng.next() & ((1 << 52) - 1));
                check_fixed6(f64::from_bits(bits));
            }
        }
        // Ties at 6 decimals: N + j/128 with j odd (their decimals end in 5 at the 7th place,
        // 0.0078125 = 1/128), N up to 2^46 - 1, the largest N for which they are doubles.
        let top = (1u64 << 46) - 1;
        let mut ns = vec![0, 1, 2, 3, 99, 1_000_000, top - 1, top];
        ns.extend((0..40).map(|_| rng.next() % (top + 1)));
        ns.extend((0..40).map(|_| rng.next() % 100_000));
        for n in ns {
            for j in (1..128).step_by(2) {
                check_fixed6(n as f64 + f64::from(j) / 128.0);
            }
        }
        // Decimal texts of 1 to 17 significant digits, the point anywhere: values near the
        // halfway points of the 6th decimal.
        for _ in 0..20_000 {
            let len = 1 + (rng.next() % 17) as usize;
            let text: String = (0..len)
                .map(|_| char::from(b'0' + (rng.next() % 10) as u8))
                .collect();
            let point = (rng.next() % (len as u64 + 1)) as usize;
            let v: f64 = format!("{}.{}", &text[..point], &text[point..])
                .trim_start_matches('.')
                .parse()
                .unwrap_or(0.0);
            check_fixed6(v);
            check_fixed6(v / 1e6);
        }
        // Carries out of the decimals into the integer part.
        for n in [
            0.0,
            1.0,
            9.0,
            99.0,
            999_999.0,
            2_147_483_647.0,
            4_503_599_627_370_495.0,
        ] {
            for tail in [
                0.999_999_5,
                0.999_999_6,
                0.999_999_4,
                0.999_999_499_999,
                0.999_999,
            ] {
                check_fixed6(n + tail);
            }
        }
        // Bounds: the exponents where `fixed6` starts and stops, the smallest value of the
        // `%f` row, and values that round to zero.
        for v in [
            f64::MIN_POSITIVE,
            1e-7,
            4.999_999e-7,
            5e-7,
            5.000_001e-7,
            1.5e-6,
            2.5e-6,
            0.5,
            2_147_483_648.0,
            4_503_599_627_370_496.0,
            9_007_199_254_740_992.0,
            9_223_372_036_854_775_808.0,
            18_446_744_073_709_549_568.0,
            18_446_744_073_709_551_616.0,
            1e20,
            1e300,
            f64::MAX,
        ] {
            check_fixed6(v);
        }
        assert_eq!(fixed6(-1e-7).unwrap().as_str(), "-0.000000");
        assert_eq!(fixed6(0.999_999_5).unwrap().as_str(), "1.000000");
        assert_eq!(fixed6(0.007_812_5).unwrap().as_str(), "0.007812");
        assert_eq!(fixed6(0.023_437_5).unwrap().as_str(), "0.023438");
        assert!(fixed6(0.0).is_none() && fixed6(5e-324).is_none());
        assert!(fixed6(f64::NAN).is_none() && fixed6(f64::INFINITY).is_none());
    }

    #[test]
    fn whole_and_int_match_std() {
        let mut rng = Rng(1);
        let wholes = [
            0.0, -0.0, 1.0, -1.0, 9.0, 10.0, 99.0, 100.0, I32_MIN, I32_MAX,
        ]
        .into_iter()
        .chain((0..20_000).map(|_| (rng.next() as i64 >> (rng.next() % 64)) as i32 as f64));
        for v in wholes {
            let mut out = String::from("x");
            write_whole(&mut out, v);
            assert_eq!(&out[1..], format!("{v:.1}"), "{v:?}");
        }
        let powers = (0..19).flat_map(|e| {
            let p = 10i64.pow(e);
            [p - 1, p, p + 1, -p - 1, -p, -p + 1]
        });
        let ints = [0, i64::MIN, i64::MIN + 1, i64::MAX, i64::MAX - 1]
            .into_iter()
            .chain(powers)
            .chain((0..20_000).map(|_| rng.next().cast_signed() >> (rng.next() % 64)));
        for i in ints {
            let mut out = String::from("x");
            write_int(&mut out, i);
            assert_eq!(&out[1..], i.to_string());
        }
    }

    #[test]
    fn write_float_matches_reference() {
        let mut rng = Rng(3);
        // Random bits, which are mostly huge or tiny values; exponent fields near the rows'
        // bounds; and values near whole numbers and near the bound 1e-7 of the middle row.
        let random: Vec<f64> = (0..2_000).map(|_| f64::from_bits(rng.next())).collect();
        let near: Vec<f64> = (0..20_000)
            .map(|_| {
                let biased = 1023 - 30 + rng.next() % 64;
                f64::from_bits((biased << 52) | (rng.next() & ((1 << 52) - 1)))
            })
            .collect();
        let edges: Vec<f64> = (0..2_000)
            .flat_map(|_| {
                let t = ((rng.next() % (1 << 33)) as f64 - 4_294_967_296.0).trunc();
                let d = [1e-7, 0.0, 1e-6, 0.5][(rng.next() % 4) as usize];
                [t, t + d, t - d, (t + d).next_up(), (t + d).next_down()]
            })
            .collect();
        let specials = [
            0.0,
            -0.0,
            f64::NAN,
            -f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            5e-324,
            -5e-324,
            I32_MAX,
            I32_MIN,
            I32_MAX + 1.0,
            I32_MIN - 1.0,
            I32_MAX + 0.5,
            I32_MIN - 0.5,
        ];
        for v in random.into_iter().chain(near).chain(edges).chain(specials) {
            assert_eq!(f(v), reference_float(v), "{v:?}");
        }
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
