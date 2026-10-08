//! Controlled workloads (clean-room work item C16, task P1): generated documents that each stress
//! one scan or one structure of the parser, so that a change to that scan can be measured where
//! it matters, and shown to make no input of its kind slower (WORKLIST C16, owner decision of
//! 2026-10-06). `parse_benchmarks` runs each one as `<group>/<name>`.
//!
//! Each document has at least [`SIZE`] bytes, apart from the deep ones, which repeat a chain at
//! the deepest nesting the parser accepts until they do. Nothing depends on the clock or on
//! hashing, so a document is the same on every platform; `tests/bench_documents.rs` pins them by
//! digest, and `benches/check-documents.sh` checks each one against libucl.
//!
//! - `parse/strings`: array elements, one string form each: double-quoted ASCII of 1 to 4096
//!   bytes, then at 64 bytes double-quoted with an escape every eight bytes, double-quoted
//!   non-ASCII, single-quoted, and unquoted; single-quoted of 4096 bytes; heredocs of 1024 bytes;
//!   double-quoted with an escape after every one or two bytes, control characters among them.
//! - `parse/keys`: sections of eight entries, with bare keys of 4, 22, 23 and 64 bytes and
//!   double-quoted keys of 23 bytes. 22 bytes is the inline limit of the key copies the parser
//!   keeps in output facts and value paths; the keys of the value tree are `String`s whatever
//!   their length.
//! - `parse/numbers`: arrays of numbers, 16 to a line: integers of 1, 4, 8, 16 and 19 digits,
//!   negative integers, hex integers, floats with a few digits, with 15 significant digits, with
//!   16 and an exponent within ±22, with 17 (as in `canada`), with small and with large
//!   exponents, and numbers with multiplier and time suffixes; and integers as entry values
//!   followed by spaces, a tab or `#`.
//! - `parse/containers`: arrays of empty objects and of empty arrays, sections of entries whose
//!   values are empty containers, and arrays of objects with one key and of arrays with one
//!   element.
//! - `parse/deep`: arrays and JSON-style objects nested 1000 deep, one chain after another, and
//!   many sections nested 16 deep.
//! - `parse/comments`: sections whose entries are preceded or followed by comments, about two
//!   thirds of the bytes: `#` lines, `#` after values, block comments of prose, of `*` banners,
//!   of text full of `/`, with quoted parts, dense in `*`, `/` and `"`, and nested; `#` and block
//!   comments directly after values with no `;`; comments of about 400 bytes; the `-saved`
//!   variants parse with `save-comments` (spec §12.5).
//! - `parse/whitespace`: the same sections without any whitespace, aligned with long runs of
//!   spaces, indented with tabs, with CRLF line ends, and with blank lines between entries; and
//!   entries ended by a line break alone, with ` = `, a space alone or `: ` between key and value.
//! - `parse/objects`: sections of 8, 16, 17, 24 and 32 keys of the same length, on both sides of
//!   the 16 keys of a small object; 16 keys that share a 29-byte prefix; 8 keys that each appear
//!   twice, which makes implicit arrays (spec §8); 16 keys of 3 to 40 bytes; and 16 keys in mixed
//!   case parsed with `key-lowercase` (spec §12.1).

use super::irregular::Rng;
use serde_ucl::ParserFlags;
use std::fmt::Write;

/// The least size of a generated workload, in bytes.
pub const SIZE: usize = 200_000;

/// A generated document of a sweep.
pub struct Workload {
    /// The benchmark group, such as `parse/numbers`.
    pub group: &'static str,
    /// The benchmark's name in its group.
    pub name: String,
    /// The flags it is parsed with.
    pub flags: ParserFlags,
    pub text: String,
}

impl Workload {
    fn new(group: &'static str, name: impl Into<String>, text: String) -> Self {
        Self {
            group,
            name: name.into(),
            flags: ParserFlags::empty(),
            text,
        }
    }

