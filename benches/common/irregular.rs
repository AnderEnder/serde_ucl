//! A seeded generator of irregular configurations (clean-room work item C14).
//!
//! `config(n)` repeats one service shape. The documents here vary where it does not: keys and
//! their forms, the number of entries in a section, the order of entries, the depth of nesting,
//! and the length and alphabet of strings. They still use every value form of `config(n)`:
//! braced and named sections, `=`, `:` and nginx-style entries, repeated keys, arrays, numbers
//! with multipliers, times, booleans, the three quoted string forms and heredocs, and `#` and
//! nested block comments.
//!
//! The same seed and size give the same document on every platform: the random numbers come
//! from SplitMix64, and nothing depends on hashing or on the clock.
//!
//! Distributions (percentages of the choices made at each point):
//!
//! - Section sizes: 30 % have 0–3 entries, 35 % 4–15, 25 % 16–40 and 10 % 41–120, on both
//!   sides of the 16-key threshold of small objects.
//! - Depth: an entry of the root is a section with probability 55 %; at depth 1 to 6 with
//!   12, 9, 7, 5, 4 and 3 % (a quarter of that in sections of more than 40 entries). Sections go
//!   at most 7 deep, a named section adds one or two objects, and inline objects and arrays add
//!   up to two more levels.
//! - Quoted strings: 75.5 % have 1–24 bytes, 20 % 25–200, 4 % 200–2000 and 0.5 % 2000–16000, in
//!   English (60 %), accented Latin, Cyrillic, Greek, Chinese and Japanese, Arabic and Hebrew,
//!   or a mix with emoji. 40 % of double-quoted strings have escapes (`\n`, `\t`, `\"`, `\\`,
//!   `\/`, `\r`, `\uXXXX`); the others appear in the input as they are. Single-quoted strings
//!   break long lines and have `\'` and other backslashes. Heredocs have 10–300 bytes (70 %),
//!   300–3000 (20 %) or 3000–12000 (10 %). Unquoted values are words, host names, paths, URLs,
//!   e-mail addresses, addresses, versions, non-ASCII words, and a few runs of 20–60 words.
//! - Keys: plain words, compounds joined with `_`, `-` or `.`, camel case, one letter, words
//!   with digits, numbers, paths, non-ASCII words, and 4 % double-quoted keys with spaces,
//!   escapes or non-ASCII text.
//!
//! What the generator avoids, because it would make a document an error or change what a value
//! means (spec §1–§6): raw control bytes and surrogate `\u` escapes in double-quoted strings,
//! `$` outside single-quoted strings (§7), `#`, `/*`, brackets and backslashes in unquoted
//! values, a bracket anywhere on an nginx-style line (§3.4), a space after a number with a
//! suffix (§5.5), a backslash before a line break in single-quoted strings, and heredoc lines
//! equal to the heredoc's name.

use std::fmt::Write;

/// The irregular documents of the benchmarks: their name, seed and least size in bytes. They
/// have 78 466 and 639 049 bytes, somewhat more than `config(100)` (50 571) and `config(1000)`
/// (510 495).
pub const IRREGULAR: [(&str, u64, usize); 2] = [("60k", 1, 60_000), ("600k", 2, 600_000)];

/// An irregular configuration of at least `size` bytes, the same for the same `seed` and
/// `size`. Entries are added at the root until the document has `size` bytes, so it ends
/// after the entry that reaches it.
pub fn irregular(seed: u64, size: usize) -> String {
    let mut generator = Generator {
        rng: Rng::new(seed),
        out: String::with_capacity(size + size / 4),
        indent: "    ",
    };
    generator
        .out
        .push_str("# generated irregular configuration\n");
    while generator.out.len() < size {
        generator.indent = generator.rng.pick(&["    ", "  ", "\t"]);
        generator.entry(0, 0);
    }
    generator.out
}

