//! Serde round trips (clean-room work item C4b, spec §10.8).
//!
//! What serde writes must read back as the same value: through the new parser core
//! (`serde_ucl::parse`) and through libucl. Values are compared as typed dumps in the schema of
//! `tests/conformance.rs` (entries in order, every value of a multi-value entry, times apart from
//! floats, floats by their bits), never with `PartialEq`, which ignores key order and the sign of
//! zero and never equates NaN. A typed Rust value is compared through `to_value` of the value
//! and of what `from_value` gives back.
//!
//! - `generated_*`: random `UclValue` trees and typed values from a fixed seed (set
//!   `UCL_SERDE_SEED` to try another), serialized in every format and read back with the core.
//! - `oracle_*`: the same, read back by libucl's `target/libucl-oracle/ucl-dump` (or the binary
//!   named by `UCL_DUMP`), one document per batch; skipped when the binary is missing.
//! - `corpus_reads_back`: `tests/serde_corpus/` holds the output of fixed values in every format, and
//!   libucl's typed dump of each file (`<file>.golden.json`), so the libucl side is checked
//!   without the binary. With the binary, the dumps are checked to be current.
//!   `UCL_SERDE_REGEN=1 cargo test --test serde_roundtrip corpus` rewrites the corpus with the
//!   oracle; it refuses to write a dump that differs from the value serialized.
//!
//! - `json_output_is_json`: what `to_json_string` and `to_json_string_compact` write for the
//!   generated values is JSON that `serde_json` reads as the value, times as their seconds
//!   (WORKLIST.md C4, decision 2).
//!
//! JSON has no form for NaN and infinite floats and times, and writes a time as its number of
//! seconds, which reads back as a float: in JSON and compact JSON, a value is compared with its
//! reading in which every time is a float ([`expected_reading`]).
//!
//! Reader variables: libucl's oracle and `read_core` define `FILENAME` and `CURDIR` and register
//! `ABI`. The config format reads back exactly whatever is registered. In JSON, compact JSON and
//! YAML a string that refers to `FILENAME` or `CURDIR` must be an error, and one that refers to
//! `ABI`, a variable the reader registers, expands (a reader precondition documented in
//! `serde_ucl::ser`), so values that hold one are written but not compared.

use indexmap::IndexMap;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Value as J, json};
use serde_ucl::error::SerdeError;
use serde_ucl::parse::Parser;
use serde_ucl::{UclError, UclObject, UclValue, from_value, to_value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

// ---------------------------------------------------------------------------------------------
// Formats, reading back, dumps

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Fmt {
    Config,
    Json,
    Compact,
    Yaml,
}

const FORMATS: [Fmt; 4] = [Fmt::Config, Fmt::Json, Fmt::Compact, Fmt::Yaml];

impl Fmt {
    fn write<T: ?Sized + Serialize>(self, value: &T) -> Result<String, UclError> {
        match self {
            Fmt::Config => serde_ucl::to_string(value),
            Fmt::Json => serde_ucl::to_json_string(value),
            Fmt::Compact => serde_ucl::to_json_string_compact(value),
            Fmt::Yaml => serde_ucl::to_yaml_string(value),
        }
    }

    /// The extension of the corpus files.
    fn extension(self) -> &'static str {
        match self {
            Fmt::Config => "ucl",
            Fmt::Json => "json",
            Fmt::Compact => "compact.json",
            Fmt::Yaml => "yaml",
        }
    }

    /// Whether strings are written double-quoted, and so expand variable references.
    fn expands(self) -> bool {
        self != Fmt::Config
    }

    /// JSON or compact JSON, which are valid JSON (WORKLIST.md C4, decision 2).
    fn is_json(self) -> bool {
        matches!(self, Fmt::Json | Fmt::Compact)
    }
}

/// Reads text back with the new core, as a document given as bytes (so `FILENAME` and `CURDIR`
/// are defined), with `ABI` registered like the oracle's.
fn read_core(text: &[u8]) -> Result<UclValue, String> {
    let mut parser = Parser::new();
    parser.register_variable("ABI", "unknown");
    parser.parse(text).map_err(|e| e.to_string())
}

/// A typed dump in the schema of `tools/ucl-dump` (see `tests/conformance.rs`).
fn dump(value: &UclValue) -> J {
    match value {
        UclValue::Object(obj) => {
            let entries: Vec<J> = obj
                .iter()
                .map(
                    |(k, entry)| json!({"k": k, "v": entry.values().map(dump).collect::<Vec<_>>()}),
                )
                .collect();
            json!({"t": "object", "entries": entries})
        }
        UclValue::Array(items) => {
            json!({"t": "array", "v": items.iter().map(dump).collect::<Vec<_>>()})
        }
        UclValue::Integer(i) => json!({"t": "int", "v": i.to_string()}),
        UclValue::Float(f) => json!({"t": "float", "v": format!("{f:?}")}),
        UclValue::Time(t) => json!({"t": "time", "v": format!("{t:?}")}),
        UclValue::String(s) => json!({"t": "string", "v": s}),
        UclValue::Boolean(b) => json!({"t": "bool", "v": b}),
        UclValue::Null => json!({"t": "null"}),
    }
}

/// Removes priorities from an oracle dump; every value here has priority 0, and the root's and
/// array elements' priorities are not observable (spec §8.7).
fn strip_priorities(node: &mut J) {
    match node {
        J::Object(map) => {
            map.remove("pri");
            for value in map.values_mut() {
                strip_priorities(value);
            }
        }
        J::Array(items) => items.iter_mut().for_each(strip_priorities),
        _ => {}
    }
}

/// The first difference between two dumps. Floats and times are compared by their bits, NaN
/// equal to NaN.
fn find_diff(expected: &J, actual: &J, path: &str) -> Option<String> {
    if let (Some(et), Some(at)) = (expected.get("t"), actual.get("t"))
        && et == at
        && (et == "float" || et == "time")
    {
        let e = expected["v"].as_str().and_then(|s| s.parse::<f64>().ok());
        let a = actual["v"].as_str().and_then(|s| s.parse::<f64>().ok());
        let same = match (e, a) {
            (Some(e), Some(a)) => e.to_bits() == a.to_bits() || (e.is_nan() && a.is_nan()),
            _ => false,
        };
        return (!same).then(|| format!("{path}: expected {expected} got {actual}"));
    }
    match (expected, actual) {
        (J::Object(e), J::Object(a)) => {
            let keys: BTreeSet<_> = e.keys().chain(a.keys()).collect();
            for k in keys {
                match (e.get(k), a.get(k)) {
                    (Some(ev), Some(av)) => {
                        if let Some(d) = find_diff(ev, av, &format!("{path}.{k}")) {
                            return Some(d);
                        }
                    }
                    _ => return Some(format!("{path}: expected {expected} got {actual}")),
                }
            }
            None
        }
        (J::Array(e), J::Array(a)) => {
            for (i, (ev, av)) in e.iter().zip(a).enumerate() {
                if let Some(d) = find_diff(ev, av, &format!("{path}[{i}]")) {
                    return Some(d);
                }
            }
            (e.len() != a.len())
                .then(|| format!("{path}: length expected {} got {}", e.len(), a.len()))
        }
        _ if expected == actual => None,
        _ => Some(format!("{path}: expected {expected} got {actual}")),
    }
}

/// Whether `s` refers to the variable `name` (spec §7.3, §7.4): `$name`, with or without text
/// after it, or `${name}`. Conservative: `$$` before the name, which can keep the text as written
/// (§7.5), is not taken into account.
fn refers_to(s: &str, name: &str) -> bool {
    s.match_indices('$').any(|(i, _)| {
        let rest = &s[i + 1..];
        rest.starts_with(name)
            || rest
                .strip_prefix('{')
                .and_then(|r| r.strip_prefix(name))
                .is_some_and(|r| r.starts_with('}'))
    })
}

/// Whether any string of `value` satisfies `test`. Keys never expand, so they are not looked at.
fn any_string(value: &UclValue, test: &dyn Fn(&str) -> bool) -> bool {
    match value {
        UclValue::String(s) => test(s),
        UclValue::Object(obj) => obj
            .iter()
            .any(|(_, e)| e.values().any(|v| any_string(v, test))),
        UclValue::Array(items) => items.iter().any(|v| any_string(v, test)),
        _ => false,
    }
}

/// Whether `value` has no form in `fmt` (see `serde_ucl::ser`), so that serializing it must fail
/// with `SerdeError::Unrepresentable`: in the config format a string that single quotes cannot
/// hold, in the others a string that refers to a file variable; and a float or time of
/// [`number_unwritable`].
fn expect_error(value: &UclValue, fmt: Fmt) -> bool {
    let strings = if fmt == Fmt::Config {
        any_string(value, &config_unwritable)
    } else {
        any_string(value, &|s| {
            refers_to(s, "FILENAME") || refers_to(s, "CURDIR")
        })
    };
    strings || any_number(value, &|v, time| number_unwritable(v, time, fmt))
}