    fn with_flags(mut self, flags: ParserFlags) -> Self {
        self.flags = flags;
        self
    }

    /// The benchmark's id, `<group>/<name>`.
    pub fn id(&self) -> String {
        format!("{}/{}", self.group, self.name)
    }
}

/// Every workload, in the order of the benchmarks.
pub fn workloads() -> Vec<Workload> {
    let mut all = Vec::new();
    all.extend(strings());
    all.extend(keys());
    all.extend(numbers());
    all.extend(containers());
    all.extend(deep());
    all.extend(comments());
    all.extend(whitespace());
    all.extend(objects());
    all
}

/// Text for strings and comments: words, digits and punctuation, without `"`, `'`, `\`, `$`,
/// `#`, `*`, `/`, braces or brackets.
const PROSE: &str = "the quick brown fox jumps over the lazy dog, then 0123456789 more words \
                     follow: listen port host timeout workers user group level max min keepalive \
                     upstream backend cache-control; ";

/// `len` bytes of [`PROSE`], starting at an offset that varies with `i`.
fn prose(len: usize, i: usize) -> String {
    PROSE
        .bytes()
        .cycle()
        .skip(i * 7 % PROSE.len())
        .take(len)
        .map(char::from)
        .collect()
}

/// An array named `key` whose elements `item` writes, `per_line` to a line, until the document
/// has [`SIZE`] bytes.
fn array(key: &str, per_line: usize, mut item: impl FnMut(&mut String, usize)) -> String {
    let mut out = format!("{key} [\n    ");
    let mut i = 0;
    while out.len() < SIZE {
        if i > 0 {
            out.push_str(if i % per_line == 0 { ",\n    " } else { ", " });
        }
        item(&mut out, i);
        i += 1;
    }
    out.push_str("\n]\n");
    out
}

/// Sections `s0`, `s1`, … of `entries` entries each, which `entry` writes (with its line end),
/// until the document has [`SIZE`] bytes. `entry` gets the section's and the entry's number.
fn sections(entries: usize, mut entry: impl FnMut(&mut String, usize, usize)) -> String {
    let mut out = String::new();
    let mut s = 0;
    while out.len() < SIZE {
        writeln!(out, "s{s} {{").unwrap();
        for e in 0..entries {
            entry(&mut out, s, e);
        }
        out.push_str("}\n");
        s += 1;
    }
    out
}