/// SplitMix64: a small generator that is deterministic across platforms.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// A number in `0..n`, for `n > 0`.
    pub fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }

    /// A number in `low..=high`.
    pub fn between(&mut self, low: usize, high: usize) -> usize {
        low + self.below(high - low + 1)
    }

    /// True with probability `percent` / 100.
    pub fn percent(&mut self, percent: usize) -> bool {
        self.below(100) < percent
    }

    pub fn pick<T: Copy>(&mut self, items: &[T]) -> T {
        items[self.below(items.len())]
    }
}

/// Section depth at which no more sections are opened.
const MAX_DEPTH: usize = 7;

/// Words for keys, labels and unquoted values: ASCII letters only, and none of the keywords of
/// spec §4.5.
const WORDS: &[&str] = &[
    "listen",
    "port",
    "host",
    "timeout",
    "workers",
    "user",
    "group",
    "pid",
    "log",
    "level",
    "max",
    "min",
    "connections",
    "keepalive",
    "ssl",
    "certificate",
    "key",
    "ciphers",
    "root",
    "index",
    "location",
    "upstream",
    "server",
    "backend",
    "weight",
    "fails",
    "cache",
    "size",
    "path",
    "enabled",
    "rate",
    "limit",
    "burst",
    "zone",
    "headers",
    "allow",
    "deny",
    "proxy",
    "buffer",
    "compression",
    "format",
    "output",
    "rotate",
    "retention",
    "database",
    "dsn",
    "pool",
    "replicas",
    "shards",
    "region",
    "endpoint",
    "bucket",
    "token",
    "secret",
    "scope",
    "issuer",
    "algorithm",
    "interval",
    "threshold",
    "action",
    "symbol",
    "score",
    "description",
    "filter",
    "pattern",
    "metric",
    "labels",
    "alert",
    "severity",
    "channel",
    "recipients",
    "template",
    "subject",
    "body",
    "mode",
    "strategy",
    "priority",
    "ttl",
    "queue",
    "topic",
    "partition",
    "consumer",
    "producer",
    "broker",
    "worker",
    "task",
    "schedule",
    "retry",
    "backoff",
    "jitter",
    "module",
    "plugin",
    "handler",
    "session",
    "cookie",
    "domain",
    "policy",
    "route",
    "match",
    "rewrite",
    "redirect",
    "status",
    "health",
    "check",
    "probe",
    "memory",
    "cpu",
    "disk",
    "network",
    "interface",
    "address",
    "gateway",
    "dns",
    "resolver",
    "mirror",
    "repository",
    "package",
    "version",
    "signature",
    "fingerprint",
    "classifier",
    "statistics",
    "learn",
    "spam",
    "ham",
    "greylist",
    "whitelist",
    "multimap",
    "neural",
    "phishing",
    "dkim",
    "arc",
    "milter",
    "redis",
    "servers",
    "expire",
    "prefix",
    "suffix",
    "selector",
];

/// Non-ASCII words, used as bare keys and unquoted values.
const NON_ASCII_WORDS: &[&str] = &[
    "größe",
    "straße",
    "Zürich",
    "café",
    "naïve",
    "ключ",
    "значение",
    "Київ",
    "їжак",
    "τιμή",
    "διακομιστής",
    "設定",
    "サーバー",
    "配置",
    "服务器",
    "خادم",
    "שרת",
    "Ångström",
    "smörgåsbord",
];

