//! Numbers, multipliers and time suffixes as libucl reads them (spec §5).
//!
//! Run with `cargo run --example number_parsing`.

use serde::Deserialize;
use serde_ucl::parse::{self, ErrorKind, ParserBuilder};
use serde_ucl::{ParserFlags, UclError, UclValue, from_str};
use std::time::Duration;

/// Every value below is what libucl produces for the same text.
const NUMBERS: &str = r#"
# Plain numbers: an int unless there is a fraction or an exponent. A comment may follow a plain
# number after spaces, but a suffix must be followed directly by the end of the line, `;`, `,`
# or `#`: after a space the value is a string (see `suffix_then_space`).
integer = 42
negative = -17
leading_zeros = 007          # decimal, not octal
float = 2.5
exponent = 1e3               # an exponent makes a float
hex = 0x1F
negative_hex = -0x10

# Decimal multipliers (x 1000, x 10^6, x 10^9) keep the type of the number.
kilo = 5k
# `m` is mega; minutes are `min`.
mega = 2m
giga = 1g
kilo_float = 1.5k

# Binary multipliers (x 1024, x 1024^2, x 1024^3) always give an int; a float is truncated first.
kilobytes = 64kb
megabytes = 512mb
gigabytes = 2gb
float_kilobytes = 1.5kb
hex_kilobytes = 0x10kb

# Time suffixes give time values, a float number of seconds.
seconds = 30s
milliseconds = 250ms
minutes = 1.5min
hours = 2h
days = 1d
weeks = 1w
# A year is 365 days.
years = 1y
# Hex numbers take multipliers but ignore time suffixes.
hex_seconds = 0x10s

# Keywords, lowercase only.
infinity = inf
not_a_number = nan

# Not numbers, so strings.
quoted = "42"
space_before_suffix = 5 kb
suffix_then_space = 30s # a space after a suffix makes the value a string
unknown_suffix = 10tb
version = 1.2.3
plus = +1
leading_dot = .5
"#;

fn main() -> Result<(), UclError> {
    typed_values()?;
    no_time_flag()?;
    range_errors();
    serde_targets()?;
    Ok(())
}

/// The value tree keeps the type: int, float, time or string.
fn typed_values() -> Result<(), UclError> {
    println!("== Types of number values");
    let root = parse::parse(NUMBERS.as_bytes())?;
    let root = root.as_object().expect("the root is an object");
    for (key, entry) in root {
        let value = entry.first();
        println!("{key:22} {:7} {value:?}", value.type_name());
    }

    let int = UclValue::Integer;
    let float = UclValue::Float;
    let time = UclValue::Time;
    let string = |s: &str| UclValue::String(s.to_string());
    let expected = [
        ("integer", int(42)),
        ("negative", int(-17)),
        ("leading_zeros", int(7)),
        ("float", float(2.5)),
        ("exponent", float(1000.0)),
        ("hex", int(31)),
        ("negative_hex", int(-16)),
        ("kilo", int(5_000)),
        ("mega", int(2_000_000)),
        ("giga", int(1_000_000_000)),
        ("kilo_float", float(1500.0)),
        ("kilobytes", int(64 * 1024)),
        ("megabytes", int(512 * 1024 * 1024)),
        ("gigabytes", int(2 * 1024 * 1024 * 1024)),
        ("float_kilobytes", int(1024)),
        ("hex_kilobytes", int(16 * 1024)),
        ("seconds", time(30.0)),
        ("milliseconds", time(0.25)),
        ("minutes", time(90.0)),
        ("hours", time(7200.0)),
        ("days", time(86_400.0)),
        ("weeks", time(604_800.0)),
        ("years", time(31_536_000.0)),
        ("hex_seconds", int(16)),
        ("infinity", float(f64::INFINITY)),
        ("quoted", string("42")),
        ("space_before_suffix", string("5 kb")),
        ("suffix_then_space", string("30s")),
        ("unknown_suffix", string("10tb")),
        ("version", string("1.2.3")),
        ("plus", string("+1")),
        ("leading_dot", string(".5")),
    ];
    for (key, value) in expected {
        assert_eq!(root[key], value, "{key}");
    }
    assert!(root["not_a_number"].as_float().is_some_and(f64::is_nan));
    Ok(())
}

/// Under `no-time`, `s`, `min`, `h`, `d`, `w` and `y` are not suffixes; `ms`, `ks` and `gs` still
/// are (spec §12.3), and the multipliers are unaffected.
fn no_time_flag() -> Result<(), UclError> {
    println!("\n== The no-time flag");
    let mut parser = ParserBuilder::new()
        .with_flags(ParserFlags::NO_TIME)
        .build();
    let root = parser.parse(b"a = 30s\nb = 250ms\nc = 2min\nd = 64kb\ne = 1ks\n")?;
    let root = root.as_object().expect("the root is an object");
    for (key, entry) in root {
        println!("{key} = {:?}", entry.first());
    }
    assert_eq!(root["a"], UclValue::String("30s".into()));
    assert_eq!(root["b"], UclValue::Time(0.25));
    assert_eq!(root["c"], UclValue::String("2min".into()));
    assert_eq!(root["d"], UclValue::Integer(65_536));
    assert_eq!(root["e"], UclValue::Time(1000.0));
    Ok(())
}

/// An int outside the 64-bit signed range, or a float that overflows, rejects the whole
/// document; every other malformed number is a string (spec §5.8).
fn range_errors() {
    println!("\n== Range errors");
    assert!(parse::parse(b"a = 9223372036854775807").is_ok());
    for input in [
        "a = 9223372036854775808",
        "a = 0xFFFFFFFFFFFFFFFF",
        "a = 1e309",
    ] {
        let err = parse::parse(input.as_bytes()).unwrap_err();
        println!("{input:28} -> {err}");
        assert_eq!(err.kind(), &ErrorKind::NumberOutOfRange);
    }
}

#[derive(Debug, Deserialize)]
struct Limits {
    /// Sizes read as integers.
    max_body: u64,
    cache: u64,
    /// A time read as seconds.
    timeout: f64,
    /// A time into a `Duration` through the `time` module.
    #[serde(with = "serde_ucl::time")]
    keepalive: Duration,
    /// An integral float or time converts to an integer target.
    retries: u32,
    /// A repeated key is a sequence.
    port: Vec<u16>,
}

fn serde_targets() -> Result<(), UclError> {
    println!("\n== Numbers into Rust types");
    let limits: Limits = from_str(
        "max_body = 16mb\ncache = 2gb\ntimeout = 1.5min\nkeepalive = 250ms\n\
         retries = 3.0\nport = 80\nport = 0x1bb\n",
    )?;
    println!("{limits:#?}");
    assert_eq!(limits.max_body, 16 * 1024 * 1024);
    assert_eq!(limits.cache, 2 * 1024 * 1024 * 1024);
    assert_eq!(limits.timeout, 90.0);
    assert_eq!(limits.keepalive, Duration::from_millis(250));
    assert_eq!(limits.retries, 3);
    assert_eq!(limits.port, [80, 443]);

    // A float that is not integral does not fit an integer target.
    #[derive(Debug, Deserialize)]
    struct Retries {
        #[allow(
            dead_code,
            reason = "a deserialization target; the example shows the error"
        )]
        retries: u32,
    }
    let err = from_str::<Retries>("retries = 2.5").unwrap_err();
    println!("retries = 2.5 -> {err}");
    assert!(matches!(err, UclError::Deserialize(_)));
    // The error names the value and where it was written.
    let position = err.position().expect("a position");
    assert_eq!((position.line, position.column), (1, 11));
    Ok(())
}