fn strings() -> Vec<Workload> {
    const GROUP: &str = "parse/strings";
    let mut all = Vec::new();
    for len in [1, 8, 16, 24, 32, 64, 256, 4096] {
        let text = array("strings", 1, |out, i| {
            write!(out, "\"{}\"", prose(len, i)).unwrap()
        });
        all.push(Workload::new(GROUP, format!("dq-{len}"), text));
    }
    // An escape every eight bytes, of each kind (spec §6.1); 64 bytes as written.
    const ESCAPES: [&str; 7] = ["\\n", "\\t", "\\\"", "\\\\", "\\/", "\\r", "\\u00e9"];
    let text = array("strings", 1, |out, i| {
        out.push('"');
        let mut written = 0;
        let mut k = i;
        while written < 64 {
            let escape = ESCAPES[k % ESCAPES.len()];
            let plain = prose(8usize.saturating_sub(escape.len()).max(2), i + k);
            out.push_str(&plain);
            out.push_str(escape);
            written += plain.len() + escape.len();
            k += 1;
        }
        out.push('"');
    });
    all.push(Workload::new(GROUP, "dq-escaped-64", text));
    // Non-ASCII text in several alphabets, about 64 bytes.
    const ALPHABETS: [&str; 4] = [
        "привет мир, ",
        "日本語の文章です。",
        "ελληνικό κείμενο ",
        "café déjà vu à côté ",
    ];
    let text = array("strings", 1, |out, i| {
        out.push('"');
        let mut s = String::new();
        let mut k = i;
        while s.len() < 64 {
            s.push_str(ALPHABETS[k % ALPHABETS.len()]);
            k += 1;
        }
        out.push_str(&s);
        out.push('"');
    });
    all.push(Workload::new(GROUP, "dq-utf8-64", text));
    for len in [64, 4096] {
        let text = array("strings", 1, |out, i| {
            write!(out, "'{}'", prose(len, i)).unwrap()
        });
        all.push(Workload::new(GROUP, format!("sq-{len}"), text));
    }
    // Unquoted values: host names, paths and words joined by `-`, `.`, `_` and `/`.
    const PARTS: [&str; 8] = [
        "backend",
        "eu-west",
        "example",
        "org",
        "api",
        "v1",
        "items",
        "cache_node",
    ];
    let text = array("strings", 1, |out, i| {
        let mut s = String::from("h");
        let mut k = i;
        while s.len() < 64 {
            s.push(['-', '.', '_', '/'][k % 4]);
            s.push_str(PARTS[k % PARTS.len()]);
            k += 3;
        }
        out.push_str(&s);
    });
    all.push(Workload::new(GROUP, "unquoted-64", text));
    // Heredocs of about 1024 bytes, in lines of 60.
    let text = sections(8, |out, _, e| {
        writeln!(out, "    h{e} = <<EOD").unwrap();
        for line in 0..17 {
            out.push_str(&prose(60, e * 17 + line));
            out.push('\n');
        }
        out.push_str("EOD\n");
    });
    all.push(Workload::new(GROUP, "heredoc-1024", text));
    // An escape after every one or two bytes, 64 bytes as written: control characters, which
    // the output formats write escaped (BS, FF, VT, 0x01, 0x1F, DEL), and `\n`, `\r`, `\t`,
    // `"`, `\` and `/` (spec §6.1).
    const DENSE: [&str; 12] = [
        "\\\"", "\\\\", "\\n", "\\r", "\\t", "\\b", "\\f", "\\u000b", "\\u0001", "\\u001f",
        "\\u007f", "\\/",
    ];
    let text = array("strings", 1, |out, i| {
        out.push('"');
        let mut written = 0;
        let mut k = i;
        while written < 64 {
            if k.is_multiple_of(2) {
                out.push(char::from(b'a' + (k % 26) as u8));
                written += 1;
            }
            let escape = DENSE[k % DENSE.len()];
            out.push_str(escape);
            written += escape.len();
            k += 1;
        }
        out.push('"');
    });
    all.push(Workload::new(GROUP, "dq-escaped-dense-64", text));
    all
}

/// A bare key of `len` bytes, unique among the entries of a section for each `e` below 10.
fn bare_key(len: usize, e: usize) -> String {
    let mut key: String = "keyname".chars().cycle().take(len - 1).collect();
    key.push(char::from(b'0' + e as u8));
    key
}

fn keys() -> Vec<Workload> {
    const GROUP: &str = "parse/keys";
    let mut all = Vec::new();
    for len in [4, 22, 23, 64] {
        let text = sections(8, |out, s, e| {
            writeln!(out, "    {} = {};", bare_key(len, e), s + e).unwrap()
        });
        all.push(Workload::new(GROUP, format!("bare-{len}"), text));
    }
    let text = sections(8, |out, s, e| {
        writeln!(out, "    \"service name part {e:04}\" = {};", s + e).unwrap()
    });
    all.push(Workload::new(GROUP, "quoted-23", text));
    all
}