/// Word lists of the scripts that strings are written in.
const ENGLISH: &[&str] = &[
    "the",
    "server",
    "request",
    "failed",
    "after",
    "three",
    "retries",
    "connection",
    "was",
    "reset",
    "by",
    "peer",
    "while",
    "reading",
    "response",
    "header",
    "from",
    "upstream",
    "client",
    "sent",
    "invalid",
    "certificate",
    "chain",
    "cache",
    "entry",
    "expired",
    "and",
    "will",
    "be",
    "refreshed",
    "on",
    "next",
    "access",
    "message",
    "rejected",
    "policy",
    "violation",
    "for",
    "domain",
    "queue",
    "is",
    "full",
    "please",
    "try",
    "again",
    "later",
    "this",
    "value",
    "overrides",
    "default",
    "see",
    "documentation",
    "(section",
    "4.2)",
    "[deprecated]",
    "{legacy}",
    "100%",
    "a/b",
    "x-y",
    "#tag",
    "@admin",
    "e-mail",
    "rate:",
    "limit!",
    "ok?",
    "a+b",
    "k=v",
    "*",
    "&",
];
const LATIN: &[&str] = &[
    "déjà",
    "vu",
    "façade",
    "résumé",
    "élève",
    "año",
    "niño",
    "über",
    "Größe",
    "Straße",
    "smörgåsbord",
    "Ångström",
    "naïve",
    "coöperate",
    "Zürich",
    "São",
    "Paulo",
    "Kraków",
    "Łódź",
    "Dvořák",
    "Čeština",
    "Ελλάδα",
    "garçon",
    "œuvre",
];
const CYRILLIC: &[&str] = &[
    "сервер",
    "запрос",
    "ошибка",
    "конфигурация",
    "значение",
    "привет",
    "мир",
    "Київ",
    "їжак",
    "ґанок",
    "соединение",
    "сброшено",
    "повторите",
    "позже",
    "очередь",
    "переполнена",
];
const GREEK: &[&str] = &[
    "διακομιστής",
    "σφάλμα",
    "τιμή",
    "αίτημα",
    "σύνδεση",
    "ρύθμιση",
    "ουρά",
    "Ωμέγα",
];
const CJK: &[&str] = &[
    "設定",
    "サーバー",
    "エラー",
    "値",
    "日本語",
    "中文",
    "配置文件",
    "服务器",
    "错误",
    "请求",
    "失败",
    "接続",
    "再試行",
    "、",
    "。",
];
const RTL: &[&str] = &["خادم", "خطأ", "طلب", "שרת", "שגיאה", "בקשה", "اتصال"];
const MIXED: &[&str] = &[
    "🚀",
    "✅",
    "⚠️",
    "🔥",
    "📦",
    "deploy",
    "ready",
    "готово",
    "完了",
    "✓",
    "→",
    "€",
    "£",
    "±",
    "∞",
    "½",
    "™",
];

/// Escapes for double-quoted strings (spec §6.1), none of them a surrogate.
const ESCAPES: &[&str] = &[
    "\\n", "\\t", "\\\"", "\\\\", "\\/", "\\r", "\\u00e9", "\\u00FC", "\\u20ac", "\\u03a9",
    "\\u4e2d", "\\u0416", "\\u2713", "\\u00a0", "\\u0041",
];

/// Heredoc names: several letters, not one repeated letter (spec §6.3).
const HEREDOC_NAMES: &[&str] = &["EOD", "EOT", "SCRIPT", "TEXT", "END", "BODY", "SQL"];

#[derive(Clone, Copy)]
enum Quoting {
    /// Double-quoted, with or without escapes.
    Double { escapes: bool },
    /// Single-quoted: raw line breaks, `\'`, and backslashes before letters.
    Single,
    /// Heredoc content: anything but `$` and CR.
    Heredoc,
    /// A block or line comment: no quotes, no brackets, no `*` or `/`.
    Comment,
}

struct Generator {
    rng: Rng,
    out: String,
    indent: &'static str,
}

