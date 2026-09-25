//! Generated inputs: mutations of seed documents (the conformance cases), and documents built
//! from a small grammar of UCL constructs. Everything is driven by [`Rng`], so a run is
//! reproducible from its seed.

/// SplitMix64: small, fast and good enough to pick mutations.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// A number in `0..n`; `n` must not be 0.
    pub fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }

    /// True with probability `percent` / 100.
    pub fn chance(&mut self, percent: u64) -> bool {
        self.next_u64() % 100 < percent
    }

    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

/// Fragments that UCL gives a meaning to, inserted and substituted by the mutations.
const TOKENS: &[&str] = &[
    "{",
    "}",
    "[",
    "]",
    "=",
    ":",
    ";",
    ",",
    " ",
    "\t",
    "\n",
    "\r\n",
    "\r",
    "\x0b",
    "\x0c",
    "\"",
    "'",
    "\\",
    "#",
    "# c\n",
    "/*",
    "*/",
    "/* c */",
    "/* /* */ */",
    "//",
    "<<EOD\n",
    "\nEOD\n",
    "<<EOD",
    "EOD",
    "<<\n",
    "$",
    "$ABI",
    "${ABI}",
    "$$",
    "${",
    "\\$",
    "$FILENAME",
    "${CURDIR}",
    "\\u0041",
    "\\u00e9",
    "\\uD800",
    "\\U0041",
    "\\n",
    "\\t",
    "\\\"",
    "\\'",
    "\\\n",
    "a",
    "b",
    "A",
    "key",
    "\"quoted key\"",
    "\"\"",
    "'sq'",
    "a.b",
    "a-b",
    "_x",
    "/p",
    "é",
    "\u{1F600}",
    "true",
    "false",
    "yes",
    "no",
    "on",
    "off",
    "null",
    "nan",
    "inf",
    "-inf",
    "0",
    "1",
    "-1",
    "0x1F",
    "0X10",
    "-0x10",
    "1e3",
    "1.5",
    "-0.0",
    ".5",
    "1.",
    "1e",
    "1e+",
    "1E-5",
    "10k",
    "10kb",
    "10K",
    "1mb",
    "1gb",
    "1min",
    "1ms",
    "1s",
    "1h",
    "1d",
    "1w",
    "1y",
    "1e308k",
    "1.5x10",
    "9223372036854775807",
    "9223372036854775808",
    "-9223372036854775808",
    ".include",
    ".try_include",
    ".priority",
    ".inherit",
    ".load",
    ".includes",
    ".foo",
    "(",
    ")",
    "(try=true)",
    "(priority=3)",
    "(duplicate=\"merge\")",
    "(duplicate=\"rewrite\")",
    "(duplicate=\"error\")",
    "(duplicate=\"append\")",
    "(key=\"k\")",
    "(prefix=true)",
    "(glob=true)",
    "(target=\"array\")",
    "(replace=true)",
    "(path=[\"files\"])",
    "(url=true)",
    "(trim=true)",
    "(escape=true)",
    "(multiline=true)",
    " \"missing\"",
    " {missing}",
    ".priority 3\n",
    ".priority 1\n",
    " { }",
    "{a=1}",
    "[1,2]",
    "a = 1;\n",
    "a { b = 1 }\n",
    "x \"y{\" z\n",
    ".emit",
    ".seen",
    ".fail",
    ".ctx",
    "\0",
    "\x7f",
];

/// Single bytes that UCL gives a meaning to, or that are ordinary in it.
const BYTES: &[u8] =
    b"{}[]=:;,\"'\\#/*$.<>()+-_ \t\n\r\x0b\x0c\x00aAeExXkKmMsSbBdDhHwWyY0123456789";

/// A mutation of `seed`, which may take a piece of `other`, at most `max_len` bytes long.
pub fn mutate(rng: &mut Rng, seed: &[u8], other: &[u8], max_len: usize) -> Vec<u8> {
    let mut out = seed.to_vec();
    let rounds = 1 + rng.below(4) + if rng.chance(20) { rng.below(8) } else { 0 };
    for _ in 0..rounds {
        let at = rng.below(out.len() + 1);
        match rng.below(10) {
            0 | 1 => insert(&mut out, at, rng.pick(TOKENS).as_bytes()),
            2 => {
                let end = (at + rng.below(8)).min(out.len());
                out.splice(at..end, rng.pick(TOKENS).bytes());
            }
            3 => {
                let end = (at + 1 + rng.below(16)).min(out.len());
                out.drain(at..end);
            }
            4 => {
                if !out.is_empty() {
                    let from = rng.below(out.len());
                    let end = (from + 1 + rng.below(64)).min(out.len());
                    let piece = out[from..end].to_vec();
                    insert(&mut out, at, &piece);
                }
            }
            5 => {
                if !other.is_empty() {
                    let from = rng.below(other.len());
                    let end = (from + 1 + rng.below(256)).min(other.len());
                    insert(&mut out, at, &other[from..end]);
                }
            }
            6 => {
                if at < out.len() {
                    out[at] = *rng.pick(BYTES);
                } else {
                    out.push(*rng.pick(BYTES));
                }
            }
            7 => out.truncate(at),
            8 => insert(
                &mut out,
                at,
                rng.pick(&[" ", "\t", "\n", "\r\n", "\x0b", "\x0c", ""])
                    .as_bytes(),
            ),
            _ => {
                // A line of the other seed in place of one of this one.
                let line = pick_line(rng, other);
                let start = out[..at]
                    .iter()
                    .rposition(|&b| b == b'\n')
                    .map_or(0, |i| i + 1);
                let end = out[at..]
                    .iter()
                    .position(|&b| b == b'\n')
                    .map_or(out.len(), |i| at + i);
                out.splice(start..end, line.iter().copied());
            }
        }
    }
    out.truncate(max_len);
    out
}