fn numbers() -> Vec<Workload> {
    const GROUP: &str = "parse/numbers";
    let mut all = Vec::new();
    let mut add = |name: &str, mut number: Box<dyn FnMut(&mut Rng) -> String>| {
        let seed = name
            .bytes()
            .fold(0, |h: u64, b| h.wrapping_mul(31) + u64::from(b));
        let mut rng = Rng::new(seed);
        let text = array("numbers", 16, |out, _| out.push_str(&number(&mut rng)));
        all.push(Workload::new(GROUP, name, text));
    };
    add("int-1", Box::new(|r| r.below(10).to_string()));
    add("int-4", Box::new(|r| r.between(1000, 9999).to_string()));
    add(
        "int-8",
        Box::new(|r| r.between(10_000_000, 99_999_999).to_string()),
    );
    add(
        "int-16",
        Box::new(|r| (1_000_000_000_000_000 + r.next_u64() % 9_000_000_000_000_000).to_string()),
    );
    // Up to i64::MAX, 9223372036854775807.
    add(
        "int-19",
        Box::new(|r| {
            (1_000_000_000_000_000_000 + r.next_u64() % 8_223_372_036_854_775_808).to_string()
        }),
    );
    add(
        "int-neg-8",
        Box::new(|r| format!("-{}", r.between(10_000_000, 99_999_999))),
    );
    add(
        "float-short",
        Box::new(|r| format!("{}.{}", r.below(1000), r.between(1, 99))),
    );
    // 15 significant digits: the digits fit in 53 bits.
    add(
        "float-15",
        Box::new(|r| {
            format!(
                "{}.{:012}",
                r.between(100, 999),
                r.next_u64() % 1_000_000_000_000
            )
        }),
    );
    // 17 significant digits, as in `canada`: the digits do not fit in 53 bits.
    add(
        "float-17",
        Box::new(|r| {
            format!(
                "-{}.{:015}",
                r.between(10, 99),
                r.next_u64() % 1_000_000_000_000_000
            )
        }),
    );
    // Exponents within ±22.
    add(
        "float-exp",
        Box::new(|r| {
            let sign = if r.percent(50) { "-" } else { "" };
            format!(
                "{}.{:02}e{sign}{}",
                r.between(1, 9),
                r.below(100),
                r.between(1, 22)
            )
        }),
    );
    // Exponents beyond ±22, with 16 significant digits.
    add(
        "float-exp-large",
        Box::new(|r| {
            let sign = if r.percent(50) { "-" } else { "" };
            format!(
                "{}.{:015}e{sign}{}",
                r.between(1, 9),
                r.next_u64() % 1_000_000_000_000_000,
                r.between(100, 300)
            )
        }),
    );
    // Multiplier and time suffixes (spec §5.5); nothing but `,` or a line end follows them.
    add(
        "suffixed",
        Box::new(|r| {
            let n = r.between(1, 999);
            match r.below(8) {
                0 => format!("{n}k"),
                1 => format!("{n}kb"),
                2 => format!("{n}mb"),
                3 => format!("{n}s"),
                4 => format!("{n}ms"),
                5 => format!("{n}min"),
                6 => format!("{n}h"),
                _ => format!("{n}.5s"),
            }
        }),
    );
    // Hex integers of 1 to 15 digits, in either case (spec §5.2).
    add(
        "hex",
        Box::new(|r| {
            let digits = r.between(1, 15);
            let n = r.next_u64() >> (64 - 4 * digits);
            if r.percent(50) {
                format!("0x{n:x}")
            } else {
                format!("0X{n:X}")
            }
        }),
    );
    // 16 significant digits with an exponent within ±22: the digits are on both sides of 2^53
    // (9 007 199 254 740 992), about half each.
    add(
        "float-16-exp",
        Box::new(|r| {
            let digits = 8_000_000_000_000_000 + r.next_u64() % 2_000_000_000_000_000;
            let digits = digits.to_string();
            let sign = if r.percent(50) { "-" } else { "" };
            format!(
                "{}.{}e{sign}{}",
                &digits[..1],
                &digits[1..],
                r.between(1, 22)
            )
        }),
    );
    // Integers followed by spaces or a tab before `;`, `#` or the line end (spec §5.5), as entry
    // values, since `#` ends the line.
    let mut rng = Rng::new(5);
    let text = sections(8, |out, _, e| {
        let n = rng.between(1, 99_999_999);
        let tail = match e % 4 {
            0 => "  ;",
            1 => "\t# a comment",
            2 => " #comment",
            _ => "   ",
        };
        writeln!(out, "    port{e} = {n}{tail}").unwrap();
    });
    all.push(Workload::new(GROUP, "int-then-space-or-hash", text));
    all
}