impl Generator {
    fn word(&mut self) -> &'static str {
        self.rng.pick(WORDS)
    }

    /// Indentation for `depth`, unless the line already has text (after `a = 1; `).
    fn indent(&mut self, depth: usize) {
        if self.out.is_empty() || self.out.ends_with('\n') {
            for _ in 0..depth {
                self.out.push_str(self.indent);
            }
        }
    }

    /// One entry of a section at `depth` (0 is the root) that has `width` entries.
    fn entry(&mut self, depth: usize, width: usize) {
        if self.rng.percent(6) {
            self.indent(depth);
            self.line_comment();
        }
        if self.rng.percent(2) {
            self.indent(depth);
            self.block_comment(depth);
        }
        let section = match depth {
            0 => 55,
            1 => 12,
            2 => 9,
            3 => 7,
            4 => 5,
            5 => 4,
            6 => 3,
            _ => 0,
        };
        let section = if width > 40 { section / 4 } else { section };
        self.indent(depth);
        if depth < MAX_DEPTH && self.rng.percent(section) {
            self.section(depth);
        } else {
            self.value_entry(depth);
        }
        if self.rng.percent(8) && self.out.ends_with('\n') {
            self.out.push('\n');
        }
    }

    /// A braced section, possibly with names (spec §3.4).
    fn section(&mut self, depth: usize) {
        let key = self.key();
        let header = match self.rng.below(100) {
            0..=44 => format!("{key} {{"),
            45..=59 => format!("{key} = {{"),
            60..=64 => format!("{key}: {{"),
            65..=67 => format!("{key} : {{"),
            68..=79 if !key.starts_with('"') => format!("{key} {} {{", self.label()),
            80..=89 if !key.starts_with('"') => {
                format!("{key} \"{}\" {{", self.quoted_label())
            }
            90..=92 if !key.starts_with('"') => {
                format!("{key} {} {} {{", self.label(), self.label())
            }
            _ => format!("{key} {{"),
        };
        self.out.push_str(&header);
        let width = self.section_width();
        if width == 0 && self.rng.percent(50) {
            self.out.push('}');
        } else {
            self.out.push('\n');
            for _ in 0..width {
                self.entry(depth + 1, width);
            }
            if !self.out.ends_with('\n') {
                self.out.push('\n');
            }
            self.indent(depth);
            self.out.push('}');
        }
        self.out
            .push_str(if self.rng.percent(5) { ";\n" } else { "\n" });
    }

    fn section_width(&mut self) -> usize {
        match self.rng.below(100) {
            0..=29 => self.rng.between(0, 3),
            30..=64 => self.rng.between(4, 15),
            65..=89 => self.rng.between(16, 40),
            _ => self.rng.between(41, 120),
        }
    }

    /// A key and a value that is not a section; sometimes the key is repeated (an implicit
    /// array).
    fn value_entry(&mut self, depth: usize) {
        let key = self.key();
        let kind = self.value_kind(depth);
        let repeats = if self.rng.percent(5) {
            self.rng.between(2, 4)
        } else {
            1
        };
        for n in 0..repeats {
            if n > 0 {
                self.indent(depth);
            }
            let value = self.value(kind, depth);
            let multi_line = value.contains('\n');
            // An nginx-style line has no bracket anywhere after the key (spec §3.4), except
            // an object or array value itself.
            let brackets = value.contains(['{', '[']) && !value.starts_with(['{', '[']);
            let separator = match self.rng.below(100) {
                0..=39 => " = ",
                40..=49 => "=",
                50..=59 => ": ",
                60..=62 => " : ",
                _ if brackets => " = ",
                _ => " ",
            };
            self.out.push_str(&key);
            self.out.push_str(separator);
            self.out.push_str(&value);
            self.terminator(multi_line || kind == Kind::Heredoc);
        }
    }

    /// What follows a value: a terminator, a comment, or the next entry on the same line.
    fn terminator(&mut self, needs_line_break: bool) {
        match self.rng.below(100) {
            0..=39 => self.out.push_str(";\n"),
            40..=79 => self.out.push('\n'),
            80..=87 => self.out.push_str(",\n"),
            88..=93 => {
                self.out.push_str("; ");
                self.line_comment();
            }
            _ if needs_line_break => self.out.push('\n'),
            _ => self.out.push_str("; "),
        }
    }

    fn value_kind(&mut self, depth: usize) -> Kind {
        let containers = depth < MAX_DEPTH + 1;
        loop {
            let kind = match self.rng.below(100) {
                0..=7 => Kind::Bool,
                8..=19 => Kind::Int,
                20..=24 => Kind::Float,
                25..=29 => Kind::Multiplier,
                30..=36 => Kind::Time,
                37..=54 => Kind::Atom,
                55..=72 => Kind::DoubleQuoted,
                73..=77 => Kind::SingleQuoted,
                78..=79 => Kind::Heredoc,
                80..=89 => Kind::Array,
                90..=93 => Kind::InlineObject,
                94..=96 => Kind::Json,
                _ => Kind::Null,
            };
            if containers || !matches!(kind, Kind::InlineObject | Kind::Json) {
                return kind;
            }
        }
    }

    fn value(&mut self, kind: Kind, depth: usize) -> String {
        match kind {
            Kind::Bool => self
                .rng
                .pick(&[
                    "true", "false", "yes", "no", "on", "off", "True", "FALSE", "Yes",
                ])
                .to_string(),
            Kind::Null => "null".to_string(),
            Kind::Int => self.int(),
            Kind::Float => self.float(),
            Kind::Multiplier => self.multiplier(),
            Kind::Time => self.time(),
            Kind::Atom => self.atom(),
            Kind::DoubleQuoted => {
                let escapes = self.rng.percent(40);
                self.double_quoted(escapes)
            }
            Kind::SingleQuoted => self.single_quoted(),
            Kind::Heredoc => self.heredoc(),
            Kind::Array => self.array(depth),
            Kind::InlineObject => self.inline_object(),
            Kind::Json => self.json_object(depth),
        }
    }

    fn key(&mut self) -> String {
        match self.rng.below(100) {
            0..=39 => self.word().to_string(),
            40..=57 => format!("{}_{}", self.word(), self.word()),
            58..=65 => format!("{}-{}", self.word(), self.word()),
            66..=72 => format!("{}{}", self.word(), self.rng.below(100)),
            73..=76 => format!("{}.{}", self.word(), self.word()),
            77..=79 => ((b'a' + self.rng.below(26) as u8) as char).to_string(),
            80..=83 => {
                let n = self.rng.between(3, 5);
                let words: Vec<&str> = (0..n).map(|_| self.word()).collect();
                words.join("_")
            }
            84..=87 => {
                let second = self.word();
                let mut camel = self.word().to_string();
                camel.push_str(&second[..1].to_ascii_uppercase());
                camel.push_str(&second[1..]);
                camel
            }
            88..=90 => self
                .rng
                .pick(&["404", "500", "8080", "0", "2024"])
                .to_string(),
            91..=93 => format!("/{}/{}", self.word(), self.word()),
            94..=95 => self.rng.pick(NON_ASCII_WORDS).to_string(),
            _ => self.quoted_key(),
        }
    }

    fn quoted_key(&mut self) -> String {
        match self.rng.below(5) {
            0 => format!("\"{} {}\"", self.word(), self.word()),
            1 => format!("\"X-{}-{}\"", self.word(), self.word()),
            2 => format!("\"{} {}\"", self.rng.pick(NON_ASCII_WORDS), self.word()),
            3 => format!("\"{}\\t{}\"", self.word(), self.word()),
            _ => format!("\"\\u00e9t\\u00e9 {}\"", self.word()),
        }
    }

    /// A bare section name.
    fn label(&mut self) -> String {
        match self.rng.below(4) {
            0 => self.word().to_string(),
            1 => format!("{}-{}", self.word(), self.rng.below(50)),
            2 => format!("{}.example.org", self.word()),
            _ => self.rng.below(1000).to_string(),
        }
    }

    /// The text of a double-quoted section name.
    fn quoted_label(&mut self) -> String {
        match self.rng.below(3) {
            0 => format!("{} {}", self.word(), self.word()),
            1 => format!("{} office", self.rng.pick(NON_ASCII_WORDS)),
            _ => format!("{}.example.org", self.word()),
        }
    }

    fn int(&mut self) -> String {
        match self.rng.below(10) {
            0..=3 => self.rng.below(100).to_string(),
            4 => self.rng.between(1024, 65535).to_string(),
            5 => (self.rng.next_u64() >> self.rng.between(1, 40)).to_string(),
            6 => format!("-{}", self.rng.below(100_000)),
            7 => format!("0x{:x}", self.rng.next_u64() >> 36),
            8 => format!("0X{:X}", self.rng.below(65536)),
            _ => format!("{:03}", self.rng.below(1000)),
        }
    }

    fn float(&mut self) -> String {
        match self.rng.below(5) {
            0 => format!("0.{}", self.rng.below(1000)),
            1 => format!("-{}.{}", self.rng.below(100), self.rng.below(100)),
            2 => format!(
                "{}.{}e{}",
                self.rng.below(10),
                self.rng.below(100),
                self.rng.between(0, 40) as i32 - 20
            ),
            3 => format!("{}E+{}", self.rng.between(1, 9), self.rng.below(10)),
            _ => format!("{}.{:05}", self.rng.below(10), self.rng.below(100_000)),
        }
    }

    fn multiplier(&mut self) -> String {
        let suffix = self
            .rng
            .pick(&["k", "m", "g", "kb", "mb", "gb", "K", "KB", "Mb"]);
        if self.rng.percent(20) {
            format!("{}.{}{suffix}", self.rng.below(100), self.rng.between(1, 9))
        } else {
            format!("{}{suffix}", self.rng.between(1, 4096))
        }
    }

    fn time(&mut self) -> String {
        let suffix = self
            .rng
            .pick(&["s", "ms", "min", "h", "d", "w", "y", "S", "MS", "Min"]);
        if self.rng.percent(20) {
            format!("{}.{}{suffix}", self.rng.below(100), self.rng.between(1, 9))
        } else {
            format!("{}{suffix}", self.rng.between(1, 1000))
        }
    }

    /// An unquoted string: no `#`, `/*`, brackets, backslashes, quotes or `$`, and not starting
    /// with a digit or `-` unless it is not a number either way (spec §4, §5).
    fn atom(&mut self) -> String {
        match self.rng.below(100) {
            0..=24 => {
                let (a, b) = (self.word(), self.word());
                self.rng
                    .pick(&[a, "round-robin", "least_conn", b])
                    .to_string()
            }
            25..=39 => format!(
                "{}-{}.{}.example.org",
                self.word(),
                self.rng.below(20),
                self.word()
            ),
            40..=54 => format!(
                "/var/lib/{}/{}_{}.db",
                self.word(),
                self.word(),
                self.rng.below(100)
            ),
            55..=64 => format!(
                "https://{}.example.com:{}/{}/{}?{}={}&{}={}",
                self.word(),
                self.rng.between(1024, 65535),
                self.word(),
                self.word(),
                self.word(),
                self.rng.below(1000),
                self.word(),
                self.word()
            ),
            65..=72 => format!(
                "{}.{}@{}.example.net",
                self.word(),
                self.word(),
                self.word()
            ),
            73..=82 => format!("{} {} {}", self.word(), self.word(), self.word()),
            83..=86 => format!(
                "10.{}.{}.{}:{}",
                self.rng.below(256),
                self.rng.below(256),
                self.rng.below(256),
                self.rng.between(1, 65535)
            ),
            87..=88 => format!(
                "192.168.{}.0/{}",
                self.rng.below(256),
                self.rng.between(8, 30)
            ),
            89..=93 => {
                let words: Vec<&str> = (0..self.rng.between(1, 3))
                    .map(|_| self.rng.pick(NON_ASCII_WORDS))
                    .collect();
                words.join(" ")
            }
            94..=96 => {
                let n = self.rng.between(20, 60);
                let words: Vec<&str> = (0..n).map(|_| self.word()).collect();
                words.join(" ")
            }
            _ => format!(
                "v{}.{}.{}",
                self.rng.below(10),
                self.rng.below(30),
                self.rng.below(100)
            ),
        }
    }

    /// The length of a string's text, in bytes.
    fn string_length(&mut self) -> usize {
        match self.rng.below(1000) {
            0..=4 => self.rng.between(2000, 16000),
            5..=44 => self.rng.between(200, 2000),
            45..=244 => self.rng.between(25, 200),
            _ => self.rng.between(1, 24),
        }
    }

    /// The words of a script, and whether they are separated by spaces.
    fn script(&mut self) -> (&'static [&'static str], bool) {
        match self.rng.below(100) {
            0..=59 => (ENGLISH, true),
            60..=69 => (LATIN, true),
            70..=79 => (CYRILLIC, true),
            80..=83 => (GREEK, true),
            84..=91 => (CJK, false),
            92..=94 => (RTL, true),
            _ => (MIXED, true),
        }
    }

    /// Text of about `len` bytes for `quoting`.
    fn text(&mut self, len: usize, quoting: Quoting) -> String {
        let (script, spaced) = match quoting {
            Quoting::Comment => (ENGLISH_PLAIN, true),
            _ => self.script(),
        };
        let mut text = String::new();
        let mut line = 0;
        while text.len() < len {
            if !text.is_empty() {
                let long_line = text.len() - line > 72;
                match quoting {
                    Quoting::Single | Quoting::Heredoc if long_line && self.rng.percent(60) => {
                        text.push('\n');
                        line = text.len();
                    }
                    _ if spaced => text.push(' '),
                    _ => {}
                }
            }
            match quoting {
                Quoting::Double { escapes: true } if self.rng.percent(10) => {
                    text.push_str(self.rng.pick(ESCAPES));
                }
                Quoting::Single if self.rng.percent(3) => {
                    let word = self.word();
                    text.push_str(self.rng.pick(&["\\'", "it\\'s", "C:\\Users\\"]));
                    text.push_str(word);
                }
                _ => {}
            }
            text.push_str(self.rng.pick(script));
        }
        text
    }

    fn double_quoted(&mut self, escapes: bool) -> String {
        let len = self.string_length();
        format!("\"{}\"", self.text(len, Quoting::Double { escapes }))
    }

    fn single_quoted(&mut self) -> String {
        let len = self.string_length();
        let mut text = self.text(len, Quoting::Single);
        if self.rng.percent(10) {
            text.push_str(" $HOME \"quoted\"");
        }
        format!("'{text}'")
    }

    fn heredoc(&mut self) -> String {
        let name = self.rng.pick(HEREDOC_NAMES);
        let len = match self.rng.below(10) {
            0..=6 => self.rng.between(10, 300),
            7..=8 => self.rng.between(300, 3000),
            _ => self.rng.between(3000, 12000),
        };
        let mut text = String::new();
        for line in self.text(len, Quoting::Heredoc).lines() {
            if !text.is_empty() {
                text.push('\n');
            }
            for _ in 0..self.rng.below(3) {
                text.push_str("    ");
            }
            text.push_str(line);
        }
        format!("<<{name}\n{text}\n{name}")
    }

    fn array(&mut self, depth: usize) -> String {
        let n = match self.rng.below(100) {
            0..=4 => 0,
            5..=74 => self.rng.between(1, 5),
            75..=94 => self.rng.between(6, 30),
            _ => self.rng.between(31, 300),
        };
        let mixed = self.rng.percent(30);
        let mut kind = self.element_kind(depth);
        let mut elements = Vec::with_capacity(n);
        for _ in 0..n {
            if mixed {
                kind = self.element_kind(depth);
            }
            elements.push(self.value(kind, depth + 1));
        }
        let multi_line = n > 6 || elements.iter().any(|e| e.contains('\n')) || self.rng.percent(20);
        if !multi_line {
            return format!("[{}]", elements.join(", "));
        }
        let mut out = String::from("[\n");
        let last = elements.len().saturating_sub(1);
        for (i, element) in elements.iter().enumerate() {
            for _ in 0..=depth {
                out.push_str(self.indent);
            }
            out.push_str(element);
            out.push_str(if i < last || self.rng.percent(50) {
                ",\n"
            } else {
                "\n"
            });
        }
        for _ in 0..depth {
            out.push_str(self.indent);
        }
        out.push(']');
        out
    }

    fn element_kind(&mut self, depth: usize) -> Kind {
        loop {
            let kind = match self.rng.below(100) {
                0..=29 => Kind::Atom,
                30..=54 => Kind::DoubleQuoted,
                55..=59 => Kind::SingleQuoted,
                60..=74 => Kind::Int,
                75..=79 => Kind::Float,
                80..=84 => Kind::Time,
                85..=87 => Kind::Multiplier,
                88..=90 => Kind::Bool,
                91..=95 => Kind::InlineObject,
                _ => Kind::Array,
            };
            let nested = matches!(kind, Kind::InlineObject | Kind::Array);
            if !nested || depth < MAX_DEPTH + 1 {
                return kind;
            }
        }
    }

    /// `{ a = 1; b = "x"; }` on one line: every value ends with `;`, so a suffix never meets a
    /// space (spec §5.5).
    fn inline_object(&mut self) -> String {
        let n = self.rng.between(0, 6);
        let mut out = String::from("{ ");
        for _ in 0..n {
            let key = self.word();
            let value = match self.rng.below(6) {
                0 => self.int(),
                1 => self.time(),
                2 => self.double_quoted(false),
                3 => self.atom(),
                4 => self.value(Kind::Bool, 0),
                _ => self.multiplier(),
            };
            let value = if value.contains('\n') {
                self.int()
            } else {
                value
            };
            write!(out, "{key} = {value}; ").unwrap();
        }
        out.push('}');
        out
    }

    /// An object in JSON syntax, one entry per line.
    fn json_object(&mut self, depth: usize) -> String {
        let n = self.rng.between(1, 12);
        let mut out = String::from("{\n");
        for i in 0..n {
            for _ in 0..=depth {
                out.push_str(self.indent);
            }
            let key = self.word();
            let value = match self.rng.below(8) {
                0 | 1 => self.rng.below(100_000).to_string(),
                2 => self.float(),
                3 | 4 => {
                    let escapes = self.rng.percent(40);
                    let len = self.rng.between(1, 60);
                    format!("\"{}\"", self.text(len, Quoting::Double { escapes }))
                }
                5 => self.rng.pick(&["true", "false", "null"]).to_string(),
                6 => {
                    let items: Vec<String> = (0..self.rng.between(0, 8))
                        .map(|_| self.rng.below(1000).to_string())
                        .collect();
                    format!("[{}]", items.join(", "))
                }
                _ => format!(
                    "{{\"{}\": {}, \"{}\": \"{}\"}}",
                    self.word(),
                    self.rng.below(10),
                    self.word(),
                    self.word()
                ),
            };
            write!(out, "\"{key}\": {value}").unwrap();
            out.push_str(if i + 1 < n { ",\n" } else { "\n" });
        }
        for _ in 0..depth {
            out.push_str(self.indent);
        }
        out.push('}');
        out
    }

    fn line_comment(&mut self) {
        let len = self.rng.between(5, 80);
        let text = self.text(len, Quoting::Comment);
        self.out.push_str(self.rng.pick(&["# ", "#", "## "]));
        self.out.push_str(&text);
        self.out.push('\n');
    }

    /// A block comment on its own lines, sometimes with a nested one (spec §2.3).
    fn block_comment(&mut self, depth: usize) {
        let len = self.rng.between(10, 200);
        let text = self.text(len, Quoting::Comment);
        match self.rng.below(3) {
            0 => writeln!(self.out, "/* {text} */").unwrap(),
            1 => {
                let inner = self.text(20, Quoting::Comment);
                writeln!(self.out, "/* {text}\n   /* {inner} */ */").unwrap();
            }
            _ => {
                self.out.push_str("/*\n");
                for chunk in text.split(' ').collect::<Vec<_>>().chunks(8) {
                    self.indent(depth);
                    writeln!(self.out, " * {}", chunk.join(" ")).unwrap();
                }
                self.indent(depth);
                self.out.push_str(" */\n");
            }
        }
    }
}

/// Words for comments: no quotes, brackets, `*` or `/`.
const ENGLISH_PLAIN: &[&str] = &[
    "the",
    "server",
    "section",
    "below",
    "overrides",
    "default",
    "values",
    "keep",
    "in",
    "sync",
    "with",
    "production",
    "settings",
    "do",
    "not",
    "edit",
    "generated",
    "by",
    "deploy",
    "tool",
    "see",
    "ticket",
    "OPS-1234",
    "für",
    "Größe",
    "настройка",
    "設定",
    "✓",
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Bool,
    Null,
    Int,
    Float,
    Multiplier,
    Time,
    Atom,
    DoubleQuoted,
    SingleQuoted,
    Heredoc,
    Array,
    InlineObject,
    Json,
}