/// Whether the output of `value` in `fmt` expands when read by `read_core` or the oracle, which
/// register `ABI`: a reader precondition (see `serde_ucl::ser`), so such values are not compared.
fn reader_dependent(value: &UclValue, fmt: Fmt) -> bool {
    fmt.expands() && any_string(value, &|s| refers_to(s, "ABI"))
}

/// The smallest positive time with a form: the smallest normal float followed by `ms`
/// (spec §10.8).
const SMALLEST_MS_TIME: f64 = f64::MIN_POSITIVE / 1000.0;

/// Whether any float or time of `value` satisfies `test`, which is told whether it is a time.
fn any_number(value: &UclValue, test: &dyn Fn(f64, bool) -> bool) -> bool {
    match value {
        UclValue::Float(f) => test(*f, false),
        UclValue::Time(t) => test(*t, true),
        UclValue::Object(obj) => obj
            .iter()
            .any(|(_, e)| e.values().any(|v| any_number(v, test))),
        UclValue::Array(items) => items.iter().any(|v| any_number(v, test)),
        _ => false,
    }
}

/// Whether a float or time has no form in `fmt` (see `serde_ucl::ser`, spec §10.8): a subnormal
/// float, a NaN time and a time closer to zero than [`SMALLEST_MS_TIME`] in every format, and in
/// JSON also NaN, the infinities and every subnormal value.
fn number_unwritable(v: f64, time: bool, fmt: Fmt) -> bool {
    if fmt.is_json() {
        !v.is_finite() || v.is_subnormal()
    } else if time {
        v.is_nan() || (v.is_subnormal() && v.abs() < SMALLEST_MS_TIME)
    } else {
        v.is_subnormal()
    }
}

/// The value that reading the output of `value` in `fmt` gives: `value` itself, except that in
/// JSON and compact JSON every time is written as its seconds and so reads back as a float.
fn expected_reading(value: &UclValue, fmt: Fmt) -> UclValue {
    if !fmt.is_json() {
        return value.clone();
    }
    match value {
        UclValue::Time(t) => UclValue::Float(*t),
        UclValue::Object(obj) => {
            let mut out = UclObject::new();
            for (k, e) in obj.iter() {
                for v in e.values() {
                    out.append(k.clone(), expected_reading(v, fmt));
                }
            }
            UclValue::Object(out)
        }
        UclValue::Array(items) => {
            UclValue::Array(items.iter().map(|v| expected_reading(v, fmt)).collect())
        }
        other => other.clone(),
    }
}

/// Whether a string has no config form (see `serde_ucl::ser`): it contains `$`, and a
/// backslash, paired from the left, stands before `'`, LF, CR or the end.
fn config_unwritable(s: &str) -> bool {
    if !s.contains('$') {
        return false;
    }
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\' {
            match b.get(i + 1) {
                None | Some(b'\'' | b'\n' | b'\r') => return true,
                Some(_) => i += 2,
            }
        } else {
            i += 1;
        }
    }
    false
}

/// How one check ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Checked {
    /// Written, read back and equal.
    Exact,
    /// The value has no form in the format, and serializing it failed as it must.
    Error,
    /// Written, but not compared: it refers to `ABI` (see [`reader_dependent`]).
    Skipped,
}

/// Serializes `value` in `fmt`, reads the text back with the core and compares.
fn check_core(value: &UclValue, fmt: Fmt) -> Result<Checked, String> {
    let expected_error = expect_error(value, fmt);
    let text = match fmt.write(value) {
        Ok(text) if expected_error => {
            return Err(format!("{fmt:?}: expected an error, wrote {text:?}"));
        }
        Ok(text) => text,
        Err(UclError::Serde(SerdeError::Unrepresentable(_))) if expected_error => {
            return Ok(Checked::Error);
        }
        Err(e) => return Err(format!("{fmt:?}: serialization failed: {e}")),
    };
    if reader_dependent(value, fmt) {
        return Ok(Checked::Skipped);
    }
    let back = read_core(text.as_bytes()).map_err(|e| format!("{fmt:?}: {e}\n{text}"))?;
    match find_diff(&dump(&expected_reading(value, fmt)), &dump(&back), "$") {
        None => Ok(Checked::Exact),
        Some(d) => Err(format!("{fmt:?}: {d}\n--- text:\n{text}")),
    }
}

/// Counts of [`Checked`] outcomes per format.
#[derive(Debug, Default)]
struct Tally {
    exact: [usize; 4],
    error: [usize; 4],
    skipped: [usize; 4],
}