fn containers() -> Vec<Workload> {
    const GROUP: &str = "parse/containers";
    vec![
        Workload::new(
            GROUP,
            "empty-objects",
            array("items", 8, |out, _| out.push_str("{}")),
        ),
        Workload::new(
            GROUP,
            "empty-arrays",
            array("items", 8, |out, _| out.push_str("[]")),
        ),
        Workload::new(
            GROUP,
            "empty-values",
            sections(8, |out, _, e| {
                if e % 2 == 0 {
                    writeln!(out, "    o{e} {{}}").unwrap();
                } else {
                    writeln!(out, "    a{e} = [];").unwrap();
                }
            }),
        ),
        Workload::new(
            GROUP,
            "one-key-objects",
            array("items", 4, |out, i| write!(out, "{{\"v\": {i}}}").unwrap()),
        ),
        Workload::new(
            GROUP,
            "one-element-arrays",
            array("items", 8, |out, i| write!(out, "[{i}]").unwrap()),
        ),
    ]
}

/// The depth of the deep workloads: the root and 1000 containers, within the 1024 that may be
/// open at once (spec §11.2).
pub const DEPTH: usize = 1000;

/// Arrays and JSON-style objects nested `depth` deep, one chain after another until the
/// document has [`SIZE`] bytes: `parse/deep/arrays-<depth>` and `json-objects-<depth>`. The
/// benchmarks use [`DEPTH`]; `benches/check-documents.sh` compares the same documents at a depth
/// whose dump its JSON reader can read.
pub fn deep_chains(depth: usize) -> Vec<Workload> {
    const GROUP: &str = "parse/deep";
    let mut arrays = String::new();
    let mut i = 0;
    while arrays.len() < SIZE {
        write!(arrays, "a{i} = ").unwrap();
        arrays.push_str(&"[".repeat(depth));
        arrays.push('1');
        arrays.push_str(&"]".repeat(depth));
        arrays.push_str(";\n");
        i += 1;
    }
    let mut objects = String::from("{\n");
    let mut i = 0;
    while objects.len() < SIZE {
        if i > 0 {
            objects.push_str(",\n");
        }
        write!(objects, "\"o{i}\": ").unwrap();
        objects.push_str(&"{\"a\": ".repeat(depth - 1));
        objects.push('1');
        objects.push_str(&"}".repeat(depth - 1));
        i += 1;
    }
    objects.push_str("\n}\n");
    vec![
        Workload::new(GROUP, format!("arrays-{depth}"), arrays),
        Workload::new(GROUP, format!("json-objects-{depth}"), objects),
    ]
}

/// The deep workloads: [`deep_chains`] at [`DEPTH`], and many sections nested 16 deep.
fn deep() -> Vec<Workload> {
    let mut repeated = String::new();
    let mut i = 0;
    while repeated.len() < SIZE {
        write!(repeated, "n{i} {{").unwrap();
        for level in 0..15 {
            write!(repeated, " l{level} {{").unwrap();
        }
        write!(repeated, " leaf = {i};").unwrap();
        repeated.push_str(&" }".repeat(16));
        repeated.push('\n');
        i += 1;
    }
    let mut all = deep_chains(DEPTH);
    all.push(Workload::new("parse/deep", "repeated-16", repeated));
    all
}