fn insert(out: &mut Vec<u8>, at: usize, bytes: &[u8]) {
    out.splice(at..at, bytes.iter().copied());
}

fn pick_line<'a>(rng: &mut Rng, text: &'a [u8]) -> &'a [u8] {
    let lines: Vec<&[u8]> = text.split(|&b| b == b'\n').collect();
    rng.pick(&lines)
}

/// Keys drawn from a small set, so that generated documents repeat keys (spec §8).
const KEYS: &[&str] = &[
    "a", "b", "c", "A", "\"a\"", "\"b c\"", "k", "d", "e", "x.y", "\"\"", "'q'",
];

/// A document built from a small grammar: entries, containers, repeated keys, comments and the
/// macros that change how values combine (`.priority`, `.inherit`).
pub fn generate(rng: &mut Rng) -> Vec<u8> {
    let mut out = String::new();
    match rng.below(10) {
        0 => {
            out.push('{');
            entries(rng, &mut out, 0);
            out.push('}');
        }
        1 => {
            out.push('[');
            elements(rng, &mut out, 0);
            out.push(']');
        }
        _ => entries(rng, &mut out, 0),
    }
    out.into_bytes()
}

fn entries(rng: &mut Rng, out: &mut String, depth: usize) {
    for _ in 0..rng.below(7) {
        match rng.below(12) {
            0 => out.push_str(rng.pick(&["# c\n", "/* c */", "// c\n", "\n", " "])),
            1 => out.push_str(&format!(".priority {}\n", rng.below(16))),
            2 => out.push_str(&format!(".inherit {}\n", rng.pick(KEYS))),
            3 => out.push_str(rng.pick(&[
                ".include(try=true) \"missing\"\n",
                ".try_include \"missing\"\n",
                ".include(duplicate=\"merge\", try=true) \"missing\"\n",
            ])),
            _ => {
                out.push_str(rng.pick(KEYS));
                if rng.chance(20) {
                    // A section name (spec §3.4).
                    out.push(' ');
                    out.push_str(rng.pick(KEYS));
                }
                out.push_str(rng.pick(&[" = ", "=", ": ", ":", " ", ""]));
                value(rng, out, depth);
                out.push_str(rng.pick(&[";\n", "\n", ",", ";", " ", ""]));
            }
        }
    }
}

fn elements(rng: &mut Rng, out: &mut String, depth: usize) {
    for i in 0..rng.below(5) {
        if i > 0 {
            out.push_str(rng.pick(&[", ", ",", ";", "\n", ",\n"]));
        }
        value(rng, out, depth);
    }
    if rng.chance(20) {
        out.push(',');
    }
}

fn value(rng: &mut Rng, out: &mut String, depth: usize) {
    let containers = if depth < 4 { 3 } else { 0 };
    match rng.below(8 + containers) {
        0 => out.push_str(&rng.below(1000).to_string()),
        1 => out.push_str(rng.pick(&[
            "-5", "0x10", "1e3", "1.5", "-0.0", "10k", "10kb", "1min", "1ms", "2h", "1.5x10",
        ])),
        2 => out.push_str(rng.pick(&["true", "false", "yes", "off", "null", "nan", "inf"])),
        3 => out.push_str(rng.pick(&[
            "\"s\"",
            "\"a b\"",
            "\"\\u0041\\n\"",
            "\"$ABI\"",
            "\"\"",
            "\"${ABI}x\"",
        ])),
        4 => out.push_str(rng.pick(&["'s'", "'a\\'b'", "''", "'$ABI'"])),
        5 => out.push_str(rng.pick(&["<<EOD\nline\nEOD\n", "<<EOD\na\n\nb\nEOD\n", "<<\nx\n\n"])),
        6 => out.push_str(rng.pick(&["word", "a b", "$ABI", "x/y", "1.2.3", "-", "."])),
        7 => out.push_str(&rng.below(10).to_string()),
        8 => {
            out.push_str("{ ");
            entries(rng, out, depth + 1);
            out.push('}');
        }
        9 => {
            out.push('[');
            elements(rng, out, depth + 1);
            out.push(']');
        }
        _ => {
            out.push('{');
            out.push_str(rng.pick(KEYS));
            out.push_str(" = ");
            value(rng, out, depth + 1);
            out.push('}');
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_seed_gives_the_same_inputs() {
        let seed = b"a = 1;\nb { c = [1, 2] }\n";
        let run = |s| {
            let mut rng = Rng::new(s);
            (0..100)
                .map(|_| {
                    (
                        mutate(&mut rng, seed, b"x = \"y\"\n", 4096),
                        generate(&mut rng),
                    )
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(run(7), run(7));
        assert_ne!(run(7), run(8));
    }

    #[test]
    fn mutations_keep_to_the_length_limit() {
        let mut rng = Rng::new(1);
        let seed = vec![b'a'; 100];
        for _ in 0..1000 {
            assert!(mutate(&mut rng, &seed, &seed, 64).len() <= 64);
        }
    }

    #[test]
    fn below_stays_in_range() {
        let mut rng = Rng::new(3);
        assert!((0..1000).all(|_| rng.below(5) < 5));
    }
}