impl Tally {
    fn add(&mut self, format: usize, checked: Checked) {
        match checked {
            Checked::Exact => self.exact[format] += 1,
            Checked::Error => self.error[format] += 1,
            Checked::Skipped => self.skipped[format] += 1,
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Generator

/// SplitMix64: a small deterministic generator, so that every run checks the same values.
struct Rng {
    state: u64,
    /// Whether generated strings may refer to reader variables (see [`REFERENCE_PIECES`]).
    references: bool,
    /// Whether generated floats and times may be values that JSON cannot write: NaN, the
    /// infinities and subnormal times.
    non_json: bool,
}

impl Rng {
    fn new() -> Self {
        let seed = std::env::var("UCL_SERDE_SEED")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0x5eed_c4b0);
        Rng {
            state: seed,
            references: true,
            non_json: true,
        }
    }

    fn next(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn chance(&mut self, percent: usize) -> bool {
        self.below(100) < percent
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

const STRING_PIECES: &[&str] = &[
    "a",
    "b",
    "Z",
    "0",
    "9",
    " ",
    "\t",
    "\n",
    "\r",
    "\r\n",
    "\"",
    "'",
    "\\",
    "\\\\",
    "$",
    "{",
    "}",
    "[",
    "]",
    ";",
    ",",
    "#",
    "/",
    "*",
    "/*",
    "=",
    ":",
    "\u{0}",
    "\u{1}",
    "\u{b}",
    "\u{c}",
    "\u{8}",
    "\u{1f}",
    "\u{7f}",
    "é",
    "€",
    "😀",
    "EOD",
    "\nEOD\n",
    "<<",
    "<<EOD\n",
    "$ABI",
    "${CURDIR}",
    "$FILENAME",
    "$$",
    "${",
    "${x}",
    "nan",
    "true",
    "-",
    ".",
    "1",
    "1s",
    "null",
];

const KEY_PIECES: &[&str] = &[
    "a", "k", "Z", "_", "/", "0", "7", "-", ".", "é", "日", " ", "$", "\"", "\\", ";", "#", "}",
    "{", "[", "]", "=", ":", ",", "\n", "\t", "\u{0}", "\u{7f}", "'", "+", "*", "ABI",
];

fn gen_text(rng: &mut Rng, pieces: &[&str], max: usize) -> String {
    let n = rng.below(max + 1);
    (0..n).map(|_| *rng.pick(pieces)).collect()
}

/// The pieces of [`STRING_PIECES`] that refer to reader variables.
const REFERENCE_PIECES: &[&str] = &["$ABI", "${CURDIR}", "$FILENAME"];

fn gen_string(rng: &mut Rng) -> String {
    let n = rng.below(7);
    (0..n)
        .map(|_| match *rng.pick(STRING_PIECES) {
            piece if !rng.references && REFERENCE_PIECES.contains(&piece) => "$",
            piece => piece,
        })
        .collect()
}

fn gen_key(rng: &mut Rng) -> String {
    loop {
        let key = gen_text(rng, KEY_PIECES, 4);
        if !key.is_empty() {
            return key;
        }
    }
}

const SPECIAL_FLOATS: &[f64] = &[
    0.0,
    -0.0,
    0.1,
    1.0,
    -1.5,
    1.0 / 3.0,
    2.0 / 3.0,
    1e16,
    1e15,
    1e-4,
    1e-5,
    1e-7,
    5e-5,
    1e21,
    1e22,
    1e300,
    -1e-300,
    123456789.123,
    0.30000000000000004,
    2147483648.0,
    9007199254740993.0,
    f64::MAX,
    f64::MIN,
    f64::MIN_POSITIVE,
    -f64::MIN_POSITIVE,
    f64::EPSILON,
    f64::INFINITY,
    f64::NEG_INFINITY,
];

/// A float with a form in the config and YAML formats (spec §10.8): any finite non-subnormal
/// value, NaN and the infinities; without `non_json`, finite values only.
fn gen_float(rng: &mut Rng) -> f64 {
    loop {
        let v = match rng.below(5) {
            0 => *rng.pick(SPECIAL_FLOATS),
            1 => f64::NAN,
            2 => {
                let digits = (rng.next() % 2_000_001) as f64 - 1_000_000.0;
                digits / 10f64.powi(rng.below(12) as i32)
            }
            _ => f64::from_bits(rng.next()),
        };
        if !v.is_subnormal() && (rng.non_json || v.is_finite()) {
            return v;
        }
    }
}

/// Subnormal times at the edges of the range that `ms` gives (spec §10.8): the smallest one,
/// the times just below it, which have no form, and the largest subnormal value.
/// (`2.2250738585065e-311` is the value just below the smallest one.)
const SUBNORMAL_TIMES: &[f64] = &[
    SMALLEST_MS_TIME,
    -SMALLEST_MS_TIME,
    2.2250738585065e-311,
    5e-324,
    -1e-315,
    2.225073858507201e-308,
    1e-310,
];

/// A time: a float of [`gen_float`] other than NaN, or with `non_json` sometimes a subnormal
/// time, most of which have a form through `ms`.
fn gen_time(rng: &mut Rng) -> f64 {
    if rng.non_json && rng.chance(10) {
        return match rng.below(2) {
            0 => *rng.pick(SUBNORMAL_TIMES),
            _ => {
                let v = f64::from_bits(rng.next() % (1 << 52));
                if rng.chance(50) { -v } else { v }
            }
        };
    }
    loop {
        let v = gen_float(rng);
        if !v.is_nan() {
            return v;
        }
    }
}

fn gen_int(rng: &mut Rng) -> i64 {
    match rng.below(3) {
        0 => *rng.pick(&[0, 1, -1, 42, i64::MIN, i64::MAX, i64::MIN + 1, 1 << 53]),
        1 => rng.next() as i64 % 1000,
        _ => rng.next() as i64,
    }
}

fn gen_value(rng: &mut Rng, depth: usize) -> UclValue {
    let kinds = if depth == 0 { 7 } else { 10 };
    match rng.below(kinds) {
        0 => UclValue::Integer(gen_int(rng)),
        1 => UclValue::Float(gen_float(rng)),
        2 => UclValue::Time(gen_time(rng)),
        3 | 4 => UclValue::String(gen_string(rng)),
        5 => UclValue::Boolean(rng.chance(50)),
        6 => UclValue::Null,
        7 | 8 => UclValue::Object(gen_object(rng, depth - 1)),
        _ => UclValue::Array(
            (0..rng.below(5))
                .map(|_| gen_value(rng, depth - 1))
                .collect(),
        ),
    }
}

/// An object whose keys sometimes repeat, which makes multi-value entries (spec §8.2).
fn gen_object(rng: &mut Rng, depth: usize) -> UclObject {
    let mut object = UclObject::new();
    let mut keys: Vec<String> = Vec::new();
    for _ in 0..rng.below(6) {
        let key = if !keys.is_empty() && rng.chance(25) {
            rng.pick(&keys).clone()
        } else {
            gen_key(rng)
        };
        keys.push(key.clone());
        object.append(key, gen_value(rng, depth));
    }
    object
}

/// A document root: an object, or sometimes an array.
fn gen_root(rng: &mut Rng) -> UclValue {
    if rng.chance(15) {
        UclValue::Array((0..rng.below(5)).map(|_| gen_value(rng, 2)).collect())
    } else {
        UclValue::Object(gen_object(rng, 3))
    }
}

// ---------------------------------------------------------------------------------------------
// Typed values

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Sample {
    flag: bool,
    small: i8,
    medium: i16,
    word: i32,
    big: i64,
    byte: u8,
    ushort: u16,
    uword: u32,
    ubig: u64,
    wide: i128,
    uwide: u128,
    single: f32,
    double: f64,
    letter: char,
    text: String,
    maybe: Option<i32>,
    nothing: Option<String>,
    list: Vec<i64>,
    octets: Vec<u8>,
    raw: Bytes,
    nested: Vec<Inner>,
    by_name: BTreeMap<String, f64>,
    by_number: BTreeMap<u32, String>,
    by_signed: BTreeMap<i64, bool>,
    by_flag: BTreeMap<bool, i8>,
    by_char: BTreeMap<char, u8>,
    by_color: BTreeMap<Color, u16>,
    ordered: IndexMap<String, Vec<String>>,
    pair: (i32, String, f64),
    fixed: [u16; 3],
    unit: (),
    marker: Marker,
    id: Id,
    shapes: Vec<Shape>,
    #[serde(with = "serde_ucl::time")]
    timeout: Duration,
    empty_list: Vec<String>,
    empty_map: BTreeMap<String, i32>,
    tree: UclValue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Inner {
    name: String,
    weight: f32,
    tags: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
enum Color {
    Red,
    Green,
    Blue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Marker;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Id(i64);

#[derive(Debug, Clone, Serialize, Deserialize)]
enum Shape {
    Point,
    Circle(f64),
    Rect(i32, i32),
    Poly { sides: u8, name: String },
}

/// Bytes written with `serialize_bytes`, as `serde_bytes` does.
#[derive(Debug, Clone)]
struct Bytes(Vec<u8>);

impl Serialize for Bytes {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_bytes(&self.0)
    }
}

impl<'de> Deserialize<'de> for Bytes {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = Bytes;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("bytes")
            }
            fn visit_bytes<E: serde::de::Error>(self, v: &[u8]) -> Result<Bytes, E> {
                Ok(Bytes(v.to_vec()))
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Bytes, E> {
                Ok(Bytes(v.as_bytes().to_vec()))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<Bytes, A::Error> {
                let mut out = Vec::new();
                while let Some(b) = seq.next_element()? {
                    out.push(b);
                }
                Ok(Bytes(out))
            }
        }
        d.deserialize_byte_buf(V)
    }
}

const CHARS: &[char] = &[
    'a', 'Z', '0', ' ', '\0', '\n', '\t', '"', '\'', '\\', '$', '{', 'é', '😀', '\u{7f}',
];

/// Without `non_json`, finite values only.
fn gen_f32(rng: &mut Rng) -> f32 {
    loop {
        let v = match rng.below(4) {
            0 => *rng.pick(&[
                0.1f32,
                -0.0,
                f32::MAX,
                f32::MIN_POSITIVE,
                f32::NAN,
                f32::INFINITY,
                1e-45,
            ]),
            _ => f32::from_bits(rng.next() as u32),
        };
        if rng.non_json || v.is_finite() {
            return v;
        }
    }
}

/// A duration that a 64-bit float of seconds holds exactly (see `serde_ucl::time`).
fn gen_duration(rng: &mut Rng) -> Duration {
    let d = match rng.below(3) {
        0 => Duration::from_millis(rng.next() % 10_000_000),
        1 => Duration::from_secs(rng.next() % (1 << 40)),
        _ => Duration::new(rng.next() % 100_000, (rng.next() % 1_000_000_000) as u32),
    };
    match Duration::try_from_secs_f64(d.as_secs_f64()) {
        Ok(back) if back == d => d,
        _ => Duration::from_secs(1),
    }
}

fn gen_sample(rng: &mut Rng) -> Sample {
    let int = |rng: &mut Rng| gen_int(rng);
    Sample {
        flag: rng.chance(50),
        small: rng.next() as i8,
        medium: rng.next() as i16,
        word: rng.next() as i32,
        big: int(rng),
        byte: rng.next() as u8,
        ushort: rng.next() as u16,
        uword: rng.next() as u32,
        ubig: rng.next() >> 1,
        wide: i128::from(int(rng)),
        uwide: u128::from(rng.next() >> 1),
        single: gen_f32(rng),
        double: gen_float(rng),
        letter: *rng.pick(CHARS),
        text: gen_string(rng),
        maybe: rng.chance(50).then(|| rng.next() as i32),
        nothing: None,
        list: (0..rng.below(4)).map(|_| int(rng)).collect(),
        octets: (0..rng.below(4)).map(|_| rng.next() as u8).collect(),
        raw: Bytes((0..rng.below(5)).map(|_| rng.next() as u8).collect()),
        nested: (0..rng.below(3))
            .map(|_| Inner {
                name: gen_string(rng),
                weight: gen_f32(rng),
                tags: (0..rng.below(3)).map(|_| gen_string(rng)).collect(),
            })
            .collect(),
        by_name: (0..rng.below(4))
            .map(|_| (gen_key(rng), gen_float(rng)))
            .collect(),
        by_number: (0..rng.below(4))
            .map(|_| (rng.next() as u32, gen_string(rng)))
            .collect(),
        by_signed: (0..rng.below(4))
            .map(|_| (int(rng), rng.chance(50)))
            .collect(),
        by_flag: (0..rng.below(3))
            .map(|_| (rng.chance(50), rng.next() as i8))
            .collect(),
        by_char: (0..rng.below(3))
            .map(|_| (*rng.pick(CHARS), rng.next() as u8))
            .collect(),
        by_color: (0..rng.below(3))
            .map(|_| {
                (
                    *rng.pick(&[Color::Red, Color::Green, Color::Blue]),
                    rng.next() as u16,
                )
            })
            .collect(),
        ordered: (0..rng.below(4))
            .map(|_| {
                (
                    gen_key(rng),
                    (0..rng.below(3)).map(|_| gen_string(rng)).collect(),
                )
            })
            .collect(),
        pair: (rng.next() as i32, gen_string(rng), gen_float(rng)),
        fixed: [rng.next() as u16, rng.next() as u16, rng.next() as u16],
        unit: (),
        marker: Marker,
        id: Id(int(rng)),
        shapes: (0..rng.below(5))
            .map(|_| match rng.below(4) {
                0 => Shape::Point,
                1 => Shape::Circle(gen_float(rng)),
                2 => Shape::Rect(rng.next() as i32, rng.next() as i32),
                _ => Shape::Poly {
                    sides: rng.next() as u8,
                    name: gen_string(rng),
                },
            })
            .collect(),
        timeout: gen_duration(rng),
        empty_list: Vec::new(),
        empty_map: BTreeMap::new(),
        tree: gen_value(rng, 2),
    }
}

/// The value tree of a typed value after deserializing it back from `back`.
fn typed_back<T: Serialize + DeserializeOwned>(back: UclValue) -> Result<UclValue, UclError> {
    to_value(&from_value::<T>(back)?)
}

/// Serializes a typed value in `fmt`, reads it back with the core, deserializes it and compares
/// the value trees.
fn check_typed<T: Serialize + DeserializeOwned>(original: &T, fmt: Fmt) -> Result<Checked, String> {
    let tree = to_value(original).map_err(|e| e.to_string())?;
    match check_core(&tree, fmt)? {
        Checked::Exact => {}
        other => return Ok(other),
    }
    let text = fmt.write(original).map_err(|e| e.to_string())?;
    let back = read_core(text.as_bytes())?;
    let again = typed_back::<T>(back).map_err(|e| format!("{fmt:?}: from_value: {e}\n{text}"))?;
    // In JSON a `Duration` comes back as itself through `serde_ucl::time`, and a time in a
    // `UclValue` field as a float; both sides are compared as JSON reads them.
    let expected = expected_reading(&tree, fmt);
    match find_diff(&dump(&expected), &dump(&expected_reading(&again, fmt)), "$") {
        None => Ok(Checked::Exact),
        Some(d) => Err(format!("{fmt:?}: typed: {d}\n--- text:\n{text}")),
    }
}

// ---------------------------------------------------------------------------------------------
// Generated round trips through the core

#[test]
fn generated_values_round_trip_through_the_core() {
    let mut rng = Rng::new();
    let mut tally = Tally::default();
    let mut failures = Vec::new();
    for n in 0..3000 {
        // Every other value has no float or time that JSON cannot write.
        rng.non_json = n % 2 == 0;
        let value = gen_root(&mut rng);
        for (i, fmt) in FORMATS.into_iter().enumerate() {
            match check_core(&value, fmt) {
                Ok(checked) => tally.add(i, checked),
                Err(e) => failures.push(format!("value {n}: {e}")),
            }
        }
        // The same tree deserializes into a UclValue unchanged.
        let again: UclValue = from_value(value.clone()).unwrap();
        if let Some(d) = find_diff(&dump(&value), &dump(&again), "$") {
            failures.push(format!("value {n}: from_value::<UclValue>: {d}"));
        }
    }
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures[..failures.len().min(5)].join("\n\n")
    );
    eprintln!("values (config, json, compact, yaml): {tally:?}");
    // Most values are compared in every format, and every kind of outcome occurs.
    assert!(tally.exact.iter().all(|&c| c > 2000), "{tally:?}");
    assert!(tally.error.iter().all(|&c| c > 0), "{tally:?}");
}

#[test]
fn generated_typed_values_round_trip_through_the_core() {
    let mut rng = Rng::new();
    let mut tally = Tally::default();
    let mut failures = Vec::new();
    for n in 0..600 {
        // Every other sample has no reference to a variable, so that the formats with double
        // quotes can write it and compare it, and no float or time that JSON cannot write.
        rng.references = n % 2 == 0;
        rng.non_json = n % 2 == 0;
        let sample = gen_sample(&mut rng);
        for (i, fmt) in FORMATS.into_iter().enumerate() {
            match check_typed(&sample, fmt) {
                Ok(checked) => tally.add(i, checked),
                Err(e) => failures.push(format!("sample {n}: {e}")),
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures[..failures.len().min(3)].join("\n\n")
    );
    eprintln!("typed values (config, json, compact, yaml): {tally:?}");
    assert!(
        tally.exact[0] > 500 && tally.exact[1..].iter().all(|&c| c > 300),
        "{tally:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// JSON output is JSON (WORKLIST.md C4, decision 2)

/// A JSON text as `serde_json` reads it: every member of every object in order, repeated names
/// included, integers as integers and other numbers by the bits of the `f64` serde_json gives.
#[derive(Debug, PartialEq)]
enum JsonTree {
    Null,
    Bool(bool),
    Int(i128),
    Float(u64),
    Str(String),
    Array(Vec<JsonTree>),
    Object(Vec<(String, JsonTree)>),
}

impl<'de> Deserialize<'de> for JsonTree {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = JsonTree;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a JSON value")
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<JsonTree, E> {
                Ok(JsonTree::Null)
            }
            fn visit_bool<E: serde::de::Error>(self, v: bool) -> Result<JsonTree, E> {
                Ok(JsonTree::Bool(v))
            }
            fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<JsonTree, E> {
                Ok(JsonTree::Int(v.into()))
            }
            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<JsonTree, E> {
                Ok(JsonTree::Int(v.into()))
            }
            fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<JsonTree, E> {
                Ok(JsonTree::Float(v.to_bits()))
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<JsonTree, E> {
                Ok(JsonTree::Str(v.to_owned()))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<JsonTree, A::Error> {
                let mut items = Vec::new();
                while let Some(item) = seq.next_element()? {
                    items.push(item);
                }
                Ok(JsonTree::Array(items))
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<JsonTree, A::Error> {
                let mut members = Vec::new();
                while let Some(member) = map.next_entry()? {
                    members.push(member);
                }
                Ok(JsonTree::Object(members))
            }
        }
        d.deserialize_any(V)
    }
}

/// What the JSON output of `value` must read as: one member per value of an entry, times as
/// their seconds.
fn json_tree(value: &UclValue) -> JsonTree {
    match value {
        UclValue::Object(obj) => JsonTree::Object(
            obj.iter()
                .flat_map(|(k, e)| e.values().map(move |v| (k.clone(), json_tree(v))))
                .collect(),
        ),
        UclValue::Array(items) => JsonTree::Array(items.iter().map(json_tree).collect()),
        UclValue::Integer(i) => JsonTree::Int((*i).into()),
        UclValue::Float(f) | UclValue::Time(f) => JsonTree::Float(f.to_bits()),
        UclValue::String(s) => JsonTree::Str(s.clone()),
        UclValue::Boolean(b) => JsonTree::Bool(*b),
        UclValue::Null => JsonTree::Null,
    }
}

/// Serializes `value` (whose tree is `tree`) in `fmt` and reads the text with `serde_json`.
/// Returns whether it was written; a value without a JSON form must fail as `expect_error` says.
fn check_json<T: ?Sized + Serialize>(value: &T, tree: &UclValue, fmt: Fmt) -> Result<bool, String> {
    let expected_error = expect_error(tree, fmt);
    let text = match fmt.write(value) {
        Ok(text) if expected_error => {
            return Err(format!("{fmt:?}: expected an error, wrote {text:?}"));
        }
        Ok(text) => text,
        Err(UclError::Serde(SerdeError::Unrepresentable(_))) if expected_error => {
            return Ok(false);
        }
        Err(e) => return Err(format!("{fmt:?}: serialization failed: {e}")),
    };
    let read: JsonTree =
        serde_json::from_str(&text).map_err(|e| format!("{fmt:?}: not JSON ({e}):\n{text}"))?;
    if read != json_tree(tree) {
        return Err(format!(
            "{fmt:?}: serde_json reads another value from\n{text}"
        ));
    }
    Ok(true)
}

#[test]
fn json_output_is_json() {
    // serde_json keeps the sign of zero and reads the shortest digits exactly.
    let read = |text: &str| serde_json::from_str::<JsonTree>(text).unwrap();
    assert_eq!(read("-0.0"), JsonTree::Float((-0.0f64).to_bits()));
    assert_eq!(
        read("2.2250738585072014e-308"),
        JsonTree::Float(f64::MIN_POSITIVE.to_bits())
    );
    let mut failures = Vec::new();
    let mut written = [0usize; 2];
    let mut errors = [0usize; 2];
    let formats = [Fmt::Json, Fmt::Compact];
    // The values of `generated_values_round_trip_through_the_core`.
    let mut rng = Rng::new();
    for n in 0..3000 {
        rng.non_json = n % 2 == 0;
        let value = gen_root(&mut rng);
        for (i, fmt) in formats.into_iter().enumerate() {
            match check_json(&value, &value, fmt) {
                Ok(true) => written[i] += 1,
                Ok(false) => errors[i] += 1,
                Err(e) => failures.push(format!("value {n}: {e}")),
            }
        }
    }
    // The typed values of `generated_typed_values_round_trip_through_the_core`.
    let mut rng = Rng::new();
    let mut typed_written = [0usize; 2];
    for n in 0..600 {
        rng.references = n % 2 == 0;
        rng.non_json = n % 2 == 0;
        let sample = gen_sample(&mut rng);
        let tree = to_value(&sample).unwrap();
        for (i, fmt) in formats.into_iter().enumerate() {
            match check_json(&sample, &tree, fmt) {
                Ok(true) => typed_written[i] += 1,
                Ok(false) => errors[i] += 1,
                Err(e) => failures.push(format!("sample {n}: {e}")),
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures[..failures.len().min(5)].join("\n\n")
    );
    eprintln!(
        "JSON read by serde_json (json, compact): values {written:?}, typed values \
         {typed_written:?}, errors {errors:?}"
    );
    assert!(written.iter().all(|&c| c > 2000), "{written:?}");
    assert!(typed_written.iter().all(|&c| c > 300), "{typed_written:?}");
    assert!(errors.iter().all(|&c| c > 0), "{errors:?}");
}

// ---------------------------------------------------------------------------------------------
// The oracle

/// The oracle binary, if it is there.
fn oracle() -> Option<PathBuf> {
    let path = std::env::var_os("UCL_DUMP")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR")).join("target/libucl-oracle/ucl-dump")
        });
    path.is_file().then_some(path)
}

/// libucl's typed dump of the file at `path`, read from its own directory.
fn read_oracle(binary: &Path, path: &Path) -> J {
    let output = Command::new(binary)
        .arg(path.file_name().unwrap())
        .current_dir(path.parent().unwrap())
        .output()
        .expect("the oracle runs");
    let mut dump: J = serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "{}: oracle output is not JSON ({e}): {}",
            path.display(),
            String::from_utf8_lossy(&output.stderr)
        )
    });
    strip_priorities(&mut dump);
    dump
}

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("serde_roundtrip");
    fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

/// Writes `batch` in `fmt`, has the oracle read it, and compares.
fn check_oracle_batch(binary: &Path, name: &str, batch: &UclValue, fmt: Fmt) -> Result<(), String> {
    let text = fmt
        .write(batch)
        .map_err(|e| format!("{name} {fmt:?}: {e}"))?;
    let path = scratch(&format!("{name}.{}", fmt.extension()));
    fs::write(&path, &text).unwrap();
    let dump_oracle = read_oracle(binary, &path);
    match find_diff(&dump(&expected_reading(batch, fmt)), &dump_oracle, "$") {
        None => Ok(()),
        Some(d) => Err(format!("{name} {fmt:?}: libucl reads {d}")),
    }
}

/// Root objects whose entries are generated values, and a root array of them.
fn oracle_batches(values: Vec<UclValue>, fmt: Fmt) -> Vec<(String, UclValue)> {
    let usable: Vec<UclValue> = values
        .into_iter()
        .filter(|v| !expect_error(v, fmt) && !reader_dependent(v, fmt))
        .collect();
    let mut batches = Vec::new();
    for (b, chunk) in usable.chunks(100).enumerate() {
        let object: UclObject = chunk
            .iter()
            .enumerate()
            .map(|(i, v)| (format!("k{i}"), v.clone()))
            .collect();
        batches.push((format!("batch{b}"), UclValue::Object(object)));
    }
    batches.push((
        "array".into(),
        UclValue::Array(usable.into_iter().take(100).collect()),
    ));
    batches
}

fn batch_len(batch: &UclValue) -> usize {
    match batch {
        UclValue::Object(object) => object.len(),
        UclValue::Array(items) => items.len(),
        _ => 1,
    }
}

#[test]
fn oracle_reads_generated_values_back() {
    let Some(binary) = oracle() else {
        eprintln!("skipped: no oracle binary (target/libucl-oracle/ucl-dump, or UCL_DUMP)");
        return;
    };
    let mut rng = Rng::new();
    let values: Vec<UclValue> = (0..400)
        .map(|n| {
            rng.non_json = n % 2 == 0;
            gen_value(&mut rng, 3)
        })
        .collect();
    let mut failures = Vec::new();
    for fmt in FORMATS {
        let batches = oracle_batches(values.clone(), fmt);
        let count: usize = batches.iter().map(|(_, b)| batch_len(b)).sum();
        eprintln!(
            "oracle, {fmt:?}: {count} values in {} documents",
            batches.len()
        );
        for (name, batch) in batches {
            if let Err(e) = check_oracle_batch(&binary, &format!("values-{name}"), &batch, fmt) {
                failures.push(e);
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn oracle_reads_generated_typed_values_back() {
    let Some(binary) = oracle() else {
        eprintln!("skipped: no oracle binary (target/libucl-oracle/ucl-dump, or UCL_DUMP)");
        return;
    };
    let mut rng = Rng::new();
    let trees: Vec<UclValue> = (0..200)
        .map(|n| {
            rng.references = n % 2 == 0;
            rng.non_json = n % 2 == 0;
            to_value(&gen_sample(&mut rng)).unwrap()
        })
        .collect();
    let mut failures = Vec::new();
    for fmt in FORMATS {
        let batches = oracle_batches(trees.clone(), fmt);
        let count: usize = batches.iter().map(|(_, b)| batch_len(b)).sum();
        eprintln!(
            "oracle, typed, {fmt:?}: {count} values in {} documents",
            batches.len()
        );
        for (name, batch) in batches {
            if let Err(e) = check_oracle_batch(&binary, &format!("typed-{name}"), &batch, fmt) {
                failures.push(e);
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// ---------------------------------------------------------------------------------------------
// The committed corpus

const CORPUS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/serde_corpus");

/// A corpus entry: a name, the value tree serialized, the formats it is written in, and, for a
/// typed value, how to deserialize the tree read back and serialize it again. An entry whose
/// value has floats or times that JSON cannot write has a second tree without them for JSON and
/// compact JSON (WORKLIST.md C4, decision 2).
struct CorpusEntry {
    name: &'static str,
    tree: UclValue,
    json_tree: Option<UclValue>,
    formats: &'static [Fmt],
    typed: Option<fn(UclValue) -> Result<UclValue, UclError>>,
}

impl CorpusEntry {
    /// The tree written in `fmt`.
    fn tree(&self, fmt: Fmt) -> &UclValue {
        match &self.json_tree {
            Some(tree) if fmt.is_json() => tree,
            _ => &self.tree,
        }
    }

    fn with_json_tree(mut self, tree: UclValue) -> Self {
        self.json_tree = Some(tree);
        self
    }
}

fn entry(name: &'static str, tree: UclValue, formats: &'static [Fmt]) -> CorpusEntry {
    CorpusEntry {
        name,
        tree,
        json_tree: None,
        formats,
        typed: None,
    }
}

/// A typed value, and for JSON `json`, the same type without the floats JSON cannot write.
fn typed_entry<T: Serialize + DeserializeOwned>(
    name: &'static str,
    value: &T,
    json: Option<&T>,
) -> CorpusEntry {
    CorpusEntry {
        name,
        tree: to_value(value).unwrap(),
        json_tree: json.map(|v| to_value(v).unwrap()),
        formats: &FORMATS,
        typed: Some(typed_back::<T>),
    }
}

fn obj<const N: usize>(pairs: [(&str, UclValue); N]) -> UclValue {
    UclValue::Object(pairs.into_iter().collect())
}

fn floats(values: &[f64], time: bool) -> UclValue {
    let items = values.iter().map(|&v| {
        if time {
            UclValue::Time(v)
        } else {
            UclValue::Float(v)
        }
    });
    UclValue::Array(items.collect())
}

fn strings(values: &[&str]) -> UclValue {
    UclValue::Array(
        values
            .iter()
            .map(|s| UclValue::String((*s).to_owned()))
            .collect(),
    )
}

fn fixed_sample() -> Sample {
    Sample {
        flag: true,
        small: i8::MIN,
        medium: i16::MAX,
        word: -7,
        big: i64::MIN,
        byte: 255,
        ushort: 65535,
        uword: u32::MAX,
        ubig: i64::MAX as u64,
        wide: -1,
        uwide: 1 << 62,
        single: 0.1,
        double: 1.0 / 3.0,
        letter: '"',
        text: "Hello, \"world\"\n\ttab\\".into(),
        maybe: Some(3),
        nothing: None,
        list: vec![1, -2, 3],
        octets: vec![0, 127, 255],
        raw: Bytes(b"\x00\xffUCL".to_vec()),
        nested: vec![
            Inner {
                name: "first".into(),
                weight: 1.5,
                tags: vec!["x".into(), "y z".into()],
            },
            Inner {
                name: String::new(),
                weight: f32::NAN,
                tags: vec![],
            },
        ],
        by_name: [
            ("pi".to_string(), std::f64::consts::PI),
            ("e".into(), std::f64::consts::E),
        ]
        .into_iter()
        .collect(),
        by_number: [(1, "one".to_string()), (40000, "big".into())]
            .into_iter()
            .collect(),
        by_signed: [(-5, true), (i64::MAX, false)].into_iter().collect(),
        by_flag: [(false, 0), (true, 1)].into_iter().collect(),
        by_char: [('a', 1), ('é', 2), (' ', 3)].into_iter().collect(),
        by_color: [(Color::Red, 1), (Color::Blue, 3)].into_iter().collect(),
        ordered: [
            ("z".to_string(), vec!["last".to_string()]),
            ("a b".into(), vec![]),
        ]
        .into_iter()
        .collect(),
        pair: (-1, "two".into(), 3.25),
        fixed: [1, 2, 3],
        unit: (),
        marker: Marker,
        id: Id(99),
        shapes: vec![
            Shape::Point,
            Shape::Circle(0.5),
            Shape::Rect(2, -3),
            Shape::Poly {
                sides: 6,
                name: "hex".into(),
            },
        ],
        timeout: Duration::from_millis(1500),
        empty_list: vec![],
        empty_map: BTreeMap::new(),
        tree: obj([("t", UclValue::Time(0.25)), ("n", UclValue::Null)]),
    }
}

fn corpus() -> Vec<CorpusEntry> {
    let mut control = String::new();
    for b in 0u8..0x80 {
        control.push(b as char);
    }
    let long = format!("{}\n{}", "x".repeat(90), "y".repeat(10));
    let mut multi = UclObject::new();
    multi.append("m", UclValue::Integer(1));
    multi.append("m", UclValue::Integer(2));
    multi.append("s", UclValue::String("x".into()));
    multi.append("s", UclValue::Integer(1));
    multi.append("a", UclValue::Array(vec![UclValue::Integer(1)]));
    multi.append("a", UclValue::Integer(2));
    multi.append("o", obj([]));
    multi.append("o", obj([("k", UclValue::Integer(1))]));
    multi.append("e", UclValue::Array(vec![]));
    multi.append("e", UclValue::Time(3.0));
    multi.append("e", UclValue::Null);
    let mut inner = UclObject::new();
    inner.append("x", UclValue::Boolean(true));
    inner.append("x", UclValue::Boolean(false));
    multi.append("nested", UclValue::Object(inner.clone()));
    multi.append(
        "list",
        UclValue::Array(vec![UclValue::Object(inner), UclValue::Array(vec![])]),
    );
    let key_list = [
        "a", "A1", "1", "_x", "/p", "é", "日本", "a-b.c/d", "a b", "-a", ".include", ".x", "a;b",
        "a}b", "a#b", "a,b", "$ABI", "${ABI}", "a=b", "a:b", "a\"b", "a\\b", "\n", "\u{0}", "'",
        "true", "null", "tab\tkey", "k\u{7f}", "[", "+",
    ];
    let keys: UclObject = key_list
        .iter()
        .enumerate()
        .map(|(i, k)| (*k, UclValue::Integer(i as i64)))
        .collect();
    let finite: Vec<f64> = SPECIAL_FLOATS
        .iter()
        .copied()
        .filter(|v| v.is_finite())
        .collect();
    let float_digits = [0.1f32 as f64, 1e-10, 1.5e-10, 12345.678, -98765.4321e-200];
    let time_digits = [0.001, 1.5, -2.5, 86400.0, 90.0, 1e-3];
    let shapes = |circle: f64| {
        vec![
            Shape::Point,
            Shape::Circle(circle),
            Shape::Rect(0, 0),
            Shape::Poly {
                sides: 0,
                name: "$5".into(),
            },
        ]
    };
    let mut json_sample = fixed_sample();
    json_sample.nested[1].weight = -0.0;
    vec![
        typed_entry("typed_sample", &fixed_sample(), Some(&json_sample)),
        typed_entry(
            "typed_enums",
            &shapes(f64::NEG_INFINITY),
            Some(&shapes(f64::MIN)),
        ),
        entry(
            "floats",
            obj([
                ("specials", floats(SPECIAL_FLOATS, false)),
                ("nan", UclValue::Float(f64::NAN)),
                ("digits", floats(&float_digits, false)),
            ]),
            &FORMATS,
        )
        .with_json_tree(obj([
            ("specials", floats(&finite, false)),
            ("digits", floats(&float_digits, false)),
        ])),
        entry(
            "times",
            obj([
                ("specials", floats(SPECIAL_FLOATS, true)),
                ("digits", floats(&time_digits, true)),
                (
                    "subnormal",
                    floats(
                        &[
                            SMALLEST_MS_TIME,
                            -SMALLEST_MS_TIME,
                            1e-310,
                            f64::MIN_POSITIVE - 5e-324,
                        ],
                        true,
                    ),
                ),
            ]),
            &FORMATS,
        )
        .with_json_tree(obj([
            ("specials", floats(&finite, true)),
            ("digits", floats(&time_digits, true)),
        ])),
        entry(
            "integers",
            obj([(
                "v",
                UclValue::Array(
                    [0, 1, -1, i64::MIN, i64::MAX, 1 << 53, 7, -0x10]
                        .into_iter()
                        .map(UclValue::Integer)
                        .collect(),
                ),
            )]),
            &FORMATS,
        ),
        entry(
            "strings",
            obj([
                ("bytes", UclValue::String(control)),
                (
                    "forms",
                    strings(&[
                        "",
                        "true",
                        "null",
                        "123",
                        "1s",
                        "nan",
                        "inf",
                        "-inf",
                        "<<EOD\nx\nEOD",
                        "EOD\nx",
                        "a\nEOD\nb",
                        "/* c */",
                        "# c",
                        "a;b,c",
                        "{[",
                        "]}",
                        "'single'",
                        "\\'",
                        "back\\",
                        "é€😀",
                        "cost $5",
                        "a$",
                        "$",
                        "$$",
                        "^a$|b$",
                        "${",
                        "${unknown}",
                        "$abi",
                    ]),
                ),
                ("long", UclValue::String(long)),
            ]),
            &FORMATS,
        ),
        entry(
            "dollar_strings",
            obj([(
                "v",
                strings(&[
                    "$ABI",
                    "${ABI}",
                    "$FILENAME",
                    "${CURDIR}/x",
                    "$ABI$$ABI",
                    "it's $ABI",
                    "$ABI\\x",
                    "$ABI\\\\",
                    "$ABI\\\\'",
                    "tab\t$ABI",
                    "nul\0$ABI",
                    "line\n$ABI\r\n",
                    "\\$ABI",
                    "$ABI''",
                ]),
            )]),
            &[Fmt::Config],
        ),
        entry("keys", UclValue::Object(keys), &FORMATS),
        entry("multi_values", UclValue::Object(multi), &FORMATS),
        entry(
            "nested",
            obj([
                (
                    "a",
                    obj([(
                        "b",
                        obj([(
                            "c",
                            UclValue::Array(vec![
                                UclValue::Array(vec![UclValue::Array(vec![])]),
                                obj([]),
                                obj([("d", UclValue::String("deep".into()))]),
                            ]),
                        )]),
                    )]),
                ),
                ("empty_object", obj([])),
                ("empty_array", UclValue::Array(vec![])),
            ]),
            &FORMATS,
        ),
        entry(
            "root_array",
            UclValue::Array(vec![
                UclValue::Integer(1),
                UclValue::String("two".into()),
                obj([("three", UclValue::Float(3.0))]),
                UclValue::Array(vec![UclValue::Time(4.0)]),
                UclValue::Null,
            ]),
            &FORMATS,
        ),
        entry("root_array_empty", UclValue::Array(vec![]), &FORMATS),
        entry("root_object_empty", obj([]), &FORMATS),
    ]
}

fn corpus_path(name: &str, fmt: Fmt) -> PathBuf {
    Path::new(CORPUS_DIR).join(format!("{name}.{}", fmt.extension()))
}

fn golden_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap().to_os_string();
    name.push(".golden.json");
    path.with_file_name(name)
}

/// Writes the corpus and libucl's dumps of it (`UCL_SERDE_REGEN=1`).
fn regenerate_corpus(binary: &Path) {
    fs::create_dir_all(CORPUS_DIR).unwrap();
    for e in corpus() {
        for &fmt in e.formats {
            let tree = e.tree(fmt);
            let text = fmt
                .write(tree)
                .unwrap_or_else(|err| panic!("{} {fmt:?}: {err}", e.name));
            let path = corpus_path(e.name, fmt);
            fs::write(&path, &text).unwrap();
            let dump_oracle = read_oracle(binary, &path);
            if let Some(d) = find_diff(&dump(&expected_reading(tree, fmt)), &dump_oracle, "$") {
                panic!(
                    "{}: libucl does not read the value back: {d}",
                    path.display()
                );
            }
            let golden = serde_json::to_string_pretty(&dump_oracle).unwrap() + "\n";
            assert!(
                !golden.contains(env!("CARGO_MANIFEST_DIR")),
                "{}: the dump contains the checkout path",
                path.display()
            );
            fs::write(golden_path(&path), golden).unwrap();
        }
    }
}

#[test]
fn corpus_reads_back() {
    let binary = oracle();
    if std::env::var_os("UCL_SERDE_REGEN").is_some() {
        regenerate_corpus(
            binary
                .as_deref()
                .expect("regeneration needs the oracle binary"),
        );
    }
    let mut failures = Vec::new();
    let mut files = 0;
    for e in corpus() {
        for &fmt in e.formats {
            let tree = e.tree(fmt);
            let expected = dump(&expected_reading(tree, fmt));
            let path = corpus_path(e.name, fmt);
            let id = path.file_name().unwrap().to_string_lossy().into_owned();
            files += 1;
            // The serializer still writes the committed bytes.
            let committed = fs::read(&path).unwrap_or_else(|_| panic!("{id} is missing"));
            match fmt.write(tree) {
                Ok(text) if text.as_bytes() == committed => {}
                Ok(text) => failures.push(format!("{id}: the serializer now writes\n{text}")),
                Err(err) => failures.push(format!("{id}: {err}")),
            }
            // libucl read the committed text as the value serialized.
            let golden: J = serde_json::from_slice(&fs::read(golden_path(&path)).unwrap()).unwrap();
            if let Some(d) = find_diff(&expected, &golden, "$") {
                failures.push(format!("{id}: libucl's dump: {d}"));
            }
            // The core reads it as the same value.
            match read_core(&committed) {
                Ok(back) => {
                    if let Some(d) = find_diff(&expected, &dump(&back), "$") {
                        failures.push(format!("{id}: the core reads {d}"));
                    }
                    if let Some(typed) = e.typed {
                        match typed(back) {
                            Ok(again) => {
                                let again = expected_reading(&again, fmt);
                                if let Some(d) = find_diff(&expected, &dump(&again), "$") {
                                    failures.push(format!("{id}: typed: {d}"));
                                }
                            }
                            Err(err) => failures.push(format!("{id}: from_value: {err}")),
                        }
                    }
                }
                Err(err) => failures.push(format!("{id}: the core rejects it: {err}")),
            }
            // With the binary, the committed dump is current.
            if let Some(binary) = &binary
                && let Some(d) = find_diff(&golden, &read_oracle(binary, &path), "$")
            {
                failures.push(format!("{id}: the committed dump is stale: {d}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
    assert_eq!(files, 49);
}

// ---------------------------------------------------------------------------------------------
// Errors

fn unrepresentable<T: ?Sized + Serialize>(value: &T, fmt: Fmt) -> bool {
    matches!(
        fmt.write(value),
        Err(UclError::Serde(SerdeError::Unrepresentable(_)))
    )
}

#[test]
fn values_without_a_form_are_errors() {
    #[derive(Serialize)]
    struct V<T> {
        v: T,
    }
    for fmt in FORMATS {
        assert!(unrepresentable(&V { v: u64::MAX }, fmt));
        assert!(unrepresentable(&V { v: i128::MIN }, fmt));
        assert!(unrepresentable(&V { v: u128::MAX }, fmt));
        assert!(unrepresentable(&V { v: 5e-324 }, fmt));
        assert!(unrepresentable(
            &V {
                v: -f64::MIN_POSITIVE / 2.0
            },
            fmt
        ));
        // Subnormal times closer to zero than the smallest one `ms` gives (spec §10.8).
        for t in [5e-324, 2.2250738585065e-311, -1e-315] {
            assert!(
                unrepresentable(&obj([("t", UclValue::Time(t))]), fmt),
                "{t:e}"
            );
        }
        assert!(unrepresentable(
            &obj([("t", UclValue::Time(f64::NAN))]),
            fmt
        ));
        assert!(unrepresentable(&obj([("", UclValue::Integer(1))]), fmt));
        assert!(unrepresentable(&BTreeMap::from([(String::new(), 1)]), fmt));
        assert!(unrepresentable(&BTreeMap::from([((1, 2), 1)]), fmt));
        // A UCL document is an object or an array.
        assert!(unrepresentable(&5, fmt));
        assert!(unrepresentable(&"text", fmt));
        assert!(unrepresentable(&None::<i32>, fmt));
        assert!(unrepresentable(&Shape::Point, fmt));
        // A root newtype struct is its content; a data variant is an object.
        assert!(fmt.write(&Id(1)).is_err());
        assert!(fmt.write(&Shape::Circle(1.0)).is_ok());
    }
    // JSON has no form for NaN and infinite floats and times, nor for subnormal times, which the
    // other formats write through `ms` (WORKLIST.md C4, decision 2; spec §10.8).
    for v in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        for (tree, time) in [
            (obj([("f", UclValue::Float(v))]), false),
            (obj([("t", UclValue::Time(v))]), true),
        ] {
            assert!(unrepresentable(&tree, Fmt::Json), "{tree:?}");
            assert!(unrepresentable(&tree, Fmt::Compact), "{tree:?}");
            // In the config and YAML formats only a NaN time has no form.
            let nan_time = time && v.is_nan();
            assert_eq!(unrepresentable(&tree, Fmt::Config), nan_time, "{tree:?}");
            assert_eq!(unrepresentable(&tree, Fmt::Yaml), nan_time, "{tree:?}");
        }
    }
    let subnormal = obj([("t", UclValue::Time(1e-310))]);
    assert!(unrepresentable(&subnormal, Fmt::Json));
    assert!(unrepresentable(&subnormal, Fmt::Compact));
    assert_eq!(serde_ucl::to_string(&subnormal).unwrap(), "t = 1e-307ms;\n");
    assert_eq!(
        serde_ucl::to_yaml_string(&subnormal).unwrap(),
        "t: 1e-307ms"
    );
    let smallest = obj([("t", UclValue::Time(-SMALLEST_MS_TIME))]);
    let text = serde_ucl::to_string(&smallest).unwrap();
    assert_eq!(text, "t = -2.2250738585072014e-308ms;\n");
    assert_eq!(dump(&read_core(text.as_bytes()).unwrap()), dump(&smallest));
    // to_value rejects integers it cannot hold, but takes any other value.
    assert!(matches!(
        to_value(&u64::MAX),
        Err(UclError::Serde(SerdeError::Unrepresentable(_)))
    ));
    assert_eq!(to_value(&5e-324).unwrap(), UclValue::Float(5e-324));
    assert_eq!(to_value(&7u8).unwrap(), UclValue::Integer(7));
    // Strings that refer to a file variable have no form in double quotes; config writes them
    // in single quotes. A reference to another variable is the reader's business.
    for s in ["$CURDIR/x", "${FILENAME}", "a$FILENAMEb", "$$CURDIR"] {
        for fmt in [Fmt::Json, Fmt::Compact, Fmt::Yaml] {
            assert!(unrepresentable(&V { v: s }, fmt), "{fmt:?} {s:?}");
        }
        let text = serde_ucl::to_string(&V { v: s }).unwrap();
        let back = read_core(text.as_bytes()).unwrap();
        assert_eq!(back.as_object().unwrap()["v"].as_str(), Some(s));
    }
    for s in ["$ABI", "${CURDIR", "$CURDI", "${HOME}"] {
        assert!(serde_ucl::to_json_string(&V { v: s }).is_ok(), "{s:?}");
    }
    // Strings with `$` that single quotes cannot hold have no config form; JSON writes them.
    for s in ["$ABI\\", "$x\\'", "$x\\\n", "$x\\\r\n", "$x\\\r"] {
        assert!(unrepresentable(&V { v: s }, Fmt::Config), "{s:?}");
        let json = serde_ucl::to_json_string_compact(&V { v: s }).unwrap();
        assert_eq!(serde_json::from_str::<J>(&json).unwrap()["v"], s);
    }
}

#[test]
fn nesting_limit() {
    // spec §11.2: at most 1024 containers open at once, the root included.
    let nested = |containers: usize| {
        let mut value = UclValue::Array(vec![]);
        for i in 1..containers {
            value = if i % 2 == 0 {
                UclValue::Array(vec![value])
            } else {
                obj([("k", value)])
            };
        }
        value
    };
    // The tree holds only arrays and objects with one key, so `PartialEq` compares it exactly.
    for fmt in FORMATS {
        let deepest = nested(1024);
        let text = fmt.write(&deepest).unwrap();
        let back = read_core(text.as_bytes()).unwrap();
        assert!(back == deepest, "{fmt:?}");
        assert!(unrepresentable(&nested(1025), fmt), "{fmt:?}");
    }
}

#[test]
fn to_writer_writes_the_config_text() {
    let mut out = Vec::new();
    serde_ucl::to_writer(&mut out, &fixed_sample()).unwrap();
    assert_eq!(
        out,
        serde_ucl::to_string(&fixed_sample()).unwrap().as_bytes()
    );
    let mut out = Vec::new();
    assert!(serde_ucl::to_writer(&mut out, &1).is_err());
    assert!(out.is_empty());
}

// ---------------------------------------------------------------------------------------------
// Forms, layouts and the value model

#[test]
fn forms_of_each_format() {
    let mut value = UclObject::new();
    value.append("f", UclValue::Float(0.1));
    value.append("t", UclValue::Time(1.5));
    value.append("m", UclValue::Integer(1));
    value.append("m", UclValue::Float(f64::NEG_INFINITY));
    value.append("s", UclValue::String("$ABI it's".into()));
    value.append("a b", UclValue::Array(vec![UclValue::Null]));
    let value = UclValue::Object(value);
    assert_eq!(
        serde_ucl::to_string(&value).unwrap(),
        "f = 0.1;\nt = 1.5s;\nm = 1;\nm = -1e308k;\ns = '$ABI it\\'s';\n\"a b\" [\n    null,\n]\n"
    );
    assert_eq!(
        serde_ucl::to_yaml_string(&value).unwrap(),
        "f: 0.1\nt: 1.5s\nm: 1\nm: -1e308k\ns: \"$ABI it's\"\n\"a b\": [\n    null\n]"
    );
    // JSON has no form for −∞ (WORKLIST.md C4, decision 2) ...
    assert!(unrepresentable(&value, Fmt::Json));
    assert!(unrepresentable(&value, Fmt::Compact));
    // ... and writes a time as its number of seconds.
    let mut value = value.as_object().unwrap().clone();
    value.remove("m");
    value.append("m", UclValue::Integer(1));
    value.append("m", UclValue::Float(-1e308));
    let value = UclValue::Object(value);
    assert_eq!(
        serde_ucl::to_json_string(&value).unwrap(),
        "{\n    \"f\": 0.1,\n    \"t\": 1.5,\n    \"s\": \"$ABI it's\",\n    \
         \"a b\": [\n        null\n    ],\n    \"m\": 1,\n    \"m\": -1e308\n}"
    );
    assert_eq!(
        serde_ucl::to_json_string_compact(&value).unwrap(),
        r#"{"f":0.1,"t":1.5,"s":"$ABI it's","a b":[null],"m":1,"m":-1e308}"#
    );
}

#[test]
fn ucl_value_through_other_serde_formats() {
    let mut object = UclObject::new();
    object.append("t", UclValue::Time(1.5));
    object.append("m", UclValue::Integer(1));
    object.append("m", UclValue::Integer(2));
    let value = UclValue::Object(object);
    // Other serializers see the seconds of a time and a sequence for the values of an entry.
    let json = serde_json::to_value(&value).unwrap();
    assert_eq!(json, json!({"t": 1.5, "m": [1, 2]}));
    // Other deserializers give what their format has.
    let back: UclValue =
        serde_json::from_value(json!({"a": [1, "x", null, true, 2.5], "b": {}})).unwrap();
    let expected = obj([
        (
            "a",
            UclValue::Array(vec![
                UclValue::Integer(1),
                UclValue::String("x".into()),
                UclValue::Null,
                UclValue::Boolean(true),
                UclValue::Float(2.5),
            ]),
        ),
        ("b", obj([])),
    ]);
    assert_eq!(dump(&back), dump(&expected));
    let object: UclObject = serde_json::from_str(r#"{"k": 1}"#).unwrap();
    assert_eq!(object["k"], UclValue::Integer(1));
    assert!(serde_json::from_str::<UclObject>("[1]").is_err());
    assert!(serde_json::from_str::<UclValue>("18446744073709551615").is_err());
    // From this crate's deserializer, a time stays a time and every value of an entry is kept.
    let parsed = read_core(b"t = 1.5s\nm = 1\nm = [2]\no { x = 1; x = 2 }").unwrap();
    let again: UclValue = from_value(parsed.clone()).unwrap();
    assert_eq!(dump(&again), dump(&parsed));
    let object: UclObject = from_value(parsed.clone()).unwrap();
    assert_eq!(dump(&UclValue::Object(object)), dump(&parsed));
    // A UclValue field of a struct gets the entry's values as an array.
    #[derive(Deserialize)]
    struct Field {
        m: UclValue,
        t: UclValue,
    }
    let field: Field = from_value(parsed).unwrap();
    assert_eq!(
        field.m,
        UclValue::Array(vec![
            UclValue::Integer(1),
            UclValue::Array(vec![UclValue::Integer(2)])
        ])
    );
    assert_eq!(field.t, UclValue::Time(1.5));
}

// ---------------------------------------------------------------------------------------------
// Deserializing parsed documents with from_value (the rules of `serde_ucl::de`)

fn parsed<T: DeserializeOwned>(text: &str) -> Result<T, UclError> {
    from_value(read_core(text.as_bytes()).unwrap())
}

#[test]
fn from_value_on_parsed_documents() {
    #[derive(Debug, Deserialize, PartialEq)]
    struct Repos {
        #[serde(default)]
        repo: Vec<String>,
        mirror: Option<Vec<String>>,
    }
    // One-or-many sequences.
    let one: Repos = parsed("repo = a\nmirror = m").unwrap();
    assert_eq!(one.repo, ["a"]);
    assert_eq!(one.mirror.unwrap(), ["m"]);
    let many: Repos = parsed("repo = a\nrepo = b\nmirror = null").unwrap();
    assert_eq!(many.repo, ["a", "b"]);
    assert_eq!(many.mirror, None);
    assert_eq!(parsed::<Repos>("").unwrap().repo, Vec::<String>::new());
    // An object is not a sequence.
    #[derive(Debug, Deserialize)]
    #[allow(
        dead_code,
        reason = "a deserialization target; the test checks the error"
    )]
    struct Sections {
        section: Vec<u32>,
    }
    let err = parsed::<Sections>("section { a = 1 }").unwrap_err();
    assert!(err.to_string().contains("invalid type: map"), "{err}");
    // Integral floats and times into integer targets, nothing else.
    #[derive(Debug, Deserialize)]
    struct Num {
        v: u8,
    }
    assert_eq!(parsed::<Num>("v = 30.0").unwrap().v, 30);
    assert_eq!(parsed::<Num>("v = 30s").unwrap().v, 30);
    assert!(parsed::<Num>("v = 30.5").is_err());
    assert!(parsed::<Num>("v = 300").is_err());
    // Enum shapes.
    #[derive(Debug, Deserialize, PartialEq)]
    struct E {
        e: Vec<Shape2>,
    }
    #[derive(Debug, Deserialize, PartialEq)]
    enum Shape2 {
        Point,
        Circle(f64),
        Rect(i32, i32),
        Poly { sides: u8 },
    }
    let e: E =
        parsed("e = [Point, { Circle = 1.5 }, { Rect = [1, 2] }, { Poly { sides = 3 } }]").unwrap();
    assert_eq!(
        e.e,
        [
            Shape2::Point,
            Shape2::Circle(1.5),
            Shape2::Rect(1, 2),
            Shape2::Poly { sides: 3 }
        ]
    );
    // Keys into integer, bool and char targets.
    let m: BTreeMap<u16, bool> = parsed("1 = true\n65535 = false").unwrap();
    assert_eq!(m, BTreeMap::from([(1, true), (65535, false)]));
    assert!(parsed::<BTreeMap<u8, bool>>("256 = true").is_err());
    let m: BTreeMap<bool, char> = parsed("true = x").unwrap();
    assert_eq!(m, BTreeMap::from([(true, 'x')]));
    // Bytes from a string or from an array of integers.
    let b: Bytes = parsed::<BTreeMap<String, Bytes>>("b = [0, 255]").unwrap()["b"].clone();
    assert_eq!(b.0, [0, 255]);
    let b: Bytes = parsed::<BTreeMap<String, Bytes>>("b = xyz").unwrap()["b"].clone();
    assert_eq!(b.0, b"xyz");
}