/// The value of entry `e` of section `s`: a string, an integer, a boolean or a float.
fn plain_value(s: usize, e: usize) -> String {
    match e % 4 {
        0 => format!("\"value {s} {e}\""),
        1 => (8000 + s % 1000).to_string(),
        2 => s.is_multiple_of(2).to_string(),
        _ => format!("0.{}", s % 100 + 1),
    }
}

/// The entry `e` of section `s`, `name0`, `port1`, `enabled2`, `ratio3`, `name4` and so on,
/// ended by `;`, after the indent `indent` and with the separator `sep`.
fn plain_entry(out: &mut String, s: usize, e: usize, indent: &str, sep: &str) {
    let key = ["name", "port", "enabled", "ratio"][e % 4];
    write!(out, "{indent}{key}{e}{sep}{};", plain_value(s, e)).unwrap();
}

/// Text for a block comment that is dense in `*`, `/` and `"`. No `*` and `/` are adjacent
/// outside a quoted part, so the comment neither nests nor ends early (spec §2.3), and quotes
/// come in pairs.
fn dense_comment(len: usize, i: usize) -> String {
    const TOKENS: [&str; 8] = [
        "*",
        "/",
        "\"*/\"",
        "**",
        "//",
        "\"a/b*c\"",
        "\"/*\"",
        "\"\"",
    ];
    let mut s = String::new();
    let mut k = i;
    while s.len() < len {
        s.push_str(TOKENS[k % TOKENS.len()]);
        s.push(' ');
        k += 1;
    }
    s
}

fn comments() -> Vec<Workload> {
    const GROUP: &str = "parse/comments";
    let entry = plain_entry;
    let hash_lines = sections(8, |out, s, e| {
        writeln!(out, "    # {}", prose(100, s + e)).unwrap();
        entry(out, s, e, "    ", " = ");
        out.push('\n');
    });
    let hash_after = sections(8, |out, s, e| {
        entry(out, s, e, "    ", " = ");
        writeln!(out, " # {}", prose(60, s + e)).unwrap();
    });
    let block_prose = sections(8, |out, s, e| {
        writeln!(
            out,
            "    /* {}\n       {} */",
            prose(60, s + e),
            prose(60, s + e + 1)
        )
        .unwrap();
        entry(out, s, e, "    ", " = ");
        out.push('\n');
    });
    let block_stars = sections(8, |out, s, e| {
        let banner = "*".repeat(60);
        writeln!(
            out,
            "    /{banner}\n     * {}\n     * {}\n     {banner}/",
            prose(40, s + e),
            prose(40, s + e + 2)
        )
        .unwrap();
        entry(out, s, e, "    ", " = ");
        out.push('\n');
    });
    let block_slashes = sections(8, |out, s, e| {
        writeln!(
            out,
            "    /* see /etc/app/conf.d//{s}.conf and https://example.org/a/b/{e}/ // -- // \
             /usr/local/share/app/ /var/lib/app/state/ ../../relative/path/ a/b/c/d/e/f/g */"
        )
        .unwrap();
        entry(out, s, e, "    ", " = ");
        out.push('\n');
    });
    let block_quotes = sections(8, |out, s, e| {
        writeln!(
            out,
            "    /* \"{}\" and \"{}\" then \"x\" \"y\" \"\" \"{}\" */",
            prose(30, s + e),
            prose(20, s),
            prose(25, e)
        )
        .unwrap();
        entry(out, s, e, "    ", " = ");
        out.push('\n');
    });
    let block_dense = sections(8, |out, s, e| {
        writeln!(out, "    /* {}*/", dense_comment(120, s + e)).unwrap();
        entry(out, s, e, "    ", " = ");
        out.push('\n');
    });
    let block_nested = sections(8, |out, s, e| {
        writeln!(
            out,
            "    /* {} /* {} /* {} */ {} */ {} */",
            prose(30, s),
            prose(30, e),
            prose(20, s + e),
            prose(20, s + 1),
            prose(20, e + 1)
        )
        .unwrap();
        entry(out, s, e, "    ", " = ");
        out.push('\n');
    });
    let mut all = vec![
        Workload::new(GROUP, "hash-lines", hash_lines),
        Workload::new(GROUP, "hash-after-values", hash_after.clone()),
        Workload::new(GROUP, "block-prose", block_prose.clone()),
        Workload::new(GROUP, "block-stars", block_stars),
        Workload::new(GROUP, "block-slashes", block_slashes),
        Workload::new(GROUP, "block-quotes", block_quotes),
        Workload::new(GROUP, "block-dense", block_dense),
        Workload::new(GROUP, "block-nested", block_nested),
        Workload::new(GROUP, "hash-after-values-saved", hash_after)
            .with_flags(ParserFlags::SAVE_COMMENTS),
        Workload::new(GROUP, "block-prose-saved", block_prose)
            .with_flags(ParserFlags::SAVE_COMMENTS),
    ];
    // A `#` comment directly after a value, with no `;` (spec §2.2, §5.5).
    let hash_after_bare = sections(8, |out, s, e| {
        let key = ["name", "port", "enabled", "ratio"][e % 4];
        writeln!(
            out,
            "    {key}{e} = {} # {}",
            plain_value(s, e),
            prose(60, s + e)
        )
        .unwrap();
    });
    // A block comment directly after a quoted value, with no `;` (§2.4). Only quoted values:
    // a number before a block comment is a string (§5.6), and an unquoted value ends where the
    // comment starts.
    let block_after_bare = sections(8, |out, s, e| {
        writeln!(
            out,
            "    name{e} = \"value {s} {e}\" /* {} */",
            prose(60, s + e)
        )
        .unwrap();
    });
    // Comments of about 400 bytes: a `#` line, and a block comment over seven lines.
    let hash_long = sections(8, |out, s, e| {
        writeln!(out, "    # {}", prose(400, s + e)).unwrap();
        entry(out, s, e, "    ", " = ");
        out.push('\n');
    });
    let block_long = sections(8, |out, s, e| {
        out.push_str("    /*");
        for line in 0..7 {
            write!(out, " {}\n      ", prose(56, s + e + line)).unwrap();
        }
        out.push_str("*/\n");
        entry(out, s, e, "    ", " = ");
        out.push('\n');
    });
    all.extend([
        Workload::new(
            GROUP,
            "hash-after-values-no-separator",
            hash_after_bare.clone(),
        ),
        Workload::new(
            GROUP,
            "block-after-values-no-separator",
            block_after_bare.clone(),
        ),
        Workload::new(
            GROUP,
            "hash-after-values-no-separator-saved",
            hash_after_bare,
        )
        .with_flags(ParserFlags::SAVE_COMMENTS),
        Workload::new(
            GROUP,
            "block-after-values-no-separator-saved",
            block_after_bare,
        )
        .with_flags(ParserFlags::SAVE_COMMENTS),
        Workload::new(GROUP, "hash-long", hash_long),
        Workload::new(GROUP, "block-long", block_long),
    ]);
    all
}

fn whitespace() -> Vec<Workload> {
    const GROUP: &str = "parse/whitespace";
    // No whitespace at all: `s0={name0="value 0 0";port1=8000;…};s1={…};`.
    let mut compact = String::new();
    let mut s = 0;
    while compact.len() < SIZE {
        write!(compact, "s{s}={{").unwrap();
        for e in 0..8 {
            plain_entry(&mut compact, s, e, "", "=");
        }
        compact.push_str("};");
        s += 1;
    }
    compact.push('\n');
    let aligned = sections(8, |out, s, e| {
        let mut line = String::new();
        plain_entry(&mut line, s, e, "", " ");
        let (key, value) = line.split_once(' ').unwrap();
        writeln!(out, "                {key:<40}= {value}").unwrap();
    });
    let tabs = sections(8, |out, s, e| {
        plain_entry(out, s, e, "\t\t", "\t=\t");
        out.push('\n');
    });
    let crlf = sections(8, |out, s, e| {
        plain_entry(out, s, e, "    ", " = ");
        out.push_str("\r\n");
    });
    let blank_lines = sections(8, |out, s, e| {
        plain_entry(out, s, e, "    ", " = ");
        out.push_str("\n\n    \n\n");
    });
    vec![
        Workload::new(GROUP, "compact", compact),
        Workload::new(GROUP, "aligned", aligned),
        Workload::new(GROUP, "tabs", tabs),
        Workload::new(GROUP, "crlf", crlf),
        Workload::new(GROUP, "blank-lines", blank_lines),
        Workload::new(GROUP, "newline-ends", bare_entries(" = ")),
        Workload::new(GROUP, "space-separated", bare_entries(" ")),
        Workload::new(GROUP, "colon-separated", bare_entries(": ")),
    ]
}

/// Sections of the entries of [`plain_entry`] ended by a line break alone, without `;`, with
/// the separator `sep` between key and value: ` = `, a space alone, or `: ` outside JSON
/// (spec §1.2).
fn bare_entries(sep: &str) -> String {
    sections(8, |out, s, e| {
        let key = ["name", "port", "enabled", "ratio"][e % 4];
        writeln!(out, "    {key}{e}{sep}{}", plain_value(s, e)).unwrap();
    })
}

fn objects() -> Vec<Workload> {
    const GROUP: &str = "parse/objects";
    let mut all = Vec::new();
    // Keys of the same length, `field_00` to `field_31`, so that their lengths tell none apart.
    for keys in [8, 16, 17, 24, 32] {
        let text = sections(keys, |out, s, e| {
            writeln!(out, "    field_{e:02} = {};", plain_value(s, e)).unwrap()
        });
        all.push(Workload::new(GROUP, format!("keys-{keys}"), text));
    }
    let text = sections(16, |out, s, e| {
        writeln!(out, "    configuration_parameter_name_{e:02} = {};", s + e).unwrap()
    });
    all.push(Workload::new(GROUP, "keys-16-long-prefix", text));
    let text = sections(16, |out, s, e| {
        plain_entry(out, s, e % 8, "    ", " = ");
        out.push('\n');
    });
    all.push(Workload::new(GROUP, "keys-16-repeated", text));
    // 16 keys of 3 to 40 bytes, so that their lengths tell them apart.
    let text = sections(16, |out, s, e| {
        let len = 3 + (e * 7) % 38;
        let key: String = format!("k{e:02}")
            .chars()
            .chain("abcdefghijklmnopqrstuvwxyz".chars().cycle())
            .take(len)
            .collect();
        writeln!(out, "    {key} = {};", plain_value(s, e)).unwrap()
    });
    all.push(Workload::new(GROUP, "keys-16-mixed-length", text));
    // 16 keys in mixed case, parsed with `key-lowercase` (spec §12.1).
    let text = sections(16, |out, s, e| {
        writeln!(out, "    Field_Name_{e:02} = {};", plain_value(s, e)).unwrap()
    });
    all.push(
        Workload::new(GROUP, "keys-16-lowercase", text).with_flags(ParserFlags::KEY_LOWERCASE),
    );
    all
}

/// `json` without the whitespace outside its strings: every other byte as it is, so that the
/// numbers and escapes stay as they are written.
pub fn compact_json(json: &str) -> String {
    let mut out = Vec::with_capacity(json.len());
    let mut in_string = false;
    let mut escaped = false;
    for &b in json.as_bytes() {
        if in_string {
            out.push(b);
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                in_string = false;
            }
        } else if !matches!(b, b' ' | b'\t' | b'\n' | b'\r') {
            out.push(b);
            in_string = b == b'"';
        }
    }
    String::from_utf8(out).expect("whitespace is ASCII")
}
